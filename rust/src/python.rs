use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyString};

use crate::browser::{Browser, DownloadFileExistsMode, LaunchOptions, LoadMode};
use crate::console::{Console, ConsoleMessage};
use crate::download::DownloadMission;
use crate::element::{Element, ElementResource};
use crate::error::OpenPageError;
use crate::intercept::{InterceptedRequest, Interceptor};
use crate::listener::{
    Listener, ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerRequestExtraInfo,
    ListenerResponse, ListenerResponseExtraInfo,
};
use crate::locator::LocatorMatch;
use crate::page::Page;
use crate::session::{
    CookieEntry, SessionElement, SessionOptions, SessionPage, SessionXPathResult,
};
use crate::webpage::{WebElement, WebMode, WebPage};

impl From<OpenPageError> for PyErr {
    fn from(value: OpenPageError) -> Self {
        PyRuntimeError::new_err(value.to_string())
    }
}

#[pyclass(module = "openpage_rs", name = "Browser")]
pub struct PyBrowser {
    inner: Browser,
}

#[pyclass(module = "openpage_rs", name = "Page")]
pub struct PyPage {
    inner: Option<Page>,
}

#[pyclass(module = "openpage_rs", name = "Element")]
pub struct PyElement {
    inner: Element,
}

#[pyclass(module = "openpage_rs", name = "SessionPage")]
pub struct PySessionPage {
    inner: SessionPage,
}

#[pyclass(module = "openpage_rs", name = "SessionElement")]
pub struct PySessionElement {
    inner: SessionElement,
}

#[pyclass(module = "openpage_rs", name = "WebPage")]
pub struct PyWebPage {
    inner: WebPage,
}

#[pyclass(module = "openpage_rs", name = "Listener")]
pub struct PyListener {
    inner: Listener,
}

#[pyclass(module = "openpage_rs", name = "Console")]
pub struct PyConsole {
    inner: Console,
}

#[pyclass(module = "openpage_rs", name = "ConsoleMessage")]
pub struct PyConsoleMessage {
    inner: ConsoleMessage,
}

#[pyclass(module = "openpage_rs", name = "Interceptor")]
pub struct PyInterceptor {
    inner: Interceptor,
}

#[pyclass(module = "openpage_rs", name = "InterceptedRequest")]
pub struct PyInterceptedRequest {
    inner: InterceptedRequest,
}

#[pyclass(module = "openpage_rs", name = "ListenerPacket")]
pub struct PyListenerPacket {
    inner: ListenerPacket,
}

#[pyclass(module = "openpage_rs", name = "ListenerRequest")]
pub struct PyListenerRequest {
    inner: ListenerRequest,
}

#[pyclass(module = "openpage_rs", name = "ListenerRequestExtraInfo")]
pub struct PyListenerRequestExtraInfo {
    inner: ListenerRequestExtraInfo,
}

#[pyclass(module = "openpage_rs", name = "ListenerResponse")]
pub struct PyListenerResponse {
    inner: ListenerResponse,
}

#[pyclass(module = "openpage_rs", name = "ListenerResponseExtraInfo")]
pub struct PyListenerResponseExtraInfo {
    inner: ListenerResponseExtraInfo,
}

#[pyclass(module = "openpage_rs", name = "ListenerFailInfo")]
pub struct PyListenerFailInfo {
    inner: ListenerFailInfo,
}

#[pyclass(module = "openpage_rs", name = "DownloadMission")]
pub struct PyDownloadMission {
    inner: DownloadMission,
}

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
            download_file_exists: DownloadFileExistsMode::parse(download_file_exists_mode)?,
            load_mode: LoadMode::parse(load_mode)?,
            user_data_dir: user_data_dir.map(PathBuf::from),
            headless,
            width,
            height,
            no_sandbox,
            ..LaunchOptions::default()
        };
        let inner = py.detach(move || Browser::launch(options))?;
        Ok(Self { inner })
    }

    #[pyo3(signature = (url=None))]
    fn new_page(&self, py: Python<'_>, url: Option<&str>) -> PyResult<Py<PyPage>> {
        let browser = self.inner.clone();
        let url = url.map(str::to_string);
        let page = py.detach(move || browser.new_page(url.as_deref()))?;
        Py::new(py, PyPage { inner: Some(page) })
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let browser = self.inner.clone();
        py.detach(move || browser.close())?;
        Ok(())
    }

    fn version(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.version()).map_err(Into::into)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_alive()).map_err(Into::into)
    }

    fn is_headless(&self) -> PyResult<bool> {
        Ok(self.inner.is_headless())
    }

    fn is_existed(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_existed()).map_err(Into::into)
    }

    fn is_incognito(&self, py: Python<'_>) -> PyResult<bool> {
        let browser = self.inner.clone();
        py.detach(move || browser.is_incognito())
            .map_err(Into::into)
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
            .map_err(Into::into)
    }

    fn download_path(&self) -> PyResult<Option<String>> {
        self.inner.download_path().map_err(Into::into)
    }

    fn set_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let path = path.to_string();
        py.detach(move || browser.set_download_path(&path))?;
        Ok(())
    }

    fn download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.download_file_exists_mode())
            .map_err(Into::into)
    }

    fn set_download_file_exists_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode)?;
        py.detach(move || browser.set_download_file_exists_mode(mode))?;
        Ok(())
    }

    fn load_mode(&self, py: Python<'_>) -> PyResult<String> {
        let browser = self.inner.clone();
        py.detach(move || browser.load_mode()).map_err(Into::into)
    }

    fn set_load_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let mode = LoadMode::parse(mode)?;
        py.detach(move || browser.set_load_mode(mode))?;
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
            .map_err(Into::into)
    }

    fn download_missions(&self, py: Python<'_>) -> PyResult<Vec<Py<PyDownloadMission>>> {
        let browser = self.inner.clone();
        py.detach(move || browser.download_missions())?
            .into_iter()
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .collect()
    }

    fn last_download(&self, py: Python<'_>) -> PyResult<Option<Py<PyDownloadMission>>> {
        let browser = self.inner.clone();
        py.detach(move || browser.last_download())?
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
        py.detach(move || browser.wait_for_download_begin(timeout_ms, cancel_it))?
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
            .map_err(Into::into)
    }

    fn tabs_count(&self, py: Python<'_>) -> PyResult<usize> {
        let browser = self.inner.clone();
        py.detach(move || browser.tabs_count()).map_err(Into::into)
    }

    fn tab_ids(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let browser = self.inner.clone();
        py.detach(move || browser.tab_ids()).map_err(Into::into)
    }

    fn get_page(&self, py: Python<'_>, target_id: &str) -> PyResult<Py<PyPage>> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let page = py.detach(move || browser.get_page(&target_id))?;
        Py::new(py, PyPage { inner: Some(page) })
    }

    fn page_download_path(&self, py: Python<'_>, target_id: &str) -> PyResult<Option<String>> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        py.detach(move || browser.page_download_path(&target_id))
            .map_err(Into::into)
    }

    fn set_page_download_path(&self, py: Python<'_>, target_id: &str, path: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let path = path.to_string();
        py.detach(move || browser.set_page_download_path(&target_id, &path))?;
        Ok(())
    }

    fn page_download_file_exists_mode(&self, py: Python<'_>, target_id: &str) -> PyResult<String> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        py.detach(move || browser.page_download_file_exists_mode(&target_id))
            .map_err(Into::into)
    }

    fn set_page_download_file_exists_mode(
        &self,
        py: Python<'_>,
        target_id: &str,
        mode: &str,
    ) -> PyResult<()> {
        let browser = self.inner.clone();
        let target_id = target_id.to_string();
        let mode = DownloadFileExistsMode::parse(mode)?;
        py.detach(move || browser.set_page_download_file_exists_mode(&target_id, mode))?;
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
        })?;
        Ok(())
    }
}

