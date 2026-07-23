mod config;
mod cookies;
mod element;
mod request;
mod snapshot;
mod transport;

pub use cookies::*;
pub use snapshot::*;
use transport::*;

use config::*;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::sleep;
use std::time::Duration;

use cookie_store::{Cookie as StoredCookie, CookieStore as StoredCookieStore, RawCookie};
use ego_tree::{NodeId, NodeRef};
use encoding_rs::Encoding;
use reqwest::blocking::{Client, ClientBuilder, RequestBuilder};
use reqwest::cookie::CookieStore as ReqwestCookieStore;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Identity, Proxy};
use scraper::{ElementRef, Html, Node, Selector};
use serde::Serialize;
use serde_json::Value;
use skyscraper::html as xpath_html;
use skyscraper::xpath as xpath_engine;
use skyscraper::xpath::XpathItemTree;
use skyscraper::xpath::grammar::{
    XpathItemTreeNode,
    data_model::{AnyAtomicType, ElementNode as XpathElementNode, XpathItem},
};
use url::Url;

use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfigHandle, elements_one_should_raise_when_missing,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{
    Locator, LocatorBatchInput, LocatorInput, LocatorKind, LocatorMatch, collect_locator_matches,
    parse_locator_batch_input, parse_optional_locator_input,
};
use crate::settings::{
    child_element_not_found_message, child_node_not_found_message,
    component_state_lock_poisoned_message, cookie_input_type_message,
    cookie_list_item_single_message, cookie_name_empty_message, cookie_name_value_required_message,
    cookie_object_requires_assignment_message, cookie_requires_url_or_domain_message,
    cookie_text_requires_assignment_message, cookie_text_separator_conflict_message,
    cookie_value_empty_message, css_locator_unsupported_for_node_queries_message,
    default_none_element_runtime_config, following_element_not_found_message,
    following_node_not_found_message, invalid_cookie_field_boolean_message,
    invalid_cookie_text_missing_value_message, invalid_css_selector_message,
    invalid_file_url_message, invalid_header_line_message, invalid_session_ini_boolean_message,
    invalid_session_ini_field_expected_message, invalid_session_ini_field_message,
    invalid_session_ini_python_string_message, invalid_session_proxy_message, invalid_url_message,
    invalid_xpath_html_message, invalid_xpath_query_message, invalid_xpath_segment_index_message,
    missing_session_ini_field_message, next_element_not_found_message, next_node_not_found_message,
    parent_element_index_must_start_message, parent_element_level_must_start_message,
    parent_element_not_found_message, preceding_element_not_found_message,
    preceding_node_not_found_message, previous_element_not_found_message,
    previous_node_not_found_message, relative_index_must_start_message,
    session_cert_read_failed_message, session_client_build_failed_message,
    session_cookie_header_decode_failed_message, session_cookie_requires_url_or_domain_message,
    session_download_file_failed_message, session_download_path_resolve_failed_message,
    session_download_retry_loop_exited_message, session_download_status_message,
    session_identity_parse_failed_message, session_local_file_failed_message,
    session_page_no_current_url_message, session_page_no_loaded_document_message,
    session_request_failed_message, session_request_retry_loop_exited_message,
    session_response_body_read_failed_message, snapshot_fragment_root_not_found_message,
    snapshot_fragment_wrapper_not_found_message, snapshot_node_no_longer_exists_message,
    unsupported_snapshot_node_kind_message, unsupported_xpath_path_message,
    unterminated_session_ini_python_string_message,
    xpath_locator_invalid_for_css_filtering_message, xpath_node_no_longer_exists_message,
    xpath_path_not_found_message, xpath_segment_not_found_message,
};

const FRAGMENT_WRAPPER_ATTR: &str = "data-openpage-fragment-root";

pub type SessionResponseHook = Arc<dyn Fn(SessionHookEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadersInput<'a> {
    Text(Cow<'a, str>),
    Pairs(Vec<(Cow<'a, str>, Cow<'a, str>)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamsInput<'a> {
    Pairs(Vec<(Cow<'a, str>, Cow<'a, str>)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAuthInput {
    None,
    Auth(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionProxyInput {
    None,
    Proxy(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionUserAgentInput {
    None,
    UserAgent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMaxRedirectsInput {
    None,
    Max(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRetryTimesInput {
    None,
    Times(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionRetryIntervalInput {
    None,
    Millis(u64),
    Seconds(f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEncodingInput {
    None,
    Encoding(String),
}

impl<'a> From<&'a str> for HeadersInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Text(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for HeadersInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Text(Cow::Borrowed(value.as_str()))
    }
}

impl From<String> for HeadersInput<'_> {
    fn from(value: String) -> Self {
        Self::Text(Cow::Owned(value))
    }
}

impl<'a> From<&'a [(String, String)]> for HeadersInput<'a> {
    fn from(value: &'a [(String, String)]) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(name.as_str()), Cow::Borrowed(value.as_str())))
                .collect(),
        )
    }
}

impl<'a, const N: usize> From<&'a [(String, String); N]> for HeadersInput<'a> {
    fn from(value: &'a [(String, String); N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a Vec<(String, String)>> for HeadersInput<'a> {
    fn from(value: &'a Vec<(String, String)>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<Vec<(String, String)>> for HeadersInput<'_> {
    fn from(value: Vec<(String, String)>) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Owned(name), Cow::Owned(value)))
                .collect(),
        )
    }
}

impl<'a> From<&'a [(&'a str, &'a str)]> for HeadersInput<'a> {
    fn from(value: &'a [(&'a str, &'a str)]) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(*name), Cow::Borrowed(*value)))
                .collect(),
        )
    }
}

impl<'a, const N: usize> From<&'a [(&'a str, &'a str); N]> for HeadersInput<'a> {
    fn from(value: &'a [(&'a str, &'a str); N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a, const N: usize> From<[(&'a str, &'a str); N]> for HeadersInput<'a> {
    fn from(value: [(&'a str, &'a str); N]) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Borrowed(name), Cow::Borrowed(value)))
                .collect(),
        )
    }
}

impl<'a> From<&'a HashMap<String, String>> for HeadersInput<'a> {
    fn from(value: &'a HashMap<String, String>) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(name.as_str()), Cow::Borrowed(value.as_str())))
                .collect(),
        )
    }
}

impl From<HashMap<String, String>> for HeadersInput<'_> {
    fn from(value: HashMap<String, String>) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Owned(name), Cow::Owned(value)))
                .collect(),
        )
    }
}

impl<'a> From<&'a [(String, String)]> for ParamsInput<'a> {
    fn from(value: &'a [(String, String)]) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(name.as_str()), Cow::Borrowed(value.as_str())))
                .collect(),
        )
    }
}

impl<'a, const N: usize> From<&'a [(String, String); N]> for ParamsInput<'a> {
    fn from(value: &'a [(String, String); N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a Vec<(String, String)>> for ParamsInput<'a> {
    fn from(value: &'a Vec<(String, String)>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<Vec<(String, String)>> for ParamsInput<'_> {
    fn from(value: Vec<(String, String)>) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Owned(name), Cow::Owned(value)))
                .collect(),
        )
    }
}

impl<'a> From<&'a [(&'a str, &'a str)]> for ParamsInput<'a> {
    fn from(value: &'a [(&'a str, &'a str)]) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(*name), Cow::Borrowed(*value)))
                .collect(),
        )
    }
}

impl<'a, const N: usize> From<&'a [(&'a str, &'a str); N]> for ParamsInput<'a> {
    fn from(value: &'a [(&'a str, &'a str); N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a, const N: usize> From<[(&'a str, &'a str); N]> for ParamsInput<'a> {
    fn from(value: [(&'a str, &'a str); N]) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Borrowed(name), Cow::Borrowed(value)))
                .collect(),
        )
    }
}

impl<'a> From<&'a HashMap<String, String>> for ParamsInput<'a> {
    fn from(value: &'a HashMap<String, String>) -> Self {
        Self::Pairs(
            value
                .iter()
                .map(|(name, value)| (Cow::Borrowed(name.as_str()), Cow::Borrowed(value.as_str())))
                .collect(),
        )
    }
}

