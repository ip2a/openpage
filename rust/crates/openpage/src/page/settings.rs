use super::*;

impl PageScroller<'_> {
    pub fn to_top(&self) -> OpenPageResult<()> {
        self.page.scroll_to_top()
    }

    pub fn to_bottom(&self) -> OpenPageResult<()> {
        self.page.scroll_to_bottom()
    }

    pub fn to_half(&self) -> OpenPageResult<()> {
        self.page.scroll_to_half()
    }

    pub fn to_rightmost(&self) -> OpenPageResult<()> {
        self.page.scroll_to_rightmost()
    }

    pub fn to_leftmost(&self) -> OpenPageResult<()> {
        self.page.scroll_to_leftmost()
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.page.scroll_to_location(x, y)
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_up(pixels)
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_down(pixels)
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_left(pixels)
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_right(pixels)
    }
}

impl PageSetter<'_> {
    pub fn window(&self) -> PageWindowSetter<'_> {
        PageWindowSetter { page: self.page }
    }

    pub fn cookie(&self) -> PageCookieSetter<'_> {
        PageCookieSetter { page: self.page }
    }

    pub fn load_mode(&self) -> PageLoadModeSetter<'_> {
        PageLoadModeSetter { page: self.page }
    }

    pub fn blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.page.set_blocked_urls(patterns)
    }

    pub fn headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        self.page.set_headers(headers)
    }

    pub fn user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        self.page.set_user_agent(user_agent, platform)
    }

    pub fn session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.page.set_session_storage(item, value)
    }

    pub fn local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.page.set_local_storage(item, value)
    }

    pub fn auto_handle_alert(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.set_auto_alert_action(accept, prompt_text)
    }

    pub fn cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }

    pub fn download_path(&self, path: &str) -> OpenPageResult<()> {
        self.page.set_download_path(path)
    }

    pub fn download_file_exists(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()> {
        self.page.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.page.when_download_file_exists(mode)
    }

    pub fn download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page
            .set_download_file_name(rename, suffix, suffix.is_some())
    }

    pub fn upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_files(files)
    }

    pub fn upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_paths(files)
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        self.page.activate()
    }

    pub fn retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        self.page.set_retry(retry_times, retry_interval_secs)
    }

    pub fn retry_times(&self, times: usize) -> OpenPageResult<()> {
        self.page.set_retry(Some(times), None)
    }

    pub fn retry_interval(&self, interval_secs: f64) -> OpenPageResult<()> {
        self.page.set_retry(None, Some(interval_secs))
    }

    pub fn timeout(&self, timeout_secs: f64) -> OpenPageResult<()> {
        self.page.set_timeouts(Some(timeout_secs), None, None)
    }

    pub fn timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        self.page
            .set_timeouts(base_secs, page_load_secs, script_secs)
    }
}

impl PageCookieSetter<'_> {
    pub fn set<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn remove(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }
}

impl PageWindowSetter<'_> {
    pub fn max(&self) -> OpenPageResult<()> {
        self.page.window_max()
    }

    pub fn mini(&self) -> OpenPageResult<()> {
        self.page.window_min()
    }

    pub fn full(&self) -> OpenPageResult<()> {
        self.page.window_full()
    }

    pub fn normal(&self) -> OpenPageResult<()> {
        self.page.window_normal()
    }

    pub fn size(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        self.page.window_size_set(width, height)
    }

    pub fn location(&self, x: Option<i64>, y: Option<i64>) -> OpenPageResult<()> {
        self.page.window_location_set(x, y)
    }

    pub fn hide(&self) -> OpenPageResult<()> {
        self.page.window_hide()
    }

    pub fn show(&self) -> OpenPageResult<()> {
        self.page.window_show()
    }
}

impl PageLoadModeSetter<'_> {
    pub fn normal(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::Normal)
    }

    pub fn eager(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::Eager)
    }

    pub fn none(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::None)
    }
}