#[pymethods]
impl PyPage {
    fn goto(&self, py: Python<'_>, url: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let url = url.to_string();
        py.detach(move || page.goto(&url))?;
        Ok(())
    }

    fn url(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.url()).map_err(Into::into)
    }

    fn title(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.title()).map_err(Into::into)
    }

    fn target_id(&self) -> PyResult<String> {
        Ok(self.page()?.target_id())
    }

    fn html(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.html()).map_err(Into::into)
    }

    fn evaluate(&self, py: Python<'_>, expression: &str) -> PyResult<String> {
        let page = self.page()?.clone();
        let expression = expression.to_string();
        let value = py.detach(move || page.evaluate(&expression))?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for(&self, py: Python<'_>, locator: &str, timeout_ms: u64) -> PyResult<Py<PyElement>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.wait_for(&locator, timeout_ms))?;
        Py::new(py, PyElement { inner: element })
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyElement>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.find(&locator))?;
        Py::new(py, PyElement { inner: element })
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyElement>>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.find_all(&locator))?
            .into_iter()
            .map(|inner| Py::new(py, PyElement { inner }))
            .collect()
    }

    fn click(&self, py: Python<'_>, locator: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.click(&locator))?;
        Ok(())
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
        let page = self.page()?.clone();
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
        })?
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
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_to_upload(&locator, &files, timeout_ms, by_js))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=None, by_js=false))]
    fn click_for_new_tab(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<Option<Py<PyPage>>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_for_new_tab(&locator, timeout_ms, by_js))?
            .map(|inner| Py::new(py, PyPage { inner: Some(inner) }))
            .transpose()
    }

    #[pyo3(signature = (locator, timeout_ms=None, get_tab=true))]
    fn click_middle(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> PyResult<Option<Py<PyPage>>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_middle(&locator, timeout_ms, get_tab))?
            .map(|inner| Py::new(py, PyPage { inner: Some(inner) }))
            .transpose()
    }

    fn fill(&self, py: Python<'_>, locator: &str, text: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        let text = text.to_string();
        py.detach(move || page.fill(&locator, &text))?;
        Ok(())
    }

    fn text(&self, py: Python<'_>, locator: &str) -> PyResult<Option<String>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.text(&locator)).map_err(Into::into)
    }

    fn attr(&self, py: Python<'_>, locator: &str, name: &str) -> PyResult<Option<String>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        let name = name.to_string();
        py.detach(move || page.attr(&locator, &name))
            .map_err(Into::into)
    }

    #[pyo3(signature = (user_agent, platform=None))]
    fn set_user_agent_override(
        &self,
        py: Python<'_>,
        user_agent: &str,
        platform: Option<&str>,
    ) -> PyResult<()> {
        let page = self.page()?.clone();
        let user_agent = user_agent.to_string();
        let platform = platform.map(str::to_string);
        py.detach(move || page.set_user_agent(&user_agent, platform.as_deref()))?;
        Ok(())
    }

    fn set_headers(&self, py: Python<'_>, headers: HashMap<String, String>) -> PyResult<()> {
        let page = self.page()?.clone();
        let headers = headers.into_iter().collect::<Vec<_>>();
        py.detach(move || page.set_headers(&headers))?;
        Ok(())
    }

    fn set_session_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.page()?.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_session_storage(&item, value.as_deref()))?;
        Ok(())
    }

    fn set_local_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.page()?.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_local_storage(&item, value.as_deref()))?;
        Ok(())
    }

    fn set_upload_files(&self, py: Python<'_>, files: Vec<String>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.set_upload_files(&files))?;
        Ok(())
    }

    fn load_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.load_mode()).map_err(Into::into)
    }

    fn set_load_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let mode = LoadMode::parse(mode)?;
        py.detach(move || page.set_load_mode(mode))?;
        Ok(())
    }

    fn window_state(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.window_state()).map_err(Into::into)
    }

    fn window_size(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.page()?.clone();
        py.detach(move || page.window_size()).map_err(Into::into)
    }

    fn window_location(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.page()?.clone();
        py.detach(move || page.window_location())
            .map_err(Into::into)
    }

    fn window_max(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_max())?;
        Ok(())
    }

    fn window_min(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_min())?;
        Ok(())
    }

    fn window_full(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_full())?;
        Ok(())
    }

    fn window_normal(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_normal())?;
        Ok(())
    }

    fn window_hide(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_hide())?;
        Ok(())
    }

    fn window_show(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_show())?;
        Ok(())
    }

    #[pyo3(signature = (width=None, height=None))]
    fn window_size_set(
        &self,
        py: Python<'_>,
        width: Option<i64>,
        height: Option<i64>,
    ) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_size_set(width, height))?;
        Ok(())
    }

    #[pyo3(signature = (left=None, top=None))]
    fn window_location_set(
        &self,
        py: Python<'_>,
        left: Option<i64>,
        top: Option<i64>,
    ) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.window_location_set(left, top))?;
        Ok(())
    }

    fn activate(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.activate())?;
        Ok(())
    }

    fn _browser_pid(&self) -> Option<u32> {
        self.page().ok().and_then(|page| page.browser_pid())
    }

    #[pyo3(signature = (path, full_page=true))]
    fn save_screenshot(&self, py: Python<'_>, path: &str, full_page: bool) -> PyResult<()> {
        let page = self.page()?.clone();
        let path = path.to_string();
        py.detach(move || page.save_screenshot(&path, full_page))?;
        Ok(())
    }

    fn set_blocked_urls(&self, py: Python<'_>, patterns: Vec<String>) -> PyResult<()> {
        let page = self.page()?.clone();
        py.detach(move || page.set_blocked_urls(&patterns))?;
        Ok(())
    }

    fn save_pdf(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let path = path.to_string();
        py.detach(move || page.save_pdf(&path))?;
        Ok(())
    }

    fn run_js(&self, py: Python<'_>, expression: &str) -> PyResult<String> {
        let page = self.page()?.clone();
        let expression = expression.to_string();
        let value = py.detach(move || page.run_js(&expression))?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    fn listener(&self, py: Python<'_>) -> PyResult<Py<PyListener>> {
        let listener = self.page()?.listener();
        Py::new(py, PyListener { inner: listener })
    }

    fn console(&self, py: Python<'_>) -> PyResult<Py<PyConsole>> {
        let console = self.page()?.console();
        Py::new(py, PyConsole { inner: console })
    }

    fn interceptor(&self, py: Python<'_>) -> PyResult<Py<PyInterceptor>> {
        let interceptor = self.page()?.interceptor();
        Py::new(py, PyInterceptor { inner: interceptor })
    }

    fn has_alert(&self) -> PyResult<bool> {
        self.page()?.has_alert().map_err(Into::into)
    }

    #[pyo3(signature = (accept=true, prompt_text=None, timeout_ms=10000))]
    fn handle_alert(
        &self,
        py: Python<'_>,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Option<String>> {
        let page = self.page()?.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.handle_alert(accept, prompt_text.as_deref(), timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (accept=true, prompt_text=None))]
    fn set_next_alert_action(
        &self,
        py: Python<'_>,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> PyResult<()> {
        let page = self.page()?.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.set_next_alert_action(accept, prompt_text.as_deref()))?;
        Ok(())
    }

    #[pyo3(signature = (accept=None, prompt_text=None))]
    fn set_auto_alert_action(
        &self,
        py: Python<'_>,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> PyResult<()> {
        let page = self.page()?.clone();
        let prompt_text = prompt_text.map(str::to_string);
        py.detach(move || page.set_auto_alert_action(accept, prompt_text.as_deref()))?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_alert_closed(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_alert_closed(timeout_ms))
            .map_err(Into::into)
    }

    fn snapshot_find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.snapshot_find(&locator))?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_find_all(
        &self,
        py: Python<'_>,
        locator: &str,
    ) -> PyResult<Vec<Py<PySessionElement>>> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.snapshot_find_all(&locator))?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn snapshot_root(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        let page = self.page()?.clone();
        let element = py.detach(move || page.snapshot_root())?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.page()?.clone();
        let expression = expression.to_string();
        py.detach(move || page.snapshot_query_xpath(&expression))?
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
    ) -> PyResult<Vec<(String, Vec<Py<PyElement>>)>> {
        let page = self.page()?.clone();
        py.detach(move || page.find_locators(&locators, any_one, first_match_only))?
            .into_iter()
            .map(|item| locator_match_element_to_py(py, item))
            .collect()
    }

    fn user_agent(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.user_agent()).map_err(Into::into)
    }

    fn ready_state(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.ready_state()).map_err(Into::into)
    }

    fn is_loading(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.is_loading()).map_err(Into::into)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.is_alive()).map_err(Into::into)
    }

    #[pyo3(signature = (text, exclude=false, timeout_ms=10000))]
    fn wait_for_url_change(
        &self,
        py: Python<'_>,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let text = text.to_string();
        py.detach(move || page.wait_for_url_change(&text, exclude, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (text, exclude=false, timeout_ms=10000))]
    fn wait_for_title_change(
        &self,
        py: Python<'_>,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let text = text.to_string();
        py.detach(move || page.wait_for_title_change(&text, exclude, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locators, timeout_ms=10000, any_one=false))]
    fn wait_for_elements_loaded(
        &self,
        py: Python<'_>,
        locators: Vec<String>,
        timeout_ms: u64,
        any_one: bool,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_elements_loaded(&locators, any_one, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_displayed(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_displayed(&locator, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_hidden(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_hidden(&locator, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_enabled(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_enabled(&locator, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_deleted(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_deleted(&locator, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for_ele_clickable(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: u64,
    ) -> PyResult<bool> {
        let page = self.page()?.clone();
        let locator = locator.to_string();
        py.detach(move || page.wait_for_ele_clickable(&locator, timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_load_start(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_load_start(timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_doc_loaded(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_doc_loaded(timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000, cancel_it=false))]
    fn wait_for_download_begin(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_download_begin(timeout_ms, cancel_it))?
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
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_downloads_done(timeout_ms, cancel_if_timeout))
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_upload_paths_inputted(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.page()?.clone();
        py.detach(move || page.wait_for_upload_paths_inputted(timeout_ms))
            .map_err(Into::into)
    }

    fn cookie_header(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.page()?.clone();
        py.detach(move || page.cookie_header()).map_err(Into::into)
    }

    fn cookies(&self, py: Python<'_>) -> PyResult<Vec<(String, String, Option<String>)>> {
        let page = self.page()?.clone();
        py.detach(move || page.cookies())
            .map(cookie_entries_to_tuples)
            .map_err(Into::into)
    }

    fn set_cookie_header(&self, py: Python<'_>, url: &str, cookie_header: &str) -> PyResult<()> {
        let page = self.page()?.clone();
        let url = url.to_string();
        let cookie_header = cookie_header.to_string();
        py.detach(move || page.set_cookie_header(&url, &cookie_header))?;
        Ok(())
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(page) = self.inner.take() {
            py.detach(move || page.close())?;
        }
        Ok(())
    }
}

#[pymethods]
impl PyElement {
    fn click(&self) -> PyResult<()> {
        self.inner.click()?;
        Ok(())
    }

    #[pyo3(signature = (offset_x=None, offset_y=None, button="left", count=1))]
    fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> PyResult<()> {
        self.inner
            .click_at(offset_x, offset_y, button, count)
            .map_err(Into::into)
    }

    #[pyo3(signature = (times=2))]
    fn click_multi(&self, times: u32) -> PyResult<()> {
        self.inner.click_multi(times).map_err(Into::into)
    }

    fn click_left(&self) -> PyResult<()> {
        self.inner.click_left().map_err(Into::into)
    }

    fn click_middle(&self) -> PyResult<()> {
        self.inner.click_middle().map_err(Into::into)
    }

    fn click_right(&self) -> PyResult<()> {
        self.inner.click_right().map_err(Into::into)
    }

    #[pyo3(signature = (text, clear=false, by_js=false))]
    fn input(&self, text: &str, clear: bool, by_js: bool) -> PyResult<()> {
        self.inner.input_with_options(text, clear, by_js)?;
        Ok(())
    }

    #[pyo3(signature = (values, clear=false, by_js=false))]
    fn input_keys(&self, values: Vec<String>, clear: bool, by_js: bool) -> PyResult<()> {
        self.inner.input_keys_with_options(&values, clear, by_js)?;
        Ok(())
    }

    fn clear(&self) -> PyResult<()> {
        self.inner.clear()?;
        Ok(())
    }

    fn focus(&self) -> PyResult<()> {
        self.inner.focus()?;
        Ok(())
    }

    #[pyo3(signature = (offset_x=None, offset_y=None))]
    fn hover(&self, offset_x: Option<f64>, offset_y: Option<f64>) -> PyResult<()> {
        if offset_x.is_none() && offset_y.is_none() {
            self.inner.hover().map_err(Into::into)
        } else {
            self.inner
                .hover_with_offset(offset_x, offset_y)
                .map_err(Into::into)
        }
    }

    fn press_key(&self, key: &str) -> PyResult<()> {
        self.inner.press_key(key)?;
        Ok(())
    }

    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(Into::into)
    }

    fn html(&self) -> PyResult<Option<String>> {
        self.inner.html().map_err(Into::into)
    }

    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(Into::into)
    }

    fn link(&self) -> PyResult<Option<String>> {
        self.inner.link().map_err(Into::into)
    }

    fn child_count(&self) -> PyResult<usize> {
        self.inner.child_count().map_err(Into::into)
    }

    fn css_path(&self) -> PyResult<String> {
        self.inner.css_path().map_err(Into::into)
    }

    fn xpath(&self) -> PyResult<String> {
        self.inner.xpath().map_err(Into::into)
    }

    fn comments(&self) -> PyResult<Vec<String>> {
        self.inner.comments().map_err(Into::into)
    }

    #[pyo3(signature = (text_node_only=false))]
    fn texts(&self, text_node_only: bool) -> PyResult<Vec<String>> {
        self.inner.texts(text_node_only).map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000, base64_to_bytes=true))]
    fn src(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .src(timeout_ms, base64_to_bytes)?
            .map(|resource| element_resource_to_py(py, resource))
            .transpose()
    }

    #[pyo3(signature = (path=None, name=None, timeout_ms=10000, rename=true))]
    fn save(
        &self,
        path: Option<&str>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> PyResult<String> {
        let path = path.map(PathBuf::from);
        let saved = self.inner.save(path.as_deref(), name, timeout_ms, rename)?;
        Ok(saved.to_string_lossy().into_owned())
    }

    fn save_screenshot(&self, path: &str) -> PyResult<()> {
        self.inner.save_screenshot(path)?;
        Ok(())
    }

    #[pyo3(signature = (offset_x=0.0, offset_y=0.0, duration_secs=0.5))]
    fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> PyResult<()> {
        self.inner.drag(offset_x, offset_y, duration_secs)?;
        Ok(())
    }

    #[pyo3(signature = (target, duration_secs=0.5))]
    fn drag_to(&self, target: &Bound<'_, PyAny>, duration_secs: f64) -> PyResult<()> {
        if let Ok(target) = target.extract::<PyRef<'_, PyElement>>() {
            self.inner.drag_to(&target.inner, duration_secs)?;
            return Ok(());
        }

        if let Ok((x, y)) = target.extract::<(f64, f64)>() {
            self.inner.drag_to_point(x, y, duration_secs)?;
            return Ok(());
        }

        if let Ok(coords) = target.extract::<Vec<f64>>() {
            if coords.len() == 2 {
                self.inner
                    .drag_to_point(coords[0], coords[1], duration_secs)?;
                return Ok(());
            }
        }

        Err(PyTypeError::new_err(
            "drag_to() expects an Element or a 2-item coordinate sequence",
        ))
    }

    fn run_js(&self, script: &str) -> PyResult<String> {
        let value = self.inner.run_js(script)?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    fn is_selected(&self) -> PyResult<bool> {
        self.inner.is_selected().map_err(Into::into)
    }

    fn is_checked(&self) -> PyResult<bool> {
        self.inner.is_checked().map_err(Into::into)
    }

    fn is_displayed(&self) -> PyResult<bool> {
        self.inner.is_displayed().map_err(Into::into)
    }

    fn is_enabled(&self) -> PyResult<bool> {
        self.inner.is_enabled().map_err(Into::into)
    }

    fn is_alive(&self) -> PyResult<bool> {
        self.inner.is_alive().map_err(Into::into)
    }

    fn has_rect(&self) -> PyResult<Option<Vec<(f64, f64)>>> {
        self.inner.rect_corners().map_err(Into::into)
    }

    fn is_in_viewport(&self) -> PyResult<bool> {
        self.inner.is_in_viewport().map_err(Into::into)
    }

    fn is_whole_in_viewport(&self) -> PyResult<bool> {
        self.inner.is_whole_in_viewport().map_err(Into::into)
    }

    fn is_covered(&self) -> PyResult<bool> {
        self.inner.is_covered().map_err(Into::into)
    }

    fn is_clickable(&self) -> PyResult<bool> {
        self.inner.is_clickable().map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_displayed(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_displayed(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_hidden(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_hidden(timeout_ms).map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_enabled(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_enabled(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_disabled(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_disabled(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_deleted(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_deleted(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_clickable(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_clickable(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_has_rect(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_has_rect(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_covered(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_covered(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_not_covered(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_not_covered(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_disabled_or_deleted(timeout_ms)
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_stop_moving(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_stop_moving(timeout_ms)
            .map_err(Into::into)
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyElement>> {
        Py::new(
            py,
            PyElement {
                inner: self.inner.find(locator)?,
            },
        )
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyElement>>> {
        self.inner
            .find_all(locator)?
            .into_iter()
            .map(|inner| Py::new(py, PyElement { inner }))
            .collect()
    }

    fn snapshot_query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .snapshot_query_xpath(expression)?
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
    ) -> PyResult<Vec<(String, Vec<Py<PyElement>>)>> {
        self.inner
            .find_locators(&locators, any_one, first_match_only)?
            .into_iter()
            .map(|item| locator_match_element_to_py(py, item))
            .collect()
    }
}

#[pymethods]
impl PySessionPage {
    #[staticmethod]
    #[pyo3(signature = (timeout_secs=10, user_agent=None))]
    fn create(py: Python<'_>, timeout_secs: u64, user_agent: Option<String>) -> PyResult<Self> {
        let options = SessionOptions {
            timeout_secs,
            user_agent,
            ..SessionOptions::default()
        };
        let inner = py.detach(move || SessionPage::new(options))?;
        Ok(Self { inner })
    }

    fn get(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.get(&url)).map_err(Into::into)
    }

    fn post(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post(&url)).map_err(Into::into)
    }

    #[pyo3(signature = (url, payload_json=None))]
    fn post_json(&self, py: Python<'_>, url: &str, payload_json: Option<&str>) -> PyResult<bool> {
        let payload = payload_json
            .map(|value| serde_json::from_str(value))
            .transpose()
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post_json(&url, payload))
            .map_err(Into::into)
    }

    fn url(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.url()).map_err(Into::into)
    }

    fn status_code(&self, py: Python<'_>) -> PyResult<Option<u16>> {
        let page = self.inner.clone();
        py.detach(move || page.status_code()).map_err(Into::into)
    }

    fn raw_data(&self, py: Python<'_>) -> PyResult<Py<PyBytes>> {
        let page = self.inner.clone();
        let raw = py.detach(move || page.raw_data())?;
        Ok(PyBytes::new(py, &raw).into())
    }

    fn encoding(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.encoding()).map_err(Into::into)
    }

    fn html(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.html()).map_err(Into::into)
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
            .map_err(Into::into)
    }

    fn title(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.title()).map_err(Into::into)
    }

    fn user_agent(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.user_agent()).map_err(Into::into)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_alive()).map_err(Into::into)
    }

    fn is_loading(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_loading()).map_err(Into::into)
    }

    fn ready_state(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.ready_state()).map_err(Into::into)
    }

    fn is_headless(&self) -> PyResult<bool> {
        Ok(self.inner.is_headless())
    }

    fn set_user_agent(&self, py: Python<'_>, user_agent: Option<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_user_agent(user_agent))?;
        Ok(())
    }

    fn set_headers(&self, py: Python<'_>, headers: HashMap<String, String>) -> PyResult<()> {
        let page = self.inner.clone();
        let headers = headers.into_iter().collect::<Vec<_>>();
        py.detach(move || page.set_headers(&headers))?;
        Ok(())
    }

    fn cookie_header(&self, py: Python<'_>, url: &str) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.cookie_header(&url))
            .map_err(Into::into)
    }

    fn cookies(&self, py: Python<'_>) -> PyResult<Vec<(String, String, Option<String>)>> {
        let page = self.inner.clone();
        py.detach(move || page.cookies())
            .map(cookie_entries_to_tuples)
            .map_err(Into::into)
    }

    fn set_cookie_header(&self, py: Python<'_>, url: &str, cookie_header: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let url = url.to_string();
        let cookie_header = cookie_header.to_string();
        py.detach(move || page.set_cookie_header(&url, &cookie_header))?;
        Ok(())
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.find(&locator))?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PySessionElement>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.find_all(&locator))?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        py.detach(move || page.query_xpath(&expression))?
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
    ) -> PyResult<Vec<(String, Vec<Py<PySessionElement>>)>> {
        let page = self.inner.clone();
        py.detach(move || page.find_locators(&locators, any_one, first_match_only))?
            .into_iter()
            .map(|item| locator_match_session_to_py(py, item))
            .collect()
    }

    fn root(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let element = py.detach(move || page.root())?;
        Py::new(py, PySessionElement { inner: element })
    }
}

#[pymethods]
impl PySessionElement {
    fn tag(&self) -> PyResult<String> {
        self.inner.tag().map_err(Into::into)
    }

    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(Into::into)
    }

    fn html(&self) -> PyResult<Option<String>> {
        self.inner.html().map_err(Into::into)
    }

    fn inner_html(&self) -> PyResult<Option<String>> {
        self.inner.inner_html().map_err(Into::into)
    }

    fn raw_text(&self) -> PyResult<Option<String>> {
        self.inner.raw_text().map_err(Into::into)
    }

    fn attrs(&self) -> PyResult<Vec<(String, String)>> {
        self.inner.attrs().map_err(Into::into)
    }

    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(Into::into)
    }

    fn link(&self) -> PyResult<Option<String>> {
        self.inner.link().map_err(Into::into)
    }

    fn child_count(&self) -> PyResult<usize> {
        self.inner.child_count().map_err(Into::into)
    }

    fn css_path(&self) -> PyResult<String> {
        self.inner.css_path().map_err(Into::into)
    }

    fn xpath(&self) -> PyResult<String> {
        self.inner.xpath().map_err(Into::into)
    }

    fn comments(&self) -> PyResult<Vec<String>> {
        self.inner.comments().map_err(Into::into)
    }

    #[pyo3(signature = (text_node_only=false))]
    fn texts(&self, text_node_only: bool) -> PyResult<Vec<String>> {
        self.inner.texts(text_node_only).map_err(Into::into)
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let element = self.inner.find(locator)?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PySessionElement>>> {
        self.inner
            .find_all(locator)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .query_xpath(expression)?
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
    ) -> PyResult<Vec<(String, Vec<Py<PySessionElement>>)>> {
        self.inner
            .find_locators(&locators, any_one, first_match_only)?
            .into_iter()
            .map(|item| locator_match_session_to_py(py, item))
            .collect()
    }

    fn parent(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent()?,
            },
        )
    }

    fn parent_level(&self, py: Python<'_>, level: usize) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent_level(level)?,
            },
        )
    }

    fn parent_with(
        &self,
        py: Python<'_>,
        locator: &str,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent_with(locator, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None, index=1))]
    fn child(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.child_with(normalized, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None))]
    fn children(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
    ) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .children_with(normalized)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None, index=1))]
    fn prev(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.prev_with(normalized, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None, index=1))]
    fn next(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.next_with(normalized, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None, index=1))]
    fn before(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.before_with(normalized, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None, index=1))]
    fn after(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
        index: usize,
    ) -> PyResult<Py<PySessionElement>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.after_with(normalized, index)?,
            },
        )
    }

    #[pyo3(signature = (locator=None))]
    fn prevs(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .prevs_with(normalized)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None))]
    fn nexts(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .nexts_with(normalized)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None))]
    fn befores(
        &self,
        py: Python<'_>,
        locator: Option<&str>,
    ) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .befores_with(normalized)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None))]
    fn afters(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .afters_with(normalized)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }
}

