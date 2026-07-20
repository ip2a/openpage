use super::*;

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
        let inner = py
            .detach(move || SessionPage::new(options))
            .map_err(py_err)?;
        Ok(Self { inner })
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

    fn status_code(&self, py: Python<'_>) -> PyResult<Option<u16>> {
        let page = self.inner.clone();
        py.detach(move || page.status_code()).map_err(py_err)
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

    fn html(&self, py: Python<'_>) -> PyResult<String> {
        let page = self.inner.clone();
        py.detach(move || page.html()).map_err(py_err)
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

    fn title(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.title()).map_err(py_err)
    }

    fn user_agent(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        py.detach(move || page.user_agent()).map_err(py_err)
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

    fn set_user_agent(&self, py: Python<'_>, user_agent: Option<String>) -> PyResult<()> {
        let page = self.inner.clone();
        py.detach(move || page.set_user_agent(user_agent))
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

    fn cookie_header(&self, py: Python<'_>, url: &str) -> PyResult<Option<String>> {
        let page = self.inner.clone();
        let url = url.to_string();
        py.detach(move || page.cookie_header(&url)).map_err(py_err)
    }

    fn cookies(&self, py: Python<'_>) -> PyResult<Vec<(String, String, Option<String>)>> {
        let page = self.inner.clone();
        py.detach(move || page.cookies())
            .map(cookie_entries_to_tuples)
            .map_err(py_err)
    }

    fn set_cookie_header(&self, py: Python<'_>, url: &str, cookie_header: &str) -> PyResult<()> {
        let page = self.inner.clone();
        let url = url.to_string();
        let cookie_header = cookie_header.to_string();
        py.detach(move || page.set_cookie_header(&url, &cookie_header))
            .map_err(py_err)?;
        Ok(())
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        let element = py.detach(move || page.find(&locator)).map_err(py_err)?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PySessionElement>>> {
        let page = self.inner.clone();
        let locator = locator.to_string();
        py.detach(move || page.find_all(&locator))
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        let page = self.inner.clone();
        let expression = expression.to_string();
        py.detach(move || page.query_xpath(&expression))
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
    ) -> PyResult<Vec<(String, Vec<Py<PySessionElement>>)>> {
        let page = self.inner.clone();
        py.detach(move || page.find_locators(&locators, any_one, first_match_only))
            .map_err(py_err)?
            .into_iter()
            .map(|item| locator_match_session_to_py(py, item))
            .collect()
    }

    fn root(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        let page = self.inner.clone();
        let element = py.detach(move || page.root()).map_err(py_err)?;
        Py::new(py, PySessionElement { inner: element })
    }
}

#[pymethods]
impl PySessionElement {
    fn tag(&self) -> PyResult<String> {
        self.inner.tag().map_err(py_err)
    }

    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(py_err)
    }

    fn html(&self) -> PyResult<Option<String>> {
        self.inner.html().map_err(py_err)
    }

    fn inner_html(&self) -> PyResult<Option<String>> {
        self.inner.inner_html().map_err(py_err)
    }

    fn raw_text(&self) -> PyResult<Option<String>> {
        self.inner.raw_text().map_err(py_err)
    }

    fn attrs(&self) -> PyResult<Vec<(String, String)>> {
        self.inner.attrs().map_err(py_err)
    }

    fn attr(&self, name: &str) -> PyResult<Option<String>> {
        self.inner.attr(name).map_err(py_err)
    }

    fn link(&self) -> PyResult<Option<String>> {
        self.inner.link().map_err(py_err)
    }

    fn child_count(&self) -> PyResult<usize> {
        self.inner.child_count().map_err(py_err)
    }

    fn css_path(&self) -> PyResult<String> {
        self.inner.css_path().map_err(py_err)
    }

    fn xpath(&self) -> PyResult<String> {
        self.inner.xpath().map_err(py_err)
    }

    fn comments(&self) -> PyResult<Vec<String>> {
        self.inner.comments().map_err(py_err)
    }

    #[pyo3(signature = (text_node_only=false))]
    fn texts(&self, text_node_only: bool) -> PyResult<Vec<String>> {
        self.inner.texts(text_node_only).map_err(py_err)
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PySessionElement>> {
        let element = self.inner.find(locator).map_err(py_err)?;
        Py::new(py, PySessionElement { inner: element })
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PySessionElement>>> {
        self.inner
            .find_all(locator)
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    fn query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .query_xpath(expression)
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
    ) -> PyResult<Vec<(String, Vec<Py<PySessionElement>>)>> {
        self.inner
            .find_locators(&locators, any_one, first_match_only)
            .map_err(py_err)?
            .into_iter()
            .map(|item| locator_match_session_to_py(py, item))
            .collect()
    }

    fn parent(&self, py: Python<'_>) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent().map_err(py_err)?,
            },
        )
    }

    fn parent_level(&self, py: Python<'_>, level: usize) -> PyResult<Py<PySessionElement>> {
        Py::new(
            py,
            PySessionElement {
                inner: self.inner.parent_level(level).map_err(py_err)?,
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
                inner: self.inner.parent_with(locator, index).map_err(py_err)?,
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
                inner: self.inner.child_with(normalized, index).map_err(py_err)?,
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
            .children_with(normalized)
            .map_err(py_err)?
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
                inner: self.inner.prev_with(normalized, index).map_err(py_err)?,
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
                inner: self.inner.next_with(normalized, index).map_err(py_err)?,
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
                inner: self.inner.before_with(normalized, index).map_err(py_err)?,
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
                inner: self.inner.after_with(normalized, index).map_err(py_err)?,
            },
        )
    }

    #[pyo3(signature = (locator=None))]
    fn prevs(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .prevs_with(normalized)
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None))]
    fn nexts(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .nexts_with(normalized)
            .map_err(py_err)?
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
            .befores_with(normalized)
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }

    #[pyo3(signature = (locator=None))]
    fn afters(&self, py: Python<'_>, locator: Option<&str>) -> PyResult<Vec<Py<PySessionElement>>> {
        let normalized = locator.map(str::trim).filter(|locator| !locator.is_empty());
        self.inner
            .afters_with(normalized)
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PySessionElement { inner }))
            .collect()
    }
}
