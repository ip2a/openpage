use super::*;

impl Page {
    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        self.browser_backed_ref("tabs_count")?.tabs_count()
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        self.browser_backed_ref("tab_ids")?.tab_ids()
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
        self.browser_backed_ref("get_tab")?
            .get_tab(id_or_num, title, url, tab_type, as_id)
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
        self.browser_backed_ref("get_tabs")?
            .get_tabs(title, url, tab_type, as_id)
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser_backed_ref("latest_tab")?.latest_tab()
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> OpenPageResult<Page> {
        self.browser_backed_ref("new_tab")?
            .new_tab(url, new_window, background, new_context)
    }

    pub fn activate_tab<'a, T>(&self, target: T) -> OpenPageResult<()>
    where
        T: Into<BrowserTabSelector<'a>>,
    {
        self.browser_backed_ref("activate_tab")?
            .activate_tab(target)
    }

    pub fn close_tabs<'a, T>(&self, targets: T, others: bool) -> OpenPageResult<usize>
    where
        T: Into<BrowserTabTargetsInput<'a>>,
    {
        self.browser_backed_ref("close_tabs")?
            .close_tabs(targets, others)
    }

    pub fn close_with_options(&self, others: bool, _session: bool) -> OpenPageResult<()> {
        if others {
            self.close_tabs(self, true)?;
        } else {
            self.close_tabs(self, false)?;
        }
        Ok(())
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        #[cfg(target_os = "macos")]
        if let Some(browser_pid) = self.browser_pid {
            set_app_visibility(browser_pid, true)?;
        }
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.bring_to_front(), "bring to front").await?;
            Ok::<(), OpenPageError>(())
        })?;
        #[cfg(target_os = "macos")]
        if let Some(browser_pid) = self.browser_pid {
            activate_app(browser_pid)?;
        }
        Ok(())
    }

    pub fn window_id(&self) -> OpenPageResult<i64> {
        Ok(*self.window_info()?.window_id.inner())
    }
}