#[pymethods]
impl PyWebPage {
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
        let mode = WebMode::parse(mode)?;
        let launch_options = LaunchOptions {
            browser_path: browser_path.map(PathBuf::from),
            download_path: download_path.map(PathBuf::from),
            download_file_exists: DownloadFileExistsMode::parse(download_file_exists_mode)?,
            load_mode: LoadMode::parse(load_mode)?,
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
        let inner = py.detach(move || WebPage::new(mode, launch_options, session_options))?;
        Ok(Self { inner })
    }

    fn mode(&self) -> PyResult<String> {
        self.inner
            .mode()
            .map(|mode| mode.as_str().to_string())
            .map_err(Into::into)
    }

    fn tabs_count(&self, py: Python<'_>) -> PyResult<usize> {
        let page = self.inner.clone();
        py.detach(move || page.tabs_count()).map_err(Into::into)
    }

    fn tab_ids(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let page = self.inner.clone();
        py.detach(move || page.tab_ids()).map_err(Into::into)
    }

    fn download_path(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.download_path()).map_err(Into::into)
    }

    fn set_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let path = path.to_string();
        py.detach(move || page.set_download_path(&path))?;
        Ok(())
    }

    fn current_tab_download_path(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.current_tab_download_path())
            .map_err(Into::into)
    }

    fn set_current_tab_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let path = path.to_string();
        py.detach(move || page.set_current_tab_download_path(&path))?;
        Ok(())
    }

    fn set_blocked_urls(&self, py: Python<'_>, patterns: Vec<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_blocked_urls(&patterns))?;
        Ok(())
    }

    fn download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.download_file_exists_mode())
            .map_err(Into::into)
    }

    fn set_download_file_exists_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode)?;
        py.detach(move || page.set_download_file_exists_mode(mode))?;
        Ok(())
    }

    fn current_tab_download_file_exists_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.current_tab_download_file_exists_mode())
            .map_err(Into::into)
    }

    fn set_current_tab_download_file_exists_mode(
        &self,
        py: Python<'_>,
        mode: &str,
    ) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = DownloadFileExistsMode::parse(mode)?;
        py.detach(move || page.set_current_tab_download_file_exists_mode(mode))?;
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
        })?;
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
            .map_err(Into::into)
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
        })?
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
            .map_err(Into::into)
    }

    #[pyo3(signature = (locator, timeout_ms=None, by_js=false))]
    fn click_for_new_tab(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<Option<Py<PyPage>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_for_new_tab(&locator, timeout_ms, by_js))?
            .map(|inner| Py::new(py, PyPage { inner: Some(inner) }))
            .transpose()
    }

    #[pyo3(signature = (locator, timeout_ms=None, get_tab=true))]
    fn click_middle(
        &self,
        py: Python<'_>,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> PyResult<Option<Py<PyPage>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.click_middle(&locator, timeout_ms, get_tab))?
            .map(|inner| Py::new(py, PyPage { inner: Some(inner) }))
            .transpose()
    }

    fn download_missions(&self, py: Python<'_>) -> PyResult<Vec<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.download_missions())?
            .into_iter()
            .map(|inner| Py::new(py, PyDownloadMission { inner }))
            .collect()
    }

    fn last_download(&self, py: Python<'_>) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.last_download())?
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
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000, cancel_it=false))]
    fn wait_for_download_begin(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> PyResult<Option<Py<PyDownloadMission>>> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_download_begin(timeout_ms, cancel_it))?
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
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_upload_paths_inputted(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_upload_paths_inputted(timeout_ms))
            .map_err(Into::into)
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
        py.detach(move || page.get(&url)).map_err(Into::into)
    }

    fn post(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post(&url)).map_err(Into::into)
    }

    #[pyo3(signature = (url, payload_json=None))]
    fn post_json(&self, py: Python<'_>, url: &str, payload_json: Option<&str>) -> PyResult<bool> {
        let payload = payload_json
            .map(|value| serde_json::from_str(value))
            .transpose()
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.post_json(&url, payload))
            .map_err(Into::into)
    }

    fn url(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.url()).map_err(Into::into)
    }

    fn title(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.title()).map_err(Into::into)
    }

    fn user_agent(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.user_agent()).map_err(Into::into)
    }

    fn status_code(&self, py: Python<'_>) -> PyResult<Option<u16>> {
        let page = self.inner.clone();
        py.detach(move || page.status_code()).map_err(Into::into)
    }

    fn cookies(&self, py: Python<'_>) -> PyResult<Vec<(String, String, Option<String>)>> {
        let page = self.inner.clone();
        py.detach(move || page.cookies())
            .map(cookie_entries_to_tuples)
            .map_err(Into::into)
    }

    fn html(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.html()).map_err(Into::into)
    }

    fn raw_data(&self, py: Python<'_>) -> PyResult<Py<PyBytes>> {
        let page = self.inner.clone();
        let raw = py.detach(move || page.raw_data())?;
        Ok(PyBytes::new(py, &raw).into())
    }

    fn encoding(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.encoding()).map_err(Into::into)
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
            .map_err(Into::into)
    }

    fn is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_alive()).map_err(Into::into)
    }

    fn is_loading(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_loading()).map_err(Into::into)
    }

    fn ready_state(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.ready_state()).map_err(Into::into)
    }

    fn is_headless(&self) -> PyResult<bool> {
        Ok(self.inner.is_headless())
    }

    fn has_alert(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.has_alert()).map_err(Into::into)
    }

    fn is_existed(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_existed()).map_err(Into::into)
    }

    fn is_incognito(&self, py: Python<'_>) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.is_incognito()).map_err(Into::into)
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
            .map_err(Into::into)
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
        py.detach(move || page.set_next_alert_action(accept, prompt_text.as_deref()))?;
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
        py.detach(move || page.set_auto_alert_action(accept, prompt_text.as_deref()))?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_alert_closed(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_alert_closed(timeout_ms))
            .map_err(Into::into)
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
        py.detach(move || page.set_user_agent(&user_agent, platform.as_deref()))?;
        Ok(())
    }

    fn set_headers(&self, py: Python<'_>, headers: HashMap<String, String>) -> PyResult<()> {
        let page = self.inner.clone();
        let headers = headers.into_iter().collect::<Vec<_>>();
        py.detach(move || page.set_headers(&headers))?;
        Ok(())
    }

    fn set_session_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.inner.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_session_storage(&item, value.as_deref()))?;
        Ok(())
    }

    fn set_local_storage(&self, py: Python<'_>, item: &str, value: Option<&str>) -> PyResult<()> {
        let page = self.inner.clone();
        let item = item.to_string();
        let value = value.map(str::to_string);
        py.detach(move || page.set_local_storage(&item, value.as_deref()))?;
        Ok(())
    }

    fn set_upload_files(&self, py: Python<'_>, files: Vec<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_upload_files(&files))?;
        Ok(())
    }

    fn load_mode(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.load_mode()).map_err(Into::into)
    }

    fn set_load_mode(&self, py: Python<'_>, mode: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let mode = LoadMode::parse(mode)?;
        py.detach(move || page.set_load_mode(mode))?;
        Ok(())
    }

    fn window_state(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.window_state()).map_err(Into::into)
    }

    fn window_size(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.inner.clone();
        py.detach(move || page.window_size()).map_err(Into::into)
    }

    fn window_location(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let page = self.inner.clone();
        py.detach(move || page.window_location())
            .map_err(Into::into)
    }

    fn window_max(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_max())?;
        Ok(())
    }

    fn window_min(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_min())?;
        Ok(())
    }

    fn window_full(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_full())?;
        Ok(())
    }

    fn window_normal(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_normal())?;
        Ok(())
    }

    fn window_hide(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_hide())?;
        Ok(())
    }

    fn window_show(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.window_show())?;
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
        py.detach(move || page.window_size_set(width, height))?;
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
        py.detach(move || page.window_location_set(left, top))?;
        Ok(())
    }

    fn activate(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.activate())?;
        Ok(())
    }

    fn _browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid()
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyAny>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.find(&locator))?;
        wrap_web_element(py, element)
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.find_all(&locator))?
            .into_iter()
            .map(|element| wrap_web_element(py, element))
            .collect()
    }

    fn snapshot_find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.snapshot_find(&locator))?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_find_all(
        &self,
        py: Python<'_>,
        locator: &str,
    ) -> PyResult<Vec<Py<PySessionElement>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.snapshot_find_all(&locator))?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn snapshot_root(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let element = py.detach(move || page.snapshot_root())?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn snapshot_query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        py.detach(move || page.snapshot_query_xpath(&expression))?
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
        py.detach(move || page.find_locators(&locators, any_one, first_match_only))?
            .into_iter()
            .map(|item| locator_match_web_to_py(py, item))
            .collect()
    }

    fn run_js(&self, py: Python<'_>, expression: &str) -> PyResult<String> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        let value = py.detach(move || page.run_js(&expression))?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
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
            .map_err(Into::into)
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
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_load_start(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_load_start(timeout_ms))
            .map_err(Into::into)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_for_doc_loaded(&self, py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
        let page = self.inner.clone();
        py.detach(move || page.wait_for_doc_loaded(timeout_ms))
            .map_err(Into::into)
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
            .map_err(Into::into)
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
            .map_err(Into::into)
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
            .map_err(Into::into)
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
            .map_err(Into::into)
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
            .map_err(Into::into)
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
            .map_err(Into::into)
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
        let mode = mode.map(WebMode::parse).transpose()?;
        py.detach(move || page.change_mode(mode, go, copy_cookies))?;
        Ok(())
    }

    #[pyo3(signature = (copy_user_agent=true))]
    fn cookies_to_session(&self, py: Python<'_>, copy_user_agent: bool) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.cookies_to_session(copy_user_agent))?;
        Ok(())
    }

    fn cookies_to_browser(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.cookies_to_browser())?;
        Ok(())
    }

    fn quit(&self, py: Python<'_>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.quit())?;
        Ok(())
    }
}

