use super::*;

impl Browser {
    pub fn launch(options: LaunchOptions) -> OpenPageResult<Self> {
        let runtime =
            Arc::new(Runtime::new().map_err(|err| browser_launch_error("create runtime", err))?);

        let mut options = options;
        if let Some(path) = browser_path_env_override() {
            options.browser_path = Some(path);
        }

        if options.auto_port && options.remote_debugging_port.is_none() {
            options.remote_debugging_port = Some(find_free_port(options.auto_port_scope())?);
        }

        if let Some(ws_address) = options.ws_address.as_deref() {
            return Self::connect(ws_address);
        }

        if let Some(address) = options.address.as_deref() {
            if options.existing_only
                || !is_local_debugger_address(address)
                || debugger_address_port(address).is_none()
                || local_debugger_address_is_open(address)
            {
                let debugger_url = format!("http://{address}");
                return Self::connect(&debugger_url);
            }
        }

        if options.existing_only {
            let port = options.remote_debugging_port.unwrap_or(9222);
            let debugger_url = format!("http://127.0.0.1:{port}");
            return Self::connect(&debugger_url);
        }

        let (resolved_user_data_dir, use_temp_user_data_dir) =
            resolve_launch_user_data_dir(&options)?;
        if options.new_env {
            if let Some(user_data_dir) = resolved_user_data_dir.as_deref() {
                reset_browser_user_data_dir(user_data_dir)?;
            }
        }
        let base_tmp = options.tmp_path.as_deref();
        let download_spool_dir = make_temp_download_dir(base_tmp)?;
        if let Some(user_data_dir) = &resolved_user_data_dir {
            if !options.prefs.is_empty() || !options.prefs_to_remove.is_empty() {
                write_chrome_prefs(
                    user_data_dir,
                    &options.args,
                    &options.prefs,
                    &options.prefs_to_remove,
                )?;
            }
            if options.clear_file_flags || !options.flags.is_empty() {
                write_chrome_flags(user_data_dir, &options.flags, options.clear_file_flags)?;
            }
        }
        let config = build_browser_config(&options, resolved_user_data_dir.as_deref())?;
        let configured_download_path = options.download_path.clone();
        let debugger_address = options.address();
        let connect_timeout = browser_connect_timeout_duration();
        let connect_timeout_ms = timeout_duration_millis(connect_timeout);
        let (mut browser, mut handler) = runtime
            .block_on(
                async move { tokio_timeout(connect_timeout, OxBrowser::launch(config)).await },
            )
            .map_err(|_| timeout_error("Browser::launch()", connect_timeout_ms))?
            .map_err(|err| browser_launch_error("launch browser", err))?;

        let handler_task = runtime.spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    eprintln!("openpage handler error: {err}");
                }
            }
        });
        let newest_tab_id = Arc::new(StdMutex::new(None));
        let (target_created_task, target_destroyed_task) =
            attach_newest_tab_tracker(&runtime, &browser, Arc::clone(&newest_tab_id))?;
        let (downloads, download_task) = attach_download_store(Arc::clone(&runtime), &browser)?;
        configure_download_behavior(&runtime, &browser, &download_spool_dir)?;
        let launched_browser_pid = browser_pid(&mut browser);

        let browser = Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                debugger_address,
                browser_pid: launched_browser_pid,
                downloads,
                newest_tab_id,
                download_path: StdMutex::new(configured_download_path.clone()),
                download_file_exists: StdMutex::new(options.download_file_exists),
                browser_download_naming: StdMutex::new(BrowserDownloadNaming::default()),
                load_mode: StdMutex::new(options.load_mode),
                page_cache: StdMutex::new(HashMap::new()),
                isolated_contexts: StdMutex::new(HashMap::new()),
                page_download_settings: StdMutex::new(HashMap::new()),
                mission_download_settings: StdMutex::new(HashMap::new()),
                download_spool_dir: download_spool_dir.clone(),
                temp_user_data_dir: if use_temp_user_data_dir {
                    resolved_user_data_dir
                } else {
                    None
                },
                temp_download_dir: Some(download_spool_dir),
                headless: options.headless,
                timeouts: StdMutex::new(options.timeouts),
                retry_times: StdMutex::new(options.retry_times),
                retry_interval_millis: StdMutex::new(options.retry_interval_millis),
                _download_task: download_task,
                _handler_task: handler_task,
                _target_created_task: target_created_task,
                _target_destroyed_task: target_destroyed_task,
            }),
        };
        browser.seed_newest_tab_id_from_tab_infos()?;

        if let Some(path) = configured_download_path {
            browser.set_download_path(path)?;
        }

        Ok(browser)
    }

    pub fn connect(debugger_url: &str) -> OpenPageResult<Self> {
        let runtime =
            Arc::new(Runtime::new().map_err(|err| browser_launch_error("create runtime", err))?);
        let download_spool_dir = make_temp_download_dir(None)?;
        let url = debugger_url.to_string();
        let debugger_address = normalize_debugger_address(debugger_url).0;
        let connect_timeout = browser_connect_timeout_duration();
        let connect_timeout_ms = timeout_duration_millis(connect_timeout);
        let (mut browser, mut handler) = runtime
            .block_on(async move { tokio_timeout(connect_timeout, OxBrowser::connect(url)).await })
            .map_err(|_| timeout_error("Browser::connect()", connect_timeout_ms))?
            .map_err(|err| browser_launch_error("connect browser", err))?;

        let handler_task = runtime.spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    eprintln!("openpage handler error: {err}");
                }
            }
        });
        let newest_tab_id = Arc::new(StdMutex::new(None));
        let (target_created_task, target_destroyed_task) =
            attach_newest_tab_tracker(&runtime, &browser, Arc::clone(&newest_tab_id))?;

        runtime.block_on(async {
            run_browser_future_with_cdp_timeout(
                browser.fetch_targets(),
                "Browser::connect().fetch_targets()",
            )
            .await
        })?;

        let (downloads, download_task) = attach_download_store(Arc::clone(&runtime), &browser)?;
        configure_download_behavior(&runtime, &browser, &download_spool_dir)?;

        let browser = Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                debugger_address,
                browser_pid: None,
                downloads,
                newest_tab_id,
                download_path: StdMutex::new(None),
                download_file_exists: StdMutex::new(DownloadFileExistsMode::Rename),
                browser_download_naming: StdMutex::new(BrowserDownloadNaming::default()),
                load_mode: StdMutex::new(LoadMode::Normal),
                page_cache: StdMutex::new(HashMap::new()),
                isolated_contexts: StdMutex::new(HashMap::new()),
                page_download_settings: StdMutex::new(HashMap::new()),
                mission_download_settings: StdMutex::new(HashMap::new()),
                download_spool_dir: download_spool_dir.clone(),
                temp_user_data_dir: None,
                temp_download_dir: Some(download_spool_dir),
                headless: false,
                timeouts: StdMutex::new(TimeoutConfig::default()),
                retry_times: StdMutex::new(3),
                retry_interval_millis: StdMutex::new(2_000),
                _download_task: download_task,
                _handler_task: handler_task,
                _target_created_task: target_created_task,
                _target_destroyed_task: target_destroyed_task,
            }),
        };
        browser.seed_newest_tab_id_from_tab_infos()?;

        Ok(browser)
    }

    fn tracked_newest_tab_id(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .newest_tab_id
            .lock()
            .map(|target_id| target_id.clone())
            .map_err(|_| browser_newest_tab_lock_poisoned_error())
    }

    fn set_tracked_newest_tab_id(&self, target_id: Option<String>) -> OpenPageResult<()> {
        *self
            .inner
            .newest_tab_id
            .lock()
            .map_err(|_| browser_newest_tab_lock_poisoned_error())? = target_id;
        Ok(())
    }

    fn seed_newest_tab_id_from_tab_infos(&self) -> OpenPageResult<()> {
        let target_id = self
            .tab_infos()?
            .into_iter()
            .next()
            .map(|info| info.target_id);
        self.set_tracked_newest_tab_id(target_id)
    }

    pub(crate) fn newest_tab_id(&self) -> OpenPageResult<Option<String>> {
        let current_ids = self.tab_ids()?;
        Ok(resolve_newest_tab_id(
            &current_ids,
            self.tracked_newest_tab_id()?,
        ))
    }

    pub fn new_page<'a, U>(&self, url: U) -> OpenPageResult<Page>
    where
        U: Into<BrowserPageUrlInput<'a>>,
    {
        let target_url = match url.into() {
            BrowserPageUrlInput::None => "about:blank".to_string(),
            BrowserPageUrlInput::Url(url) => url.into_owned(),
        };
        let load_mode = self.load_mode_value()?;
        let page = self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::new_page().lock()").await?;
            run_browser_future_with_cdp_timeout(browser.new_page(target_url), "Browser::new_page()")
                .await
        })?;
        let page = self.realize_page(page, load_mode)?;
        self.set_tracked_newest_tab_id(Some(page.target_id()))?;
        Ok(page)
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> OpenPageResult<Page> {
        let isolated_context_id = if new_context {
            Some(self.inner.runtime.block_on(async {
                let browser = lock_with_cdp_timeout(
                    &self.inner.browser,
                    "Browser::new_tab().create_browser_context().lock()",
                )
                .await?;
                run_browser_future_with_cdp_timeout(
                    browser.create_browser_context(CreateBrowserContextParams::default()),
                    "Browser::new_tab().create_browser_context()",
                )
                .await
                .map(|id| id.as_ref().to_string())
            })?)
        } else {
            None
        };

        let effective_new_window = new_window || new_context;
        let mut params = CreateTargetParams::builder()
            .url(url.unwrap_or("about:blank"))
            .new_window(effective_new_window)
            .background(background);
        if let Some(browser_context_id) = isolated_context_id.as_deref() {
            params = params.browser_context_id(browser_context_id.to_string());
        }
        let params = params.build().map_err(OpenPageError::BrowserOperation)?;
        let load_mode = self.load_mode_value()?;
        let page = match self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::new_tab().new_page().lock()")
                    .await?;
            run_browser_future_with_cdp_timeout(
                browser.new_page(params),
                "Browser::new_tab().new_page()",
            )
            .await
        }) {
            Ok(page) => page,
            Err(err) => {
                if let Some(browser_context_id) = isolated_context_id.as_deref() {
                    let _ = self.dispose_browser_context(browser_context_id);
                }
                return Err(err);
            }
        };
        let target_id = page.target_id().as_ref().to_string();
        let page = match self.realize_page(page, load_mode) {
            Ok(page) => page,
            Err(err) => {
                if let Some(browser_context_id) = isolated_context_id.as_deref() {
                    let _ = self.dispose_browser_context(browser_context_id);
                }
                return Err(err);
            }
        };
        if let Some(browser_context_id) = isolated_context_id.as_deref() {
            if let Err(err) = self.record_isolated_context(&target_id, browser_context_id) {
                let _ = self.dispose_browser_context(browser_context_id);
                let _ = self.remove_cached_page(&target_id);
                return Err(err);
            }
        }
        self.set_tracked_newest_tab_id(Some(target_id))?;
        Ok(page)
    }

    pub fn pages(&self) -> OpenPageResult<Vec<Page>> {
        let load_mode = self.load_mode_value()?;
        let pages = self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::pages().lock()").await?;
            run_browser_future_with_cdp_timeout(browser.pages(), "Browser::pages()").await
        })?;
        let target_ids = pages
            .iter()
            .map(|page| page.target_id().as_ref().to_string())
            .collect::<Vec<_>>();
        self.prune_cached_pages(&target_ids)?;
        pages
            .into_iter()
            .map(|page| self.realize_page(page, load_mode))
            .collect()
    }

    pub fn get_page(&self, target_id: &str) -> OpenPageResult<Page> {
        let load_mode = self.load_mode_value()?;
        let page = self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::get_page().lock()").await?;
            run_browser_future_with_cdp_timeout(
                browser.get_page(TargetId::new(target_id)),
                "Browser::get_page()",
            )
            .await
        });
        match page {
            Ok(page) => self.realize_page(page, load_mode),
            Err(err) => {
                let _ = self.remove_cached_page(target_id);
                Err(err)
            }
        }
    }

    pub(crate) fn wait_for_page(&self, target_id: &str, timeout_ms: u64) -> OpenPageResult<Page> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_page(target_id) {
                Ok(page) => return Ok(page),
                Err(OpenPageError::BrowserOperation(message))
                    if message.contains("Requested value not found.")
                        && Instant::now() < deadline => {}
                Err(err) => return Err(err),
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        self.get_page(target_id)
    }

    fn realize_page(
        &self,
        page: chromiumoxide::page::Page,
        load_mode: LoadMode,
    ) -> OpenPageResult<Page> {
        let runtime = Arc::clone(&self.inner.runtime);
        let target_id = page.target_id().as_ref().to_string();
        if !singleton_tab_obj_enabled() {
            return Ok(Page::new_with_load_mode(runtime, page, load_mode)
                .with_browser(self.clone())
                .with_browser_pid(self.inner.browser_pid));
        }

        let mut cache = self.inner.page_cache.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page cache",
                "页面缓存",
            ))
        })?;
        if let Some(base_page) = cache.get(&target_id) {
            base_page.set_runtime_load_mode(load_mode)?;
            return Ok(base_page
                .clone()
                .with_browser(self.clone())
                .with_browser_pid(self.inner.browser_pid));
        }

        let base_page = Page::new_with_load_mode(runtime, page, load_mode)
            .with_browser_pid(self.inner.browser_pid);
        let page = base_page
            .clone()
            .with_browser(self.clone())
            .with_browser_pid(self.inner.browser_pid);
        cache.insert(target_id, base_page);
        Ok(page)
    }

    fn prune_cached_pages(&self, target_ids: &[String]) -> OpenPageResult<()> {
        if !singleton_tab_obj_enabled() {
            return Ok(());
        }
        let current = target_ids.iter().cloned().collect::<HashSet<_>>();
        self.inner
            .page_cache
            .lock()
            .map(|mut cache| cache.retain(|target_id, _| current.contains(target_id)))
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "page cache",
                    "页面缓存",
                ))
            })
    }

    pub(crate) fn remove_cached_page(&self, target_id: &str) -> OpenPageResult<()> {
        self.inner
            .page_cache
            .lock()
            .map(|mut cache| {
                cache.remove(target_id);
            })
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "page cache",
                    "页面缓存",
                ))
            })
    }

    fn record_isolated_context(
        &self,
        target_id: &str,
        browser_context_id: &str,
    ) -> OpenPageResult<()> {
        self.inner
            .isolated_contexts
            .lock()
            .map(|mut contexts| {
                contexts.insert(target_id.to_string(), browser_context_id.to_string());
            })
            .map_err(|_| isolated_context_lock_poisoned_error())
    }

    pub(crate) fn isolated_context_id(&self, target_id: &str) -> OpenPageResult<Option<String>> {
        self.inner
            .isolated_contexts
            .lock()
            .map(|contexts| contexts.get(target_id).cloned())
            .map_err(|_| isolated_context_lock_poisoned_error())
    }

    pub(crate) fn browser_context_id(&self, target_id: &str) -> OpenPageResult<Option<String>> {
        self.isolated_context_id(target_id)
    }

    fn clear_isolated_context(&self, target_id: &str) {
        if let Ok(mut contexts) = self.inner.isolated_contexts.lock() {
            contexts.remove(target_id);
        }
    }

    fn dispose_browser_context(&self, browser_context_id: &str) -> OpenPageResult<()> {
        let browser_context_id = browser_context_id.to_string();
        self.inner.runtime.block_on(async {
            let browser = lock_with_cdp_timeout(
                &self.inner.browser,
                "Browser::dispose_browser_context().lock()",
            )
            .await?;
            run_browser_future_with_cdp_timeout(
                browser.dispose_browser_context(browser_context_id),
                "Browser::dispose_browser_context()",
            )
            .await?;
            Ok(())
        })
    }

    pub(crate) fn close_target(&self, target_id: &str) -> OpenPageResult<()> {
        if let Some(browser_context_id) = self.isolated_context_id(target_id)? {
            self.dispose_browser_context(&browser_context_id)?;
            self.clear_isolated_context(target_id);
        } else {
            execute_browser_command_blocking(
                self.inner.runtime.as_ref(),
                &self.inner.browser,
                CloseTargetParams::new(TargetId::new(target_id)),
                "Browser::close_target()",
            )?;
        }
        let _ = self.remove_cached_page(target_id);
        Ok(())
    }

    pub fn get_tab<'a, I, T>(
        &self,
        id_or_num: Option<I>,
        title: Option<&str>,
        url: Option<&str>,
        tab_type: Option<T>,
        as_id: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>>
    where
        I: Into<BrowserTabSelector<'a>>,
        T: Into<BrowserTabTypeInput<'a>>,
    {
        let infos = self.tab_infos()?;
        if let Some(id_or_num) = id_or_num {
            let selected = select_browser_tab_info_by_selector(&infos, id_or_num.into())?;
            return selected
                .map(|info| self.browser_tab_reference(info.target_id.clone(), as_id))
                .transpose();
        }

        let tab_types = tab_type
            .map(|value| normalize_browser_tab_types(value.into()))
            .unwrap_or_else(|| vec!["page".to_string()]);
        let selected = infos
            .iter()
            .find(|info| browser_tab_info_matches(info, title, url, &tab_types));
        selected
            .map(|info| self.browser_tab_reference(info.target_id.clone(), as_id))
            .transpose()
    }

    pub fn get_tabs<'a, T>(
        &self,
        title: Option<&str>,
        url: Option<&str>,
        tab_type: Option<T>,
        as_id: bool,
    ) -> OpenPageResult<Vec<BrowserTabReference>>
    where
        T: Into<BrowserTabTypeInput<'a>>,
    {
        let tab_types = tab_type
            .map(|value| normalize_browser_tab_types(value.into()))
            .unwrap_or_else(|| vec!["page".to_string()]);
        self.tab_infos()?
            .into_iter()
            .filter(|info| browser_tab_info_matches(info, title, url, &tab_types))
            .map(|info| self.browser_tab_reference(info.target_id, as_id))
            .collect()
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        Ok(self.tab_infos()?.len())
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        Ok(self
            .tab_infos()?
            .into_iter()
            .map(|info| info.target_id)
            .collect())
    }

    pub fn tab_infos(&self) -> OpenPageResult<Vec<TabInfo>> {
        let mut infos = execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            GetTargetsParams::default(),
            "Browser::tab_infos()",
        )?
        .target_infos
        .into_iter()
        .filter(|target| is_tab_like_type(&target.r#type) && !target.url.starts_with("devtools://"))
        .map(|target| TabInfo {
            target_id: target.target_id.as_ref().to_string(),
            tab_type: target.r#type,
            title: target.title,
            url: target.url,
            attached: target.attached,
        })
        .collect::<Vec<_>>();
        move_newest_tab_info_to_front(&mut infos, self.tracked_newest_tab_id()?);
        Ok(infos)
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<BrowserTabReference>> {
        let Some(target_id) = self.newest_tab_id()? else {
            return Ok(None);
        };
        self.browser_tab_reference(target_id, !singleton_tab_obj_enabled())
            .map(Some)
    }

    pub fn activate_tab<'a, T>(&self, target: T) -> OpenPageResult<()>
    where
        T: Into<BrowserTabSelector<'a>>,
    {
        let target_id = resolve_browser_tab_target_id(&self.tab_infos()?, target.into())?;
        let params = ActivateTargetParams::new(TargetId::new(target_id));
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::activate_tab()",
        )?;
        Ok(())
    }

    pub fn close_tabs<'a, T>(&self, targets: T, others: bool) -> OpenPageResult<usize>
    where
        T: Into<BrowserTabTargetsInput<'a>>,
    {
        let target_ids = resolve_browser_tab_target_ids(&self.tab_infos()?, targets.into())?;
        let closing_ids = if others {
            let keep = target_ids.iter().cloned().collect::<HashSet<_>>();
            self.tab_infos()?
                .into_iter()
                .map(|info| info.target_id)
                .filter(|target_id| !keep.contains(target_id))
                .collect::<Vec<_>>()
        } else {
            target_ids
        };
        if closing_ids.is_empty() {
            return Ok(0);
        }
        for target_id in &closing_ids {
            self.close_target(target_id)?;
        }
        Ok(closing_ids.len())
    }

    fn browser_tab_reference(
        &self,
        target_id: String,
        as_id: bool,
    ) -> OpenPageResult<BrowserTabReference> {
        if as_id {
            Ok(BrowserTabReference::Id(target_id))
        } else {
            if let Some(page) = self
                .pages()?
                .into_iter()
                .find(|page| page.target_id() == target_id)
            {
                Ok(BrowserTabReference::Page(page))
            } else {
                self.get_page(&target_id).map(BrowserTabReference::Page)
            }
        }
    }

    pub fn version(&self) -> OpenPageResult<String> {
        self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::version().lock()").await?;
            let version =
                run_browser_future_with_cdp_timeout(browser.version(), "Browser::version()")
                    .await?;
            Ok(version.product)
        })
    }

    pub fn address(&self) -> String {
        self.inner.debugger_address.clone()
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let cookie = browser_cookie_param(name, value, url, domain, path);
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            SetCookiesParams::new(vec![cookie]),
            "Browser::set_cookie()",
        )?;
        Ok(())
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = parse_browser_cookie_header_url(url)?;
        let cookies = browser_cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            SetCookiesParams::new(cookies),
            "Browser::set_cookie_header()",
        )?;
        Ok(())
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let params = browser_delete_cookie_params(name, url, domain, path);
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::remove_cookie()",
        )?;
        Ok(())
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            ClearBrowserCookiesParams::default(),
            "Browser::clear_cookies()",
        )?;
        Ok(())
    }

    pub fn set_permission(
        &self,
        permission: PermissionDescriptor,
        setting: PermissionSetting,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
        browser_context_id: Option<&str>,
    ) -> OpenPageResult<()> {
        let mut params = SetPermissionParams::builder()
            .permission(permission)
            .setting(setting);
        if let Some(origin) = origin {
            params = params.origin(origin.to_string());
        }
        if let Some(embedded_origin) = embedded_origin {
            params = params.embedded_origin(embedded_origin.to_string());
        }
        if let Some(browser_context_id) = browser_context_id {
            params = params.browser_context_id(browser_context_id.to_string());
        }
        let params = params.build().map_err(OpenPageError::BrowserOperation)?;
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::set_permission()",
        )?;
        Ok(())
    }

    pub fn reset_permissions(&self, browser_context_id: Option<&str>) -> OpenPageResult<()> {
        let mut params = ResetPermissionsParams::builder();
        if let Some(browser_context_id) = browser_context_id {
            params = params.browser_context_id(browser_context_id.to_string());
        }
        let params = params.build();
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::reset_permissions()",
        )?;
        Ok(())
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(self.version().is_ok())
    }

    pub fn is_headless(&self) -> bool {
        self.inner.headless
    }

    pub fn is_existed(&self) -> OpenPageResult<bool> {
        self.is_alive()
    }

    pub fn is_incognito(&self) -> OpenPageResult<bool> {
        self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::is_incognito().lock()")
                    .await?;
            Ok(browser.is_incognito())
        })
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid
    }

    pub fn process_id(&self) -> Option<u32> {
        self.browser_pid()
    }

    pub fn timeouts(&self) -> OpenPageResult<TimeoutConfig> {
        self.inner
            .timeouts
            .lock()
            .map(|t| t.clone())
            .map_err(|_| browser_timeouts_lock_poisoned_error())
    }

    pub fn set_timeouts(&self, timeouts: TimeoutConfig) -> OpenPageResult<()> {
        *self
            .inner
            .timeouts
            .lock()
            .map_err(|_| browser_timeouts_lock_poisoned_error())? = timeouts;
        Ok(())
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        self.inner
            .retry_times
            .lock()
            .map(|retry_times| *retry_times)
            .map_err(|_| browser_retry_times_lock_poisoned_error())
    }

    pub fn retry_interval_millis(&self) -> OpenPageResult<u64> {
        self.inner
            .retry_interval_millis
            .lock()
            .map(|retry_interval_millis| *retry_interval_millis)
            .map_err(|_| browser_retry_interval_lock_poisoned_error())
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> OpenPageResult<()> {
        if let Some(retry_times) = retry_times {
            *self
                .inner
                .retry_times
                .lock()
                .map_err(|_| browser_retry_times_lock_poisoned_error())? = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            *self
                .inner
                .retry_interval_millis
                .lock()
                .map_err(|_| browser_retry_interval_lock_poisoned_error())? = retry_interval_millis;
        }
        Ok(())
    }

    pub fn wait_for_new_tab(
        &self,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        let baseline = self.tab_ids()?;
        let tracked_baseline = self.tracked_newest_tab_id()?;
        let baseline_marker = current_tab_id
            .map(str::trim)
            .filter(|target_id| !target_id.is_empty())
            .map(str::to_string)
            .or_else(|| resolve_newest_tab_id(&baseline, tracked_baseline.clone()));
        let baseline_newest = resolve_newest_tab_id(&baseline, tracked_baseline);
        if let Some(new_id) = find_new_tab_id(
            &baseline,
            &baseline,
            baseline_marker.as_deref(),
            baseline_newest.as_deref(),
        ) {
            return Ok(Some(new_id));
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let current = self.tab_ids()?;
            let current_newest = resolve_newest_tab_id(&current, self.tracked_newest_tab_id()?);
            if let Some(new_id) = find_new_tab_id(
                &baseline,
                &current,
                baseline_marker.as_deref(),
                current_newest.as_deref(),
            ) {
                return Ok(Some(new_id));
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        if wait_failed_should_raise() {
            return Err(timeout_error("Browser::wait_for_new_tab()", timeout_ms));
        }
        Ok(None)
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .download_path
            .lock()
            .map(|path| {
                path.as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .map_err(|_| browser_download_path_lock_poisoned_error())
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        let path = path.as_ref().to_path_buf();
        create_download_directory(&path)?;

        self.inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())?
            .replace(path);
        Ok(())
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.inner
            .download_file_exists
            .lock()
            .map(|mode| mode.as_str().to_string())
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? = mode;
        Ok(())
    }

    pub(crate) fn snapshot_browser_download_settings(
        &self,
    ) -> OpenPageResult<BrowserDownloadSettingsSnapshot> {
        let path = self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())?
            .clone();
        let file_exists = *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())?;
        let naming = self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())?
            .clone();
        Ok(BrowserDownloadSettingsSnapshot {
            path,
            file_exists,
            rename: naming.rename,
            suffix: naming.suffix,
        })
    }

    pub(crate) fn restore_browser_download_settings(
        &self,
        settings: BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        *self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())? = settings.path;
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? =
            settings.file_exists;
        *self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())? = BrowserDownloadNaming {
            rename: settings.rename,
            suffix: settings.suffix,
        };
        Ok(())
    }

    pub(crate) fn apply_browser_download_settings(
        &self,
        settings: &BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        if let Some(path) = &settings.path {
            create_download_directory(path)?;
        }
        *self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())? = settings.path.clone();
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? =
            settings.file_exists;
        *self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())? = BrowserDownloadNaming {
            rename: settings.rename.clone(),
            suffix: settings.suffix.clone(),
        };
        Ok(())
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_download_file_exists_mode(DownloadFileExistsMode::parse(mode)?)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self
            .inner
            .load_mode
            .lock()
            .map_err(|_| browser_load_mode_lock_poisoned_error())? = mode;
        Ok(())
    }

    pub fn page_download_path(&self, target_id: &str) -> OpenPageResult<Option<String>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(path) = settings
            .get(&frame_id)
            .and_then(|settings| settings.path.as_ref())
        {
            return Ok(Some(path.to_string_lossy().into_owned()));
        }
        drop(settings);
        self.download_path()
    }

    pub fn set_page_download_path(
        &self,
        target_id: &str,
        path: impl AsRef<Path>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let path = path.as_ref().to_path_buf();
        create_download_directory(&path)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        settings.entry(frame_id).or_default().path = Some(path);
        Ok(())
    }

    pub fn page_download_file_exists_mode(&self, target_id: &str) -> OpenPageResult<String> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(mode) = settings
            .get(&frame_id)
            .and_then(|settings| settings.file_exists)
        {
            return Ok(mode.as_str().to_string());
        }
        drop(settings);
        self.download_file_exists_mode()
    }

    pub fn set_page_download_file_exists_mode(
        &self,
        target_id: &str,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        settings.entry(frame_id).or_default().file_exists = Some(mode);
        Ok(())
    }

    pub fn when_page_download_file_exists(
        &self,
        target_id: &str,
        mode: &str,
    ) -> OpenPageResult<()> {
        self.set_page_download_file_exists_mode(target_id, DownloadFileExistsMode::parse(mode)?)
    }

    pub fn set_page_download_filename(
        &self,
        target_id: &str,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let entry = settings.entry(frame_id).or_default();
        entry.rename = rename.map(str::to_string);
        entry.suffix = if suffix_specified {
            Some(suffix.map(str::to_string))
        } else {
            None
        };
        Ok(())
    }

    pub(crate) fn snapshot_page_download_settings(
        &self,
        target_id: &str,
    ) -> OpenPageResult<Option<PageDownloadSettings>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        Ok(settings.get(&frame_id).cloned())
    }

    pub(crate) fn restore_page_download_settings(
        &self,
        target_id: &str,
        settings: Option<PageDownloadSettings>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut current = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(settings) = settings {
            current.insert(frame_id, settings);
        } else {
            current.remove(&frame_id);
        }
        Ok(())
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .mission_ids()?
            .into_iter()
            .map(|guid| DownloadMission::new(self.clone(), guid))
            .collect())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .last_guid()?
            .map(|guid| DownloadMission::new(self.clone(), guid)))
    }

    pub fn clear_finished_downloads(&self) -> OpenPageResult<usize> {
        self.inner.downloads.clear_finished()
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match filename {
            Some(filename) => {
                let info = self.inner.downloads.wait_for_name(filename, timeout_ms)?;
                self.finalize_download(&info, None)
            }
            None => {
                let completed_before = self.inner.downloads.completed_len()?;
                let info = self
                    .inner
                    .downloads
                    .wait_for_next_after(completed_before, timeout_ms)?;
                self.finalize_download(&info, None)
            }
        }
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after(started_before, timeout_ms, cancel_it)
    }

    pub fn wait_for_download_begin_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
            cancel_it,
        )
    }

    pub(crate) fn download_started_len(&self) -> OpenPageResult<usize> {
        self.inner.downloads.started_len()
    }

    pub(crate) fn wait_for_download_begin_after(
        &self,
        started_before: usize,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self
            .inner
            .downloads
            .wait_for_begin_after(started_before, timeout_ms)
        {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub(crate) fn wait_for_download_begin_after_in_frames(
        &self,
        started_before: usize,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self.inner.downloads.wait_for_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
        ) {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self.inner.downloads.wait_until_idle(timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids()? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn wait_for_downloads_done_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self
            .inner
            .downloads
            .wait_until_idle_in_frames(frame_ids, timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids_in_frames(frame_ids)? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        let guid = guid.to_string();
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            CancelDownloadParams::new(guid),
            "Browser::cancel_download()",
        )?;
        Ok(())
    }

    pub fn reconnect(&self) -> OpenPageResult<Self> {
        let mut browser = Browser::connect(&format!("http://{}", self.address()))?;
        browser.set_timeouts(self.timeouts()?)?;
        browser.set_retry(
            Some(self.retry_times()?),
            Some(self.retry_interval_millis()?),
        )?;
        browser.set_load_mode(self.load_mode_value()?)?;
        browser.apply_browser_download_settings(&self.snapshot_browser_download_settings()?)?;

        let page_download_settings = self
            .inner
            .page_download_settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let mission_download_settings = self
            .inner
            .mission_download_settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| mission_download_settings_lock_poisoned_error())?;
        let isolated_contexts = self
            .inner
            .isolated_contexts
            .lock()
            .map(|contexts| contexts.clone())
            .map_err(|_| isolated_context_lock_poisoned_error())?;

        if let Some(state) = Arc::get_mut(&mut browser.inner) {
            state.browser_pid = self.inner.browser_pid;
            state.headless = self.inner.headless;
            state.temp_user_data_dir = self.inner.temp_user_data_dir.clone();
            state.temp_download_dir = self.inner.temp_download_dir.clone();
            *state
                .page_download_settings
                .get_mut()
                .map_err(|_| page_download_settings_lock_poisoned_error())? =
                page_download_settings;
            *state
                .mission_download_settings
                .get_mut()
                .map_err(|_| mission_download_settings_lock_poisoned_error())? =
                mission_download_settings;
            *state
                .isolated_contexts
                .get_mut()
                .map_err(|_| isolated_context_lock_poisoned_error())? = isolated_contexts;
        }

        Ok(browser)
    }

    pub fn close(&self) -> OpenPageResult<()> {
        self.inner.runtime.block_on(async {
            let mut browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::close().lock()").await?;
            run_browser_future_with_cdp_timeout(browser.close(), "Browser::close()").await?;
            run_browser_future_with_cdp_timeout(browser.wait(), "Browser::close().wait()").await?;
            Ok::<(), OpenPageError>(())
        })?;
        if let Ok(mut cache) = self.inner.page_cache.lock() {
            cache.clear();
        }
        if let Ok(mut contexts) = self.inner.isolated_contexts.lock() {
            contexts.clear();
        }
        if let Some(path) = &self.inner.temp_user_data_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        if let Some(path) = &self.inner.temp_download_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        Ok(())
    }

    pub(crate) fn download_info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.inner.downloads.info(guid)
    }

    pub(crate) fn download_folder(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        let settings = self.resolve_download_settings(&info)?;
        let path = settings.path;
        Ok(path.to_string_lossy().into_owned())
    }

    pub(crate) fn download_tab_id(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        self.resolve_frame_target_id(&info.frame_id)
    }

    pub(crate) fn download_tmp_path(&self, guid: &str) -> OpenPageResult<String> {
        Ok(self
            .inner
            .download_spool_dir
            .join(guid)
            .to_string_lossy()
            .into_owned())
    }

    pub(crate) fn wait_for_download_guid(
        &self,
        guid: &str,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<Option<String>> {
        let info = match timeout_ms {
            Some(timeout_ms) => match self.inner.downloads.wait_for_guid(guid, timeout_ms) {
                Ok(info) => info,
                Err(OpenPageError::Timeout(_)) => {
                    if let Ok(info) = self.download_info(guid) {
                        if info.state != DownloadState::Running {
                            return self.finish_waited_download(&info);
                        }
                    }
                    if cancel_if_timeout {
                        let _ = self.cancel_download(guid);
                    }
                    return Ok(None);
                }
                Err(err) => return Err(err),
            },
            None => self.inner.downloads.wait_for_guid_forever(guid)?,
        };
        self.finish_waited_download(&info)
    }

    fn finalize_download(
        &self,
        info: &DownloadInfo,
        filename: Option<&str>,
    ) -> OpenPageResult<String> {
        if info.state == DownloadState::Canceled {
            return Err(OpenPageError::BrowserOperation(download_canceled_message(
                &info.guid,
            )));
        }

        if info.state == DownloadState::Skipped {
            return info.final_path.clone().ok_or_else(|| {
                OpenPageError::BrowserOperation(download_skipped_without_final_path_message(
                    &info.guid,
                ))
            });
        }

        let source_path = download_source_path(info, &self.inner.download_spool_dir)?;
        let settings = self.resolve_download_settings(info)?;
        let download_dir = settings.path;
        let mode = settings.file_exists;
        let rename = settings.rename;
        let suffix = settings.suffix;
        let preferred_name = filename.map(str::to_string).unwrap_or_else(|| {
            resolved_download_name(
                &info.suggested_filename,
                rename.as_deref(),
                suffix.as_ref().map(|value| value.as_deref()),
            )
        });
        let preferred_path = download_dir.join(&preferred_name);
        let (state, final_path) = finalize_download_path(&source_path, &preferred_path, mode)?;

        self.inner
            .downloads
            .set_finalized(&info.guid, state, final_path.clone())?;
        Ok(final_path)
    }

    fn finish_waited_download(&self, info: &DownloadInfo) -> OpenPageResult<Option<String>> {
        match info.state {
            DownloadState::Canceled => Ok(None),
            _ => self.finalize_download(info, None).map(Some),
        }
    }

    fn capture_mission_download_settings(&self, info: &DownloadInfo) -> OpenPageResult<()> {
        let settings = self.resolve_download_settings(info)?;
        let mut mission_settings = self
            .inner
            .mission_download_settings
            .lock()
            .map_err(|_| mission_download_settings_lock_poisoned_error())?;
        mission_settings.insert(info.guid.clone(), settings);
        Ok(())
    }

    fn resolve_page_frame_id(&self, target_id: &str) -> OpenPageResult<String> {
        self.get_page(target_id)?.main_frame_id()
    }

    fn resolve_frame_target_id(&self, frame_id: &str) -> OpenPageResult<String> {
        for target_id in self.tab_ids()? {
            let page = self.get_page(&target_id)?;
            if page
                .download_scope_frame_ids()?
                .iter()
                .any(|current| current == frame_id)
            {
                return Ok(target_id);
            }
        }

        Err(OpenPageError::BrowserOperation(
            download_frame_not_mapped_to_tab_message(frame_id),
        ))
    }

    fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.inner
            .load_mode
            .lock()
            .map(|mode| *mode)
            .map_err(|_| browser_load_mode_lock_poisoned_error())
    }

    fn resolve_download_settings(
        &self,
        info: &DownloadInfo,
    ) -> OpenPageResult<ResolvedDownloadSettings> {
        if let Some(settings) = self
            .inner
            .mission_download_settings
            .lock()
            .map_err(|_| mission_download_settings_lock_poisoned_error())?
            .get(&info.guid)
            .cloned()
        {
            return Ok(settings);
        }

        let page_settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let page_settings = page_settings.get(&info.frame_id);
        let path = if let Some(path) = page_settings.and_then(|settings| settings.path.clone()) {
            path
        } else {
            self.inner
                .download_path
                .lock()
                .map_err(|_| browser_download_path_lock_poisoned_error())?
                .clone()
                .ok_or_else(|| {
                    OpenPageError::UnsupportedOperation(download_path_not_configured_message())
                })?
        };
        let mode = if let Some(mode) = page_settings.and_then(|settings| settings.file_exists) {
            mode
        } else {
            *self
                .inner
                .download_file_exists
                .lock()
                .map_err(|_| browser_download_file_exists_lock_poisoned_error())?
        };
        let rename = page_settings.and_then(|settings| settings.rename.clone());
        let browser_naming = self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())?;
        let rename = rename.or_else(|| browser_naming.rename.clone());
        let suffix = page_settings
            .and_then(|settings| settings.suffix.clone())
            .or_else(|| browser_naming.suffix.clone());
        Ok(ResolvedDownloadSettings {
            path,
            file_exists: mode,
            rename,
            suffix,
        })
    }
}
