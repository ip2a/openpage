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
                owns_browser: true,
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
                owns_browser: false,
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
}
