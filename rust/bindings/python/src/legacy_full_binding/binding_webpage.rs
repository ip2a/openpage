use super::*;

#[pymethods]
impl PyWebPage {
    fn target_id(&self) -> String {
        self.inner.target_id()
    }

    #[staticmethod]
    #[pyo3(signature = (mode="d", browser_path=None, download_path=None, download_file_exists_mode="rename", load_mode="normal", headless=true, user_data_dir=None, width=1280, height=900, no_sandbox=false, timeout_secs=10, user_agent=None))]
    fn create(
        py: Python<'_>,
        mode: &str,
        browser_path: Option<String>,
        download_path: Option<String>,
        download_file_exists_mode: &str,
        load_mode: &str,
        headless: bool,
        user_data_dir: Option<String>,
        width: u32,
        height: u32,
        no_sandbox: bool,
        timeout_secs: u64,
        user_agent: Option<String>,
    ) -> PyResult<Self> {
        let mode = WebMode::parse(mode).map_err(py_err)?;
        let launch_options = LaunchOptions {
            browser_path: browser_path.map(PathBuf::from),
            download_path: download_path.map(PathBuf::from),
            download_file_exists: DownloadFileExistsMode::parse(download_file_exists_mode)
                .map_err(py_err)?,
            load_mode: LoadMode::parse(load_mode).map_err(py_err)?,
            user_data_dir: user_data_dir.map(PathBuf::from),
            headless,
            width,
            height,
            no_sandbox,
            ..LaunchOptions::default()
        };
        let session_options = SessionOptions {
            timeout_secs,
            user_agent,
            ..SessionOptions::default()
        };
        let inner = py
            .detach(move || WebPage::new(mode, launch_options, session_options))
            .map_err(py_err)?;
        Ok(Self { inner })
    }

    fn mode(&self) -> PyResult<String> {
        self.inner
            .mode()
            .map(|mode| mode.as_str().to_string())
            .map_err(py_err)
    }

    fn tabs_count(&self, py: Python<'_>) -> PyResult<usize> {
        let page = self.inner.clone();
        py.detach(move || page.tabs_count()).map_err(py_err)
    }

    fn tab_ids(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let page = self.inner.clone();
        py.detach(move || page.tab_ids()).map_err(py_err)
    }

