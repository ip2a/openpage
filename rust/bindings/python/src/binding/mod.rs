use openpage::{
    Browser, Document, DocumentElement, Element, InterceptedRequest, Interceptor, LaunchOptions,
    Listener, ListenerPacket, ListenerRequest, ListenerResponse, Page, Response, Session,
    SessionOptions,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::path::Path;

fn error(value: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(value.to_string())
}

#[pyclass(module = "openpage_rs", name = "Browser")]
pub struct PyBrowser {
    inner: Browser,
}
#[pyclass(module = "openpage_rs", name = "Page")]
pub struct PyPage {
    inner: Page,
}
#[pyclass(module = "openpage_rs", name = "Element")]
pub struct PyElement {
    inner: Element,
}
#[pyclass(module = "openpage_rs", name = "Session")]
pub struct PySession {
    inner: Session,
}
#[pyclass(module = "openpage_rs", name = "Response")]
pub struct PyResponse {
    inner: Response,
}
#[pyclass(module = "openpage_rs", name = "Document")]
pub struct PyDocument {
    inner: Document,
}
#[pyclass(module = "openpage_rs", name = "DocumentElement")]
pub struct PyDocumentElement {
    inner: DocumentElement,
}
#[pyclass(module = "openpage_rs", name = "NetworkListener")]
pub struct PyNetworkListener {
    inner: Listener,
}
#[pyclass(module = "openpage_rs", name = "NetworkPacket")]
pub struct PyNetworkPacket {
    inner: ListenerPacket,
}
#[pyclass(module = "openpage_rs", name = "NetworkRequest")]
pub struct PyNetworkRequest {
    inner: ListenerRequest,
}
#[pyclass(module = "openpage_rs", name = "NetworkResponse")]
pub struct PyNetworkResponse {
    inner: ListenerResponse,
}
#[pyclass(module = "openpage_rs", name = "NetworkInterceptor")]
pub struct PyNetworkInterceptor {
    inner: Interceptor,
}
#[pyclass(module = "openpage_rs", name = "InterceptedRequest")]
pub struct PyInterceptedRequest {
    inner: InterceptedRequest,
}

#[pymethods]
impl PyBrowser {
    #[staticmethod]
    #[pyo3(signature = (
        browser_path=None,
        headless=None,
        incognito=None,
        proxy=None,
        user_agent=None,
        download_path=None,
        user_data_path=None,
        no_js=None,
    ))]
    fn launch(
        browser_path: Option<&str>,
        headless: Option<bool>,
        incognito: Option<bool>,
        proxy: Option<&str>,
        user_agent: Option<&str>,
        download_path: Option<&str>,
        user_data_path: Option<&str>,
        no_js: Option<bool>,
    ) -> PyResult<Self> {
        let mut options = LaunchOptions::default();
        if let Some(path) = browser_path {
            options.set_browser_path(path);
        }
        if let Some(value) = headless {
            options.headless(value);
        }
        if let Some(value) = incognito {
            options.incognito(value);
        }
        if let Some(value) = proxy {
            options.set_proxy(value);
        }
        if let Some(value) = user_agent {
            options.set_user_agent(value);
        }
        if let Some(path) = download_path {
            options.set_download_path(path);
        }
        if let Some(path) = user_data_path {
            options.set_user_data_path(path);
        }
        if let Some(value) = no_js {
            options.no_js(value);
        }
        Browser::launch(options)
            .map(|inner| Self { inner })
            .map_err(error)
    }
    #[pyo3(signature = (url=None))]
    fn new_page(&self, url: Option<&str>) -> PyResult<PyPage> {
        self.inner
            .new_page(url)
            .map(|inner| PyPage { inner })
            .map_err(error)
    }
    fn close(&self) -> PyResult<()> {
        self.inner.close().map_err(error)
    }
}

