use super::*;

#[pymethods]
impl PyBrowser {
    #[staticmethod]
    #[pyo3(signature = (browser_path=None, download_path=None, download_file_exists_mode="rename", load_mode="normal", headless=true, user_data_dir=None, width=1280, height=900, no_sandbox=false))]
    fn launch(
        py: Python<'_>,
        browser_path: Option<String>,
        download_path: Option<String>,
        download_file_exists_mode: &str,
        load_mode: &str,
        headless: bool,
        user_data_dir: Option<String>,
        width: u32,
        height: u32,
        no_sandbox: bool,
    ) -> PyResult<Self> {
        let options = LaunchOptions {
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
        let inner = py
            .detach(move || Browser::launch(options))
            .map_err(py_err)?;
        Ok(Self { inner })
    }

    #[pyo3(signature = (url=None))]
    fn new_page(&self, py: Python<'_>, url: Option<&str>) -> PyResult<Py<PyPage>> {
        let browser = self.inner.clone();
        let url = url.map(str::to_string);
        let page = py
            .detach(move || browser.new_page(url.as_deref()))
            .map_err(py_err)?;
        Py::new(py, PyPage { inner: Some(page) })
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let browser = self.inner.clone();
        py.detach(move || browser.close()).map_err(py_err)?;
        Ok(())
    }

    fn version(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.version()).map_err(py_err)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_alive()).map_err(py_err)
    }

    fn is_headless(&self) -> PyResult<bool> {
        Ok(self.inner.is_headless())
    }

    fn is_existed(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_existed()).map_err(py_err)
    }

    fn is_incognito(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_incognito()).map_err(py_err)
    }

    #[pyo3(signature = (current_tab_id=None, timeout_ms=10000))]
    fn wait_for_new_tab(
        &self,
        py: Python<'_>,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Option<String>> {
        let browser = self.inner.clone();
        let current_tab_id = current_tab_id.map(str::to_string);
        py.detach(move || browser.wait_for_new_tab(current_tab_id.as_deref(), timeout_ms))
            .map_err(py_err)
    }

    fn download_path(&self) -> PyResult<Option<String>> {
        self.inner.download_path().map_err(py_err)
    }

    fn set_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let path = path.to_string();
        py.detach(move || browser.set_download_path(&path))
            .map_err(py_err)?;
        Ok(())
    }

    fn download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.download_file_exists_mode())
            .map_err(py_err)
    }

    fn set_download_file_exists_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode).map_err(py_err)?;
        py.detach(move || browser.set_download_file_exists_mode(mode))
            .map_err(py_err)?;
        Ok(())
    }

    fn load_mode(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.load_mode()).map_err(py_err)
    }

    fn set_load_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let mode = LoadMode::parse(mode).map_err(py_err)?;
        py.detach(move || browser.set_load_mode(mode))
            .map_err(py_err)?;
        Ok(())
    }

    fn _browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid()
    }

    #[pyo3(signature = (filename=None, timeout_ms=10000))]
    fn wait_for_download(
        &self,
        py: Python<'_>,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<String> {
        let browser = self.inner.clone();
        let filename = filename.map(str::to_string);
        py.detach(move || browser.wait_for_download(filename.as_deref(), timeout_ms))
            .map_err(py_err)
    }

    fn download_missions(&self, py: Python<'_>) -> PyResult<Vec<Py<PyDownloadMission>>> {
        let browser = self.inner.clone();
        py.detach(move || browser.download_missions())
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .collect()
    }

    fn last_download(&self, py: Python<'_>) -> PyResult<Option<Py<PyDownloadMission>>> {
        let browser = self.inner.clone();
        py.detach(move || browser.last_download())
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .transpose()
    }

    #[pyo3(signature = (timeout_ms=10000, cancel_it=false))]
    fn wait_for_download_begin(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> PyResult<Option<Py<PyDownloadMission>>> {
        let browser = self.inner.clone();
        py.detach(move || browser.wait_for_download_begin(timeout_ms, cancel_it))
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
        let browser = self.inner.clone();
        py.detach(move || browser.wait_for_downloads_done(timeout_ms, cancel_if_timeout))
            .map_err(py_err)
    }

    fn tabs_count(&self, py: Python<'_>) -> PyResult<usize> {
        let browser = self.inner.clone();
        py.detach(move || browser.tabs_count()).map_err(py_err)
    }

    fn tab_ids(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let browser = self.inner.clone();
        py.detach(move || browser.tab_ids()).map_err(py_err)
    }

    fn get_page(&self, py: Python<'_>, target_id: &str) -> PyResult<Py<PyPage>> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let page = py
            .detach(move || browser.get_page(&target_id))
            .map_err(py_err)?;
        Py::new(py, PyPage { inner: Some(page) })
    }

    fn page_download_path(&self, py: Python<'_>, target_id: &str) -> PyResult<Option<String>> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        py.detach(move || browser.page_download_path(&target_id))
            .map_err(py_err)
    }

    fn set_page_download_path(&self, py: Python<'_>, target_id: &str, path: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let path = path.to_string();
        py.detach(move || browser.set_page_download_path(&target_id, &path))
            .map_err(py_err)?;
        Ok(())
    }

    fn page_download_file_exists_mode(&self, py: Python<'_>, target_id: &str) -> PyResult<String> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        py.detach(move || browser.page_download_file_exists_mode(&target_id))
            .map_err(py_err)
    }

    fn set_page_download_file_exists_mode(
        &self,
        py: Python<'_>,
        target_id: &str,
        mode: &str,
    ) -> PyResult<()> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let mode = DownloadFileExistsMode::parse(mode).map_err(py_err)?;
        py.detach(move || browser.set_page_download_file_exists_mode(&target_id, mode))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (target_id, rename=None, suffix=None, suffix_specified=false))]
    fn set_page_download_filename(
        &self,
        py: Python<'_>,
        target_id: &str,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> PyResult<()> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let rename = rename.map(str::to_string);
        let suffix = suffix.map(str::to_string);
        py.detach(move || {
            browser.set_page_download_filename(
                &target_id,
                rename.as_deref(),
                suffix.as_deref(),
                suffix_specified,
            )
        })
        .map_err(py_err)?;
        Ok(())
    }
}