#[pymethods]
impl PyListener {
    #[pyo3(signature = (targets=None, is_regex=false, methods=None, resource_types=None))]
    fn start(
        &self,
        py: Python<'_>,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.start(targets, is_regex, methods, resource_types))?;
        Ok(())
    }

    #[pyo3(signature = (targets=None, is_regex=false, methods=None, resource_types=None))]
    fn set_targets(
        &self,
        py: Python<'_>,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.set_targets(targets, is_regex, methods, resource_types))?;
        Ok(())
    }

    #[pyo3(signature = (count=1, timeout_ms=None, fit_count=true))]
    fn wait(
        &self,
        py: Python<'_>,
        count: usize,
        timeout_ms: Option<u64>,
        fit_count: bool,
    ) -> PyResult<Vec<Py<PyListenerPacket>>> {
        let listener = self.inner.clone();
        py.detach(move || listener.wait(count, timeout_ms, fit_count))?
            .into_iter()
            .map(|inner| Py::new(py, PyListenerPacket { inner }))
            .collect()
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.clear())?;
        Ok(())
    }

    #[pyo3(signature = (clear=true))]
    fn pause(&self, py: Python<'_>, clear: bool) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.pause(clear))?;
        Ok(())
    }

    fn resume(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.resume())?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=None, targets_only=false))]
    fn wait_until_idle(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
        targets_only: bool,
    ) -> PyResult<bool> {
        let listener = self.inner.clone();
        py.detach(move || listener.wait_until_idle(timeout_ms, targets_only))
            .map_err(Into::into)
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.stop())?;
        Ok(())
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(Into::into)
    }
}

