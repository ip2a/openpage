use super::*;

#[pymethods]
impl PyElement {
    fn click(&self) -> PyResult<()> {
        self.inner.click().map_err(py_err)?;
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
            .map_err(py_err)
    }

    #[pyo3(signature = (times=2))]
    fn click_multi(&self, times: u32) -> PyResult<()> {
        self.inner.click_multi(times).map_err(py_err)
    }

    fn click_left(&self) -> PyResult<()> {
        self.inner.click_left().map_err(py_err)
    }

    fn click_middle(&self) -> PyResult<()> {
        self.inner.click_middle().map_err(py_err)
    }

    fn click_right(&self) -> PyResult<()> {
        self.inner.click_right().map_err(py_err)
    }

    #[pyo3(signature = (text, clear=false, by_js=false))]
    fn input(&self, text: &str, clear: bool, by_js: bool) -> PyResult<()> {
        self.inner
            .input_with_options(text, clear, by_js)
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (values, clear=false, by_js=false))]
    fn input_keys(&self, values: Vec<String>, clear: bool, by_js: bool) -> PyResult<()> {
        self.inner
            .input_keys_with_options(&values, clear, by_js)
            .map_err(py_err)?;
        Ok(())
    }

    fn clear(&self) -> PyResult<()> {
        self.inner.clear().map_err(py_err)?;
        Ok(())
    }

    fn focus(&self) -> PyResult<()> {
        self.inner.focus().map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (offset_x=None, offset_y=None))]
    fn hover(&self, offset_x: Option<f64>, offset_y: Option<f64>) -> PyResult<()> {
        if offset_x.is_none() && offset_y.is_none() {
            self.inner.hover().map_err(py_err)
        } else {
            self.inner
                .hover_with_offset(offset_x, offset_y)
                .map_err(py_err)
        }
    }

    fn press_key(&self, key: &str) -> PyResult<()> {
        self.inner.press_key(key).map_err(py_err)?;
        Ok(())
    }

    fn text(&self) -> PyResult<Option<String>> {
        self.inner.text().map_err(py_err)
    }

    fn html(&self) -> PyResult<Option<String>> {
        self.inner.html().map_err(py_err)
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

    #[pyo3(signature = (timeout_ms=10000, base64_to_bytes=true))]
    fn src(
        &self,
        py: Python<'_>,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .src(timeout_ms, base64_to_bytes)
            .map_err(py_err)?
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
        let saved = self
            .inner
            .save(path.as_deref(), name, timeout_ms, rename)
            .map_err(py_err)?;
        Ok(saved.to_string_lossy().into_owned())
    }

    fn save_screenshot(&self, path: &str) -> PyResult<()> {
        self.inner.save_screenshot(path).map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (offset_x=0.0, offset_y=0.0, duration_secs=0.5))]
    fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> PyResult<()> {
        self.inner
            .drag(offset_x, offset_y, duration_secs)
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (target, duration_secs=0.5))]
    fn drag_to(&self, target: &Bound<'_, PyAny>, duration_secs: f64) -> PyResult<()> {
        if let Ok(target) = target.extract::<PyRef<'_, PyElement>>() {
            self.inner
                .drag_to(&target.inner, duration_secs)
                .map_err(py_err)?;
            return Ok(());
        }

        if let Ok((x, y)) = target.extract::<(f64, f64)>() {
            self.inner
                .drag_to_point(x, y, duration_secs)
                .map_err(py_err)?;
            return Ok(());
        }

        if let Ok(coords) = target.extract::<Vec<f64>>() {
            if coords.len() == 2 {
                self.inner
                    .drag_to_point(coords[0], coords[1], duration_secs)
                    .map_err(py_err)?;
                return Ok(());
            }
        }

        Err(PyTypeError::new_err(
            "drag_to() expects an Element or a 2-item coordinate sequence",
        ))
    }

    fn run_js(&self, script: &str) -> PyResult<String> {
        let value = self.inner.run_js(script).map_err(py_err)?;
        serde_json::to_string(&value)
            .map_err(|err| py_err(OpenPageError::Serialization(err.to_string())))
    }

    fn is_selected(&self) -> PyResult<bool> {
        self.inner.is_selected().map_err(py_err)
    }

    fn is_checked(&self) -> PyResult<bool> {
        self.inner.is_checked().map_err(py_err)
    }

    fn is_displayed(&self) -> PyResult<bool> {
        self.inner.is_displayed().map_err(py_err)
    }

    fn is_enabled(&self) -> PyResult<bool> {
        self.inner.is_enabled().map_err(py_err)
    }

    fn is_alive(&self) -> PyResult<bool> {
        self.inner.is_alive().map_err(py_err)
    }

    fn has_rect(&self) -> PyResult<Option<Vec<(f64, f64)>>> {
        self.inner.rect_corners().map_err(py_err)
    }

    fn is_in_viewport(&self) -> PyResult<bool> {
        self.inner.is_in_viewport().map_err(py_err)
    }

    fn is_whole_in_viewport(&self) -> PyResult<bool> {
        self.inner.is_whole_in_viewport().map_err(py_err)
    }

    fn is_covered(&self) -> PyResult<bool> {
        self.inner.is_covered().map_err(py_err)
    }

    fn is_clickable(&self) -> PyResult<bool> {
        self.inner.is_clickable().map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_displayed(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_displayed(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_hidden(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_hidden(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_enabled(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_enabled(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_disabled(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_disabled(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_deleted(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_deleted(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_clickable(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_clickable(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_has_rect(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_has_rect(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_covered(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner.wait_until_covered(timeout_ms).map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_not_covered(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_not_covered(timeout_ms)
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_disabled_or_deleted(timeout_ms)
            .map_err(py_err)
    }

    #[pyo3(signature = (timeout_ms=10000))]
    fn wait_until_stop_moving(&self, timeout_ms: u64) -> PyResult<bool> {
        self.inner
            .wait_until_stop_moving(timeout_ms)
            .map_err(py_err)
    }

    fn find(&self, py: Python<'_>, locator: &str) -> PyResult<Py<PyElement>> {
        Py::new(
            py,
            PyElement {
                inner: self.inner.find(locator).map_err(py_err)?,
            },
        )
    }

    fn find_all(&self, py: Python<'_>, locator: &str) -> PyResult<Vec<Py<PyElement>>> {
        self.inner
            .find_all(locator)
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PyElement { inner }))
            .collect()
    }

    fn snapshot_query_xpath(&self, py: Python<'_>, expression: &str) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .snapshot_query_xpath(expression)
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
    ) -> PyResult<Vec<(String, Vec<Py<PyElement>>)>> {
        self.inner
            .find_locators(&locators, any_one, first_match_only)
            .map_err(py_err)?
            .into_iter()
            .map(|item| locator_match_element_to_py(py, item))
            .collect()
    }
}