impl From<HashMap<String, String>> for ParamsInput<'_> {
    fn from(value: HashMap<String, String>) -> Self {
        Self::Pairs(
            value
                .into_iter()
                .map(|(name, value)| (Cow::Owned(name), Cow::Owned(value)))
                .collect(),
        )
    }
}

impl From<(String, String)> for SessionAuthInput {
    fn from(value: (String, String)) -> Self {
        Self::Auth(value.0, value.1)
    }
}

impl From<(&str, &str)> for SessionAuthInput {
    fn from(value: (&str, &str)) -> Self {
        Self::Auth(value.0.to_string(), value.1.to_string())
    }
}

impl From<Option<(String, String)>> for SessionAuthInput {
    fn from(value: Option<(String, String)>) -> Self {
        match value {
            Some((username, password)) => Self::Auth(username, password),
            None => Self::None,
        }
    }
}

impl From<&str> for SessionProxyInput {
    fn from(value: &str) -> Self {
        Self::Proxy(value.to_string())
    }
}

impl From<String> for SessionProxyInput {
    fn from(value: String) -> Self {
        Self::Proxy(value)
    }
}

impl From<Option<String>> for SessionProxyInput {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(proxy) => Self::Proxy(proxy),
            None => Self::None,
        }
    }
}

impl From<&str> for SessionUserAgentInput {
    fn from(value: &str) -> Self {
        Self::UserAgent(value.to_string())
    }
}

impl From<String> for SessionUserAgentInput {
    fn from(value: String) -> Self {
        Self::UserAgent(value)
    }
}

impl From<Option<String>> for SessionUserAgentInput {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(user_agent) => Self::UserAgent(user_agent),
            None => Self::None,
        }
    }
}

impl From<usize> for SessionMaxRedirectsInput {
    fn from(value: usize) -> Self {
        Self::Max(value)
    }
}

impl From<Option<usize>> for SessionMaxRedirectsInput {
    fn from(value: Option<usize>) -> Self {
        match value {
            Some(max_redirects) => Self::Max(max_redirects),
            None => Self::None,
        }
    }
}

impl From<usize> for SessionRetryTimesInput {
    fn from(value: usize) -> Self {
        Self::Times(value)
    }
}

impl From<Option<usize>> for SessionRetryTimesInput {
    fn from(value: Option<usize>) -> Self {
        match value {
            Some(times) => Self::Times(times),
            None => Self::None,
        }
    }
}

impl From<u64> for SessionRetryIntervalInput {
    fn from(value: u64) -> Self {
        Self::Millis(value)
    }
}

impl From<Option<u64>> for SessionRetryIntervalInput {
    fn from(value: Option<u64>) -> Self {
        match value {
            Some(millis) => Self::Millis(millis),
            None => Self::None,
        }
    }
}

impl From<f64> for SessionRetryIntervalInput {
    fn from(value: f64) -> Self {
        Self::Seconds(value)
    }
}

impl From<&str> for SessionEncodingInput {
    fn from(value: &str) -> Self {
        Self::Encoding(value.to_string())
    }
}

impl From<String> for SessionEncodingInput {
    fn from(value: String) -> Self {
        Self::Encoding(value)
    }
}

impl From<Option<String>> for SessionEncodingInput {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(encoding) => Self::Encoding(encoding),
            None => Self::None,
        }
    }
}

fn session_cookie_header_decode_error(err: impl ToString) -> OpenPageError {
    OpenPageError::Http(session_cookie_header_decode_failed_message(
        &err.to_string(),
    ))
}

#[derive(Clone, Debug)]
pub struct SessionHookEvent {
    pub requested_url: String,
    pub response: SessionResponseInfo,
    pub raw_data: Arc<Vec<u8>>,
}

#[derive(Clone, Default)]
pub struct SessionHooks {
    response: Vec<SessionResponseHook>,
}

impl std::fmt::Debug for SessionHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHooks")
            .field("response_hooks", &self.response.len())
            .finish()
    }
}