#[pymethods]
impl PyConsole {
    fn start(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.start())?;
        Ok(())
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.stop())?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
    ) -> PyResult<Option<Py<PyConsoleMessage>>> {
        let console = self.inner.clone();
        py.detach(move || console.wait(timeout_ms))?
            .map(|inner| Py::new(py, PyConsoleMessage { inner }))
            .transpose()
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(Into::into)
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.clear())?;
        Ok(())
    }

    fn messages(&self, py: Python<'_>) -> PyResult<Vec<Py<PyConsoleMessage>>> {
        let console = self.inner.clone();
        py.detach(move || console.messages())?
            .into_iter()
            .map(|inner| Py::new(py, PyConsoleMessage { inner }))
            .collect()
    }
}

#[pymethods]
impl PyInterceptor {
    #[pyo3(signature = (targets=None, is_regex=false, methods=None, resource_types=None))]
    fn start(
        &self,
        py: Python<'_>,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> PyResult<()> {
        let interceptor = self.inner.clone();
        py.detach(move || interceptor.start(targets, is_regex, methods, resource_types))?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
    ) -> PyResult<Option<Py<PyInterceptedRequest>>> {
        let interceptor = self.inner.clone();
        py.detach(move || interceptor.wait(timeout_ms))?
            .map(|inner| Py::new(py, PyInterceptedRequest { inner }))
            .transpose()
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let interceptor = self.inner.clone();
        py.detach(move || interceptor.stop())?;
        Ok(())
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(Into::into)
    }
}

