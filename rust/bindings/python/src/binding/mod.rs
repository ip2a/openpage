use openpage::{
    Browser, Document, DocumentElement, Element, LaunchOptions, Page, Response, Session,
    SessionOptions,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

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

#[pymethods]
impl PyBrowser {
    #[staticmethod]
    fn launch() -> PyResult<Self> {
        Browser::launch(LaunchOptions::default())
            .map(|inner| Self { inner })
            .map_err(error)
    }
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
    fn goto(&self, url: &str) -> PyResult<()> {
        self.inner.goto(url).map_err(error)
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
    fn click(&self, locator: &str) -> PyResult<()> {
        self.inner.click(locator).map_err(error)
    }
    fn input(&self, locator: &str, text: &str) -> PyResult<()> {
        self.inner.fill(locator, text).map_err(error)
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
    fn click(&self) -> PyResult<()> {
        self.inner.click().map_err(error)
    }
    fn input(&self, text: &str) -> PyResult<()> {
        self.inner.input(text).map_err(error)
    }
    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(error)
    }
    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(error)
    }
}

#[pymethods]
impl PySession {
    #[staticmethod]
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

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrowser>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyElement>()?;
    m.add_class::<PySession>()?;
    m.add_class::<PyResponse>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyDocumentElement>()?;
    Ok(())
}