impl SessionHooks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_response<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(SessionHookEvent) + Send + Sync + 'static,
    {
        self.response.push(Arc::new(hook));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.response.is_empty()
    }

    pub fn response_count(&self) -> usize {
        self.response.len()
    }

    fn extend_response_hooks(&mut self, other: &SessionHooks) {
        self.response.extend(other.response.iter().cloned());
    }

    fn response_hooks(&self) -> &[SessionResponseHook] {
        &self.response
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SessionCert {
    Pem(PathBuf),
    PemPair { cert: PathBuf, key: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCertInput {
    None,
    Cert(SessionCert),
}

impl From<SessionCert> for SessionCertInput {
    fn from(value: SessionCert) -> Self {
        Self::Cert(value)
    }
}

impl From<Option<SessionCert>> for SessionCertInput {
    fn from(value: Option<SessionCert>) -> Self {
        match value {
            Some(cert) => Self::Cert(cert),
            None => Self::None,
        }
    }
}

impl From<&str> for SessionCertInput {
    fn from(value: &str) -> Self {
        Self::Cert(SessionCert::Pem(PathBuf::from(value)))
    }
}

impl From<String> for SessionCertInput {
    fn from(value: String) -> Self {
        Self::Cert(SessionCert::Pem(PathBuf::from(value)))
    }
}

impl From<&Path> for SessionCertInput {
    fn from(value: &Path) -> Self {
        Self::Cert(SessionCert::Pem(value.to_path_buf()))
    }
}

impl From<&PathBuf> for SessionCertInput {
    fn from(value: &PathBuf) -> Self {
        Self::Cert(SessionCert::Pem(value.clone()))
    }
}

impl From<PathBuf> for SessionCertInput {
    fn from(value: PathBuf) -> Self {
        Self::Cert(SessionCert::Pem(value))
    }
}

impl From<(&str, &str)> for SessionCertInput {
    fn from(value: (&str, &str)) -> Self {
        Self::Cert(SessionCert::PemPair {
            cert: PathBuf::from(value.0),
            key: PathBuf::from(value.1),
        })
    }
}

impl From<(String, String)> for SessionCertInput {
    fn from(value: (String, String)) -> Self {
        Self::Cert(SessionCert::PemPair {
            cert: PathBuf::from(value.0),
            key: PathBuf::from(value.1),
        })
    }
}

impl From<(PathBuf, PathBuf)> for SessionCertInput {
    fn from(value: (PathBuf, PathBuf)) -> Self {
        Self::Cert(SessionCert::PemPair {
            cert: value.0,
            key: value.1,
        })
    }
}

impl From<(&Path, &Path)> for SessionCertInput {
    fn from(value: (&Path, &Path)) -> Self {
        Self::Cert(SessionCert::PemPair {
            cert: value.0.to_path_buf(),
            key: value.1.to_path_buf(),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SessionAdapter {
    pub timeout_secs: Option<u64>,
    pub http_proxy: Option<Option<String>>,
    pub https_proxy: Option<Option<String>>,
    pub verify: Option<bool>,
    pub cert: Option<Option<SessionCert>>,
    pub trust_env: Option<bool>,
    pub max_redirects: Option<Option<usize>>,
}

impl SessionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timeout_secs(&self) -> Option<u64> {
        self.timeout_secs
    }

    pub fn http_proxy(&self) -> Option<Option<&str>> {
        self.http_proxy.as_ref().map(|proxy| proxy.as_deref())
    }

    pub fn https_proxy(&self) -> Option<Option<&str>> {
        self.https_proxy.as_ref().map(|proxy| proxy.as_deref())
    }

    pub fn verify(&self) -> Option<bool> {
        self.verify
    }

    pub fn cert(&self) -> Option<Option<&SessionCert>> {
        self.cert.as_ref().map(|cert| cert.as_ref())
    }

    pub fn trust_env(&self) -> Option<bool> {
        self.trust_env
    }

    pub fn max_redirects(&self) -> Option<Option<usize>> {
        self.max_redirects
    }

    pub fn set_timeout(&mut self, timeout_secs: u64) -> &mut Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }

    pub fn set_proxies<H, S>(&mut self, http_proxy: H, https_proxy: S) -> &mut Self
    where
        H: Into<SessionProxyInput>,
        S: Into<SessionProxyInput>,
    {
        self.http_proxy = Some(session_proxy_input(http_proxy));
        self.https_proxy = Some(session_proxy_input(https_proxy));
        self
    }

    pub fn set_verify(&mut self, verify: bool) -> &mut Self {
        self.verify = Some(verify);
        self
    }

    pub fn set_cert<C>(&mut self, cert: C) -> &mut Self
    where
        C: Into<SessionCertInput>,
    {
        self.cert = Some(session_cert_input(cert));
        self
    }

    pub fn set_trust_env(&mut self, trust_env: bool) -> &mut Self {
        self.trust_env = Some(trust_env);
        self
    }

    pub fn set_max_redirects<M>(&mut self, max_redirects: M) -> &mut Self
    where
        M: Into<SessionMaxRedirectsInput>,
    {
        self.max_redirects = Some(session_max_redirects_input(max_redirects));
        self
    }

    fn merged_client_options(&self, base: &SessionClientOptions) -> SessionClientOptions {
        SessionClientOptions {
            timeout_secs: self.timeout_secs.unwrap_or(base.timeout_secs),
            http_proxy: self
                .http_proxy
                .clone()
                .unwrap_or_else(|| base.http_proxy.clone()),
            https_proxy: self
                .https_proxy
                .clone()
                .unwrap_or_else(|| base.https_proxy.clone()),
            verify: self.verify.unwrap_or(base.verify),
            cert: self.cert.clone().unwrap_or_else(|| base.cert.clone()),
            trust_env: self.trust_env.unwrap_or(base.trust_env),
            max_redirects: self.max_redirects.unwrap_or(base.max_redirects),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionAdapterMount {
    pub url_prefix: String,
    pub adapter: SessionAdapter,
}

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub timeout_secs: u64,
    pub user_agent: Option<String>,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<SessionCookieParam>,
    pub download_path: PathBuf,
    pub retry_times: usize,
    pub retry_interval_millis: u64,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub params: Vec<(String, String)>,
    pub verify: bool,
    pub auth: Option<(String, String)>,
    pub hooks: SessionHooks,
    pub stream: bool,
    pub cert: Option<SessionCert>,
    pub trust_env: bool,
    pub max_redirects: Option<usize>,
    pub adapters: Vec<SessionAdapterMount>,
    pub source_ini_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionRequestOptions {
    pub timeout_secs: Option<u64>,
    pub retry_times: Option<usize>,
    pub retry_interval_millis: Option<u64>,
    pub user_agent: Option<String>,
    pub headers: Vec<(String, String)>,
    pub params: Vec<(String, String)>,
    pub auth: Option<(String, String)>,
    pub hooks: Option<SessionHooks>,
    pub stream: Option<bool>,
}

impl SessionRequestOptions {
    pub fn timeout_secs(&self) -> Option<u64> {
        self.timeout_secs
    }

    pub fn retry_times(&self) -> Option<usize> {
        self.retry_times
    }

    pub fn retry_interval_millis(&self) -> Option<u64> {
        self.retry_interval_millis
    }

    pub fn retry_interval(&self) -> Option<f64> {
        self.retry_interval_millis
            .map(|millis| millis as f64 / 1000.0)
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn auth(&self) -> Option<(&str, &str)> {
        self.auth
            .as_ref()
            .map(|(username, password)| (username.as_str(), password.as_str()))
    }

    pub fn hooks(&self) -> Option<&SessionHooks> {
        self.hooks.as_ref()
    }

    pub fn stream(&self) -> Option<bool> {
        self.stream
    }
}

pub enum CookieInput<'a> {
    Text(&'a str),
    SessionCookie(&'a SessionCookieParam),
    SessionCookies(&'a [SessionCookieParam]),
    Json(&'a Value),
}

impl<'a> From<&'a str> for CookieInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Text(value)
    }
}

impl<'a> From<&'a String> for CookieInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Text(value)
    }
}

impl<'a> From<&'a SessionCookieParam> for CookieInput<'a> {
    fn from(value: &'a SessionCookieParam) -> Self {
        Self::SessionCookie(value)
    }
}

impl<'a> From<&'a [SessionCookieParam]> for CookieInput<'a> {
    fn from(value: &'a [SessionCookieParam]) -> Self {
        Self::SessionCookies(value)
    }
}

impl<'a> From<&'a Vec<SessionCookieParam>> for CookieInput<'a> {
    fn from(value: &'a Vec<SessionCookieParam>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a Value> for CookieInput<'a> {
    fn from(value: &'a Value) -> Self {
        Self::Json(value)
    }
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            timeout_secs: 10,
            user_agent: None,
            headers: Vec::new(),
            cookies: Vec::new(),
            download_path: PathBuf::from("."),
            retry_times: 3,
            retry_interval_millis: 2_000,
            http_proxy: None,
            https_proxy: None,
            params: Vec::new(),
            verify: true,
            auth: None,
            hooks: SessionHooks::default(),
            stream: false,
            cert: None,
            trust_env: true,
            max_redirects: Some(30),
            adapters: Vec::new(),
            source_ini_path: None,
        }
    }
}

impl SessionOptions {
    pub fn new(read_file: bool, ini_path: Option<&Path>) -> OpenPageResult<Self> {
        Self::from_ini_options(read_file, ini_path)
    }

    pub fn from_ini_options(read_file: bool, ini_path: Option<&Path>) -> OpenPageResult<Self> {
        if read_file {
            Self::from_ini(ini_path)
        } else {
            built_in_session_options_defaults()
        }
    }

    pub fn from_ini(path: Option<&Path>) -> OpenPageResult<Self> {
        let path = resolve_session_options_ini_path(path)?;
        let content = std::fs::read_to_string(&path)?;
        let mut options = parse_session_options_ini(&content)?;
        options.source_ini_path = Some(path);
        Ok(options)
    }

    pub fn save(&self, path: Option<&Path>) -> OpenPageResult<PathBuf> {
        let path = resolve_session_options_ini_path(path.or(self.source_ini_path.as_deref()))?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let template = load_session_options_ini_template(&path, self.source_ini_path.as_deref());
        std::fs::write(
            &path,
            serialize_session_options_ini(self, template.as_deref()),
        )?;
        Ok(path)
    }

    pub fn save_to_default(&self) -> OpenPageResult<PathBuf> {
        let path = default_session_options_ini_path();
        self.save(Some(path.as_path()))
    }

    pub fn source_ini_path(&self) -> Option<&Path> {
        self.source_ini_path.as_deref()
    }

    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn cookies(&self) -> &[SessionCookieParam] {
        &self.cookies
    }

    pub fn download_path(&self) -> &Path {
        self.download_path.as_path()
    }

    pub fn retry_times(&self) -> usize {
        self.retry_times
    }

    pub fn retry_interval_millis(&self) -> u64 {
        self.retry_interval_millis
    }

    pub fn retry_interval(&self) -> f64 {
        self.retry_interval_millis as f64 / 1000.0
    }

    pub fn http_proxy(&self) -> Option<&str> {
        self.http_proxy.as_deref()
    }

    pub fn https_proxy(&self) -> Option<&str> {
        self.https_proxy.as_deref()
    }

    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn verify(&self) -> bool {
        self.verify
    }

    pub fn auth(&self) -> Option<(&str, &str)> {
        self.auth
            .as_ref()
            .map(|(username, password)| (username.as_str(), password.as_str()))
    }

    pub fn stream(&self) -> bool {
        self.stream
    }

    pub fn cert(&self) -> Option<&SessionCert> {
        self.cert.as_ref()
    }

    pub fn trust_env(&self) -> bool {
        self.trust_env
    }

    pub fn max_redirects(&self) -> Option<usize> {
        self.max_redirects
    }

    pub fn set_timeout(&mut self, timeout_secs: u64) -> &mut Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn set_user_agent<U>(&mut self, user_agent: U) -> &mut Self
    where
        U: Into<SessionUserAgentInput>,
    {
        self.user_agent = session_user_agent_input(user_agent);
        self
    }

    pub fn set_headers<'a, H>(&mut self, headers: H) -> OpenPageResult<&mut Self>
    where
        H: Into<HeadersInput<'a>>,
    {
        let headers = parse_headers_input(headers)?;
        self.headers.clear();
        for (name, value) in headers {
            upsert_header_pair(&mut self.headers, name, value);
        }
        Ok(self)
    }

    pub fn set_a_header(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        upsert_header_pair(&mut self.headers, name.into(), value.into());
        self
    }

    pub fn remove_a_header(&mut self, name: &str) -> &mut Self {
        remove_header_pairs(&mut self.headers, name);
        self
    }

    pub fn clear_headers(&mut self) -> &mut Self {
        self.headers.clear();
        self
    }

    pub fn set_cookies<'a, C>(&mut self, cookies: C) -> OpenPageResult<&mut Self>
    where
        C: Into<CookieInput<'a>>,
    {
        self.cookies = cookie_input_to_params(cookies.into(), None)?;
        Ok(self)
    }

    pub fn clear_cookies(&mut self) -> &mut Self {
        self.cookies.clear();
        self
    }

    pub fn set_retry<T, I>(&mut self, retry_times: T, retry_interval: I) -> &mut Self
    where
        T: Into<SessionRetryTimesInput>,
        I: Into<SessionRetryIntervalInput>,
    {
        if let Some(retry_times) = session_retry_times_input(retry_times) {
            self.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = session_retry_interval_input(retry_interval) {
            self.retry_interval_millis = retry_interval_millis;
        }
        self
    }

    pub fn set_proxies<H, S>(&mut self, http_proxy: H, https_proxy: S) -> &mut Self
    where
        H: Into<SessionProxyInput>,
        S: Into<SessionProxyInput>,
    {
        self.http_proxy = session_proxy_input(http_proxy);
        self.https_proxy = session_proxy_input(https_proxy);
        self
    }

    pub fn set_download_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.download_path = path.as_ref().to_path_buf();
        self
    }

    pub fn set_auth<A>(&mut self, auth: A) -> &mut Self
    where
        A: Into<SessionAuthInput>,
    {
        self.auth = session_auth_input(auth);
        self
    }

    pub fn set_hooks(&mut self, hooks: SessionHooks) -> &mut Self {
        self.hooks = hooks;
        self
    }

    pub fn hooks(&self) -> &SessionHooks {
        &self.hooks
    }

    pub fn set_stream(&mut self, stream: bool) -> &mut Self {
        self.stream = stream;
        self
    }

    pub fn set_params<'a, P>(&mut self, params: P) -> &mut Self
    where
        P: Into<ParamsInput<'a>>,
    {
        self.params = params_input_pairs(params);
        self
    }

    pub fn set_cert<C>(&mut self, cert: C) -> &mut Self
    where
        C: Into<SessionCertInput>,
    {
        self.cert = session_cert_input(cert);
        self
    }

    pub fn set_verify(&mut self, verify: bool) -> &mut Self {
        self.verify = verify;
        self
    }

    pub fn set_trust_env(&mut self, trust_env: bool) -> &mut Self {
        self.trust_env = trust_env;
        self
    }

    pub fn set_max_redirects<M>(&mut self, max_redirects: M) -> &mut Self
    where
        M: Into<SessionMaxRedirectsInput>,
    {
        self.max_redirects = session_max_redirects_input(max_redirects);
        self
    }

    pub fn add_adapter(
        &mut self,
        url_prefix: impl Into<String>,
        adapter: SessionAdapter,
    ) -> &mut Self {
        self.adapters.push(SessionAdapterMount {
            url_prefix: url_prefix.into(),
            adapter,
        });
        self
    }

    pub fn adapters(&self) -> &[SessionAdapterMount] {
        &self.adapters
    }
}

fn session_retry_interval_seconds_to_millis(seconds: f64) -> u64 {
    if seconds <= 0.0 || !seconds.is_finite() {
        0
    } else {
        (seconds * 1000.0).round() as u64
    }
}

fn parse_session_headers(value: &str) -> OpenPageResult<Vec<(String, String)>> {
    match parse_ini_json_like_value(value, "headers")? {
        Value::Null => Ok(Vec::new()),
        Value::Object(map) => map
            .into_iter()
            .map(|(key, value)| Ok((key, json_scalar_to_string(&value, "headers")?)))
            .collect(),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message("headers", "object"),
        )),
    }
}

pub(crate) fn parse_headers_input<'a, H>(headers: H) -> OpenPageResult<Vec<(String, String)>>
where
    H: Into<HeadersInput<'a>>,
{
    match headers.into() {
        HeadersInput::Text(text) => parse_headers_text(&text),
        HeadersInput::Pairs(pairs) => Ok(pairs
            .into_iter()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect()),
    }
}

fn parse_headers_text(text: &str) -> OpenPageResult<Vec<(String, String)>> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(parse_header_line)
        .collect()
}

fn parse_header_line(line: &str) -> OpenPageResult<(String, String)> {
    let Some((name, value)) = line.split_once(':') else {
        return Err(OpenPageError::Http(invalid_header_line_message(line)));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(OpenPageError::Http(invalid_header_line_message(line)));
    }
    Ok((name.to_string(), value.trim_start().to_string()))
}

fn params_input_pairs<'a, P>(params: P) -> Vec<(String, String)>
where
    P: Into<ParamsInput<'a>>,
{
    match params.into() {
        ParamsInput::Pairs(pairs) => pairs
            .into_iter()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect(),
    }
}

fn session_cert_input<C>(cert: C) -> Option<SessionCert>
where
    C: Into<SessionCertInput>,
{
    match cert.into() {
        SessionCertInput::None => None,
        SessionCertInput::Cert(cert) => Some(cert),
    }
}

fn session_auth_input<A>(auth: A) -> Option<(String, String)>
where
    A: Into<SessionAuthInput>,
{
    match auth.into() {
        SessionAuthInput::None => None,
        SessionAuthInput::Auth(username, password) => Some((username, password)),
    }
}

fn session_proxy_input<P>(proxy: P) -> Option<String>
where
    P: Into<SessionProxyInput>,
{
    match proxy.into() {
        SessionProxyInput::None => None,
        SessionProxyInput::Proxy(proxy) => Some(proxy),
    }
}

fn session_user_agent_input<U>(user_agent: U) -> Option<String>
where
    U: Into<SessionUserAgentInput>,
{
    match user_agent.into() {
        SessionUserAgentInput::None => None,
        SessionUserAgentInput::UserAgent(user_agent) => Some(user_agent),
    }
}

fn session_max_redirects_input<M>(max_redirects: M) -> Option<usize>
where
    M: Into<SessionMaxRedirectsInput>,
{
    match max_redirects.into() {
        SessionMaxRedirectsInput::None => None,
        SessionMaxRedirectsInput::Max(max_redirects) => Some(max_redirects),
    }
}

fn session_retry_times_input<T>(retry_times: T) -> Option<usize>
where
    T: Into<SessionRetryTimesInput>,
{
    match retry_times.into() {
        SessionRetryTimesInput::None => None,
        SessionRetryTimesInput::Times(times) => Some(times),
    }
}

fn session_retry_interval_input<I>(retry_interval: I) -> Option<u64>
where
    I: Into<SessionRetryIntervalInput>,
{
    match retry_interval.into() {
        SessionRetryIntervalInput::None => None,
        SessionRetryIntervalInput::Millis(millis) => Some(millis),
        SessionRetryIntervalInput::Seconds(seconds) => {
            Some(session_retry_interval_seconds_to_millis(seconds))
        }
    }
}

fn session_encoding_input<E>(encoding: E) -> Option<String>
where
    E: Into<SessionEncodingInput>,
{
    match encoding.into() {
        SessionEncodingInput::None => None,
        SessionEncodingInput::Encoding(encoding) => Some(encoding),
    }
}

fn parse_session_params(value: &str) -> OpenPageResult<Vec<(String, String)>> {
    match parse_ini_json_like_value(value, "params")? {
        Value::Null => Ok(Vec::new()),
        Value::Object(map) => map
            .into_iter()
            .map(|(key, value)| Ok((key, json_scalar_to_string(&value, "params")?)))
            .collect(),
        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Array(pair) if pair.len() == 2 => Ok((
                    json_scalar_to_string(&pair[0], "params")?,
                    json_scalar_to_string(&pair[1], "params")?,
                )),
                _ => Err(OpenPageError::Http(
                    invalid_session_ini_field_expected_message("params", "[key, value] pairs"),
                )),
            })
            .collect(),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message("params", "object or pair list"),
        )),
    }
}