#[pymethods]
impl PyInterceptedRequest {
    fn request_id(&self) -> String {
        self.inner.request_id()
    }

    fn frame_id(&self) -> String {
        self.inner.frame_id()
    }

    fn url(&self) -> String {
        self.inner.url()
    }

    fn method(&self) -> String {
        self.inner.method()
    }

    fn headers(&self) -> Vec<(String, String)> {
        header_tuples(&self.inner.headers())
    }

    fn resource_type(&self) -> String {
        self.inner.resource_type()
    }

    fn has_post_data(&self) -> bool {
        self.inner.has_post_data()
    }

    fn post_data_entries(&self) -> usize {
        self.inner.post_data_entries()
    }

    #[pyo3(signature = (url=None, method=None, headers=None, post_data=None))]
    fn continue_request(
        &self,
        py: Python<'_>,
        url: Option<&str>,
        method: Option<&str>,
        headers: Option<HashMap<String, String>>,
        post_data: Option<&str>,
    ) -> PyResult<()> {
        let request = self.inner.clone();
        let url = url.map(|value| value.to_string());
        let method = method.map(|value| value.to_string());
        let post_data = post_data.map(|value| value.to_string());
        py.detach(move || {
            request.continue_request(
                url.as_deref(),
                method.as_deref(),
                headers,
                post_data.as_deref(),
            )
        })?;
        Ok(())
    }