#[pymethods]
impl PyPage {
    fn goto(&self, py: Python<'_>, url: &str) -> PyResult<()> {
        py.detach(|| self.inner.goto(url)).map_err(error)
    }
    #[pyo3(signature = (ignore_cache=false))]
    fn refresh(&self, ignore_cache: bool) -> PyResult<()> {
        self.inner.refresh(ignore_cache).map_err(error)
    }
    #[pyo3(signature = (steps=1))]
    fn back(&self, steps: usize) -> PyResult<bool> {
        self.inner.back(steps).map_err(error)
    }
    #[pyo3(signature = (steps=1))]
    fn forward(&self, steps: usize) -> PyResult<bool> {
        self.inner.forward(steps).map_err(error)
    }
    fn ready_state(&self) -> PyResult<String> {
        self.inner.ready_state().map_err(error)
    }
    fn is_loading(&self) -> PyResult<bool> {
        self.inner.is_loading().map_err(error)
    }
    #[pyo3(signature = (timeout_ms=10_000))]
    fn wait_for_doc_loaded(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_for_doc_loaded(timeout_ms).map_err(error)
    }
    fn scroll_to_top(&self) -> PyResult<()> {
        self.inner.scroll_to_top().map_err(error)
    }
    fn scroll_to_bottom(&self) -> PyResult<()> {
        self.inner.scroll_to_bottom().map_err(error)
    }
    fn scroll_to_location(&self, x: f64, y: f64) -> PyResult<()> {
        self.inner.scroll_to_location(x, y).map_err(error)
    }
    fn scroll_up(&self, pixels: f64) -> PyResult<()> {
        self.inner.scroll_up(pixels).map_err(error)
    }
    fn scroll_down(&self, pixels: f64) -> PyResult<()> {
        self.inner.scroll_down(pixels).map_err(error)
    }
    fn scroll_left(&self, pixels: f64) -> PyResult<()> {
        self.inner.scroll_left(pixels).map_err(error)
    }
    fn scroll_right(&self, pixels: f64) -> PyResult<()> {
        self.inner.scroll_right(pixels).map_err(error)
    }
    fn has_alert(&self) -> PyResult<bool> {
        self.inner.has_alert().map_err(error)
    }
    fn alert_text(&self) -> PyResult<Option<String>> {
        self.inner.alert_text().map_err(error)
    }
    #[pyo3(signature = (accept, prompt_text=None, timeout_ms=10_000))]
    fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Option<String>> {
        self.inner
            .handle_alert(accept, prompt_text, timeout_ms)
            .map_err(error)
    }
    fn tabs_count(&self) -> PyResult<usize> {
        self.inner.tabs_count().map_err(error)
    }
    fn tab_ids(&self) -> PyResult<Vec<String>> {
        self.inner.tab_ids().map_err(error)
    }
    #[pyo3(signature = (url=None, new_window=false, background=false, new_context=false))]
    fn new_page(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> PyResult<PyPage> {
        self.inner
            .new_tab(url, new_window, background, new_context)
            .map(|inner| PyPage { inner })
            .map_err(error)
    }
    fn activate_tab(&self, target: &str) -> PyResult<()> {
        self.inner.activate_tab(target).map_err(error)
    }
    #[pyo3(signature = (target, others=false))]
    fn close_tab(&self, target: &str, others: bool) -> PyResult<usize> {
        self.inner.close_tabs(target, others).map_err(error)
    }
    fn activate(&self) -> PyResult<()> {
        self.inner.activate().map_err(error)
    }
    fn window_id(&self) -> PyResult<i64> {
        self.inner.window_id().map_err(error)
    }
    fn cookie_header(&self) -> PyResult<Option<String>> {
        self.inner.cookie_header().map_err(error)
    }
    #[pyo3(signature = (name, value, url=None, domain=None, path=None))]
    fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .set_cookie(name, value, url, domain, path)
            .map_err(error)
    }
    #[pyo3(signature = (name, url=None, domain=None, path=None))]
    fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .remove_cookie(name, url, domain, path)
            .map_err(error)
    }
    fn clear_cookies(&self) -> PyResult<()> {
        self.inner.clear_cookies().map_err(error)
    }
    fn set_session_storage(&self, item: &str, value: Option<&str>) -> PyResult<()> {
        self.inner.set_session_storage(item, value).map_err(error)
    }
    fn set_local_storage(&self, item: &str, value: Option<&str>) -> PyResult<()> {
        self.inner.set_local_storage(item, value).map_err(error)
    }
    fn download(&self, url: &str) -> PyResult<String> {
        self.inner.download(url).map_err(error)
    }
    fn download_to(&self, url: &str, path: &str) -> PyResult<String> {
        self.inner.download_to(url, Path::new(path)).map_err(error)
    }
    fn active_element(&self) -> PyResult<Option<PyElement>> {
        self.inner
            .active_element()
            .map(|element| element.map(|inner| PyElement { inner }))
            .map_err(error)
    }
    fn remove_element(&self, locator: &str) -> PyResult<bool> {
        self.inner.remove_element(locator).map_err(error)
    }
    fn set_upload_files(&self, files: Vec<String>) -> PyResult<()> {
        self.inner.set_upload_files(files).map_err(error)
    }
    #[pyo3(signature = (locator, files, timeout_ms=None, by_js=false))]
    fn click_to_upload(
        &self,
        locator: &str,
        files: Vec<String>,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<bool> {
        self.inner
            .click_to_upload(locator, files, timeout_ms, by_js)
            .map_err(error)
    }
    #[pyo3(signature = (locator, timeout_ms=None, by_js=false))]
    fn click_for_new_page(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> PyResult<Option<PyPage>> {
        self.inner
            .click_for_new_tab(locator, timeout_ms, by_js)
            .map(|page| page.map(|inner| PyPage { inner }))
            .map_err(error)
    }
    fn zoom_factor(&self) -> PyResult<f64> {
        self.inner.zoom_factor().map_err(error)
    }
    fn set_zoom_factor(&self, factor: f64) -> PyResult<()> {
        self.inner.set_zoom_factor(factor).map_err(error)
    }
    fn reset_zoom_factor(&self) -> PyResult<()> {
        self.inner.reset_zoom_factor().map_err(error)
    }
    fn clipboard_read_text(&self) -> PyResult<String> {
        self.inner.clipboard_read_text().map_err(error)
    }
    fn clipboard_write_text(&self, text: &str) -> PyResult<()> {
        self.inner.clipboard_write_text(text).map_err(error)
    }
    fn url(&self) -> PyResult<String> {
        self.inner.url().map_err(error)
    }
    fn title(&self) -> PyResult<String> {
        self.inner.title().map_err(error)
    }
    fn html(&self) -> PyResult<String> {
        self.inner.html().map_err(error)
    }
    #[pyo3(signature = (locator, timeout_ms=10_000))]
    fn wait_for(&self, locator: &str, timeout_ms: u64) -> PyResult<PyElement> {
        self.inner
            .wait_for(locator, timeout_ms)
            .map(|inner| PyElement { inner })
            .map_err(error)
    }
    #[pyo3(signature = (full_page=false))]
    fn screenshot(&self, py: Python<'_>, full_page: bool) -> PyResult<Py<PyBytes>> {
        self.inner
            .screenshot_bytes(full_page, None, None)
            .map(|bytes| PyBytes::new(py, &bytes).unbind())
            .map_err(error)
    }
    #[pyo3(signature = (path, full_page=false))]
    fn save_screenshot(&self, path: &str, full_page: bool) -> PyResult<()> {
        self.inner
            .save_screenshot(Path::new(path), full_page)
            .map_err(error)
    }
    fn listen(&self) -> PyNetworkListener {
        PyNetworkListener {
            inner: self.inner.listener(),
        }
    }
    fn intercept(&self) -> PyNetworkInterceptor {
        PyNetworkInterceptor {
            inner: self.inner.interceptor(),
        }
    }
    fn close(&self) -> PyResult<()> {
        self.inner.clone().close().map_err(error)
    }
    fn find(&self, locator: &str) -> PyResult<PyElement> {
        self.inner
            .find(locator)
            .map(|inner| PyElement { inner })
            .map_err(error)
    }
    fn find_all(&self, locator: &str) -> PyResult<Vec<PyElement>> {
        self.inner
            .find_all(locator)
            .map(|items| items.into_iter().map(|inner| PyElement { inner }).collect())
            .map_err(error)
    }
    fn snapshot(&self) -> PyResult<PyDocument> {
        self.inner
            .snapshot()
            .map(|inner| PyDocument { inner })
            .map_err(error)
    }
    #[pyo3(signature = (locator, timeout_ms=None))]
    fn click(&self, locator: &str, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.click_with_timeout(locator, timeout_ms),
            None => self.inner.click(locator),
        }
        .map_err(error)
    }
    #[pyo3(signature = (locator, text, timeout_ms=None))]
    fn input(&self, locator: &str, text: &str, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.fill_with_timeout(locator, text, timeout_ms),
            None => self.inner.fill(locator, text),
        }
        .map_err(error)
    }
    fn text(&self, locator: &str) -> PyResult<Option<String>> {
        self.inner.text(locator).map_err(error)
    }
    fn attr(&self, locator: &str, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(locator, name).map_err(error)
    }
}