fn parse_session_cookies(value: &str) -> OpenPageResult<Vec<SessionCookieParam>> {
    match parse_ini_json_like_value(value, "cookies")? {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items.into_iter().map(parse_session_cookie_param).collect(),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message("cookies", "list"),
        )),
    }
}

fn parse_session_cookie_param(value: Value) -> OpenPageResult<SessionCookieParam> {
    let Value::Object(map) = value else {
        return Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message("cookie", "object"),
        ));
    };
    Ok(SessionCookieParam {
        name: json_required_string(map.get("name"), "cookies.name")?,
        value: json_required_string(map.get("value"), "cookies.value")?,
        url: json_optional_string(map.get("url"), "cookies.url")?,
        domain: json_optional_string(map.get("domain"), "cookies.domain")?,
        path: json_optional_string(map.get("path"), "cookies.path")?,
        secure: json_optional_bool(map.get("secure"), "cookies.secure")?.unwrap_or(false),
        http_only: json_optional_bool(map.get("http_only"), "cookies.http_only")?.unwrap_or(false),
        same_site: json_optional_string(map.get("same_site"), "cookies.same_site")?,
    })
}

fn parse_optional_ini_string(value: &str, field: &str) -> OpenPageResult<Option<String>> {
    match parse_ini_json_like_value(value, field)? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        other => Ok(Some(json_scalar_to_string(&other, field)?)),
    }
}