    #[pyo3(signature = (reason="BlockedByClient"))]
    fn fail(&self, py: Python<'_>, reason: &str) -> PyResult<()> {
        let request = self.inner.clone();
        let reason = reason.parse().map_err(OpenPageError::BrowserOperation)?;
        py.detach(move || request.fail(reason))?;
        Ok(())
    }

    #[pyo3(signature = (response_code=200, body=None, headers=None, response_phrase=None, body_base64=false))]
    fn fulfill(
        &self,
        py: Python<'_>,
        response_code: i64,
        body: Option<&Bound<'_, PyAny>>,
        headers: Option<HashMap<String, String>>,
        response_phrase: Option<&str>,
        body_base64: bool,
    ) -> PyResult<()> {
        let request = self.inner.clone();
        let response_phrase = response_phrase.map(|value| value.to_string());
        let body_bytes = match body {
            None => None,
            Some(body) => {
                if let Ok(bytes) = body.extract::<Vec<u8>>() {
                    Some(bytes)
                } else {
                    Some(body.extract::<String>()?.into_bytes())
                }
            }
        };
        py.detach(move || {
            let payload = body_bytes
                .map(|bytes| {
                    if body_base64 {
                        BASE64_STANDARD
                            .decode(bytes)
                            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
                    } else {
                        Ok(bytes)
                    }
                })
                .transpose()?;
            request.fulfill(
                response_code,
                payload.as_deref(),
                headers,
                response_phrase.as_deref(),
            )
        })?;
        Ok(())
    }
}

#[pymethods]
impl PyListenerPacket {
    fn target(&self) -> Option<String> {
        self.inner.matched_target.clone()
    }

    fn frame_id(&self) -> Option<String> {
        self.inner.frame_id.clone()
    }

    fn url(&self) -> String {
        self.inner.url.clone()
    }

    fn method(&self) -> String {
        self.inner.method.clone()
    }

    fn resource_type(&self) -> Option<String> {
        self.inner.resource_type.clone()
    }

    fn is_failed(&self) -> bool {
        self.inner.is_failed
    }

    fn request(&self, py: Python<'_>) -> PyResult<Py<PyListenerRequest>> {
        Py::new(
            py,
            PyListenerRequest {
                inner: self.inner.request.clone(),
            },
        )
    }

    fn response(&self, py: Python<'_>) -> PyResult<Option<Py<PyListenerResponse>>> {
        self.inner
            .response
            .clone()
            .map(|inner| Py::new(py, PyListenerResponse { inner }))
            .transpose()
    }

    fn fail_info(&self, py: Python<'_>) -> PyResult<Option<Py<PyListenerFailInfo>>> {
        self.inner
            .fail_info
            .clone()
            .map(|inner| Py::new(py, PyListenerFailInfo { inner }))
            .transpose()
    }
}

#[pymethods]
impl PyConsoleMessage {
    fn all_info(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.all_info)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    fn source(&self) -> String {
        self.inner.source.clone()
    }

    fn level(&self) -> String {
        self.inner.level.clone()
    }

    fn text(&self) -> String {
        self.inner.text.clone()
    }

    fn body(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.body())
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    fn url(&self) -> Option<String> {
        self.inner.url.clone()
    }

    fn line(&self) -> Option<i64> {
        self.inner.line
    }

    fn column(&self) -> Option<i64> {
        self.inner.column
    }
}

#[pymethods]
impl PyListenerRequest {
    fn url(&self) -> String {
        self.inner.url.clone()
    }

    fn method(&self) -> String {
        self.inner.method.clone()
    }

    fn headers(&self) -> Vec<(String, String)> {
        header_tuples(&self.inner.headers)
    }

    fn post_data(&self) -> Option<String> {
        self.inner.post_data.clone()
    }

    fn extra_info(&self, py: Python<'_>) -> PyResult<Option<Py<PyListenerRequestExtraInfo>>> {
        self.inner
            .extra_info
            .clone()
            .map(|inner| Py::new(py, PyListenerRequestExtraInfo { inner }))
            .transpose()
    }
}

#[pymethods]
impl PyListenerRequestExtraInfo {
    fn headers(&self) -> Vec<(String, String)> {
        header_tuples(&self.inner.headers)
    }
}

#[pymethods]
impl PyListenerResponse {
    fn url(&self) -> String {
        self.inner.url.clone()
    }

    fn status(&self) -> i64 {
        self.inner.status
    }

    fn status_text(&self) -> String {
        self.inner.status_text.clone()
    }

    fn headers(&self) -> Vec<(String, String)> {
        header_tuples(&self.inner.headers)
    }

    fn mime_type(&self) -> String {
        self.inner.mime_type.clone()
    }

    fn body(&self) -> Option<String> {
        self.inner.body.clone()
    }

    fn body_base64(&self) -> bool {
        self.inner.body_base64
    }

