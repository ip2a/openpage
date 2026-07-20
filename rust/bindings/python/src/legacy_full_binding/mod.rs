use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyString};

use openpage::browser::{Browser, DownloadFileExistsMode, LaunchOptions, LoadMode};
use openpage::console::{Console, ConsoleMessage};
use openpage::download::DownloadMission;
use openpage::element::{Element, ElementResource};
use openpage::error::OpenPageError;
use openpage::intercept::{InterceptedRequest, Interceptor};
use openpage::listener::{
    Listener, ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerRequestExtraInfo,
    ListenerResponse, ListenerResponseExtraInfo,
};
use openpage::locator::LocatorMatch;
use openpage::page::Page;
use openpage::session::{
    CookieEntry, SessionElement, SessionOptions, SessionPage, SessionXPathResult,
};
use openpage::webpage::{WebElement, WebMode, WebPage};

pub(crate) fn py_err(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[pyclass(module = "openpage_rs", name = "Browser")]
pub(crate) struct PyBrowser {
    inner: Browser,
}

#[pyclass(module = "openpage_rs", name = "Page")]
pub(crate) struct PyPage {
    inner: Option<Page>,
}

#[pyclass(module = "openpage_rs", name = "Element")]
pub(crate) struct PyElement {
    inner: Element,
}

#[pyclass(module = "openpage_rs", name = "SessionPage")]
pub(crate) struct PySessionPage {
    inner: SessionPage,
}

#[pyclass(module = "openpage_rs", name = "SessionElement")]
pub(crate) struct PySessionElement {
    inner: SessionElement,
}

#[pyclass(module = "openpage_rs", name = "WebPage")]
pub(crate) struct PyWebPage {
    inner: WebPage,
}

#[pyclass(module = "openpage_rs", name = "Listener")]
pub(crate) struct PyListener {
    inner: Listener,
}

#[pyclass(module = "openpage_rs", name = "Console")]
pub(crate) struct PyConsole {
    inner: Console,
}

#[pyclass(module = "openpage_rs", name = "ConsoleMessage")]
pub(crate) struct PyConsoleMessage {
    inner: ConsoleMessage,
}

#[pyclass(module = "openpage_rs", name = "Interceptor")]
pub(crate) struct PyInterceptor {
    inner: Interceptor,
}

#[pyclass(module = "openpage_rs", name = "InterceptedRequest")]
pub(crate) struct PyInterceptedRequest {
    inner: InterceptedRequest,
}

#[pyclass(module = "openpage_rs", name = "ListenerPacket")]
pub(crate) struct PyListenerPacket {
    inner: ListenerPacket,
}

#[pyclass(module = "openpage_rs", name = "ListenerRequest")]
pub(crate) struct PyListenerRequest {
    inner: ListenerRequest,
}

#[pyclass(module = "openpage_rs", name = "ListenerRequestExtraInfo")]
pub(crate) struct PyListenerRequestExtraInfo {
    inner: ListenerRequestExtraInfo,
}

#[pyclass(module = "openpage_rs", name = "ListenerResponse")]
pub(crate) struct PyListenerResponse {
    inner: ListenerResponse,
}

#[pyclass(module = "openpage_rs", name = "ListenerResponseExtraInfo")]
pub(crate) struct PyListenerResponseExtraInfo {
    inner: ListenerResponseExtraInfo,
}

#[pyclass(module = "openpage_rs", name = "ListenerFailInfo")]
pub(crate) struct PyListenerFailInfo {
    inner: ListenerFailInfo,
}

#[pyclass(module = "openpage_rs", name = "DownloadMission")]
pub(crate) struct PyDownloadMission {
    inner: DownloadMission,
}

mod binding_browser;
mod binding_element;
mod binding_events;
mod binding_page;
mod binding_session;
mod binding_webpage;

pub(crate) fn wrap_web_element(py: Python<'_>, element: WebElement) -> PyResult<Py<PyAny>> {
    match element {
        WebElement::Browser(inner) => Ok(Py::new(py, PyElement { inner })?.into_any()),
        WebElement::Session(inner) => Ok(Py::new(py, PySessionElement { inner })?.into_any()),
        WebElement::Mix { element, .. } => {
            Ok(Py::new(py, PyElement { inner: element })?.into_any())
        }
    }
}

pub(crate) fn session_xpath_result_to_py(
    py: Python<'_>,
    item: SessionXPathResult,
) -> PyResult<Py<PyAny>> {
    match item {
        SessionXPathResult::Document => {
            let dict = PyDict::new(py);
            dict.set_item("type", "document").map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Element(inner) => {
            Ok(Py::new(py, PySessionElement { inner })?.into_any())
        }
        SessionXPathResult::Text(value) => Ok(PyString::new(py, &value).into_any().unbind()),
        SessionXPathResult::Comment(value) => {
            let dict = PyDict::new(py);
            dict.set_item("type", "comment").map_err(py_err)?;
            dict.set_item("value", value).map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Attribute { name, value } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "attribute").map_err(py_err)?;
            dict.set_item("name", name).map_err(py_err)?;
            dict.set_item("value", value).map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::ProcessingInstruction { target, data } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "processing_instruction")
                .map_err(py_err)?;
            dict.set_item("target", target).map_err(py_err)?;
            dict.set_item("data", data).map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Doctype {
            name,
            public_id,
            system_id,
        } => {
            let dict = PyDict::new(py);
            dict.set_item("type", "doctype").map_err(py_err)?;
            dict.set_item("name", name).map_err(py_err)?;
            dict.set_item("public_id", public_id).map_err(py_err)?;
            dict.set_item("system_id", system_id).map_err(py_err)?;
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
            dict.set_item("type", "qname").map_err(py_err)?;
            dict.set_item("namespace_uri", namespace_uri)
                .map_err(py_err)?;
            dict.set_item("local_name", local_name).map_err(py_err)?;
            dict.set_item("prefix", prefix).map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
        SessionXPathResult::Function(value) => {
            let dict = PyDict::new(py);
            dict.set_item("type", "function").map_err(py_err)?;
            dict.set_item("value", value).map_err(py_err)?;
            Ok(dict.into_any().unbind())
        }
    }
}

pub(crate) fn locator_match_session_to_py(
    py: Python<'_>,
    item: LocatorMatch<SessionElement>,
) -> PyResult<(String, Vec<Py<PySessionElement>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| Py::new(py, PySessionElement { inner }))
        .collect::<PyResult<Vec<_>>>()
        .map_err(py_err)?;
    Ok((item.locator, elements))
}

pub(crate) fn locator_match_element_to_py(
    py: Python<'_>,
    item: LocatorMatch<Element>,
) -> PyResult<(String, Vec<Py<PyElement>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| Py::new(py, PyElement { inner }))
        .collect::<PyResult<Vec<_>>>()
        .map_err(py_err)?;
    Ok((item.locator, elements))
}

