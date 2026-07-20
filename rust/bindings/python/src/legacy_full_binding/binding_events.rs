use super::*;

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
        py.detach(move || listener.start(targets, is_regex, methods, resource_types))
            .map_err(py_err)?;
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
        py.detach(move || listener.set_targets(targets, is_regex, methods, resource_types))
            .map_err(py_err)?;
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
        py.detach(move || listener.wait(count, timeout_ms, fit_count))
            .map_err(py_err)?
            .into_iter()
            .map(|inner| Py::new(py, PyListenerPacket { inner }))
            .collect()
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.clear()).map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (clear=true))]
    fn pause(&self, py: Python<'_>, clear: bool) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.pause(clear)).map_err(py_err)?;
        Ok(())
    }

    fn resume(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.resume()).map_err(py_err)?;
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
            .map_err(py_err)
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let listener = self.inner.clone();
        py.detach(move || listener.stop()).map_err(py_err)?;
        Ok(())
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(py_err)
    }
}

#[pymethods]
impl PyConsole {
    fn start(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.start()).map_err(py_err)?;
        Ok(())
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.stop()).map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
    ) -> PyResult<Option<Py<PyConsoleMessage>>> {
        let console = self.inner.clone();
        py.detach(move || console.wait(timeout_ms))
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyConsoleMessage { inner }))
            .transpose()
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(py_err)
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let console = self.inner.clone();
        py.detach(move || console.clear()).map_err(py_err)?;
        Ok(())
    }

    fn messages(&self, py: Python<'_>) -> PyResult<Vec<Py<PyConsoleMessage>>> {
        let console = self.inner.clone();
        py.detach(move || console.messages())
            .map_err(py_err)?
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
        py.detach(move || interceptor.start(targets, is_regex, methods, resource_types))
            .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (timeout_ms=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout_ms: Option<u64>,
    ) -> PyResult<Option<Py<PyInterceptedRequest>>> {
        let interceptor = self.inner.clone();
        py.detach(move || interceptor.wait(timeout_ms))
            .map_err(py_err)?
            .map(|inner| Py::new(py, PyInterceptedRequest { inner }))
            .transpose()
    }

    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let interceptor = self.inner.clone();
        py.detach(move || interceptor.stop()).map_err(py_err)?;
        Ok(())
    }

    fn is_listening(&self) -> PyResult<bool> {
        self.inner.is_listening().map_err(py_err)
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
        })
        .map_err(py_err)?;
        Ok(())
    }

    #[pyo3(signature = (reason="BlockedByClient"))]
    fn fail(&self, py: Python<'_>, reason: &str) -> PyResult<()> {
        let request = self.inner.clone();
        let reason = reason
            .parse()
            .map_err(OpenPageError::BrowserOperation)
            .map_err(py_err)?;
        py.detach(move || request.fail(reason)).map_err(py_err)?;
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
        })
        .map_err(py_err)?;
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
            .map_err(|err| py_err(OpenPageError::Serialization(err.to_string())))
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
            .map_err(|err| py_err(OpenPageError::Serialization(err.to_string())))
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
        self.inner.tab_id().map_err(py_err)
    }

    fn url(&self) -> PyResult<String> {
        self.inner.url().map_err(py_err)
    }

    fn folder(&self) -> PyResult<String> {
        self.inner.folder().map_err(py_err)
    }

    fn name(&self) -> PyResult<String> {
        self.inner.name().map_err(py_err)
    }

    fn suggested_filename(&self) -> PyResult<String> {
        self.inner.suggested_filename().map_err(py_err)
    }

    fn tmp_path(&self) -> PyResult<String> {
        self.inner.tmp_path().map_err(py_err)
    }

    fn state(&self) -> PyResult<String> {
        self.inner.state().map_err(py_err)
    }

    fn received_bytes(&self) -> PyResult<u64> {
        self.inner.received_bytes().map_err(py_err)
    }

    fn total_bytes(&self) -> PyResult<Option<u64>> {
        self.inner.total_bytes().map_err(py_err)
    }

    fn rate(&self) -> PyResult<Option<f64>> {
        self.inner.rate().map_err(py_err)
    }

    fn final_path(&self) -> PyResult<Option<String>> {
        self.inner.final_path().map_err(py_err)
    }

    fn is_done(&self) -> PyResult<bool> {
        self.inner.is_done().map_err(py_err)
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
            .map_err(py_err)
    }

    fn cancel(&self, py: Python<'_>) -> PyResult<()> {
        let mission = self.inner.clone();
        py.detach(move || mission.cancel()).map_err(py_err)?;
        Ok(())
    }
}
