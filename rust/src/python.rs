use std::collections::HashMap;
use std::path::PathBuf;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::browser::{Browser, LaunchOptions};
use crate::element::Element;
use crate::error::OpenPageError;
use crate::listener::{
    Listener, ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerResponse,
};
use crate::page::Page;
use crate::session::{CookieEntry, SessionElement, SessionOptions, SessionPage};
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

#[pyclass(module = "openpage_rs", name = "ListenerPacket")]
pub struct PyListenerPacket {
    inner: ListenerPacket,
}

#[pyclass(module = "openpage_rs", name = "ListenerRequest")]
pub struct PyListenerRequest {
    inner: ListenerRequest,
}

#[pyclass(module = "openpage_rs", name = "ListenerResponse")]
pub struct PyListenerResponse {
    inner: ListenerResponse,
}

#[pyclass(module = "openpage_rs", name = "ListenerFailInfo")]
pub struct PyListenerFailInfo {
    inner: ListenerFailInfo,
}

#[pymethods]
impl PyBrowser {
    #[staticmethod]
    #[pyo3(signature = (browser_path=None, download_path=None, headless=true, user_data_dir=None, width=1280, height=900, no_sandbox=false))]
    fn launch(
        py: Python<'_>,
        browser_path: Option<String>,
        download_path: Option<String>,
        headless: bool,
        user_data_dir: Option<String>,
        width: u32,
        height: u32,
        no_sandbox: bool,
    ) -> PyResult<Self> {
        let options = LaunchOptions {
            browser_path: browser_path.map(PathBuf::from),
            download_path: download_path.map(PathBuf::from),
            user_data_dir: user_data_dir.map(PathBuf::from),
            headless,
            width,
            height,
            no_sandbox,
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

    fn download_path(&self) -> PyResult<Option<String>> {
        self.inner.download_path().map_err(Into::into)
    }

    fn set_download_path(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let browser = self.inner.clone();
        let path = path.to_string();
        py.detach(move || browser.set_download_path(&path))?;
        Ok(())
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

    #[pyo3(signature = (path, full_page=true))]
    fn save_screenshot(&self, py: Python<'_>, path: &str, full_page: bool) -> PyResult<()> {
        let page = self.page()?.clone();
        let path = path.to_string();
        py.detach(move || page.save_screenshot(&path, full_page))?;
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

    fn user_agent(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.page()?.clone();
        py.detach(move || page.user_agent()).map_err(Into::into)
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

    fn input(&self, text: &str) -> PyResult<()> {
        self.inner.input(text)?;
        Ok(())
    }

    fn clear(&self) -> PyResult<()> {
        self.inner.clear()?;
        Ok(())
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

    fn save_screenshot(&self, path: &str) -> PyResult<()> {
        self.inner.save_screenshot(path)?;
        Ok(())
    }

    fn run_js(&self, script: &str) -> PyResult<String> {
        let value = self.inner.run_js(script)?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
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
}

#[pymethods]
impl PySessionPage {
    #[staticmethod]
    #[pyo3(signature = (timeout_secs=15, user_agent=None))]
    fn create(py: Python<'_>, timeout_secs: u64, user_agent: Option<String>) -> PyResult<Self> {
        let options = SessionOptions {
            timeout_secs,
            user_agent,
        };
        let inner = py.detach(move || SessionPage::new(options))?;
        Ok(Self { inner })
    }

    fn get(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.get(&url)).map_err(Into::into)
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

    fn set_user_agent(&self, py: Python<'_>, user_agent: Option<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_user_agent(user_agent))?;
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

    fn parent(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent()?,
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
    #[pyo3(signature = (mode="d", browser_path=None, download_path=None, headless=true, user_data_dir=None, width=1280, height=900, no_sandbox=false, timeout_secs=15, user_agent=None))]
    fn create(
        py: Python<'_>,
        mode: &str,
        browser_path: Option<String>,
        download_path: Option<String>,
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
            user_data_dir: user_data_dir.map(PathBuf::from),
            headless,
            width,
            height,
            no_sandbox,
        };
        let session_options = SessionOptions {
            timeout_secs,
            user_agent,
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

    fn listener(&self, py: Python<'_>) -> PyResult<Py<PyListener>> {
        let listener = self.inner.listener();
        Py::new(py, PyListener { inner: listener })
    }

    fn get(&self, py: Python<'_>, url: &str) -> PyResult<bool> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.get(&url)).map_err(Into::into)
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

    fn run_js(&self, py: Python<'_>, expression: &str) -> PyResult<String> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        let value = py.detach(move || page.run_js(&expression))?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
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
impl PyListenerPacket {
    fn target(&self) -> Option<String> {
        self.inner.matched_target.clone()
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
    m.add_class::<PyListener>()?;
    m.add_class::<PyListenerPacket>()?;
    m.add_class::<PyListenerRequest>()?;
    m.add_class::<PyListenerResponse>()?;
    m.add_class::<PyListenerFailInfo>()?;
    Ok(())
}