    fn download_path(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.download_path()).map_err(py_err)
    }

    fn set_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let path = path.to_string();
        py.detach(move || page.set_download_path(&path))
            .map_err(py_err)?;
        Ok(())
    }

    fn current_tab_download_path(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.current_tab_download_path())
            .map_err(py_err)
    }

    fn set_current_tab_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let path = path.to_string();
        py.detach(move || page.set_current_tab_download_path(&path))
            .map_err(py_err)?;
        Ok(())
    }

    fn set_blocked_urls(&self, py: Python<'_>, patterns: Vec<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_blocked_urls(&patterns))
            .map_err(py_err)?;
        Ok(())
    }

    fn download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.download_file_exists_mode())
            .map_err(py_err)
    }

    fn set_download_file_exists_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode).map_err(py_err)?;
        py.detach(move || page.set_download_file_exists_mode(mode))
            .map_err(py_err)?;
        Ok(())
    }

    fn current_tab_download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.current_tab_download_file_exists_mode())
            .map_err(py_err)
    }

    fn set_current_tab_download_file_exists_mode(
        &self,
        py: Python<'_>,
        mode: &str,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode).map_err(py_err)?;
        py.detach(move || page.set_current_tab_download_file_exists_mode(mode))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (rename=None, suffix=None, suffix_specified=false))]
    fn set_current_tab_download_filename(
        &self,
        py: Python<'_>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let rename = rename.map(str::to_string);
        let suffix = suffix.map(str::to_string);
        py.detach(move || {
            page.set_current_tab_download_filename(
                rename.as_deref(),
                suffix.as_deref(),
                suffix_specified,
            )
        })
        .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (filename=None, timeout_ms=10000))]
    fn wait_for_download(
        &self,
        py: Python<'_>,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<String> {
        let page = self.inner.clone();
        let filename = filename.map(str::to_string);
        py.detach(move || page.wait_for_download(filename.as_deref(), timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, save_path=None, rename=None, suffix=None, suffix_specified=false, timeout_ms=None, by_js=false, new_tab=false))]
    fn click_to_download(
        &self,
        py: Python<'_>,
        locator: &str,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let save_path = save_path.map(str::to_string);
        let rename = rename.map(str::to_string);
        let suffix = suffix.map(str::to_string);
        py.detach(move || {
            page.click_to_download(
                &locator,
                save_path.as_deref(),
                rename.as_deref(),
                suffix.as_deref(),
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            )
        })
        .map_err(py_err)?
        .map(|inner| Py::new(py, PyDownloadMission { inner }))
        .transpose()
    }

    #[pyo3(signature = (locator, files, timeout_ms=None, by_js=false))]
    fn click_to_upload(
        &self,
        py: Python<'_>,
        locator: &str,
        files: Vec<String>,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_to_upload(&locator, &files, timeout_ms, by_js))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=None, by_js=false))]
    fn click_for_new_tab(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<Option<Py<PyWebPage>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_for_new_tab(&locator, timeout_ms, by_js))
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyWebPage { inner }))
            .transpose()
    }

    #[pyo3(signature = (locator, timeout_ms=None, get_tab=true))]
    fn click_middle(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> PyResult<Option<Py<PyWebPage>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_middle(&locator, timeout_ms, get_tab))
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyWebPage { inner }))
            .transpose()
    }

    fn download_missions(&self, py: Python<'_>) -> PyResult<Vec<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.download_missions())
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .collect()
    }

    fn last_download(&self, py: Python<'_>) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.last_download())
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .transpose()
    }

    #[pyo3(signature = (current_tab_id=None, timeout_ms=10000))]
    fn wait_for_new_tab(
        &self,
        py: Python<'_>,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        let current_tab_id = current_tab_id.map(str::to_string);
        py.detach(move || page.wait_for_new_tab(current_tab_id.as_deref(), timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000, cancel_it=false))]
    fn wait_for_download_begin(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_download_begin(timeout_ms, cancel_it))
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .transpose()
    }

    #[pyo3(signature = (timeout_ms=10000, cancel_if_timeout=true))]
    fn wait_for_downloads_done(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_downloads_done(timeout_ms, cancel_if_timeout))
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_upload_paths_inputted(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_upload_paths_inputted(timeout_ms))
            .map_err(py_err)
    }

    fn listener(&self, py: Python<'_>) -> PyResult<Py<PyListener>> {
        let listener = self.inner.listener();
        Py::new(py, PyListener { inner: listener })
    }

    fn console(&self, py: Python<'_>) -> PyResult<Py<PyConsole>> {
        let console = self.inner.console();
        Py::new(py, PyConsole { inner: console })
    }

    fn interceptor(&self, py: Python<'_>) -> PyResult<Py<PyInterceptor>> {
        let interceptor = self.inner.interceptor();
        Py::new(py, PyInterceptor { inner: interceptor })
    }

    fn get(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.get(&url)).map_err(py_err)
    }

    fn post(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post(&url)).map_err(py_err)
    }

    #[pyo3(signature = (url, payload_json=None))]
    fn post_json(&self, py: Python<'_>, url: &str, payload_json: Option<&str>) -> PyResult<bool> {
        let payload = payload_json
            .map(|value| serde_json::from_str(value))
            .transpose()
            .map_err(|err| OpenPageError::Serialization(err.to_string()))
            .map_err(py_err)?;
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post_json(&url, payload))
            .map_err(py_err)
    }

    fn url(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.url()).map_err(py_err)
    }

    fn title(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.title()).map_err(py_err)
    }

    fn user_agent(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.user_agent()).map_err(py_err)
    }

    fn status_code(&self, py: Python<'_>) -> PyResult<Option<u16>> {
        let page = self.inner.clone();
        py.detach(move || page.status_code()).map_err(py_err)
    }

    fn cookies(&self, py: Python<'_>) -> PyResult<Vec<(String, String, Option<String>)>> {
        let page = self.inner.clone();
        py.detach(move || page.cookies())
            .map(cookie_entries_to_tuples)
            .map_err(py_err)
    }

    fn html(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.html()).map_err(py_err)
    }

    fn raw_data(&self, py: Python<'_>) -> PyResult<Py<PyBytes>> {
        let page = self.inner.clone();
        let raw = py.detach(move || page.raw_data()).map_err(py_err)?;
        Ok(PyBytes::new(py, &raw).into())
    }

    fn encoding(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.encoding()).map_err(py_err)
    }

    fn json(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.json())
            .and_then(|value| {
                value
                    .map(|value| serde_json::to_string(&value))
                    .transpose()
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))
            })
            .map_err(py_err)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_alive()).map_err(py_err)
    }

    fn is_loading(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_loading()).map_err(py_err)
    }

    fn ready_state(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.ready_state()).map_err(py_err)
    }

    fn is_headless(&self) -> PyResult<bool> {
        Ok(self.inner.is_headless())
    }

    fn has_alert(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.has_alert()).map_err(py_err)
    }

    fn is_existed(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_existed()).map_err(py_err)
    }

    fn is_incognito(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_incognito()).map_err(py_err)
    }

    #[pyo3(signature = (accept=true, prompt_text=None, timeout_ms=10000))]
    fn handle_alert(
        &self,
        py: Python<'_>,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.handle_alert(accept, prompt_text.as_deref(), timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (accept=true, prompt_text=None))]
    fn set_next_alert_action(
        &self,
        py: Python<'_>,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.set_next_alert_action(accept, prompt_text.as_deref()))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (accept=None, prompt_text=None))]
    fn set_auto_alert_action(
        &self,
        py: Python<'_>,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.set_auto_alert_action(accept, prompt_text.as_deref()))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_alert_closed(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_alert_closed(timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (user_agent, platform=None))]
    fn set_user_agent_override(
        &self,
        py: Python<'_>,
        user_agent: &str,
        platform: Option<&str>,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let user_agent = user_agent.to_string();
        let platform = platform.map(str::to_string);
        py.detach(move || page.set_user_agent(&user_agent, platform.as_deref()))
            .map_err(py_err)?;
        Ok(())
    }

    fn set_headers(&self, py: Python<'_>, headers: HashMap<String, String>) -> PyResult<()> {
        let page = self.inner.clone();
        let headers = headers.into_iter().collect::<Vec<_>>();
        py.detach(move || page.set_headers(&headers))
            .map_err(py_err)?;
        Ok(())
    }

    fn set_session_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.inner.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_session_storage(&item, value.as_deref()))
            .map_err(py_err)?;
        Ok(())
    }

    fn set_local_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.inner.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_local_storage(&item, value.as_deref()))
            .map_err(py_err)?;
        Ok(())
    }

    fn set_upload_files(&self, py: Python<'_>, files: Vec<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_upload_files(&files))
            .map_err(py_err)?;
        Ok(())
    }

    fn load_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.load_mode()).map_err(py_err)
    }

    fn set_load_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = LoadMode::parse(mode).map_err(py_err)?;
        py.detach(move || page.set_load_mode(mode))
            .map_err(py_err)?;
        Ok(())
    }

    fn window_state(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.window_state()).map_err(py_err)
    }

    fn window_size(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.inner.clone();
        py.detach(move || page.window_size()).map_err(py_err)
    }

    fn window_location(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.inner.clone();
        py.detach(move || page.window_location()).map_err(py_err)
    }

    fn window_max(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_max()).map_err(py_err)?;
        Ok(())
    }

    fn window_min(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_min()).map_err(py_err)?;
        Ok(())
    }

    fn window_full(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_full()).map_err(py_err)?;
        Ok(())
    }

    fn window_normal(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_normal()).map_err(py_err)?;
        Ok(())
    }

    fn window_hide(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_hide()).map_err(py_err)?;
        Ok(())
    }

    fn window_show(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_show()).map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (width=None, height=None))]
    fn window_size_set(
        &self,
        py: Python<'_>,
        width: Option<i64>,
        height: Option<i64>,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_size_set(width, height))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (left=None, top=None))]
    fn window_location_set(
        &self,
        py: Python<'_>,
        left: Option<i64>,
        top: Option<i64>,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_location_set(left, top))
            .map_err(py_err)?;
        Ok(())
    }

    fn activate(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.activate()).map_err(py_err)?;
        Ok(())
    }

    fn _browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid()
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyAny>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.find(&locator)).map_err(py_err)?;
        wrap_web_element(py, element)
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.find_all(&locator))
            .map_err(py_err)?
            .into_iter()
            .map(|element| wrap_web_element(py, element))
            .collect()
    }

    fn snapshot_find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py
            .detach(move || page.snapshot_find(&locator))
            .map_err(py_err)?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_find_all(
        &self,
        py: Python<'_>,
        locator: &str,
    ) -> PyResult<Vec<Py<PySessionElement>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.snapshot_find_all(&locator))
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn snapshot_root(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let element = py.detach(move || page.snapshot_root()).map_err(py_err)?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        py.detach(move || page.snapshot_query_xpath(&expression))
            .map_err(py_err)?
            .into_iter()
            .map(|item| session_xpath_result_to_py(py, item))
            .collect()
    }

    #[pyo3(signature = (locators, any_one=false, first_match_only=true))]
    fn find_locators(
        &self,
        py: Python<'_>,
        locators: Vec<String>,
        any_one: bool,
        first_match_only: bool,
    ) -> PyResult<Vec<(String, Vec<Py<PyAny>>)>> {
        let page = self.inner.clone();
        py.detach(move || page.find_locators(&locators, any_one, first_match_only))
            .map_err(py_err)?
            .into_iter()
            .map(|item| locator_match_web_to_py(py, item))
            .collect()
    }

    fn run_js(&self, py: Python<'_>, expression: &str) -> PyResult<String> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        let value = py
            .detach(move || page.run_js(&expression))
            .map_err(py_err)?;
        serde_json::to_string(&value)
            .map_err(|err| py_err(OpenPageError::Serialization(err.to_string())))
    }

    #[pyo3(signature = (text, exclude=false, timeout_ms=10000))]
    fn wait_for_url_change(
        &self,
        py: Python<'_>,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let text = text.to_string();
        py.detach(move || page.wait_for_url_change(&text, exclude, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (text, exclude=false, timeout_ms=10000))]
    fn wait_for_title_change(
        &self,
        py: Python<'_>,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let text = text.to_string();
        py.detach(move || page.wait_for_title_change(&text, exclude, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_load_start(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_load_start(timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_doc_loaded(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_doc_loaded(timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locators, timeout_ms=10000, any_one=false))]
    fn wait_for_elements_loaded(
        &self,
        py: Python<'_>,
        locators: Vec<String>,
        timeout_ms: u64,
        any_one: bool,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_elements_loaded(&locators, any_one, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_displayed(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_displayed(&locator, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_hidden(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_hidden(&locator, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_enabled(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_enabled(&locator, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_deleted(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_deleted(&locator, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_clickable(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_clickable(&locator, timeout_ms))
            .map_err(py_err)
    }

    #[pyo3(signature = (mode=None, go=true, copy_cookies=true))]
    fn change_mode(
        &self,
        py: Python<'_>,
        mode: Option<&str>,
        go: bool,
        copy_cookies: bool,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = mode.map(WebMode::parse).transpose().map_err(py_err)?;
        py.detach(move || page.change_mode(mode, go, copy_cookies))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (copy_user_agent=true))]
    fn cookies_to_session(&self, py: Python<'_>, copy_user_agent: bool) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.cookies_to_session(copy_user_agent))
            .map_err(py_err)?;
        Ok(())
    }

    fn cookies_to_browser(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.cookies_to_browser())
            .map_err(py_err)?;
        Ok(())
    }

    fn quit(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.quit()).map_err(py_err)?;
        Ok(())
    }
}