fn parse_optional_ini_string_pair(
    value: &str,
    field: &str,
) -> OpenPageResult<Option<(String, String)>> {
    match parse_ini_json_like_value(value, field)? {
        Value::Null => Ok(None),
        Value::Array(items) if items.len() == 2 => Ok(Some((
            json_scalar_to_string(&items[0], field)?,
            json_scalar_to_string(&items[1], field)?,
        ))),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message(field, "2-item list"),
        )),
    }
}

fn parse_optional_ini_bool(value: &str) -> OpenPageResult<Option<bool>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.eq_ignore_ascii_case("null") || value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    Ok(Some(parse_ini_bool(value)?))
}

fn parse_optional_ini_usize(value: &str, field: &str) -> OpenPageResult<Option<usize>> {
    match parse_ini_json_like_value(value, field)? {
        Value::Null => Ok(None),
        Value::Number(number) => number
            .as_u64()
            .map(|value| value as usize)
            .map(Some)
            .ok_or_else(|| {
                OpenPageError::Http(invalid_session_ini_field_expected_message(
                    field,
                    "positive integer",
                ))
            }),
        Value::String(text) if text.trim().is_empty() => Ok(None),
        Value::String(text) => text.parse::<usize>().map(Some).map_err(|err| {
            OpenPageError::Http(invalid_session_ini_field_message(field, &err.to_string()))
        }),
        _ => Err(OpenPageError::Http(format!(
            "{}",
            invalid_session_ini_field_expected_message(field, "integer or null")
        ))),
    }
}

fn parse_optional_ini_cert(value: &str) -> OpenPageResult<Option<SessionCert>> {
    match parse_ini_json_like_value(value, "cert")? {
        Value::Null => Ok(None),
        Value::String(path) => Ok(Some(SessionCert::Pem(PathBuf::from(path)))),
        Value::Array(items) if items.len() == 2 => Ok(Some(SessionCert::PemPair {
            cert: PathBuf::from(json_scalar_to_string(&items[0], "cert")?),
            key: PathBuf::from(json_scalar_to_string(&items[1], "cert")?),
        })),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message("cert", "path or 2-item list"),
        )),
    }
}

fn parse_ini_bool(value: &str) -> OpenPageResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(OpenPageError::Http(invalid_session_ini_boolean_message(
            value,
        ))),
    }
}

fn parse_ini_json_like_value(value: &str, field: &str) -> OpenPageResult<Value> {
    if let Ok(parsed) = serde_json::from_str(value) {
        return Ok(parsed);
    }
    let normalized = python_literal_to_json(value)?;
    serde_json::from_str(&normalized).map_err(|err| {
        OpenPageError::Http(invalid_session_ini_field_message(field, &err.to_string()))
    })
}

fn python_literal_to_json(value: &str) -> OpenPageResult<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut normalized = String::new();

    while index < chars.len() {
        match chars[index] {
            '\'' | '"' => {
                let quote = chars[index];
                index += 1;
                let mut content = String::new();
                let mut closed = false;

                while index < chars.len() {
                    let ch = chars[index];
                    if ch == '\\' {
                        index += 1;
                        if index >= chars.len() {
                            return Err(OpenPageError::Http(
                                invalid_session_ini_python_string_message(),
                            ));
                        }
                        let escaped = chars[index];
                        match escaped {
                            '\\' => content.push('\\'),
                            '\'' => content.push('\''),
                            '"' => content.push('"'),
                            'n' => content.push('\n'),
                            'r' => content.push('\r'),
                            't' => content.push('\t'),
                            other => content.push(other),
                        }
                        index += 1;
                        continue;
                    }
                    if ch == quote {
                        index += 1;
                        closed = true;
                        break;
                    }
                    content.push(ch);
                    index += 1;
                }

                if !closed {
                    return Err(OpenPageError::Http(
                        unterminated_session_ini_python_string_message(),
                    ));
                }

                normalized.push_str(&serde_json::to_string(&content).unwrap());
            }
            ch if ch.is_ascii_alphabetic() => {
                let start = index;
                index += 1;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
                let ident = chars[start..index].iter().collect::<String>();
                match ident.as_str() {
                    "True" => normalized.push_str("true"),
                    "False" => normalized.push_str("false"),
                    "None" => normalized.push_str("null"),
                    _ => normalized.push_str(&ident),
                }
            }
            '(' => {
                normalized.push('[');
                index += 1;
            }
            ')' => {
                normalized.push(']');
                index += 1;
            }
            ch => {
                normalized.push(ch);
                index += 1;
            }
        }
    }

    Ok(normalized)
}

fn json_scalar_to_string(value: &Value, field: &str) -> OpenPageResult<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message(field, "scalar value"),
        )),
    }
}

fn json_required_string(value: Option<&Value>, field: &str) -> OpenPageResult<String> {
    value
        .ok_or_else(|| OpenPageError::Http(missing_session_ini_field_message(field)))
        .and_then(|value| json_scalar_to_string(value, field))
}

fn json_optional_string(value: Option<&Value>, field: &str) -> OpenPageResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(json_scalar_to_string(value, field)?)),
    }
}

fn json_optional_bool(value: Option<&Value>, field: &str) -> OpenPageResult<Option<bool>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(boolean)) => Ok(Some(*boolean)),
        Some(Value::String(text)) => parse_optional_ini_bool(text),
        _ => Err(OpenPageError::Http(
            invalid_session_ini_field_expected_message(field, "boolean"),
        )),
    }
}

