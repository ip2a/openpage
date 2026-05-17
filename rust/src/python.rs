use std::path::PathBuf;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::browser::{Browser, LaunchOptions};
use crate::element::Element;
use crate::error::OpenPageError;
use crate::page::Page;

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

#[pymethods]
impl PyBrowser {
    #[staticmethod]
    #[pyo3(signature = (browser_path=None, headless=true, user_data_dir=None, width=1280, height=900, no_sandbox=false))]
    fn launch(
        browser_path: Option<String>,
        headless: bool,
        user_data_dir: Option<String>,
        width: u32,
        height: u32,
        no_sandbox: bool,
    ) -> PyResult<Self> {
        let options = LaunchOptions {
            browser_path: browser_path.map(PathBuf::from),
            user_data_dir: user_data_dir.map(PathBuf::from),
            headless,
            width,
            height,
            no_sandbox,
        };
        Ok(Self {
            inner: Browser::launch(options)?,
        })
    }

    #[pyo3(signature = (url=None))]
    fn new_page(&self, py: Python<'_>, url: Option<&str>) -> PyResult<Py<PyPage>> {
        Py::new(
            py,
            PyPage {
                inner: Some(self.inner.new_page(url)?),
            },
        )
    }

    fn close(&self) -> PyResult<()> {
        self.inner.close()?;
        Ok(())
    }

    fn version(&self) -> PyResult<String> {
        self.inner.version().map_err(Into::into)
    }

    fn tabs_count(&self) -> PyResult<usize> {
        self.inner.tabs_count().map_err(Into::into)
    }

    fn tab_ids(&self) -> PyResult<Vec<String>> {
        self.inner.tab_ids().map_err(Into::into)
    }

    fn get_page(&self, py: Python<'_>, target_id: &str) -> PyResult<Py<PyPage>> {
        Py::new(
            py,
            PyPage {
                inner: Some(self.inner.get_page(target_id)?),
            },
        )
    }
}

#[pymethods]
impl PyPage {
    fn goto(&self, url: &str) -> PyResult<()> {
        self.page()?.goto(url)?;
        Ok(())
    }

    fn url(&self) -> PyResult<String> {
        self.page()?.url().map_err(Into::into)
    }

    fn title(&self) -> PyResult<String> {
        self.page()?.title().map_err(Into::into)
    }

    fn target_id(&self) -> PyResult<String> {
        Ok(self.page()?.target_id())
    }

    fn html(&self) -> PyResult<String> {
        self.page()?.html().map_err(Into::into)
    }

    fn evaluate(&self, expression: &str) -> PyResult<String> {
        let value = self.page()?.evaluate(expression)?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    #[pyo3(signature = (locator, timeout_ms=10000))]
    fn wait_for(&self, py: Python<'_>, locator: &str, timeout_ms: u64) -> PyResult<Py<PyElement>> {
        Py::new(
            py,
            PyElement {
                inner: self.page()?.wait_for(locator, timeout_ms)?,
            },
        )
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyElement>> {
        Py::new(
            py,
            PyElement {
                inner: self.page()?.find(locator)?,
            },
        )
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyElement>>> {
        self.page()?
            .find_all(locator)?
            .into_iter()
            .map(|inner| Py::new(py, PyElement { inner }))
            .collect()
    }

    fn click(&self, locator: &str) -> PyResult<()> {
        self.page()?.click(locator)?;
        Ok(())
    }

    fn fill(&self, locator: &str, text: &str) -> PyResult<()> {
        self.page()?.fill(locator, text)?;
        Ok(())
    }

    fn text(&self, locator: &str) -> PyResult<Option<String>> {
        self.page()?.text(locator).map_err(Into::into)
    }

    fn attr(&self, locator: &str, name: &str) -> PyResult<Option<String>> {
        self.page()?.attr(locator, name).map_err(Into::into)
    }

    #[pyo3(signature = (path, full_page=true))]
    fn save_screenshot(&self, path: &str, full_page: bool) -> PyResult<()> {
        self.page()?.save_screenshot(path, full_page)?;
        Ok(())
    }

    fn save_pdf(&self, path: &str) -> PyResult<()> {
        self.page()?.save_pdf(path)?;
        Ok(())
    }

    fn run_js(&self, expression: &str) -> PyResult<String> {
        let value = self.page()?.run_js(expression)?;
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()).into())
    }

    fn close(&mut self) -> PyResult<()> {
        if let Some(page) = self.inner.take() {
            page.close()?;
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

impl PyPage {
    fn page(&self) -> PyResult<&Page> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("page has already been closed"))
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrowser>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyElement>()?;
    Ok(())
}