pub(crate) fn locator_match_web_to_py(
    py: Python<'_>,
    item: LocatorMatch<WebElement>,
) -> PyResult<(String, Vec<Py<PyAny>>)> {
    let elements = item
        .elements
        .into_iter()
        .map(|inner| wrap_web_element(py, inner))
        .collect::<PyResult<Vec<_>>>()
        .map_err(py_err)?;
    Ok((item.locator, elements))
}

pub(crate) fn element_resource_to_py(
    py: Python<'_>,
    resource: ElementResource,
) -> PyResult<Py<PyAny>> {
    match resource {
        ElementResource::Bytes(bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
        ElementResource::Text(text) => Ok(PyString::new(py, &text).into_any().unbind()),
    }
}

pub(crate) fn cookie_entries_to_tuples(
    entries: Vec<CookieEntry>,
) -> Vec<(String, String, Option<String>)> {
    entries
        .into_iter()
        .map(|entry| (entry.name, entry.value, entry.domain))
        .collect()
}

pub(crate) fn header_tuples(headers: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut headers = headers
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    headers.sort_by(|left, right| left.0.cmp(&right.0));
    headers
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrowser>().map_err(py_err)?;
    m.add_class::<PyPage>().map_err(py_err)?;
    m.add_class::<PyElement>().map_err(py_err)?;
    m.add_class::<PySessionPage>().map_err(py_err)?;
    m.add_class::<PySessionElement>().map_err(py_err)?;
    m.add_class::<PyWebPage>().map_err(py_err)?;
    m.add_class::<PyConsole>().map_err(py_err)?;
    m.add_class::<PyConsoleMessage>().map_err(py_err)?;
    m.add_class::<PyListener>().map_err(py_err)?;
    m.add_class::<PyInterceptor>().map_err(py_err)?;
    m.add_class::<PyInterceptedRequest>().map_err(py_err)?;
    m.add_class::<PyListenerPacket>().map_err(py_err)?;
    m.add_class::<PyListenerRequest>().map_err(py_err)?;
    m.add_class::<PyListenerRequestExtraInfo>()
        .map_err(py_err)?;
    m.add_class::<PyListenerResponse>().map_err(py_err)?;
    m.add_class::<PyListenerResponseExtraInfo>()
        .map_err(py_err)?;
    m.add_class::<PyListenerFailInfo>().map_err(py_err)?;
    m.add_class::<PyDownloadMission>().map_err(py_err)?;
    Ok(())
}