#[derive(Clone, Debug, Default)]
struct CookieDraft {
    name: String,
    value: String,
    url: Option<String>,
    domain: Option<String>,
    path: Option<String>,
    secure: bool,
    http_only: bool,
    same_site: Option<String>,
}

pub(crate) fn cookie_input_to_params<'a>(
    input: CookieInput<'a>,
    default_url: Option<&str>,
) -> OpenPageResult<Vec<SessionCookieParam>> {
    cookie_input_to_params_internal(input, default_url, false)
}

pub(crate) fn cookie_input_to_params_allow_missing_scope<'a>(
    input: CookieInput<'a>,
) -> OpenPageResult<Vec<SessionCookieParam>> {
    cookie_input_to_params_internal(input, None, true)
}

fn cookie_input_to_params_internal<'a>(
    input: CookieInput<'a>,
    default_url: Option<&str>,
    allow_missing_scope: bool,
) -> OpenPageResult<Vec<SessionCookieParam>> {
    let mut cookies = match input {
        CookieInput::Text(text) => parse_cookie_text_input(text)?,
        CookieInput::SessionCookie(cookie) => vec![cookie.clone()],
        CookieInput::SessionCookies(cookies) => cookies.to_vec(),
        CookieInput::Json(value) => parse_cookie_json_input(value)?,
    };
    let default_url = default_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    for cookie in &mut cookies {
        if cookie.url.is_none() && cookie.domain.is_none() {
            cookie.url = default_url.clone();
        }
        if cookie.name.trim().is_empty() {
            return Err(OpenPageError::Http(cookie_name_empty_message()));
        }
        if cookie.value.trim().is_empty() {
            return Err(OpenPageError::Http(cookie_value_empty_message(
                &cookie.name,
            )));
        }
        if !allow_missing_scope && cookie.url.is_none() && cookie.domain.is_none() {
            return Err(OpenPageError::Http(cookie_requires_url_or_domain_message(
                &cookie.name,
            )));
        }
    }
    Ok(cookies)
}

fn parse_cookie_text_input(text: &str) -> OpenPageResult<Vec<SessionCookieParam>> {
    let has_semicolon = text.contains(';');
    let has_comma = text.contains(',');
    if has_semicolon && has_comma {
        return Err(OpenPageError::Http(cookie_text_separator_conflict_message()));
    }

    let segments: Vec<&str> = if has_comma {
        text.split(',').collect()
    } else {
        text.split(';').collect()
    };
    let mut entries = Vec::new();
    for raw in segments {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some((name, value)) = raw.split_once('=') {
            entries.push((
                canonical_cookie_attr_key(name),
                Some(value.trim().to_string()),
            ));
        } else {
            entries.push((canonical_cookie_attr_key(raw), None));
        }
    }

    if entries.is_empty() {
        return Ok(Vec::new());
    }
    if entries
        .iter()
        .any(|(key, _)| key == "name" || key == "value")
    {
        return Ok(vec![cookie_draft_to_param(parse_single_cookie_entries(
            &entries,
            "cookie text",
        )?)]);
    }

    let shared = parse_shared_cookie_entries(&entries, "cookie text")?;
    let mut cookies = Vec::new();
    for (key, value) in entries {
        if is_shared_cookie_key(&key) {
            continue;
        }
        let Some(value) = value else {
            return Err(OpenPageError::Http(
                invalid_cookie_text_missing_value_message(&key),
            ));
        };
        cookies.push(cookie_draft_to_param(CookieDraft {
            name: key,
            value,
            url: shared.url.clone(),
            domain: shared.domain.clone(),
            path: shared.path.clone(),
            secure: shared.secure,
            http_only: shared.http_only,
            same_site: shared.same_site.clone(),
        }));
    }
    if cookies.is_empty() {
        return Err(OpenPageError::Http(
            cookie_text_requires_assignment_message(),
        ));
    }
    Ok(cookies)
}

fn parse_cookie_json_input(value: &Value) -> OpenPageResult<Vec<SessionCookieParam>> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(text) => parse_cookie_text_input(text),
        Value::Object(map) => parse_cookie_json_object(map),
        Value::Array(items) => {
            let mut cookies = Vec::new();
            for item in items {
                let item_cookies = parse_cookie_json_input(item)?;
                if item_cookies.len() != 1 {
                    return Err(OpenPageError::Http(cookie_list_item_single_message()));
                }
                cookies.extend(item_cookies);
            }
            Ok(cookies)
        }
        _ => Err(OpenPageError::Http(cookie_input_type_message())),
    }
}

fn parse_cookie_json_object(
    map: &serde_json::Map<String, Value>,
) -> OpenPageResult<Vec<SessionCookieParam>> {
    if map.contains_key("name") || map.contains_key("value") {
        return Ok(vec![cookie_draft_to_param(parse_cookie_object(
            map,
            "cookie object",
        )?)]);
    }

    let shared = parse_shared_cookie_object(map, "cookie object")?;
    let mut cookies = Vec::new();
    for (key, value) in map {
        let canonical = canonical_cookie_attr_key(key);
        if is_shared_cookie_key(&canonical) {
            continue;
        }
        cookies.push(cookie_draft_to_param(CookieDraft {
            name: key.trim().to_string(),
            value: json_scalar_to_string(value, key)?,
            url: shared.url.clone(),
            domain: shared.domain.clone(),
            path: shared.path.clone(),
            secure: shared.secure,
            http_only: shared.http_only,
            same_site: shared.same_site.clone(),
        }));
    }
    if cookies.is_empty() {
        return Err(OpenPageError::Http(
            cookie_object_requires_assignment_message(),
        ));
    }
    Ok(cookies)
}

fn parse_single_cookie_entries(
    entries: &[(String, Option<String>)],
    field: &str,
) -> OpenPageResult<CookieDraft> {
    let mut cookie = CookieDraft::default();
    for (key, value) in entries {
        match key.as_str() {
            "name" => {
                cookie.name = value.clone().unwrap_or_default();
            }
            "value" => {
                cookie.value = value.clone().unwrap_or_default();
            }
            "url" => {
                cookie.url = value.clone().filter(|value| !value.trim().is_empty());
            }
            "domain" => {
                cookie.domain = value.clone().filter(|value| !value.trim().is_empty());
            }
            "path" => {
                cookie.path = value.clone().filter(|value| !value.trim().is_empty());
            }
            "secure" => {
                cookie.secure = parse_cookie_flag_value(value.as_deref(), field, "secure")?;
            }
            "http_only" => {
                cookie.http_only = parse_cookie_flag_value(value.as_deref(), field, "http_only")?;
            }
            "same_site" => {
                cookie.same_site = value.clone().filter(|value| !value.trim().is_empty());
            }
            "expires" | "expiry" | "max_age" => {}
            other => {
                if cookie.name.is_empty() {
                    cookie.name = other.to_string();
                    cookie.value = value.clone().unwrap_or_default();
                }
            }
        }
    }
    if cookie.name.trim().is_empty() || cookie.value.trim().is_empty() {
        return Err(OpenPageError::Http(cookie_name_value_required_message(
            field,
        )));
    }
    Ok(cookie)
}

fn parse_shared_cookie_entries(
    entries: &[(String, Option<String>)],
    field: &str,
) -> OpenPageResult<CookieDraft> {
    let mut shared = CookieDraft::default();
    for (key, value) in entries {
        match key.as_str() {
            "url" => {
                shared.url = value.clone().filter(|value| !value.trim().is_empty());
            }
            "domain" => {
                shared.domain = value.clone().filter(|value| !value.trim().is_empty());
            }
            "path" => {
                shared.path = value.clone().filter(|value| !value.trim().is_empty());
            }
            "secure" => {
                shared.secure = parse_cookie_flag_value(value.as_deref(), field, "secure")?;
            }
            "http_only" => {
                shared.http_only = parse_cookie_flag_value(value.as_deref(), field, "http_only")?;
            }
            "same_site" => {
                shared.same_site = value.clone().filter(|value| !value.trim().is_empty());
            }
            "expires" | "expiry" | "max_age" => {}
            _ => {}
        }
    }
    Ok(shared)
}

