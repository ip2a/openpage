use super::*;

impl WebPage {
    pub fn mode(&self) -> OpenPageResult<WebMode> {
        self.mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "webpage mode",
                "网页模式",
            ))
        })
    }

    pub fn navigation_snapshot(&self) -> OpenPageResult<crate::page::PageNavigationSnapshot> {
        match self.mode()? {
            WebMode::Driver => self.driver.navigation_snapshot(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("navigation_snapshot()"),
            )),
        }
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.driver.set_none_element_value(value, on_off)?;
        self.session.set_none_element_value(value, on_off)
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.driver.set_raise_when_ele_not_found(on_off)?;
        self.session.set_raise_when_ele_not_found(on_off)
    }

    pub fn actions(&self) -> OpenPageResult<Actions> {
        match self.mode()? {
            WebMode::Driver => self.driver.actions(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("actions()"),
            )),
        }
    }

    pub fn new_actions(&self) -> OpenPageResult<Actions> {
        match self.mode()? {
            WebMode::Driver => Ok(self.driver.new_actions()),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("new_actions()"),
            )),
        }
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        self.browser.tabs_count()
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        self.browser.tab_ids()
    }

    pub fn target_id(&self) -> String {
        self.driver.target_id()
    }

    pub fn tab_infos(&self) -> OpenPageResult<Vec<TabInfo>> {
        self.browser.tab_infos()
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
        self.browser
            .get_tabs(title, url, tab_type, as_id)
            .map(|references| {
                references
                    .into_iter()
                    .map(|reference| self.mix_tab_reference(reference))
                    .collect()
            })
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser
            .latest_tab()
            .map(|reference| reference.map(|reference| self.mix_tab_reference(reference)))
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> OpenPageResult<crate::page::Page> {
        self.browser
            .new_tab(url, new_window, background, new_context)
    }

    pub fn activate_tab<'a, T>(&self, target: T) -> OpenPageResult<()>
    where
        T: Into<BrowserTabSelector<'a>>,
    {
        self.browser.activate_tab(target)
    }

    pub fn close_tabs<'a, T>(&self, targets: T, others: bool) -> OpenPageResult<usize>
    where
        T: Into<BrowserTabTargetsInput<'a>>,
    {
        self.browser.close_tabs(targets, others)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.browser.set_download_path(path)?;
        self.session.set_download_path(path)
    }

    pub fn current_tab_download_path(&self) -> OpenPageResult<Option<String>> {
        self.browser.page_download_path(&self.driver.target_id())
    }

    pub fn set_current_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.browser
            .set_page_download_path(&self.driver.target_id(), path)
    }

    pub fn set_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.set_current_tab_download_path(path)
    }

    pub fn set_blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.driver.set_blocked_urls(patterns)
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.browser.download_file_exists_mode()
    }

    pub fn current_tab_download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.browser
            .page_download_file_exists_mode(&self.driver.target_id())
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.browser.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.browser.when_download_file_exists(mode)
    }

    pub fn set_current_tab_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.browser
            .set_page_download_file_exists_mode(&self.driver.target_id(), mode)
    }

    pub fn set_tab_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_exists_mode(mode)
    }

    pub fn when_current_tab_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.browser
            .when_page_download_file_exists(&self.driver.target_id(), mode)
    }

    pub fn set_tab_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.when_current_tab_download_file_exists(mode)
    }

    pub fn set_current_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.browser.set_page_download_filename(
            &self.driver.target_id(),
            rename,
            suffix,
            suffix_specified,
        )
    }

    pub fn set_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_current_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_name(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_name(rename, suffix, suffix_specified)
    }

    pub fn click_to_download<'a, L>(
        &self,
        locator: L,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.click_to_download(
                locator,
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            ),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_to_download()"),
            )),
        }
    }

    pub fn click_to_upload<'a, 'b, L, F>(
        &self,
        locator: L,
        files: F,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorInput<'a>>,
        F: Into<UploadFilesInput<'b>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_to_upload(locator, files, timeout_ms, by_js),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_to_upload()"),
            )),
        }
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<WebPage>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_for_new_tab(locator, timeout_ms, by_js)
                .map(|page| page.map(|page| self.with_driver_page(page))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_for_new_tab()"),
            )),
        }
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<WebPage>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_middle(locator, timeout_ms, get_tab)
                .map(|page| page.map(|page| self.with_driver_page(page))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_middle()"),
            )),
        }
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.driver.set_upload_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        match self.mode()? {
            WebMode::Driver => self.driver.retry_times(),
            WebMode::Session => self.session.retry_times(),
        }
    }

    pub fn retry_interval(&self) -> OpenPageResult<f64> {
        match self.mode()? {
            WebMode::Driver => self.driver.retry_interval(),
            WebMode::Session => self
                .session
                .retry_interval_millis()
                .map(|millis| millis as f64 / 1000.0),
        }
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_retry(retry_times, retry_interval_secs),
            WebMode::Session => self.session.set_retry(
                retry_times,
                retry_interval_secs
                    .map(webpage_timeout_seconds_to_millis)
                    .transpose()?,
            ),
        }
    }

    pub fn timeouts(&self) -> OpenPageResult<HashMap<&'static str, f64>> {
        match self.mode()? {
            WebMode::Driver => self.driver.timeouts(),
            WebMode::Session => Ok(HashMap::from([(
                "base",
                self.session.timeout_secs()? as f64,
            )])),
        }
    }

    pub(super) fn implicit_wait_timeout_ms(&self) -> OpenPageResult<u64> {
        Ok(self
            .timeouts()?
            .get("base")
            .map(|seconds| (seconds * 1000.0).round().max(0.0) as u64)
            .unwrap_or(10_000))
    }

    pub fn set_timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .set_timeouts(base_secs, page_load_secs, script_secs),
            WebMode::Session => {
                if page_load_secs.is_some() || script_secs.is_some() {
                    return Err(OpenPageError::UnsupportedOperation(
                        driver_mode_only_message("set_timeouts(page_load/script)"),
                    ));
                }
                if let Some(base_secs) = base_secs {
                    if !base_secs.is_finite() || base_secs.is_sign_negative() {
                        return Err(OpenPageError::UnsupportedOperation(
                            web_timeout_base_non_negative_message(base_secs),
                        ));
                    }
                    self.session.set_timeout(base_secs.round() as u64)?;
                }
                Ok(())
            }
        }
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        self.driver.load_mode()
    }

    pub fn set_load_mode(&self, mode: crate::browser::LoadMode) -> OpenPageResult<()> {
        self.driver.set_load_mode(mode)
    }

    pub fn window_state(&self) -> OpenPageResult<String> {
        self.driver.window_state()
    }

    pub fn window_id(&self) -> OpenPageResult<i64> {
        self.driver.window_id()
    }

    pub fn window_size(&self) -> OpenPageResult<(i64, i64)> {
        self.driver.window_size()
    }

    pub fn window_location(&self) -> OpenPageResult<(i64, i64)> {
        self.driver.window_location()
    }

    pub fn window_max(&self) -> OpenPageResult<()> {
        self.driver.window_max()
    }

    pub fn window_min(&self) -> OpenPageResult<()> {
        self.driver.window_min()
    }

    pub fn window_full(&self) -> OpenPageResult<()> {
        self.driver.window_full()
    }

    pub fn window_normal(&self) -> OpenPageResult<()> {
        self.driver.window_normal()
    }

    pub fn window_hide(&self) -> OpenPageResult<()> {
        self.driver.window_hide()
    }

    pub fn window_show(&self) -> OpenPageResult<()> {
        self.driver.window_show()
    }

    pub fn window_size_set(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        self.driver.window_size_set(width, height)
    }

    pub fn window_location_set(&self, left: Option<i64>, top: Option<i64>) -> OpenPageResult<()> {
        self.driver.window_location_set(left, top)
    }

    pub fn zoom_factor(&self) -> OpenPageResult<f64> {
        self.driver.zoom_factor()
    }

    pub fn set_zoom_factor(&self, factor: f64) -> OpenPageResult<()> {
        self.driver.set_zoom_factor(factor)
    }

    pub fn reset_zoom_factor(&self) -> OpenPageResult<()> {
        self.driver.reset_zoom_factor()
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        self.browser.wait_for_download(filename, timeout_ms)
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        self.browser.download_missions()
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        self.browser.last_download()
    }

    pub fn clear_finished_downloads(&self) -> OpenPageResult<usize> {
        self.browser.clear_finished_downloads()
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        self.browser.cancel_download(guid)
    }

    pub fn last_session_download(&self) -> OpenPageResult<Option<SessionDownload>> {
        self.session.last_download()
    }

    pub fn listener(&self) -> Listener {
        self.driver.listener()
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn interceptor(&self) -> Interceptor {
        self.driver.interceptor()
    }

    pub fn intercept(&self) -> Interceptor {
        self.interceptor()
    }

    pub fn console(&self) -> Console {
        self.driver.console()
    }

    pub fn screencast(&self) -> Screencast {
        self.driver.screencast()
    }

    pub fn recorder(&self) -> crate::recorder::Recorder {
        self.driver.recorder()
    }

    pub fn change_mode(
        &self,
        mode: Option<WebMode>,
        go: bool,
        copy_cookies: bool,
    ) -> OpenPageResult<()> {
        let current = self.mode()?;
        let target = mode.unwrap_or_else(|| current.toggled());
        if target == current {
            return Ok(());
        }

        match target {
            WebMode::Session => {
                if copy_cookies {
                    self.cookies_to_session(true)?;
                }
                if go {
                    let url = self.driver.url()?;
                    if !url.is_empty() {
                        self.session.get(&url)?;
                    }
                }
            }
            WebMode::Driver => {
                if copy_cookies {
                    self.cookies_to_browser()?;
                }
                if go {
                    if let Some(url) = self.session.url()? {
                        self.driver.goto(&url)?;
                    }
                }
            }
        }

        self.set_mode(target)
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => {
                self.driver.goto(url)?;
                Ok(true)
            }
            WebMode::Session => self.session.get(url),
        }
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.download(url)
    }

    pub fn browser(&self) -> Option<&Browser> {
        Some(&self.browser)
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.driver.browser_pid()
    }

    pub fn process_id(&self) -> Option<u32> {
        self.driver.process_id()
    }

    pub fn browser_version(&self) -> OpenPageResult<String> {
        self.driver.browser_version()
    }

    pub fn address(&self) -> OpenPageResult<String> {
        self.driver.address()
    }

    pub fn evaluate(&self, expression: &str) -> OpenPageResult<Value> {
        match self.mode()? {
            WebMode::Driver => self.driver.evaluate(expression),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("evaluate()"),
            )),
        }
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_position()"),
            ));
        }
        self.driver.scroll_position()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("viewport_size()"),
            ));
        }
        self.driver.viewport_size()
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("refresh()"),
            ));
        }
        self.driver.refresh(ignore_cache)
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("back()"),
            ));
        }
        self.driver.back(steps)
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("forward()"),
            ));
        }
        self.driver.forward(steps)
    }

    pub fn scroll(&self) -> WebPageScroller<'_> {
        WebPageScroller { page: self }
    }

    pub fn set(&self) -> WebPageSetter<'_> {
        WebPageSetter { page: self }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_top()"),
            ));
        }
        self.driver.scroll_to_top()
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_bottom()"),
            ));
        }
        self.driver.scroll_to_bottom()
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_half()"),
            ));
        }
        self.driver.scroll_to_half()
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_rightmost()"),
            ));
        }
        self.driver.scroll_to_rightmost()
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_leftmost()"),
            ));
        }
        self.driver.scroll_to_leftmost()
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_location()"),
            ));
        }
        self.driver.scroll_to_location(x, y)
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_up()"),
            ));
        }
        self.driver.scroll_up(pixels)
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_down()"),
            ));
        }
        self.driver.scroll_down(pixels)
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_left()"),
            ));
        }
        self.driver.scroll_left(pixels)
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_right()"),
            ));
        }
        self.driver.scroll_right(pixels)
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.is_alive(),
            WebMode::Session => Ok(true),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.is_loading(),
            WebMode::Session => Ok(false),
        }
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => Ok(Some(self.driver.ready_state()?)),
            WebMode::Session => Ok(None),
        }
    }

    pub fn is_headless(&self) -> bool {
        self.browser.is_headless()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.driver.has_alert()
    }

    pub fn is_existed(&self) -> OpenPageResult<bool> {
        self.browser.is_existed()
    }

    pub fn is_incognito(&self) -> OpenPageResult<bool> {
        self.browser.is_incognito()
    }

    pub fn wait_for_new_tab(
        &self,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.browser.wait_for_new_tab(current_tab_id, timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.driver.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.driver
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_upload_paths_inputted(timeout_ms),
            WebMode::Session => Ok(false),
        }
    }

    pub fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.driver.handle_alert(accept, prompt_text, timeout_ms)
    }

    pub fn alert_text(&self) -> OpenPageResult<Option<String>> {
        self.driver.alert_text()
    }

    pub fn set_next_alert_action(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.driver.set_next_alert_action(accept, prompt_text)
    }

    pub fn wait_for_alert_closed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.driver.wait_for_alert_closed(timeout_ms)
    }

    pub fn wait_for_url_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.url()?;
            Ok(value.as_ref().is_some_and(|value| {
                if exclude {
                    !value.contains(text)
                } else {
                    value.contains(text)
                }
            }))
        })
    }

    pub fn wait_for_title_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.title()?;
            Ok(value.as_ref().is_some_and(|value| {
                if exclude {
                    !value.contains(text)
                } else {
                    value.contains(text)
                }
            }))
        })
    }

    pub fn wait_for_load_start(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_load_start(timeout_ms),
            WebMode::Session => Ok(false),
        }
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_doc_loaded(timeout_ms),
            WebMode::Session => Ok(true),
        }
    }

    pub fn wait_for_elements_loaded<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .wait_for_elements_loaded(locators, any_one, timeout_ms),
            WebMode::Session => {
                let locators = parse_locator_batch_input(locators)?;
                let timeout = Duration::from_millis(timeout_ms.max(1));
                let deadline = Instant::now() + timeout;
                loop {
                    let mut matched = 0usize;
                    for locator in &locators {
                        if !self.session.find_all(locator)?.is_empty() {
                            matched += 1;
                        }
                    }
                    if (!any_one && matched == locators.len()) || (any_one && matched > 0) {
                        return Ok(true);
                    }
                    if Instant::now() >= deadline {
                        return wait_timeout_result(
                            "WebPage::wait_for_elements_loaded()",
                            timeout_ms,
                        );
                    }
                    sleep(Duration::from_millis(50));
                }
            }
        }
    }

    pub fn wait_for_ele_displayed<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_displayed(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    pub fn wait_for_ele_hidden<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_hidden(target, remaining),
            |page, locator, remaining| {
                page.session_wait_until(remaining, || Ok(page.session.find(locator).is_err()))
            },
        )
    }

    pub fn wait_for_ele_enabled<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_enabled(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    pub fn wait_for_ele_deleted<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_deleted(target, remaining),
            |page, locator, remaining| {
                if page.session.find(locator).is_err() {
                    return Ok(locator.starts_with("xpath:"));
                }
                page.session_wait_until(remaining, || Ok(page.session.find(locator).is_err()))
            },
        )
    }

    pub fn wait_for_ele_clickable<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_clickable(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    fn wait_for_page_element_target<'a, L, D, S>(
        &self,
        target: L,
        timeout_ms: u64,
        driver_wait: D,
        session_wait: S,
    ) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
        D: FnOnce(&Page, PageElementTarget<'a>, u64) -> OpenPageResult<bool>,
        S: FnOnce(&Self, &str, u64) -> OpenPageResult<bool>,
    {
        let target = target.into();
        match self.mode()? {
            WebMode::Driver => driver_wait(&self.driver, target, timeout_ms),
            WebMode::Session => {
                let locator = self.session_wait_target_locator(target)?;
                session_wait(self, locator.as_str(), timeout_ms)
            }
        }
    }

    fn session_wait_target_locator<'a>(
        &self,
        target: PageElementTarget<'a>,
    ) -> OpenPageResult<String> {
        match target {
            PageElementTarget::Locator(locator) => {
                Ok(Locator::from_input(locator)?.raw().to_string())
            }
            PageElementTarget::DocumentElement(element) => {
                Ok(format!("xpath:{}", element.xpath()?))
            }
            PageElementTarget::OwnedDocumentElement(element) => {
                Ok(format!("xpath:{}", element.xpath()?))
            }
            PageElementTarget::WebElement(element) => match element {
                WebElement::Session(element) => Ok(format!("xpath:{}", element.xpath()?)),
                WebElement::Browser(_) | WebElement::Mix { .. } => Err(OpenPageError::UnsupportedOperation(
                    "browser-backed element object is not supported for session mode wait_for_ele_*()"
                        .to_string(),
                )),
            },
            PageElementTarget::OwnedWebElement(element) => match element {
                WebElement::Session(element) => Ok(format!("xpath:{}", element.xpath()?)),
                WebElement::Browser(_) | WebElement::Mix { .. } => Err(OpenPageError::UnsupportedOperation(
                    "browser-backed element object is not supported for session mode wait_for_ele_*()"
                        .to_string(),
                )),
            },
            PageElementTarget::Element(_) => Err(OpenPageError::UnsupportedOperation(
                "browser-backed element object is not supported for session mode wait_for_ele_*()"
                    .to_string(),
            )),
            PageElementTarget::OwnedElement(_) => Err(OpenPageError::UnsupportedOperation(
                "browser-backed element object is not supported for session mode wait_for_ele_*()"
                    .to_string(),
            )),
        }
    }

    fn session_wait_for_element<F>(
        &self,
        locator: &str,
        timeout_ms: u64,
        check: F,
    ) -> OpenPageResult<bool>
    where
        F: Fn(&DocumentElement) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.session.find(locator) {
                Ok(ele) => return check(&ele),
                Err(_) => {
                    sleep(Duration::from_millis(50));
                    if Instant::now() >= deadline {
                        return wait_timeout_result(
                            "WebPage::session_wait_for_element()",
                            timeout_ms,
                        );
                    }
                }
            }
        }
    }

    fn session_wait_until<F>(&self, timeout_ms: u64, check: F) -> OpenPageResult<bool>
    where
        F: Fn() -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if check()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("WebPage::session_wait_until()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for<'a, L>(&self, locator: L, timeout_ms: u64) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.find(locator.raw()) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            locator.raw(),
                            &err.to_string(),
                        )));
                    }
                    sleep(Duration::from_millis(100));
                }
            }
        }
    }

    pub fn click<'a, L>(&self, locator: L) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .click()
    }

    pub fn fill<'a, L>(&self, locator: L, text: &str) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .input(text)
    }

    pub fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .active_element()
                .map(|element| element.map(|element| self.with_driver_element(element))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("active_element()"),
            )),
        }
    }

    pub fn remove_element<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.remove_element(locator),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("remove_element()"),
            )),
        }
    }

    pub fn remove_ele<'a, L>(&self, target: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.remove_element(target)
    }

    pub fn add_element_html<'a, 'b, I, B>(
        &self,
        html: &str,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element_html(html, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element_html()"),
            )),
        }
    }

    pub fn add_element<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element(content, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element()"),
            )),
        }
    }

    pub fn add_ele<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        self.add_element(content, insert_to, before)
    }

    pub fn add_element_info<'a, 'b, I, B, H>(
        &self,
        info: H,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
        H: Into<PageElementInfo>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element_info(info, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element_info()"),
            )),
        }
    }

    pub fn main_frame_id(&self) -> OpenPageResult<String> {
        match self.mode()? {
            WebMode::Driver => self.driver.main_frame_id(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("main_frame_id()"),
            )),
        }
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame(target)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame()"),
            )),
        }
    }

    pub fn get_frame_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_with_timeout(target, timeout_ms)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_by_index(index)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index()"),
            )),
        }
    }

    pub fn get_frame_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_by_index_with_timeout(index, timeout_ms)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele(target)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele()"),
            )),
        }
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_with_timeout(target, timeout_ms)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_ele_by_index<I>(&self, index: I) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_by_index(index)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_by_index()"),
            )),
        }
    }

    pub fn get_frame_ele_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_by_index_with_timeout(index, timeout_ms)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_by_index_with_timeout()"),
            )),
        }
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frames(locator).map(|frames| {
                frames
                    .into_iter()
                    .map(|frame| self.with_driver_frame(frame))
                    .collect()
            }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frames()"),
            )),
        }
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frames_with_timeout(locator, timeout_ms)
                .map(|frames| {
                    frames
                        .into_iter()
                        .map(|frame| self.with_driver_frame(frame))
                        .collect()
                }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frames_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frame_eles(locator).map(|elements| {
                elements
                    .into_iter()
                    .map(|element| self.with_driver_element(element))
                    .collect()
            }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_eles()"),
            )),
        }
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_eles_with_timeout(locator, timeout_ms)
                .map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.with_driver_element(element))
                        .collect()
                }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_eles_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frame(target),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_context()"),
            )),
        }
    }

    pub fn get_frame_context_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frame_by_index(index),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_context_by_index()"),
            )),
        }
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frames(locator),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_contexts()"),
            )),
        }
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js()"),
            ));
        }
        self.driver.run_js(expression)
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_args()"),
            ));
        }
        self.driver.run_js_with_args(script, args, as_expr)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_options()"),
            ));
        }
        self.driver
            .run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded()"),
            ));
        }
        self.driver.run_js_loaded(script)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded_with_args()"),
            ));
        }
        self.driver.run_js_loaded_with_args(script, args, as_expr)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded_with_options()"),
            ));
        }
        self.driver
            .run_js_loaded_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js()"),
            ));
        }
        self.driver.run_async_js(script)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_args()"),
            ));
        }
        self.driver.run_async_js_with_args(script, args, as_expr)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_options()"),
            ));
        }
        self.driver
            .run_async_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("stop_loading()"),
            ));
        }
        self.driver.stop_loading()
    }

    pub fn execute_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("execute_cdp()"),
            ));
        }
        self.driver.execute_cdp(command)
    }

    pub fn run_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_cdp()"),
            ));
        }
        self.driver.run_cdp(command)
    }

    pub fn execute_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("execute_cdp_loaded()"),
            ));
        }
        self.driver.execute_cdp_loaded(command)
    }

    pub fn run_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_cdp_loaded()"),
            ));
        }
        self.driver.run_cdp_loaded(command)
    }

    pub fn set_user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_user_agent(user_agent, platform),
            WebMode::Session => self.session.set_user_agent(Some(user_agent.to_string())),
        }
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        self.driver.activate()
    }

    pub fn set_headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.set_headers(headers),
            WebMode::Session => self.session.set_headers(headers),
        }
    }

    pub fn set_session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.driver.set_session_storage(item, value)
    }

    pub fn session_storage(&self, item: Option<&str>) -> OpenPageResult<Option<Value>> {
        if self.mode()? != WebMode::Driver {
            return Ok(None);
        }
        self.driver.session_storage(item).map(Some)
    }

    pub fn set_local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.driver.set_local_storage(item, value)
    }

    pub fn local_storage(&self, item: Option<&str>) -> OpenPageResult<Option<Value>> {
        if self.mode()? != WebMode::Driver {
            return Ok(None);
        }
        self.driver.local_storage(item).map(Some)
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_init_js()"),
            ));
        }
        self.driver.add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("remove_init_js()"),
            ));
        }
        self.driver.remove_init_js(script_id)
    }

    pub fn clear_cache(
        &self,
        session_storage: bool,
        local_storage: bool,
        cache: bool,
        cookies: bool,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear_cache()"),
            ));
        }
        self.driver
            .clear_cache(session_storage, local_storage, cache, cookies)
    }

    pub fn set_permission(
        &self,
        name: &str,
        setting: &str,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
    ) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_permission()"),
            ));
        }
        self.driver
            .set_permission(name, setting, origin, embedded_origin)
    }

    pub fn reset_permissions(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("reset_permissions()"),
            ));
        }
        self.driver.reset_permissions()
    }

    pub fn clipboard_read_text(&self) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clipboard_read_text()"),
            ));
        }
        self.driver.clipboard_read_text()
    }

    pub fn clipboard_write_text(&self, text: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clipboard_write_text()"),
            ));
        }
        self.driver.clipboard_write_text(text)
    }

    pub fn close(&self, others: bool, session: bool) -> OpenPageResult<()> {
        if others {
            self.browser.close_tabs(&self.driver, true)?;
            let target_id = self.driver.target_id();
            let deadline = Instant::now() + Duration::from_millis(1_000);
            loop {
                let tab_ids = self.tab_ids()?;
                if tab_ids.len() == 1 && tab_ids[0] == target_id {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(crate::settings::timeout_error("WebPage::close()", 1_000));
                }
                sleep(Duration::from_millis(20));
            }
        }

        self.browser.close_tabs(&self.driver, false)?;
        if session {
            self.session.close()?;
        }
        Ok(())
    }

    pub fn close_with_options(&self, others: bool, session: bool) -> OpenPageResult<()> {
        self.close(others, session)
    }

    pub fn close_driver(self) -> OpenPageResult<Session> {
        self.change_mode(Some(WebMode::Session), true, true)?;
        let WebPage {
            driver, session, ..
        } = self;
        let _ = driver
            .execute_cdp(chromiumoxide::cdp::browser_protocol::browser::CloseParams::default());
        Ok(session)
    }

    pub fn close_session(self) -> OpenPageResult<Page> {
        self.change_mode(Some(WebMode::Driver), true, true)?;
        let WebPage {
            driver, session, ..
        } = self;
        session.close()?;
        Ok(driver)
    }

    pub fn quit(&self) -> OpenPageResult<()> {
        self.browser.close()
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        let driver = self.driver.reconnect(wait_ms)?;
        let browser = driver.browser().cloned().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(driver_mode_only_message("reconnect()"))
        })?;
        Ok(Self {
            browser,
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        })
    }

    pub fn with_target(&self, target_id: &str) -> OpenPageResult<Self> {
        let driver = self.browser.get_page(target_id)?;
        Ok(Self {
            browser: self.browser.clone(),
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        })
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedWebPage> {
        let target_id = self.driver.target_id();
        Ok(DisconnectedWebPage {
            browser: self.browser,
            session: self.session,
            mode: self.mode,
            target_id,
        })
    }

    pub fn set_auto_alert_action(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.driver.set_auto_alert_action(accept, prompt_text)
    }

    fn wait_for_change<F>(&self, timeout_ms: u64, mut predicate: F) -> OpenPageResult<bool>
    where
        F: FnMut(&Self) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if predicate(self)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("WebPage::wait_for_change()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn set_mode(&self, mode: WebMode) -> OpenPageResult<()> {
        let mut current = self.mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "webpage mode",
                "网页模式",
            ))
        })?;
        *current = mode;
        Ok(())
    }
}