#[pymethods]
impl PyElement {
    fn find(&self, locator: &str) -> PyResult<PyElement> {
        self.inner
            .find(locator)
            .map(|inner| PyElement { inner })
            .map_err(error)
    }
    fn find_all(&self, locator: &str) -> PyResult<Vec<PyElement>> {
        self.inner
            .find_all(locator)
            .map(|items| items.into_iter().map(|inner| PyElement { inner }).collect())
            .map_err(error)
    }
    #[pyo3(signature = (timeout_ms=None))]
    fn click(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.click_with_timeout(timeout_ms),
            None => self.inner.click(),
        }
        .map_err(error)
    }
    #[pyo3(signature = (timeout_ms=None))]
    fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.clear_with_timeout(timeout_ms),
            None => self.inner.clear(),
        }
        .map_err(error)
    }
    fn press_key(&self, key: &str) -> PyResult<()> {
        self.inner.press_key(key).map_err(error)
    }
    fn focus(&self) -> PyResult<()> {
        self.inner.focus().map_err(error)
    }
    fn submit(&self) -> PyResult<()> {
        self.inner.submit().map_err(error)
    }
    #[pyo3(signature = (timeout_ms=None))]
    fn hover(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.hover_with_timeout(timeout_ms),
            None => self.inner.hover(),
        }
        .map_err(error)
    }
    #[pyo3(signature = (text, timeout_ms=None))]
    fn input(&self, text: &str, timeout_ms: Option<u64>) -> PyResult<()> {
        match timeout_ms {
            Some(timeout_ms) => self.inner.input_with_timeout(text, timeout_ms),
            None => self.inner.input(text),
        }
        .map_err(error)
    }
    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(error)
    }
    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(error)
    }
    fn is_displayed(&self) -> PyResult<bool> {
        self.inner.is_displayed().map_err(error)
    }
    fn is_enabled(&self) -> PyResult<bool> {
        self.inner.is_enabled().map_err(error)
    }
    fn is_alive(&self) -> PyResult<bool> {
        self.inner.is_alive().map_err(error)
    }
    #[pyo3(signature = (timeout_ms=10_000))]
    fn wait_until_displayed(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_displayed(timeout_ms).map_err(error)
    }
    #[pyo3(signature = (timeout_ms=10_000))]
    fn wait_until_hidden(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_hidden(timeout_ms).map_err(error)
    }
}