fn parse_cookie_object(
    map: &serde_json::Map<String, Value>,
    field: &str,
) -> OpenPageResult<CookieDraft> {
    Ok(CookieDraft {
        name: json_required_string(map.get("name"), &format!("{field}.name"))?,
        value: json_required_string(map.get("value"), &format!("{field}.value"))?,
        url: cookie_object_optional_string(map, &["url"], &format!("{field}.url"))?,
        domain: cookie_object_optional_string(map, &["domain"], &format!("{field}.domain"))?,
        path: cookie_object_optional_string(map, &["path"], &format!("{field}.path"))?,
        secure: cookie_object_optional_bool(map, &["secure"], &format!("{field}.secure"))?
            .unwrap_or(false),
        http_only: cookie_object_optional_bool(
            map,
            &["http_only", "httpOnly", "httponly"],
            &format!("{field}.http_only"),
        )?
        .unwrap_or(false),
        same_site: cookie_object_optional_string(
            map,
            &["same_site", "sameSite", "samesite"],
            &format!("{field}.same_site"),
        )?,
    })
}

fn parse_shared_cookie_object(
    map: &serde_json::Map<String, Value>,
    field: &str,
) -> OpenPageResult<CookieDraft> {
    Ok(CookieDraft {
        name: String::new(),
        value: String::new(),
        url: cookie_object_optional_string(map, &["url"], &format!("{field}.url"))?,
        domain: cookie_object_optional_string(map, &["domain"], &format!("{field}.domain"))?,
        path: cookie_object_optional_string(map, &["path"], &format!("{field}.path"))?,
        secure: cookie_object_optional_bool(map, &["secure"], &format!("{field}.secure"))?
            .unwrap_or(false),
        http_only: cookie_object_optional_bool(
            map,
            &["http_only", "httpOnly", "httponly"],
            &format!("{field}.http_only"),
        )?
        .unwrap_or(false),
        same_site: cookie_object_optional_string(
            map,
            &["same_site", "sameSite", "samesite"],
            &format!("{field}.same_site"),
        )?,
    })
}

fn cookie_object_optional_string(
    map: &serde_json::Map<String, Value>,
    keys: &[&str],
    field: &str,
) -> OpenPageResult<Option<String>> {
    for key in keys {
        if let Some(value) = map.get(*key) {
            return json_optional_string(Some(value), field);
        }
    }
    Ok(None)
}

fn cookie_object_optional_bool(
    map: &serde_json::Map<String, Value>,
    keys: &[&str],
    field: &str,
) -> OpenPageResult<Option<bool>> {
    for key in keys {
        if let Some(value) = map.get(*key) {
            return json_optional_bool(Some(value), field);
        }
    }
    Ok(None)
}

fn canonical_cookie_attr_key(key: &str) -> String {
    match key.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "httponly" => "http_only".to_string(),
        "samesite" => "same_site".to_string(),
        other => other.to_string(),
    }
}

fn is_shared_cookie_key(key: &str) -> bool {
    matches!(
        key,
        "name"
            | "value"
            | "url"
            | "domain"
            | "path"
            | "secure"
            | "http_only"
            | "same_site"
            | "expires"
            | "expiry"
            | "max_age"
    )
}

fn parse_cookie_flag_value(value: Option<&str>, field: &str, attr: &str) -> OpenPageResult<bool> {
    match value {
        None => Ok(true),
        Some(value) => parse_optional_ini_bool(value)?
            .ok_or_else(|| OpenPageError::Http(invalid_cookie_field_boolean_message(field, attr))),
    }
}

fn cookie_draft_to_param(cookie: CookieDraft) -> SessionCookieParam {
    SessionCookieParam {
        name: cookie.name.trim().to_string(),
        value: cookie.value.trim().to_string(),
        url: cookie
            .url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        domain: cookie
            .domain
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        path: cookie
            .path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: cookie
            .same_site
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn serialize_string_map(entries: &[(String, String)]) -> String {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        map.insert(key.clone(), Value::String(value.clone()));
    }
    Value::Object(map).to_string()
}

fn serialize_session_cookies(cookies: &[SessionCookieParam]) -> String {
    serde_json::to_string(cookies).unwrap_or_else(|_| "[]".to_string())
}

fn serialize_optional_string_pair(value: Option<&(String, String)>) -> String {
    match value {
        Some((first, second)) => Value::Array(vec![
            Value::String(first.clone()),
            Value::String(second.clone()),
        ])
        .to_string(),
        None => "null".to_string(),
    }
}

fn serialize_optional_cert(cert: Option<&SessionCert>) -> String {
    match cert {
        Some(SessionCert::Pem(path)) => {
            Value::String(path.to_string_lossy().to_string()).to_string()
        }
        Some(SessionCert::PemPair { cert, key }) => Value::Array(vec![
            Value::String(cert.to_string_lossy().to_string()),
            Value::String(key.to_string_lossy().to_string()),
        ])
        .to_string(),
        None => "null".to_string(),
    }
}

fn serialize_optional_usize(value: Option<usize>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "null".to_string(),
    }
}