    fn extra_info(&self, py: Python<'_>) -> PyResult<Option<Py<PyListenerResponseExtraInfo>>> {
        self.inner
            .extra_info
            .clone()
            .map(|inner| Py::new(py, PyListenerResponseExtraInfo { inner }))
            .transpose()
    }
}

#[pymethods]
impl PyListenerResponseExtraInfo {
    fn headers(&self) -> Vec<(String, String)> {
        header_tuples(&self.inner.headers)
    }

    fn status_code(&self) -> i64 {
        self.inner.status_code
    }

    fn headers_text(&self) -> Option<String> {
        self.inner.headers_text.clone()
    }
}

#[pymethods]
impl PyListenerFailInfo {
    fn error_text(&self) -> String {
        self.inner.error_text.clone()
    }

    fn canceled(&self) -> Option<bool> {
        self.inner.canceled
    }

    fn blocked_reason(&self) -> Option<String> {
        self.inner.blocked_reason.clone()
    }
}

#[pymethods]
impl PyDownloadMission {
    fn id(&self) -> String {
        self.inner.id()
    }

    fn guid(&self) -> String {
        self.inner.guid()
    }

    fn tab_id(&self) -> PyResult<String> {
        self.inner.tab_id().map_err(Into::into)
    }

    fn url(&self) -> PyResult<String> {
        self.inner.url().map_err(Into::into)
    }

    fn folder(&self) -> PyResult<String> {
        self.inner.folder().map_err(Into::into)
    }

    fn name(&self) -> PyResult<String> {
        self.inner.name().map_err(Into::into)
    }

    fn suggested_filename(&self) -> PyResult<String> {
        self.inner.suggested_filename().map_err(Into::into)
    }

    fn tmp_path(&self) -> PyResult<String> {
        self.inner.tmp_path().map_err(Into::into)
    }

    fn state(&self) -> PyResult<String> {
        self.inner.state().map_err(Into::into)
    }

    fn received_bytes(&self) -> PyResult<u64> {
        self.inner.received_bytes().map_err(Into::into)
    }

    fn total_bytes(&self) -> PyResult<Option<u64>> {
        self.inner.total_bytes().map_err(Into::into)
    }

    fn rate(&self) -> PyResult<Option<f64>> {
        self.inner.rate().map_err(Into::into)
    }

    fn final_path(&self) -> PyResult<Option<String>> {
        self.inner.final_path().map_err(Into::into)
    }

    fn is_done(&self) -> PyResult<bool> {
        self.inner.is_done().map_err(Into::into)
    }

    #[pyo3(signature = (show=true, timeout_ms=None, cancel_if_timeout=true))]
    fn wait(
        &self,
        py: Python<'_>,
        show: bool,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> PyResult<Option<String>> {
        let mission = self.inner.clone();
        py.detach(move || mission.wait(show, timeout_ms, cancel_if_timeout))
            .map_err(Into::into)
    }

    fn cancel(&self, py: Python<'_>) -> PyResult<()> {
        let mission = self.inner.clone();
        py.detach(move || mission.cancel())?;
        Ok(())
    }
}

impl PyPage {
    fn page(&self) -> PyResult<&Page> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("page has already been closed"))
    }
}

fn wrap_web_element(py: Python<'_>, element: WebElement) -> PyResult<Py<PyAny>> {
    match element {
        WebElement::Browser(inner) => Ok(Py::new(py, PyElement { inner })?.into_any()),
        WebElement::Session(inner) => Ok(Py::new(py, PySessionElement { inner })?.into_any()),
    }
}

fn session_xpath_result_to_py(py: Python<'_>, item: SessionXPathResult) -> PyResult<Py<PyAny>> {
    match item {
        SessionXPathResult::Document => {
            let dict = PyDict::new(py);
            dict.set_item("type", "document")?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Element(inner) => {
            Ok(Py::new(py, PySessionElement { inner })?.into_any())
        }
        SessionXPathResult::Text(value) => Ok(PyString::new(py, &value).into_any().unbind()),
        SessionXPathResult::Comment(value) => {
            let dict = PyDict::new(py);
            dict.set_item("type", "comment")?;
            dict.set_item("value", value)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Attribute { name, value } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "attribute")?;
            dict.set_item("name", name)?;
            dict.set_item("value", value)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::ProcessingInstruction { target, data } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "processing_instruction")?;
            dict.set_item("target", target)?;
            dict.set_item("data", data)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Doctype {
            name,
            public_id,
            system_id,
        } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "doctype")?;
            dict.set_item("name", name)?;
            dict.set_item("public_id", public_id)?;
            dict.set_item("system_id", system_id)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Boolean(value) => {
            Ok(value.into_pyobject(py)?.to_owned().into_any().unbind())
        }
        SessionXPathResult::Integer(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
        SessionXPathResult::Number(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
        SessionXPathResult::String(value) => Ok(PyString::new(py, &value).into_any().unbind()),
        SessionXPathResult::QName {
            namespace_uri,
            local_name,
            prefix,
        } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "qname")?;
            dict.set_item("namespace_uri", namespace_uri)?;
            dict.set_item("local_name", local_name)?;
            dict.set_item("prefix", prefix)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Function(value) => {
            let dict = PyDict::new(py);
            dict.set_item("type", "function")?;
            dict.set_item("value", value)?;
            Ok(dict.into_any().unbind())
        }
    }
}

fn locator_match_session_to_py(
    py: Python<'_>,
    item: LocatorMatch<SessionElement>,
) -> PyResult<(String, Vec<Py<PySessionElement>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| Py::new(py, PySessionElement { inner }))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((item.locator, elements))
}

fn locator_match_element_to_py(
    py: Python<'_>,
    item: LocatorMatch<Element>,
) -> PyResult<(String, Vec<Py<PyElement>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| Py::new(py, PyElement { inner }))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((item.locator, elements))
}

fn locator_match_web_to_py(
    py: Python<'_>,
    item: LocatorMatch<WebElement>,
) -> PyResult<(String, Vec<Py<PyAny>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| wrap_web_element(py, inner))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((item.locator, elements))
}

fn element_resource_to_py(py: Python<'_>, resource: ElementResource) -> PyResult<Py<PyAny>> {
    match resource {
        ElementResource::Bytes(bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
        ElementResource::Text(text) => Ok(PyString::new(py, &text).into_any().unbind()),
    }
}

fn cookie_entries_to_tuples(entries: Vec<CookieEntry>) -> Vec<(String, String, Option<String>)> {
    entries
        .into_iter()
        .map(|entry| (entry.name, entry.value, entry.domain))
        .collect()
}

fn header_tuples(headers: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut headers = headers
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    headers.sort_by(|left, right| left.0.cmp(&right.0));
    headers
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrowser>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyElement>()?;
    m.add_class::<PySessionPage>()?;
    m.add_class::<PySessionElement>()?;
    m.add_class::<PyWebPage>()?;
    m.add_class::<PyConsole>()?;
    m.add_class::<PyConsoleMessage>()?;
    m.add_class::<PyListener>()?;
    m.add_class::<PyInterceptor>()?;
    m.add_class::<PyInterceptedRequest>()?;
    m.add_class::<PyListenerPacket>()?;
    m.add_class::<PyListenerRequest>()?;
    m.add_class::<PyListenerRequestExtraInfo>()?;
    m.add_class::<PyListenerResponse>()?;
    m.add_class::<PyListenerResponseExtraInfo>()?;
    m.add_class::<PyListenerFailInfo>()?;
    m.add_class::<PyDownloadMission>()?;
    Ok(())
}