#[pymethods]
impl PySession {
    #[new]
    fn create() -> PyResult<Self> {
        Session::new(SessionOptions::default())
            .map(|inner| Self { inner })
            .map_err(error)
    }
    fn get(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .get(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn post(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .post(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn post_body(&self, url: &str, body: &str) -> PyResult<PyResponse> {
        self.inner
            .post_body(url, body)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn put(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .put(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn delete(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .delete(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn patch(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .patch(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn head(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .head(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn options(&self, url: &str) -> PyResult<PyResponse> {
        self.inner
            .options(url)
            .map(|inner| PyResponse { inner })
            .map_err(error)
    }
    fn set_header(&self, name: &str, value: &str) -> PyResult<()> {
        self.inner.set_header(name, value).map_err(error)
    }
    fn set_timeout(&self, timeout_secs: u64) -> PyResult<()> {
        self.inner.set_timeout(timeout_secs).map_err(error)
    }
    fn set_user_agent(&self, user_agent: &str) -> PyResult<()> {
        self.inner.set_user_agent(user_agent).map_err(error)
    }
    fn set_download_path(&self, path: &str) -> PyResult<()> {
        self.inner.set_download_path(Path::new(path)).map_err(error)
    }
    fn set_verify(&self, verify: bool) -> PyResult<()> {
        self.inner.set_verify(verify).map_err(error)
    }
    fn set_stream(&self, stream: bool) -> PyResult<()> {
        self.inner.set_stream(stream).map_err(error)
    }
    fn set_max_redirects(&self, max_redirects: Option<usize>) -> PyResult<()> {
        self.inner.set_max_redirects(max_redirects).map_err(error)
    }
}

#[pymethods]
impl PyResponse {
    #[getter]
    fn url(&self) -> Option<String> {
        self.inner.url().map(str::to_string)
    }
    #[getter]
    fn status_code(&self) -> Option<u16> {
        self.inner.status_code()
    }
    #[getter]
    fn text(&self) -> String {
        self.inner.text().to_string()
    }
    #[getter]
    fn content(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.inner.content()).unbind()
    }
    fn is_success(&self) -> bool {
        self.inner.is_success()
    }
    #[getter]
    fn document(&self) -> PyDocument {
        PyDocument {
            inner: self.inner.document(),
        }
    }
}

#[pymethods]
impl PyDocument {
    #[getter]
    fn html(&self) -> String {
        self.inner.html().to_string()
    }
    fn find(&self, locator: &str) -> PyResult<PyDocumentElement> {
        self.inner
            .find(locator)
            .map(|inner| PyDocumentElement { inner })
            .map_err(error)
    }
    fn find_all(&self, locator: &str) -> PyResult<Vec<PyDocumentElement>> {
        self.inner
            .find_all(locator)
            .map(|items| {
                items
                    .into_iter()
                    .map(|inner| PyDocumentElement { inner })
                    .collect()
            })
            .map_err(error)
    }
}

#[pymethods]
impl PyDocumentElement {
    fn find(&self, locator: &str) -> PyResult<PyDocumentElement> {
        self.inner
            .find(locator)
            .map(|inner| PyDocumentElement { inner })
            .map_err(error)
    }
    fn find_all(&self, locator: &str) -> PyResult<Vec<PyDocumentElement>> {
        self.inner
            .find_all(locator)
            .map(|items| {
                items
                    .into_iter()
                    .map(|inner| PyDocumentElement { inner })
                    .collect()
            })
            .map_err(error)
    }
    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(error)
    }
    fn html(&self) -> PyResult<Option<String>> {
        self.inner.html().map_err(error)
    }
    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(error)
    }
}

#[pymethods]
impl PyNetworkListener {
    #[pyo3(signature = (targets=None, is_regex=false, methods=None, resource_types=None))]
    fn start(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> PyResult<()> {
        self.inner
            .start(targets, is_regex, methods, resource_types)
            .map_err(error)
    }
    #[pyo3(signature = (timeout_ms=None))]
    fn wait(&self, py: Python<'_>, timeout_ms: Option<u64>) -> PyResult<PyNetworkPacket> {
        py.detach(|| self.inner.wait_one(timeout_ms))
            .map(|inner| PyNetworkPacket { inner })
            .map_err(error)
    }
    fn clear(&self) -> PyResult<()> {
        self.inner.clear().map_err(error)
    }
    fn stop(&self) -> PyResult<()> {
        self.inner.stop().map_err(error)
    }
    #[getter]
    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(error)
    }
}

#[pymethods]
impl PyNetworkPacket {
    #[getter]
    fn url(&self) -> String {
        self.inner.url.clone()
    }
    #[getter]
    fn method(&self) -> String {
        self.inner.method.clone()
    }
    #[getter]
    fn resource_type(&self) -> Option<String> {
        self.inner.resource_type.clone()
    }
    #[getter]
    fn is_failed(&self) -> bool {
        self.inner.is_failed
    }
    #[getter]
    fn request(&self) -> PyNetworkRequest {
        PyNetworkRequest {
            inner: self.inner.request.clone(),
        }
    }
    #[getter]
    fn response(&self) -> Option<PyNetworkResponse> {
        self.inner
            .response
            .clone()
            .map(|inner| PyNetworkResponse { inner })
    }
}

#[pymethods]
impl PyNetworkRequest {
    #[getter]
    fn url(&self) -> String {
        self.inner.url.clone()
    }
    #[getter]
    fn method(&self) -> String {
        self.inner.method.clone()
    }
    #[getter]
    fn headers(&self) -> std::collections::HashMap<String, String> {
        self.inner.headers.clone()
    }
    #[getter]
    fn post_data(&self) -> Option<String> {
        self.inner.post_data.clone()
    }
    #[getter]
    fn params(&self) -> std::collections::HashMap<String, String> {
        self.inner.params()
    }
}

#[pymethods]
impl PyNetworkResponse {
    #[getter]
    fn url(&self) -> String {
        self.inner.url.clone()
    }
    #[getter]
    fn status(&self) -> i64 {
        self.inner.status
    }
    #[getter]
    fn status_text(&self) -> String {
        self.inner.status_text.clone()
    }
    #[getter]
    fn headers(&self) -> std::collections::HashMap<String, String> {
        self.inner.headers.clone()
    }
    #[getter]
    fn mime_type(&self) -> String {
        self.inner.mime_type.clone()
    }
    #[getter]
    fn body(&self, py: Python<'_>) -> PyResult<Option<Py<PyBytes>>> {
        self.inner
            .body_bytes()
            .map(|body| body.map(|body| PyBytes::new(py, &body).unbind()))
            .map_err(error)
    }
}

#[pymethods]
impl PyNetworkInterceptor {
    #[pyo3(signature = (targets=None, is_regex=false, methods=None, resource_types=None))]
    fn start(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> PyResult<()> {
        self.inner
            .start(targets, is_regex, methods, resource_types)
            .map_err(error)
    }
    #[pyo3(signature = (timeout_ms=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
    ) -> PyResult<Option<PyInterceptedRequest>> {
        py.detach(|| self.inner.wait(timeout_ms))
            .map(|request| request.map(|inner| PyInterceptedRequest { inner }))
            .map_err(error)
    }
    fn stop(&self) -> PyResult<()> {
        self.inner.stop().map_err(error)
    }
    #[getter]
    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(error)
    }
}

#[pymethods]
impl PyInterceptedRequest {
    #[getter]
    fn url(&self) -> String {
        self.inner.url()
    }
    #[getter]
    fn method(&self) -> String {
        self.inner.method()
    }
    #[getter]
    fn headers(&self) -> std::collections::HashMap<String, String> {
        self.inner.headers()
    }
    #[getter]
    fn resource_type(&self) -> String {
        self.inner.resource_type()
    }
    #[pyo3(signature = (url=None, method=None, headers=None, post_data=None))]
    fn continue_request(
        &self,
        url: Option<&str>,
        method: Option<&str>,
        headers: Option<std::collections::HashMap<String, String>>,
        post_data: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .continue_request(url, method, headers, post_data)
            .map_err(error)
    }
    fn abort(&self) -> PyResult<()> {
        self.inner.abort().map_err(error)
    }
    #[pyo3(signature = (status, body=None, headers=None, phrase=None))]
    fn fulfill(
        &self,
        status: i64,
        body: Option<&[u8]>,
        headers: Option<std::collections::HashMap<String, String>>,
        phrase: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .fulfill(status, body, headers, phrase)
            .map_err(error)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrowser>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyElement>()?;
    m.add_class::<PySession>()?;
    m.add_class::<PyResponse>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyDocumentElement>()?;
    m.add_class::<PyNetworkListener>()?;
    m.add_class::<PyNetworkPacket>()?;
    m.add_class::<PyNetworkRequest>()?;
    m.add_class::<PyNetworkResponse>()?;
    m.add_class::<PyNetworkInterceptor>()?;
    m.add_class::<PyInterceptedRequest>()?;
    Ok(())
}