fn path_to_ini_value(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn millis_to_ini_seconds(millis: u64) -> String {
    let seconds = millis as f64 / 1000.0;
    if seconds.fract() == 0.0 {
        format!("{seconds:.0}")
    } else {
        seconds.to_string()
    }
}

fn ini_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

#[derive(Debug, Clone)]
struct SessionClientOptions {
    timeout_secs: u64,
    http_proxy: Option<String>,
    https_proxy: Option<String>,
    verify: bool,
    cert: Option<SessionCert>,
    trust_env: bool,
    max_redirects: Option<usize>,
}

#[derive(Debug, Clone)]
struct SessionAdapterRuntimeMount {
    url_prefix: String,
    client: Client,
}

#[derive(Debug)]
struct SessionState {
    client: Client,
    adapter_clients: Vec<SessionAdapterRuntimeMount>,
    adapter_mounts: Vec<SessionAdapterMount>,
    cookie_jar: Arc<SessionCookieJar>,
    timeout_secs: u64,
    user_agent: Option<String>,
    download_path: PathBuf,
    last_download: Option<SessionDownload>,
    retry_times: usize,
    retry_interval_millis: u64,
    http_proxy: Option<String>,
    https_proxy: Option<String>,
    params: Vec<(String, String)>,
    verify: bool,
    auth: Option<(String, String)>,
    hooks: SessionHooks,
    stream: bool,
    cert: Option<SessionCert>,
    trust_env: bool,
    max_redirects: Option<usize>,
    headers: HashMap<String, String>,
    url: Option<String>,
    status_code: Option<u16>,
    response_headers: Vec<(String, String)>,
    response_content_type: Option<String>,
    forced_encoding: Option<String>,
    encoding: Option<String>,
    body: Option<Arc<String>>,
    raw_data: Option<Arc<Vec<u8>>>,
    json: Option<Value>,
    pending_response: Option<PendingSessionResponse>,
}

#[derive(Debug, Default)]
struct SessionCookieJar {
    inner: RwLock<StoredCookieStore>,
}

#[derive(Debug, Clone)]
struct SessionRequestContext {
    client: Client,
    user_agent: Option<String>,
    headers: Vec<(String, String)>,
    current_url: Option<String>,
    params: Vec<(String, String)>,
    auth: Option<(String, String)>,
    hooks: SessionHooks,
    retry_times: usize,
    retry_interval_millis: u64,
    timeout_secs: Option<u64>,
    stream: bool,
}

impl From<&SessionOptions> for SessionClientOptions {
    fn from(options: &SessionOptions) -> Self {
        Self {
            timeout_secs: options.timeout_secs,
            http_proxy: options.http_proxy.clone(),
            https_proxy: options.https_proxy.clone(),
            verify: options.verify,
            cert: options.cert.clone(),
            trust_env: options.trust_env,
            max_redirects: options.max_redirects,
        }
    }
}

impl From<&SessionState> for SessionClientOptions {
    fn from(state: &SessionState) -> Self {
        Self {
            timeout_secs: state.timeout_secs,
            http_proxy: state.http_proxy.clone(),
            https_proxy: state.https_proxy.clone(),
            verify: state.verify,
            cert: state.cert.clone(),
            trust_env: state.trust_env,
            max_redirects: state.max_redirects,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    inner: Arc<Mutex<SessionState>>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

pub struct SessionSettings<'a> {
    page: &'a Session,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    inner: Arc<Mutex<SessionState>>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

impl SessionHandle {
    pub fn page(&self) -> Session {
        Session {
            inner: Arc::clone(&self.inner),
            none_element_config: Arc::clone(&self.none_element_config),
        }
    }

    pub fn snapshot(&self) -> OpenPageResult<SessionRuntimeInfo> {
        self.page().session()
    }

    pub fn session_snapshot(&self) -> OpenPageResult<SessionRuntimeInfo> {
        self.snapshot()
    }

    pub fn response(&self) -> OpenPageResult<Option<SessionResponseInfo>> {
        self.page().response()
    }

    pub fn response_snapshot(&self) -> OpenPageResult<Option<SessionResponseInfo>> {
        self.response()
    }

    pub fn html(&self) -> OpenPageResult<String> {
        self.page().html()
    }

    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        self.page().raw_data()
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        self.page().json()
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        self.page().encoding()
    }
}

impl SessionSettings<'_> {
    pub fn user_agent<U>(&self, user_agent: U) -> OpenPageResult<()>
    where
        U: Into<SessionUserAgentInput>,
    {
        self.page.set_user_agent(user_agent)
    }

    pub fn headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        self.page.set_headers(headers)
    }

    pub fn header(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.page.set_header(name, value)
    }

    pub fn timeout(&self, timeout_secs: u64) -> OpenPageResult<()> {
        self.page.set_timeout(timeout_secs)
    }

    pub fn retry<T, I>(&self, retry_times: T, retry_interval: I) -> OpenPageResult<()>
    where
        T: Into<SessionRetryTimesInput>,
        I: Into<SessionRetryIntervalInput>,
    {
        self.page.set_retry(retry_times, retry_interval)
    }

    pub fn retry_times(&self, retry_times: usize) -> OpenPageResult<()> {
        self.page.set_retry(Some(retry_times), None)
    }

    pub fn retry_interval<I>(&self, retry_interval: I) -> OpenPageResult<()>
    where
        I: Into<SessionRetryIntervalInput>,
    {
        self.page.set_retry(None, retry_interval)
    }

    pub fn download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.page.set_download_path(path)
    }

    pub fn encoding<E>(&self, encoding: E) -> OpenPageResult<()>
    where
        E: Into<SessionEncodingInput>,
    {
        self.page.set_encoding(encoding)
    }

    pub fn params<'a, P>(&self, params: P) -> OpenPageResult<()>
    where
        P: Into<ParamsInput<'a>>,
    {
        self.page.set_params(params)
    }

    pub fn auth<A>(&self, auth: A) -> OpenPageResult<()>
    where
        A: Into<SessionAuthInput>,
    {
        self.page.set_auth(auth)
    }

    pub fn hooks(&self, hooks: SessionHooks) -> OpenPageResult<()> {
        self.page.set_hooks(hooks)
    }

    pub fn stream(&self, stream: bool) -> OpenPageResult<()> {
        self.page.set_stream(stream)
    }

    pub fn proxies<H, S>(&self, http_proxy: H, https_proxy: S) -> OpenPageResult<()>
    where
        H: Into<SessionProxyInput>,
        S: Into<SessionProxyInput>,
    {
        self.page.set_proxies(http_proxy, https_proxy)
    }

    pub fn verify(&self, verify: bool) -> OpenPageResult<()> {
        self.page.set_verify(verify)
    }

    pub fn cert<C>(&self, cert: C) -> OpenPageResult<()>
    where
        C: Into<SessionCertInput>,
    {
        self.page.set_cert(cert)
    }

    pub fn trust_env(&self, trust_env: bool) -> OpenPageResult<()> {
        self.page.set_trust_env(trust_env)
    }

    pub fn max_redirects<M>(&self, max_redirects: M) -> OpenPageResult<()>
    where
        M: Into<SessionMaxRedirectsInput>,
    {
        self.page.set_max_redirects(max_redirects)
    }

    pub fn add_adapter(
        &self,
        url_prefix: impl Into<String>,
        adapter: SessionAdapter,
    ) -> OpenPageResult<()> {
        self.page.add_adapter(url_prefix, adapter)
    }

    pub fn cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.set_cookie(name, value, url, domain, path)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn remove_cookie(&self, name: &str, url: Option<&str>) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionCookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub host_only: bool,
    pub persistent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionCookieParam {
    pub name: String,
    pub value: String,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionDownload {
    pub url: String,
    pub final_url: String,
    pub path: String,
    pub filename: String,
    pub content_type: Option<String>,
    pub status_code: u16,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionResponseInfo {
    pub url: Option<String>,
    pub status_code: Option<u16>,
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub encoding: Option<String>,
}

impl SessionResponseInfo {
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn status_code(&self) -> Option<u16> {
        self.status_code
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    pub fn encoding(&self) -> Option<&str> {
        self.encoding.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionRuntimeInfo {
    pub timeout_secs: u64,
    pub user_agent: Option<String>,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<SessionCookie>,
    pub download_path: String,
    pub retry_times: usize,
    pub retry_interval_millis: u64,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub params: Vec<(String, String)>,
    pub verify: bool,
    pub auth: Option<(String, String)>,
    pub stream: bool,
    pub cert: Option<SessionCert>,
    pub trust_env: bool,
    pub max_redirects: Option<usize>,
    pub adapters: Vec<SessionAdapterMount>,
    pub current_url: Option<String>,
}

impl SessionRuntimeInfo {
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn cookies(&self) -> &[SessionCookie] {
        &self.cookies
    }

    pub fn download_path(&self) -> &str {
        &self.download_path
    }

    pub fn retry_times(&self) -> usize {
        self.retry_times
    }

    pub fn retry_interval_millis(&self) -> u64 {
        self.retry_interval_millis
    }

    pub fn http_proxy(&self) -> Option<&str> {
        self.http_proxy.as_deref()
    }

    pub fn https_proxy(&self) -> Option<&str> {
        self.https_proxy.as_deref()
    }

    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn verify(&self) -> bool {
        self.verify
    }

    pub fn auth(&self) -> Option<(&str, &str)> {
        self.auth
            .as_ref()
            .map(|(username, password)| (username.as_str(), password.as_str()))
    }

    pub fn stream(&self) -> bool {
        self.stream
    }

    pub fn cert(&self) -> Option<&SessionCert> {
        self.cert.as_ref()
    }

    pub fn trust_env(&self) -> bool {
        self.trust_env
    }

    pub fn max_redirects(&self) -> Option<usize> {
        self.max_redirects
    }

    pub fn adapters(&self) -> &[SessionAdapterMount] {
        &self.adapters
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current_url.as_deref()
    }
}

struct PendingSessionResponse {
    requested_url: String,
    response: reqwest::blocking::Response,
}

impl std::fmt::Debug for PendingSessionResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingSessionResponse").finish()
    }
}

#[derive(Clone, Debug)]
pub struct SessionElement {
    html: Arc<String>,
    node_id: NodeId,
    base_url: Option<Arc<String>>,
    none_element_config: Option<ElementsOneRuntimeConfigHandle>,
}

#[derive(Clone, Debug)]
pub enum SessionXPathResult {
    Document,
    Element(SessionElement),
    Text(String),
    Comment(String),
    Attribute {
        name: String,
        value: String,
    },
    ProcessingInstruction {
        target: String,
        data: String,
    },
    Doctype {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    QName {
        namespace_uri: String,
        local_name: String,
        prefix: Option<String>,
    },
    Function(String),
}
