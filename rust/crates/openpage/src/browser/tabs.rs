use super::*;

impl Browser {
    pub(super) fn tracked_newest_tab_id(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .newest_tab_id
            .lock()
            .map(|target_id| target_id.clone())
            .map_err(|_| browser_newest_tab_lock_poisoned_error())
    }

    pub(super) fn set_tracked_newest_tab_id(
        &self,
        target_id: Option<String>,
    ) -> OpenPageResult<()> {
        *self
            .inner
            .newest_tab_id
            .lock()
            .map_err(|_| browser_newest_tab_lock_poisoned_error())? = target_id;
        Ok(())
    }

    pub(super) fn seed_newest_tab_id_from_tab_infos(&self) -> OpenPageResult<()> {
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

    pub(super) fn realize_page(
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

    pub(super) fn prune_cached_pages(&self, target_ids: &[String]) -> OpenPageResult<()> {
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

    pub(super) fn record_isolated_context(
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

    pub(super) fn clear_isolated_context(&self, target_id: &str) {
        if let Ok(mut contexts) = self.inner.isolated_contexts.lock() {
            contexts.remove(target_id);
        }
    }

    pub(super) fn dispose_browser_context(&self, browser_context_id: &str) -> OpenPageResult<()> {
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

    pub(super) fn browser_tab_reference(
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
}
