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
    component_state_lock_poisoned_message, cookie_input_type_message,
    cookie_list_item_single_message, cookie_name_empty_message, cookie_name_value_required_message,
    cookie_object_requires_assignment_message, cookie_requires_url_or_domain_message,
    cookie_text_requires_assignment_message, cookie_text_separator_conflict_message,
    cookie_value_empty_message, default_none_element_runtime_config,
    invalid_cookie_field_boolean_message, invalid_cookie_text_missing_value_message,
    invalid_file_url_message, invalid_session_ini_boolean_message,
    invalid_session_ini_field_expected_message, invalid_session_ini_field_message,
    invalid_session_ini_python_string_message, invalid_session_proxy_message, invalid_url_message,
    invalid_xpath_html_message, invalid_xpath_query_message, invalid_xpath_segment_index_message,
    missing_session_ini_field_message, parent_element_index_must_start_message,
    parent_element_level_must_start_message, parent_element_not_found_message,
    session_cert_read_failed_message, session_cookie_requires_url_or_domain_message,
    session_download_status_message, session_identity_parse_failed_message,
    session_page_no_current_url_message, session_page_no_loaded_document_message,
    session_request_failed_message, session_response_body_read_failed_message,
    snapshot_fragment_root_not_found_message, snapshot_fragment_wrapper_not_found_message,
    snapshot_node_no_longer_exists_message, unsupported_snapshot_node_kind_message,
    unsupported_xpath_path_message, unterminated_session_ini_python_string_message,
    xpath_node_no_longer_exists_message, xpath_path_not_found_message,
    xpath_segment_not_found_message,
};

const FRAGMENT_WRAPPER_ATTR: &str = "data-openpage-fragment-root";

pub type SessionResponseHook = Arc<dyn Fn(SessionHookEvent) + Send + Sync + 'static>;

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

    pub fn set_timeout(&mut self, timeout_secs: u64) -> &mut Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }

    pub fn set_proxies(
        &mut self,
        http_proxy: Option<String>,
        https_proxy: Option<String>,
    ) -> &mut Self {
        self.http_proxy = Some(http_proxy);
        self.https_proxy = Some(https_proxy);
        self
    }

    pub fn set_verify(&mut self, verify: bool) -> &mut Self {
        self.verify = Some(verify);
        self
    }

    pub fn set_cert(&mut self, cert: Option<SessionCert>) -> &mut Self {
        self.cert = Some(cert);
        self
    }

    pub fn set_trust_env(&mut self, trust_env: bool) -> &mut Self {
        self.trust_env = Some(trust_env);
        self
    }

    pub fn set_max_redirects(&mut self, max_redirects: Option<usize>) -> &mut Self {
        self.max_redirects = Some(max_redirects);
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

    pub fn set_timeout(&mut self, timeout_secs: u64) -> &mut Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn set_user_agent(&mut self, user_agent: Option<String>) -> &mut Self {
        self.user_agent = user_agent;
        self
    }

    pub fn set_headers(&mut self, headers: &[(String, String)]) -> &mut Self {
        self.headers.clear();
        for (name, value) in headers {
            upsert_header_pair(&mut self.headers, name.clone(), value.clone());
        }
        self
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

    pub fn set_retry(
        &mut self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> &mut Self {
        if let Some(retry_times) = retry_times {
            self.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            self.retry_interval_millis = retry_interval_millis;
        }
        self
    }

    pub fn set_proxies(
        &mut self,
        http_proxy: Option<String>,
        https_proxy: Option<String>,
    ) -> &mut Self {
        self.http_proxy = http_proxy;
        self.https_proxy = https_proxy;
        self
    }

    pub fn set_download_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.download_path = path.as_ref().to_path_buf();
        self
    }

    pub fn set_auth(&mut self, auth: Option<(String, String)>) -> &mut Self {
        self.auth = auth;
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

    pub fn set_params(&mut self, params: &[(String, String)]) -> &mut Self {
        self.params = params.to_vec();
        self
    }

    pub fn set_cert(&mut self, cert: Option<SessionCert>) -> &mut Self {
        self.cert = cert;
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

    pub fn set_max_redirects(&mut self, max_redirects: Option<usize>) -> &mut Self {
        self.max_redirects = max_redirects;
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

#[derive(Debug, Clone, Default)]
struct IniDocument {
    section_order: Vec<String>,
    key_order: HashMap<String, Vec<String>>,
    sections: HashMap<String, HashMap<String, String>>,
}

fn default_session_options_ini_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini")
}

fn project_session_options_ini_path() -> OpenPageResult<PathBuf> {
    Ok(env::current_dir()?.join("dp_configs.ini"))
}

fn built_in_session_options_defaults() -> OpenPageResult<SessionOptions> {
    parse_session_options_ini(include_str!("../configs.ini"))
}

fn resolve_session_options_ini_path(path: Option<&Path>) -> OpenPageResult<PathBuf> {
    match path {
        Some(path) if path.is_dir() => Ok(path.join("config.ini")),
        Some(path) => Ok(path.to_path_buf()),
        None => {
            let project_path = project_session_options_ini_path()?;
            if project_path.is_file() {
                Ok(project_path)
            } else {
                Ok(default_session_options_ini_path())
            }
        }
    }
}

fn load_session_options_ini_template(
    target_path: &Path,
    source_ini_path: Option<&Path>,
) -> Option<String> {
    read_session_options_ini_template(target_path)
        .or_else(|| {
            source_ini_path
                .filter(|source_path| *source_path != target_path)
                .and_then(|source_path| read_session_options_ini_template(source_path))
        })
        .or_else(|| {
            let default_path = default_session_options_ini_path();
            (default_path.as_path() != target_path)
                .then_some(default_path)
                .and_then(|path| read_session_options_ini_template(path.as_path()))
        })
}

fn read_session_options_ini_template(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn parse_session_options_ini(content: &str) -> OpenPageResult<SessionOptions> {
    let ini = parse_ini_document(content);
    let mut options = SessionOptions::default();

    if let Some(download_path) = ini_non_empty(ini_section_value(&ini, "paths", "download_path")) {
        options.download_path = PathBuf::from(download_path);
    }

    if let Some(timeout) = ini_non_empty(ini_section_value(&ini, "timeouts", "base")) {
        options.timeout_secs = parse_ini_timeout_secs(timeout)?;
    }

    if let Some(http_proxy) = ini_non_empty(ini_section_value(&ini, "proxies", "http")) {
        options.http_proxy = Some(http_proxy.to_string());
    }
    if let Some(https_proxy) = ini_non_empty(ini_section_value(&ini, "proxies", "https")) {
        options.https_proxy = Some(https_proxy.to_string());
    }

    if let Some(retry_times) = ini_non_empty(ini_section_value(&ini, "others", "retry_times")) {
        options.retry_times = retry_times.parse::<usize>().map_err(|err| {
            OpenPageError::Http(invalid_session_ini_field_message(
                "retry_times",
                &err.to_string(),
            ))
        })?;
    }
    if let Some(retry_interval) = ini_non_empty(ini_section_value(&ini, "others", "retry_interval"))
    {
        options.retry_interval_millis = parse_ini_retry_interval_millis(retry_interval)?;
    }

    if let Some(headers) = ini_section_value(&ini, "session_options", "headers") {
        options.set_headers(&parse_session_headers(headers)?);
    }
    if let Some(cookies) = ini_section_value(&ini, "session_options", "cookies") {
        options.set_cookies(&parse_session_cookies(cookies)?)?;
    }
    if let Some(user_agent) = ini_section_value(&ini, "session_options", "user_agent") {
        options.user_agent = parse_optional_ini_string(user_agent, "user_agent")?;
    }
    if let Some(auth) = ini_section_value(&ini, "session_options", "auth") {
        options.auth = parse_optional_ini_string_pair(auth, "auth")?;
    }
    if let Some(params) = ini_section_value(&ini, "session_options", "params") {
        options.set_params(&parse_session_params(params)?);
    }
    if let Some(verify) = ini_section_value(&ini, "session_options", "verify") {
        if let Some(verify) = parse_optional_ini_bool(verify)? {
            options.verify = verify;
        }
    }
    if let Some(cert) = ini_section_value(&ini, "session_options", "cert") {
        options.cert = parse_optional_ini_cert(cert)?;
    }
    if let Some(stream) = ini_section_value(&ini, "session_options", "stream") {
        if let Some(stream) = parse_optional_ini_bool(stream)? {
            options.stream = stream;
        }
    }
    if let Some(trust_env) = ini_section_value(&ini, "session_options", "trust_env") {
        if let Some(trust_env) = parse_optional_ini_bool(trust_env)? {
            options.trust_env = trust_env;
        }
    }
    if let Some(max_redirects) = ini_section_value(&ini, "session_options", "max_redirects") {
        options.max_redirects = parse_optional_ini_usize(max_redirects, "max_redirects")?;
    }

    Ok(options)
}

fn serialize_session_options_ini(options: &SessionOptions, template: Option<&str>) -> String {
    let mut ini = template.map(parse_ini_document).unwrap_or_default();

    set_ini_value(
        &mut ini,
        "paths",
        "download_path",
        path_to_ini_value(&options.download_path),
    );
    set_ini_value(
        &mut ini,
        "timeouts",
        "base",
        options.timeout_secs.to_string(),
    );
    set_ini_value(
        &mut ini,
        "proxies",
        "http",
        options.http_proxy.clone().unwrap_or_default(),
    );
    set_ini_value(
        &mut ini,
        "proxies",
        "https",
        options.https_proxy.clone().unwrap_or_default(),
    );
    set_ini_value(
        &mut ini,
        "others",
        "retry_times",
        options.retry_times.to_string(),
    );
    set_ini_value(
        &mut ini,
        "others",
        "retry_interval",
        millis_to_ini_seconds(options.retry_interval_millis),
    );

    set_ini_value(
        &mut ini,
        "session_options",
        "headers",
        serialize_string_map(&options.headers),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "cookies",
        serialize_session_cookies(&options.cookies),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "user_agent",
        serde_json::to_string(&options.user_agent).unwrap_or_else(|_| "null".to_string()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "auth",
        serialize_optional_string_pair(options.auth.as_ref()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "params",
        serialize_string_map(&options.params),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "verify",
        ini_bool(options.verify).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "cert",
        serialize_optional_cert(options.cert.as_ref()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "stream",
        ini_bool(options.stream).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "trust_env",
        ini_bool(options.trust_env).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "max_redirects",
        serialize_optional_usize(options.max_redirects),
    );

    serialize_ini_document(&ini)
}

fn parse_ini_document(content: &str) -> IniDocument {
    let mut document = IniDocument::default();
    let mut current_section: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim().to_string();
            ensure_ini_section(&mut document, &section);
            current_section = Some(section);
            continue;
        }
        let Some(section) = current_section.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        set_ini_value(&mut document, section, key.trim(), value.trim().to_string());
    }

    document
}

fn ensure_ini_section(document: &mut IniDocument, section: &str) {
    if !document.section_order.iter().any(|item| item == section) {
        document.section_order.push(section.to_string());
    }
    document.key_order.entry(section.to_string()).or_default();
    document.sections.entry(section.to_string()).or_default();
}

fn set_ini_value(document: &mut IniDocument, section: &str, key: &str, value: String) {
    ensure_ini_section(document, section);
    let key = key.to_string();
    let key_order = document.key_order.entry(section.to_string()).or_default();
    if !key_order.iter().any(|item| item == &key) {
        key_order.push(key.clone());
    }
    document
        .sections
        .entry(section.to_string())
        .or_default()
        .insert(key, value);
}

fn serialize_ini_document(document: &IniDocument) -> String {
    let mut blocks = Vec::new();
    let mut emitted = HashSet::new();

    for section in &document.section_order {
        if emitted.insert(section.clone()) {
            blocks.push(serialize_ini_section(document, section));
        }
    }

    let mut extra_sections = document.sections.keys().cloned().collect::<Vec<_>>();
    extra_sections.sort();
    for section in extra_sections {
        if emitted.insert(section.clone()) {
            blocks.push(serialize_ini_section(document, &section));
        }
    }

    if blocks.is_empty() {
        String::new()
    } else {
        format!("{}\n", blocks.join("\n\n"))
    }
}

fn serialize_ini_section(document: &IniDocument, section: &str) -> String {
    let mut lines = vec![format!("[{section}]")];
    let mut emitted = HashSet::new();

    if let Some(keys) = document.key_order.get(section) {
        for key in keys {
            if let Some(value) = document
                .sections
                .get(section)
                .and_then(|values| values.get(key))
            {
                emitted.insert(key.clone());
                lines.push(format!("{key} = {value}"));
            }
        }
    }

    let mut extra_keys = document
        .sections
        .get(section)
        .map(|values| {
            values
                .keys()
                .filter(|key| !emitted.contains(*key))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    extra_keys.sort();
    for key in extra_keys {
        if let Some(value) = document
            .sections
            .get(section)
            .and_then(|values| values.get(&key))
        {
            lines.push(format!("{key} = {value}"));
        }
    }

    lines.join("\n")
}

fn ini_section_value<'a>(document: &'a IniDocument, section: &str, key: &str) -> Option<&'a str> {
    document.sections.get(section)?.get(key).map(String::as_str)
}

fn ini_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_ini_timeout_secs(value: &str) -> OpenPageResult<u64> {
    let timeout = value.parse::<f64>().map_err(|err| {
        OpenPageError::Http(invalid_session_ini_field_message(
            "timeout",
            &err.to_string(),
        ))
    })?;
    if timeout.is_sign_negative() {
        return Err(OpenPageError::Http(invalid_session_ini_field_message(
            "timeout",
            "negative value",
        )));
    }
    Ok(timeout.ceil() as u64)
}

fn parse_ini_retry_interval_millis(value: &str) -> OpenPageResult<u64> {
    let retry_interval = value.parse::<f64>().map_err(|err| {
        OpenPageError::Http(invalid_session_ini_field_message(
            "retry_interval",
            &err.to_string(),
        ))
    })?;
    if retry_interval.is_sign_negative() {
        return Err(OpenPageError::Http(invalid_session_ini_field_message(
            "retry_interval",
            "negative value",
        )));
    }
    Ok((retry_interval * 1000.0).round() as u64)
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
pub struct SessionPage {
    inner: Arc<Mutex<SessionState>>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    inner: Arc<Mutex<SessionState>>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

impl SessionHandle {
    pub fn page(&self) -> SessionPage {
        SessionPage {
            inner: Arc::clone(&self.inner),
            none_element_config: Arc::clone(&self.none_element_config),
        }
    }

    pub fn snapshot(&self) -> OpenPageResult<SessionRuntimeInfo> {
        self.page().session()
    }

    pub fn response(&self) -> OpenPageResult<Option<SessionResponseInfo>> {
        self.page().response()
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

struct PendingSessionResponse {
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

impl SessionPage {
    pub fn new(options: SessionOptions) -> OpenPageResult<Self> {
        let cookie_jar = Arc::new(SessionCookieJar::default());
        initialize_session_cookies(&cookie_jar, &options.cookies)?;
        let client_options = SessionClientOptions::from(&options);
        let client = build_session_client(&client_options, Arc::clone(&cookie_jar))?;
        let adapter_clients = build_session_adapter_clients(
            &options.adapters,
            &client_options,
            Arc::clone(&cookie_jar),
        )?;
        let download_path = normalize_session_download_path(&options.download_path)?;

        let mut headers = HashMap::new();
        for (name, value) in options.headers {
            upsert_header_map(&mut headers, name, value);
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(SessionState {
                client,
                adapter_clients,
                adapter_mounts: options.adapters,
                cookie_jar,
                timeout_secs: options.timeout_secs,
                user_agent: options.user_agent,
                download_path,
                last_download: None,
                headers,
                retry_times: options.retry_times,
                retry_interval_millis: options.retry_interval_millis,
                http_proxy: options.http_proxy,
                https_proxy: options.https_proxy,
                params: options.params,
                verify: options.verify,
                auth: options.auth,
                hooks: options.hooks,
                stream: options.stream,
                cert: options.cert,
                trust_env: options.trust_env,
                max_redirects: options.max_redirects,
                url: None,
                status_code: None,
                response_headers: Vec::new(),
                response_content_type: None,
                forced_encoding: None,
                encoding: None,
                body: None,
                raw_data: None,
                json: None,
                pending_response: None,
            })),
            none_element_config: Arc::new(Mutex::new(default_none_element_runtime_config())),
        })
    }

    pub fn from_session_handle(handle: SessionHandle) -> Self {
        handle.page()
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        self.get_with_options(url, &SessionRequestOptions::default())
    }

    pub fn get_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<bool> {
        if let Some(path) = resolve_local_file_path(url)? {
            return self.load_local_file(&path);
        }
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.get(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "GET",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post(&self, url: &str) -> OpenPageResult<bool> {
        self.post_with_options(url, &SessionRequestOptions::default())
    }

    pub fn post_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<bool> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "POST",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<bool> {
        self.post_json_with_options(url, payload, &SessionRequestOptions::default())
    }

    pub fn post_json_with_options(
        &self,
        url: &str,
        payload: Option<Value>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<bool> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            let request = apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            );
            match &payload {
                Some(payload) => request.json(payload).send().map_err(|err| {
                    OpenPageError::Http(session_request_failed_message(
                        "POST",
                        &request_url,
                        &format!("{err:?}"),
                    ))
                }),
                None => request.send().map_err(|err| {
                    OpenPageError::Http(session_request_failed_message(
                        "POST",
                        &request_url,
                        &format!("{err:?}"),
                    ))
                }),
            }
        })
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.url.clone())
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        Ok(self.lock_state()?.status_code)
    }

    pub fn session_handle(&self) -> SessionHandle {
        SessionHandle {
            inner: Arc::clone(&self.inner),
            none_element_config: Arc::clone(&self.none_element_config),
        }
    }

    pub fn session(&self) -> OpenPageResult<SessionRuntimeInfo> {
        let state = self.lock_state()?;
        let cookie_jar = state.cookie_jar.clone();
        let mut headers = state
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        headers.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let snapshot = SessionRuntimeInfo {
            timeout_secs: state.timeout_secs,
            user_agent: state.user_agent.clone(),
            headers,
            cookies: Vec::new(),
            download_path: state.download_path.display().to_string(),
            retry_times: state.retry_times,
            retry_interval_millis: state.retry_interval_millis,
            http_proxy: state.http_proxy.clone(),
            https_proxy: state.https_proxy.clone(),
            params: state.params.clone(),
            verify: state.verify,
            auth: state.auth.clone(),
            stream: state.stream,
            cert: state.cert.clone(),
            trust_env: state.trust_env,
            max_redirects: state.max_redirects,
            adapters: state.adapter_mounts.clone(),
            current_url: state.url.clone(),
        };
        drop(state);

        let mut cookies = cookie_jar.all_cookies();
        cookies.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.domain.cmp(&right.domain))
                .then(left.path.cmp(&right.path))
        });

        Ok(SessionRuntimeInfo {
            cookies,
            ..snapshot
        })
    }

    pub fn response(&self) -> OpenPageResult<Option<SessionResponseInfo>> {
        let state = self.lock_state()?;
        if state.status_code.is_none()
            && state.url.is_none()
            && state.raw_data.is_none()
            && state.response_headers.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(SessionResponseInfo {
            url: state.url.clone(),
            status_code: state.status_code,
            headers: state.response_headers.clone(),
            content_type: state.response_content_type.clone(),
            encoding: state.encoding.clone(),
        }))
    }

    pub fn add_adapter(
        &self,
        url_prefix: impl Into<String>,
        adapter: SessionAdapter,
    ) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.adapter_mounts.push(SessionAdapterMount {
            url_prefix: url_prefix.into(),
            adapter,
        });
        rebuild_session_client(&mut state)
    }

    pub fn adapters(&self) -> OpenPageResult<Vec<SessionAdapterMount>> {
        Ok(self.lock_state()?.adapter_mounts.clone())
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn url_available(&self) -> OpenPageResult<bool> {
        Ok(self
            .lock_state()?
            .status_code
            .map(|status| (200..400).contains(&status))
            .unwrap_or(false))
    }

    pub fn html(&self) -> OpenPageResult<String> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state
            .body
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state
            .raw_data
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.encoding.clone())
    }

    pub fn forced_encoding(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.forced_encoding.clone())
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state.json.clone())
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        let body = self.body_arc()?;
        Ok(self.first_text(&body, "title")?)
    }

    pub fn user_agent(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.user_agent.clone())
    }

    pub fn download_path(&self) -> OpenPageResult<String> {
        Ok(self.lock_state()?.download_path.display().to_string())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<SessionDownload>> {
        Ok(self.lock_state()?.last_download.clone())
    }

    pub fn timeout_secs(&self) -> OpenPageResult<u64> {
        Ok(self.lock_state()?.timeout_secs)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        Ok(self.lock_state()?.retry_times)
    }

    pub fn retry_interval_millis(&self) -> OpenPageResult<u64> {
        Ok(self.lock_state()?.retry_interval_millis)
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(true)
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(false)
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        Ok(None)
    }

    pub fn is_headless(&self) -> bool {
        false
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        Ok(self
            .cookies_detailed(false)?
            .into_iter()
            .map(CookieEntry::from)
            .collect())
    }

    pub fn cookies_all_domains(&self) -> OpenPageResult<Vec<CookieEntry>> {
        Ok(self
            .cookies_detailed(true)?
            .into_iter()
            .map(CookieEntry::from)
            .collect())
    }

    pub fn cookies_detailed(&self, all_domains: bool) -> OpenPageResult<Vec<SessionCookie>> {
        let cookie_jar = self.lock_state()?.cookie_jar.clone();
        if all_domains {
            return Ok(cookie_jar.all_cookies());
        }

        let Some(url) = self.url()? else {
            return Ok(Vec::new());
        };
        let url = Url::parse(&url).map_err(|err| {
            OpenPageError::Http(invalid_url_message(&url, Some(&err.to_string())))
        })?;
        Ok(cookie_jar.matching_cookies(&url))
    }

    pub fn root(&self) -> OpenPageResult<SessionElement> {
        let body = self.body_arc()?;
        snapshot_root_arc(body, self.base_url_arc()?, Some(&self.none_element_config))
    }

    pub fn set_user_agent(&self, user_agent: Option<String>) -> OpenPageResult<()> {
        self.lock_state()?.user_agent = user_agent;
        Ok(())
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.lock_state()?.download_path = normalize_session_download_path(path.as_ref())?;
        Ok(())
    }

    pub fn set_headers(&self, headers: &[(String, String)]) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.headers.clear();
        for (name, value) in headers {
            upsert_header_map(&mut state.headers, name.clone(), value.clone());
        }
        Ok(())
    }

    pub fn set_header(&self, name: &str, value: &str) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        upsert_header_map(&mut state.headers, name.to_string(), value.to_string());
        Ok(())
    }

    pub fn set_timeout(&self, timeout_secs: u64) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.timeout_secs = timeout_secs;
        rebuild_session_client(&mut state)
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        if let Some(retry_times) = retry_times {
            state.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            state.retry_interval_millis = retry_interval_millis;
        }
        Ok(())
    }

    pub fn set_params(&self, params: &[(String, String)]) -> OpenPageResult<()> {
        self.lock_state()?.params = params.to_vec();
        Ok(())
    }

    pub fn set_auth(&self, auth: Option<(String, String)>) -> OpenPageResult<()> {
        self.lock_state()?.auth = auth;
        Ok(())
    }

    pub fn set_hooks(&self, hooks: SessionHooks) -> OpenPageResult<()> {
        self.lock_state()?.hooks = hooks;
        Ok(())
    }

    pub fn hooks(&self) -> OpenPageResult<SessionHooks> {
        Ok(self.lock_state()?.hooks.clone())
    }

    pub fn set_stream(&self, stream: bool) -> OpenPageResult<()> {
        self.lock_state()?.stream = stream;
        Ok(())
    }

    pub fn stream(&self) -> OpenPageResult<bool> {
        Ok(self.lock_state()?.stream)
    }

    pub fn set_proxies(
        &self,
        http_proxy: Option<String>,
        https_proxy: Option<String>,
    ) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.http_proxy = http_proxy;
        state.https_proxy = https_proxy;
        rebuild_session_client(&mut state)
    }

    pub fn set_verify(&self, verify: bool) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.verify = verify;
        rebuild_session_client(&mut state)
    }

    pub fn set_cert(&self, cert: Option<SessionCert>) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.cert = cert;
        rebuild_session_client(&mut state)
    }

    pub fn set_trust_env(&self, trust_env: bool) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.trust_env = trust_env;
        rebuild_session_client(&mut state)
    }

    pub fn set_max_redirects(&self, max_redirects: Option<usize>) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.max_redirects = max_redirects;
        rebuild_session_client(&mut state)
    }

    pub fn set_encoding(&self, encoding: Option<String>) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.forced_encoding = encoding;
        refresh_state_body_encoding(&mut state);
        Ok(())
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        self.download_with_options(url, &SessionRequestOptions::default())
    }

    pub fn download_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<String> {
        self.download_request(url, Some(options), None)
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        self.download_to_with_options(url, path, &SessionRequestOptions::default())
    }

    pub fn download_to_with_options(
        &self,
        url: &str,
        path: impl AsRef<Path>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<String> {
        self.download_request(
            url,
            Some(options),
            Some(normalize_session_download_path(path.as_ref())?),
        )
    }

    pub fn cookie_header(&self, url: &str) -> OpenPageResult<Option<String>> {
        let url = Url::parse(url)
            .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))))?;
        let jar = self.lock_state()?.cookie_jar.clone();
        jar.cookie_header(&url)
            .map(|value| {
                value
                    .to_str()
                    .map(|text| text.to_string())
                    .map_err(|err| OpenPageError::Http(err.to_string()))
            })
            .transpose()
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = Url::parse(url)
            .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))))?;
        let jar = self.lock_state()?.cookie_jar.clone();
        for cookie in cookie_header
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            jar.add_cookie_str(cookie, &url);
        }
        Ok(())
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        let current_url = self
            .url()?
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
        let cookies = cookie_input_to_params(cookies.into(), current_url.as_deref())?;
        let jar = self.lock_state()?.cookie_jar.clone();
        for cookie in &cookies {
            let url = cookie_scope_url_from_param(cookie)?;
            jar.add_cookie_str(&cookie_param_to_set_cookie(cookie), &url);
        }
        Ok(())
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let url = self.cookie_scope_url(url)?;
        let cookie = cookie_assignment(name, value, domain, path);
        self.lock_state()?.cookie_jar.add_cookie_str(&cookie, &url);
        Ok(())
    }

    pub fn remove_cookie(&self, name: &str, url: Option<&str>) -> OpenPageResult<()> {
        let url = self.cookie_scope_url(url)?;
        let header = self.cookie_header(url.as_str())?.unwrap_or_default();
        let filtered = remove_cookie_from_header(&header, name);
        self.clear_cookies()?;
        if !filtered.is_empty() {
            self.set_cookie_header(url.as_str(), &filtered)?;
        }
        Ok(())
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.cookie_jar = Arc::new(SessionCookieJar::default());
        rebuild_session_client(&mut state)
    }

    pub fn close(&self) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        rebuild_session_client(&mut state)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let body = self.body_arc()?;
        snapshot_find_arc(
            body,
            locator.raw(),
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let body = self.body_arc()?;
        snapshot_find_all_arc(
            body,
            locator.raw(),
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        self.find((by, value))
    }

    pub fn find_all_by(&self, by: &str, value: &str) -> OpenPageResult<Vec<SessionElement>> {
        self.find_all((by, value))
    }

    pub fn query_xpath(&self, expression: &str) -> OpenPageResult<Vec<SessionXPathResult>> {
        let body = self.body_arc()?;
        snapshot_query_xpath_arc(
            body,
            expression,
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<SessionElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    fn first_text(&self, body: &Arc<String>, selector: &str) -> OpenPageResult<Option<String>> {
        let html = Html::parse_document(body);
        let selector_obj = Selector::parse(selector)
            .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
        Ok(html
            .select(&selector_obj)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|text| !text.is_empty()))
    }

    fn body_arc(&self) -> OpenPageResult<Arc<String>> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        state
            .body
            .as_ref()
            .cloned()
            .ok_or_else(|| OpenPageError::Http(session_page_no_loaded_document_message()))
    }

    fn base_url_arc(&self) -> OpenPageResult<Option<Arc<String>>> {
        Ok(self
            .lock_state()?
            .url
            .as_ref()
            .map(|url| Arc::new(url.clone())))
    }

    fn lock_state(&self) -> OpenPageResult<std::sync::MutexGuard<'_, SessionState>> {
        self.inner.lock().map_err(|_| {
            OpenPageError::Http(component_state_lock_poisoned_message(
                "session state",
                "会话状态",
            ))
        })
    }

    fn cookie_scope_url(&self, url: Option<&str>) -> OpenPageResult<Url> {
        match url {
            Some(url) => Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string())),
            None => {
                let current_url =
                    self.lock_state()?.url.clone().ok_or_else(|| {
                        OpenPageError::Http(session_page_no_current_url_message())
                    })?;
                Url::parse(&current_url).map_err(|err| OpenPageError::Http(err.to_string()))
            }
        }
    }

    fn request_context(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
    ) -> OpenPageResult<SessionRequestContext> {
        let state = self.lock_state()?;
        let request_options = request_options.cloned().unwrap_or_default();
        let mut params = state.params.clone();
        params.extend(request_options.params);
        let mut hooks = state.hooks.clone();
        if let Some(request_hooks) = request_options.hooks.as_ref() {
            hooks.extend_response_hooks(request_hooks);
        }
        Ok(SessionRequestContext {
            client: session_client_for_url(&state, requested_url),
            user_agent: request_options
                .user_agent
                .or_else(|| state.user_agent.clone()),
            headers: merge_request_headers(&state.headers, &request_options.headers),
            current_url: state.url.clone(),
            params,
            auth: request_options.auth.or_else(|| state.auth.clone()),
            hooks,
            retry_times: request_options.retry_times.unwrap_or(state.retry_times),
            retry_interval_millis: request_options
                .retry_interval_millis
                .unwrap_or(state.retry_interval_millis),
            timeout_secs: request_options.timeout_secs,
            stream: request_options.stream.unwrap_or(state.stream),
        })
    }

    fn send_request_with_retry<F>(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
        mut send: F,
    ) -> OpenPageResult<bool>
    where
        F: FnMut(&SessionRequestContext) -> OpenPageResult<reqwest::blocking::Response>,
    {
        let context = self.request_context(requested_url, request_options)?;
        let retry_times = context.retry_times;
        let retry_interval_millis = context.retry_interval_millis;
        for attempt in 0..=retry_times {
            match send(&context) {
                Ok(response) => {
                    let ok = if context.stream && context.hooks.is_empty() {
                        self.store_streaming_response(requested_url, response)?
                    } else {
                        self.store_response(requested_url, response, &context.hooks)?
                    };
                    if ok || attempt == retry_times {
                        return Ok(ok);
                    }
                }
                Err(err) => {
                    if attempt == retry_times {
                        return Err(err);
                    }
                }
            }

            if retry_interval_millis > 0 {
                sleep(Duration::from_millis(retry_interval_millis));
            }
        }

        Err(OpenPageError::Http(
            "session request retry loop exited unexpectedly".to_string(),
        ))
    }

    fn download_request(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
        explicit_target: Option<PathBuf>,
    ) -> OpenPageResult<String> {
        let context = self.request_context(requested_url, request_options)?;
        let retry_times = context.retry_times;
        let retry_interval_millis = context.retry_interval_millis;

        for attempt in 0..=retry_times {
            let request_url = append_query_params(requested_url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            let response = apply_request_options(
                context.client.get(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "GET",
                    &request_url,
                    &format!("{err:?}"),
                ))
            });

            match response {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let final_url = response.url().to_string();
                    let response_headers = response
                        .headers()
                        .iter()
                        .map(|(name, value)| {
                            (
                                name.as_str().to_string(),
                                value.to_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let content_type = response
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let content_disposition = response
                        .headers()
                        .get(CONTENT_DISPOSITION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let raw_data = Arc::new(
                        response
                            .bytes()
                            .map_err(|err| {
                                OpenPageError::Http(session_response_body_read_failed_message(
                                    &request_url,
                                    &format!("{err:?}"),
                                ))
                            })?
                            .to_vec(),
                    );
                    run_response_hooks(
                        &context.hooks,
                        SessionHookEvent {
                            requested_url: request_url.clone(),
                            response: SessionResponseInfo {
                                url: Some(if final_url.is_empty() {
                                    request_url.clone()
                                } else {
                                    final_url.clone()
                                }),
                                status_code: Some(status_code),
                                headers: response_headers,
                                content_type: content_type.clone(),
                                encoding: None,
                            },
                            raw_data: Arc::clone(&raw_data),
                        },
                    );
                    if !(200..400).contains(&status_code) {
                        if attempt == retry_times {
                            return Err(OpenPageError::Http(session_download_status_message(
                                status_code,
                                &request_url,
                            )));
                        }
                    } else {
                        let filename = suggested_session_download_filename(
                            content_disposition.as_deref(),
                            &request_url,
                            &final_url,
                        );
                        let target_path = self
                            .resolve_session_download_target(explicit_target.as_ref(), &filename)?;
                        if let Some(parent) = target_path.parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|err| OpenPageError::Io(err.to_string()))?;
                        }
                        std::fs::write(&target_path, raw_data.as_ref())
                            .map_err(|err| OpenPageError::Io(err.to_string()))?;

                        let path = target_path.display().to_string();
                        let filename = target_path
                            .file_name()
                            .and_then(|value| value.to_str())
                            .map(str::to_string)
                            .unwrap_or(filename);
                        let download = SessionDownload {
                            url: request_url,
                            final_url,
                            path: path.clone(),
                            filename,
                            content_type,
                            status_code,
                            total_bytes: raw_data.len() as u64,
                        };
                        self.lock_state()?.last_download = Some(download);
                        return Ok(path);
                    }
                }
                Err(err) => {
                    if attempt == retry_times {
                        return Err(err);
                    }
                }
            }

            if retry_interval_millis > 0 {
                sleep(Duration::from_millis(retry_interval_millis));
            }
        }

        Err(OpenPageError::Http(
            "session download retry loop exited unexpectedly".to_string(),
        ))
    }

    fn store_response(
        &self,
        requested_url: &str,
        response: reqwest::blocking::Response,
        hooks: &SessionHooks,
    ) -> OpenPageResult<bool> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<Vec<_>>();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let raw_data = Arc::new(
            response
                .bytes()
                .map_err(|err| {
                    OpenPageError::Http(session_response_body_read_failed_message(
                        requested_url,
                        &format!("{err:?}"),
                    ))
                })?
                .to_vec(),
        );

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.response_headers = response_headers;
        state.response_content_type = content_type;
        state.pending_response = None;
        state.raw_data = Some(Arc::clone(&raw_data));
        refresh_state_body_encoding(&mut state);
        let hook_event = SessionHookEvent {
            requested_url: requested_url.to_string(),
            response: SessionResponseInfo {
                url: state.url.clone(),
                status_code: state.status_code,
                headers: state.response_headers.clone(),
                content_type: state.response_content_type.clone(),
                encoding: state.encoding.clone(),
            },
            raw_data,
        };
        drop(state);
        run_response_hooks(hooks, hook_event);
        Ok((200..400).contains(&status))
    }

    fn store_streaming_response(
        &self,
        requested_url: &str,
        response: reqwest::blocking::Response,
    ) -> OpenPageResult<bool> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<Vec<_>>();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.response_headers = response_headers;
        state.response_content_type = content_type;
        state.raw_data = None;
        state.body = None;
        state.json = None;
        state.pending_response = Some(PendingSessionResponse { response });
        refresh_state_body_encoding(&mut state);
        Ok((200..400).contains(&status))
    }

    fn load_local_file(&self, path: &Path) -> OpenPageResult<bool> {
        let canonical = path
            .canonicalize()
            .map_err(|err| OpenPageError::Io(err.to_string()))?;
        let raw_data =
            std::fs::read(&canonical).map_err(|err| OpenPageError::Io(err.to_string()))?;
        let file_url = Url::from_file_path(&canonical)
            .map_err(|_| {
                OpenPageError::Io(format!(
                    "failed to build file url for {}",
                    canonical.display()
                ))
            })?
            .to_string();

        let mut state = self.lock_state()?;
        state.url = Some(file_url);
        state.status_code = Some(200);
        state.response_headers = Vec::new();
        state.response_content_type = None;
        state.pending_response = None;
        state.raw_data = Some(Arc::new(raw_data));
        refresh_state_body_encoding(&mut state);
        Ok(true)
    }

    fn resolve_session_download_target(
        &self,
        explicit_target: Option<&PathBuf>,
        filename: &str,
    ) -> OpenPageResult<PathBuf> {
        if let Some(path) = explicit_target {
            return Ok(path.clone());
        }
        Ok(self.lock_state()?.download_path.join(filename))
    }
}

impl SessionCookieJar {
    fn add_cookie_str(&self, cookie: &str, url: &Url) {
        let cookies = RawCookie::parse(cookie)
            .ok()
            .map(|cookie| cookie.into_owned())
            .into_iter();
        self.inner
            .write()
            .expect("session cookie jar lock poisoned")
            .store_response_cookies(cookies, url);
    }

    fn cookie_header(&self, url: &Url) -> Option<HeaderValue> {
        let value = self
            .inner
            .read()
            .expect("session cookie jar lock poisoned")
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if value.is_empty() {
            return None;
        }
        HeaderValue::from_bytes(value.as_bytes()).ok()
    }

    fn matching_cookies(&self, url: &Url) -> Vec<SessionCookie> {
        self.inner
            .read()
            .expect("session cookie jar lock poisoned")
            .matches(url)
            .into_iter()
            .map(SessionCookie::from_store_cookie)
            .collect()
    }

    fn all_cookies(&self) -> Vec<SessionCookie> {
        self.inner
            .read()
            .expect("session cookie jar lock poisoned")
            .iter_unexpired()
            .map(SessionCookie::from_store_cookie)
            .collect()
    }
}

impl ReqwestCookieStore for SessionCookieJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let cookies = cookie_headers.filter_map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|text| RawCookie::parse(text).ok())
                .map(|cookie| cookie.into_owned())
        });
        self.inner
            .write()
            .expect("session cookie jar lock poisoned")
            .store_response_cookies(cookies, url);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.cookie_header(url)
    }
}

impl SessionCookie {
    fn from_store_cookie(cookie: &StoredCookie<'_>) -> Self {
        Self {
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            domain: cookie.domain.as_cow().map(|value| value.into_owned()),
            path: Some(String::from(&cookie.path)),
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            same_site: cookie.same_site().map(|value| value.to_string()),
            host_only: matches!(cookie.domain, cookie_store::CookieDomain::HostOnly(_)),
            persistent: cookie.is_persistent(),
        }
    }
}

impl From<SessionCookie> for CookieEntry {
    fn from(cookie: SessionCookie) -> Self {
        Self {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
        }
    }
}

fn initialize_session_cookies(
    cookie_jar: &SessionCookieJar,
    cookies: &[SessionCookieParam],
) -> OpenPageResult<()> {
    for cookie in cookies {
        let scope_url = cookie_scope_url_from_param(cookie)?;
        let cookie_str = cookie_param_to_set_cookie(cookie);
        cookie_jar.add_cookie_str(&cookie_str, &scope_url);
    }
    Ok(())
}

fn normalize_session_download_path(path: &Path) -> OpenPageResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|current_dir| current_dir.join(path))
            .map_err(|err| OpenPageError::Io(err.to_string()))?
    };
    Ok(normalize_path_components(&absolute))
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn suggested_session_download_filename(
    content_disposition: Option<&str>,
    request_url: &str,
    final_url: &str,
) -> String {
    content_disposition
        .and_then(content_disposition_filename)
        .or_else(|| filename_from_url(final_url))
        .or_else(|| filename_from_url(request_url))
        .unwrap_or_else(|| "download".to_string())
}

fn content_disposition_filename(header: &str) -> Option<String> {
    for part in header.split(';').skip(1) {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("filename*=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            let value = value
                .split_once("''")
                .map(|(_, encoded)| encoded)
                .unwrap_or(value);
            if let Some(filename) = sanitize_download_filename(value) {
                return Some(filename);
            }
        } else if let Some(value) = part.strip_prefix("filename=") {
            if let Some(filename) =
                sanitize_download_filename(value.trim().trim_matches('"').trim_matches('\''))
            {
                return Some(filename);
            }
        }
    }
    None
}

fn filename_from_url(target: &str) -> Option<String> {
    Url::parse(target).ok().and_then(|url| {
        url.path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
            .and_then(sanitize_download_filename)
    })
}

fn sanitize_download_filename(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let value = value.replace(['/', '\\'], "_");
    Some(value)
}

fn cookie_scope_url_from_param(cookie: &SessionCookieParam) -> OpenPageResult<Url> {
    if let Some(url) = cookie.url.as_deref() {
        return Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string()));
    }

    let domain = cookie
        .domain
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenPageError::Http(session_cookie_requires_url_or_domain_message(&cookie.name))
        })?;
    let host = domain.trim_start_matches('.');
    let scheme = if cookie.secure { "https" } else { "http" };
    let path = cookie
        .path
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("/");
    Url::parse(&format!("{scheme}://{host}{path}"))
        .map_err(|err| OpenPageError::Http(err.to_string()))
}

fn cookie_param_to_set_cookie(cookie: &SessionCookieParam) -> String {
    let mut value = cookie_assignment(
        &cookie.name,
        &cookie.value,
        cookie.domain.as_deref(),
        cookie.path.as_deref(),
    );
    if cookie.secure {
        value.push_str("; Secure");
    }
    if cookie.http_only {
        value.push_str("; HttpOnly");
    }
    if let Some(same_site) = cookie
        .same_site
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        value.push_str("; SameSite=");
        value.push_str(same_site);
    }
    value
}

impl SessionElement {
    pub(crate) fn none_element_runtime_config_handle(
        &self,
    ) -> Option<&ElementsOneRuntimeConfigHandle> {
        self.none_element_config.as_ref()
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        find_in_scope(
            Arc::clone(&self.html),
            self.node_id,
            locator.raw(),
            self.base_url.clone(),
            self.none_element_config.as_ref(),
        )
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                self.none_element_config.clone(),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(self.none_element_config.as_ref())? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(
                    self.none_element_config.clone(),
                ))
            }
            Err(err) => Err(err),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        find_all_in_scope(
            Arc::clone(&self.html),
            self.node_id,
            locator.raw(),
            self.base_url.clone(),
            self.none_element_config.as_ref(),
        )
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        self.find((by, value))
    }

    pub fn find_all_by(&self, by: &str, value: &str) -> OpenPageResult<Vec<SessionElement>> {
        self.find_all((by, value))
    }

    pub fn query_xpath(&self, expression: &str) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.with_element(|element| {
            xpath_query_from_scope_element(
                &self.html,
                self.base_url.as_ref(),
                element,
                expression,
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<SessionElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(element.value().name().to_string()))
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>().trim().to_string();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.html())))
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.inner_html())))
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.with_element(|element| {
            Ok(element
                .value()
                .attrs
                .iter()
                .map(|(name, value)| (name.local.to_string(), value.to_string()))
                .collect())
        })
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let raw = element.attr(name).map(ToString::to_string);
            match name {
                "href" => {
                    Ok(raw.and_then(|value| resolve_href_attr(&value, self.base_url.as_deref())))
                }
                "src" => {
                    Ok(raw.and_then(|value| resolve_src_attr(&value, self.base_url.as_deref())))
                }
                "text" => self.text(),
                "innerText" => self.raw_text(),
                "html" | "outerHTML" => self.html(),
                "innerHTML" => self.inner_html(),
                _ => Ok(element
                    .attr(&name.to_ascii_lowercase())
                    .map(ToString::to_string)),
            }
        })
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        let href = self.attr("href")?;
        if href.as_deref().is_some_and(|value| !value.is_empty()) {
            return Ok(href);
        }
        self.attr("src")
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        self.with_element(|element| Ok(element.child_elements().count()))
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(css_path_for_element(element)))
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(xpath_for_element(element)))
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.with_element(|element| {
            Ok(element
                .descendants()
                .filter_map(|node| {
                    node.value()
                        .as_comment()
                        .and_then(|comment| normalize_text_item(comment))
                })
                .collect())
        })
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.with_element(|element| {
            let mut values = Vec::new();
            for child in element.children() {
                match child.value() {
                    Node::Text(text) => {
                        if let Some(value) = normalize_text_item(text) {
                            values.push(value);
                        }
                    }
                    Node::Element(_) if !text_node_only => {
                        if let Some(child_element) = ElementRef::wrap(child) {
                            let text = child_element.text().collect::<String>();
                            if let Some(value) = normalize_text_item(text.as_str()) {
                                values.push(value);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(values)
        })
    }

    pub fn parent(&self) -> OpenPageResult<SessionElement> {
        self.parent_level(1)
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<SessionElement> {
        if level == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_level_must_start_message(),
            ));
        }
        self.with_element(|element| {
            let mut current = Some(element);
            for _ in 0..level {
                current = current.and_then(nearest_parent_element);
            }
            current
                .map(|parent| {
                    session_element_from_ref(
                        &self.html,
                        self.base_url.as_ref(),
                        parent,
                        self.none_element_config.as_ref(),
                    )
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(parent_element_not_found_message()))
        })
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_index_must_start_message(),
            ));
        }
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => self.with_element(|element| {
                let selector = parse_selector_query(locator.query())?;
                nth_from_start(
                    element
                        .ancestors()
                        .skip(1)
                        .filter_map(ElementRef::wrap)
                        .filter(|candidate| selector.matches(candidate))
                        .map(|candidate| {
                            session_element_from_ref(
                                &self.html,
                                self.base_url.as_ref(),
                                candidate,
                                self.none_element_config.as_ref(),
                            )
                        })
                        .collect(),
                    index,
                    &parent_element_not_found_message(),
                )
            }),
            LocatorKind::XPath => self.with_element(|element| {
                nth_from_start(
                    xpath_find_all_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &format!(
                            "{}[{index}]",
                            relative_axis_xpath_query("ancestor", locator.query())
                        ),
                        self.none_element_config.as_ref(),
                    )?,
                    1,
                    &parent_element_not_found_message(),
                )
            }),
        }
    }

    pub fn child(&self) -> OpenPageResult<SessionElement> {
        self.child_with(None::<&str>, 1)
    }

    pub fn child_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.child_node_with(None, 1)
    }

    pub fn child_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.children_with(locator)?,
            index,
            "child element not found",
        )
    }

    pub fn child_node_with(
        &self,
        locator: Option<&str>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult> {
        nth_from_start(
            self.children_nodes_with(locator)?,
            index,
            "child node not found",
        )
    }

    pub fn children(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.children_with(None::<&str>)
    }

    pub fn children_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.children_nodes_with(None)
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &direct_child_xpath_query(locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => collect_matching_elements(
                &self.html,
                self.base_url.as_ref(),
                element.child_elements(),
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn children_nodes_with(
        &self,
        locator: Option<&str>,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.with_element(|element| {
            relative_node_xpath_query(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator,
                "./node()",
                direct_child_xpath_query,
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn prev(&self) -> OpenPageResult<SessionElement> {
        self.prev_with(None::<&str>, 1)
    }

    pub fn prev_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.prev_node_with(None::<&str>, 1)
    }

    pub fn prev_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.prevs_with(locator)?,
            index,
            "previous element not found",
        )
    }

    pub fn prev_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.prev_nodes_with(locator)?,
            index,
            "previous node not found",
        )
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.prevs_with(None::<&str>)
    }

    pub fn prev_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.prev_nodes_with(None::<&str>)
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("preceding-sibling", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => {
                let mut items = collect_matching_elements(
                    &self.html,
                    self.base_url.as_ref(),
                    element.prev_siblings().filter_map(ElementRef::wrap),
                    locator.as_ref().map(|locator| locator.raw()),
                    self.none_element_config.as_ref(),
                )?;
                items.reverse();
                Ok(items)
            }
        })
    }

    pub fn prev_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        self.with_element(|element| {
            relative_node_xpath_query_with_locator(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator.as_ref(),
                "./preceding-sibling::node()",
                |query| relative_axis_xpath_query("preceding-sibling", query),
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn next(&self) -> OpenPageResult<SessionElement> {
        self.next_with(None::<&str>, 1)
    }

    pub fn next_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.next_node_with(None::<&str>, 1)
    }

    pub fn next_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(self.nexts_with(locator)?, index, "next element not found")
    }

    pub fn next_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(self.next_nodes_with(locator)?, index, "next node not found")
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.nexts_with(None::<&str>)
    }

    pub fn next_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.next_nodes_with(None::<&str>)
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("following-sibling", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => collect_matching_elements(
                &self.html,
                self.base_url.as_ref(),
                element.next_siblings().filter_map(ElementRef::wrap),
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn next_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        self.with_element(|element| {
            relative_node_xpath_query_with_locator(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator.as_ref(),
                "./following-sibling::node()",
                |query| relative_axis_xpath_query("following-sibling", query),
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn before(&self) -> OpenPageResult<SessionElement> {
        self.before_with(None::<&str>, 1)
    }

    pub fn before_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.before_node_with(None::<&str>, 1)
    }

    pub fn before_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.befores_with(locator)?,
            index,
            "preceding element not found",
        )
    }

    pub fn before_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.before_nodes_with(locator)?,
            index,
            "preceding node not found",
        )
    }

    pub fn befores(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.befores_with(None::<&str>)
    }

    pub fn before_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.before_nodes_with(None::<&str>)
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("preceding", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => document_relatives(
                &self.html,
                self.base_url.as_ref(),
                element,
                RelativeDirection::Before,
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn before_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        match locator.as_ref() {
            Some(locator) => self.with_element(|element| {
                filter_relative_node_results(
                    xpath_query_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &relative_axis_xpath_query("preceding", locator.query()),
                        self.none_element_config.as_ref(),
                    )?,
                    xpath_query_requests_attributes(locator.query()),
                )
            }),
            None => self.with_element(|element| {
                collect_document_relative_nodes(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    RelativeDirection::Before,
                    self.none_element_config.as_ref(),
                )
            }),
        }
    }

    pub fn after(&self) -> OpenPageResult<SessionElement> {
        self.after_with(None::<&str>, 1)
    }

    pub fn after_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.after_node_with(None::<&str>, 1)
    }

    pub fn after_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.afters_with(locator)?,
            index,
            "following element not found",
        )
    }

    pub fn after_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.after_nodes_with(locator)?,
            index,
            "following node not found",
        )
    }

    pub fn afters(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.afters_with(None::<&str>)
    }

    pub fn after_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.after_nodes_with(None::<&str>)
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("following", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => document_relatives(
                &self.html,
                self.base_url.as_ref(),
                element,
                RelativeDirection::After,
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn after_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        match locator.as_ref() {
            Some(locator) => self.with_element(|element| {
                filter_relative_node_results(
                    xpath_query_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &relative_axis_xpath_query("following", locator.query()),
                        self.none_element_config.as_ref(),
                    )?,
                    xpath_query_requests_attributes(locator.query()),
                )
            }),
            None => self.with_element(|element| {
                collect_document_relative_nodes(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    RelativeDirection::After,
                    self.none_element_config.as_ref(),
                )
            }),
        }
    }

    fn with_element<T, F>(&self, f: F) -> OpenPageResult<T>
    where
        F: FnOnce(ElementRef<'_>) -> OpenPageResult<T>,
    {
        let document = Html::parse_document(self.html.as_ref());
        let element = element_from_document(&document, self.node_id)?;
        f(element)
    }
}

pub fn cookies_from_header(url: &str, cookie_header: &str) -> OpenPageResult<Vec<CookieEntry>> {
    let parsed = Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string()))?;
    let domain = parsed.domain().map(ToString::to_string);
    Ok(cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let (name, value) = item.split_once('=')?;
            Some(CookieEntry {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
                domain: domain.clone(),
            })
        })
        .collect())
}

fn rebuild_session_client(state: &mut SessionState) -> OpenPageResult<()> {
    let client_options = SessionClientOptions::from(&*state);
    state.client = build_session_client(&client_options, Arc::clone(&state.cookie_jar))?;
    state.adapter_clients = build_session_adapter_clients(
        &state.adapter_mounts,
        &client_options,
        Arc::clone(&state.cookie_jar),
    )?;
    Ok(())
}

fn build_session_adapter_clients(
    mounts: &[SessionAdapterMount],
    base_options: &SessionClientOptions,
    cookie_jar: Arc<SessionCookieJar>,
) -> OpenPageResult<Vec<SessionAdapterRuntimeMount>> {
    mounts
        .iter()
        .map(|mount| {
            let client_options = mount.adapter.merged_client_options(base_options);
            let client = build_session_client(&client_options, Arc::clone(&cookie_jar))?;
            Ok(SessionAdapterRuntimeMount {
                url_prefix: mount.url_prefix.clone(),
                client,
            })
        })
        .collect()
}

fn session_client_for_url(state: &SessionState, requested_url: &str) -> Client {
    state
        .adapter_clients
        .iter()
        .filter(|mount| requested_url.starts_with(&mount.url_prefix))
        .max_by_key(|mount| mount.url_prefix.len())
        .map(|mount| mount.client.clone())
        .unwrap_or_else(|| state.client.clone())
}

fn build_session_client(
    options: &SessionClientOptions,
    cookie_jar: Arc<SessionCookieJar>,
) -> OpenPageResult<Client> {
    let mut builder = ClientBuilder::new()
        .cookie_provider(cookie_jar)
        .timeout(Duration::from_secs(options.timeout_secs));

    if !options.trust_env {
        builder = builder.no_proxy();
    }
    if let Some(http_proxy) = options.http_proxy.as_deref() {
        builder = builder.proxy(Proxy::http(http_proxy).map_err(|err| {
            OpenPageError::Http(invalid_session_proxy_message(
                "http",
                http_proxy,
                &format!("{err:?}"),
            ))
        })?);
    }
    if let Some(https_proxy) = options.https_proxy.as_deref() {
        builder = builder.proxy(Proxy::https(https_proxy).map_err(|err| {
            OpenPageError::Http(invalid_session_proxy_message(
                "https",
                https_proxy,
                &format!("{err:?}"),
            ))
        })?);
    }
    if !options.verify {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if let Some(cert) = options.cert.as_ref() {
        builder = builder.identity(load_session_identity(cert)?);
    }
    if let Some(max_redirects) = options.max_redirects {
        builder = builder.redirect(if max_redirects == 0 {
            Policy::none()
        } else {
            Policy::limited(max_redirects)
        });
    }

    builder
        .build()
        .map_err(|err| OpenPageError::Http(format!("{err:?}")))
}

fn load_session_identity(cert: &SessionCert) -> OpenPageResult<Identity> {
    let pem = match cert {
        SessionCert::Pem(path) => std::fs::read(path).map_err(|err| {
            OpenPageError::Io(session_cert_read_failed_message(
                "cert",
                &path.display().to_string(),
                &err.to_string(),
            ))
        })?,
        SessionCert::PemPair { cert, key } => {
            let mut pem = std::fs::read(cert).map_err(|err| {
                OpenPageError::Io(session_cert_read_failed_message(
                    "cert",
                    &cert.display().to_string(),
                    &err.to_string(),
                ))
            })?;
            if !pem.ends_with(b"\n") {
                pem.push(b'\n');
            }
            pem.extend(std::fs::read(key).map_err(|err| {
                OpenPageError::Io(session_cert_read_failed_message(
                    "key",
                    &key.display().to_string(),
                    &err.to_string(),
                ))
            })?);
            pem
        }
    };

    Identity::from_pem(&pem).map_err(|err| {
        OpenPageError::Http(session_identity_parse_failed_message(&format!("{err:?}")))
    })
}

fn apply_request_options(
    mut request: RequestBuilder,
    user_agent: Option<&str>,
    headers: &[(String, String)],
    auth: Option<&(String, String)>,
    timeout_secs: Option<u64>,
) -> RequestBuilder {
    if let Some(user_agent) = user_agent {
        request = request.header(USER_AGENT, user_agent);
    }
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some((username, password)) = auth {
        request = request.basic_auth(username, Some(password));
    }
    if let Some(timeout_secs) = timeout_secs {
        request = request.timeout(Duration::from_secs(timeout_secs));
    }
    request
}

fn merge_request_headers(
    base: &HashMap<String, String>,
    overrides: &[(String, String)],
) -> Vec<(String, String)> {
    let mut merged: Vec<_> = base
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();

    for (name, value) in overrides {
        if let Some(existing) = merged
            .iter_mut()
            .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(name))
        {
            *existing = (name.clone(), value.clone());
        } else {
            merged.push((name.clone(), value.clone()));
        }
    }

    merged
}

fn run_response_hooks(hooks: &SessionHooks, event: SessionHookEvent) {
    if hooks.is_empty() {
        return;
    }
    for hook in hooks.response_hooks() {
        hook(event.clone());
    }
}

fn effective_request_headers(
    headers: &[(String, String)],
    current_url: Option<&str>,
    request_url: &str,
) -> OpenPageResult<Vec<(String, String)>> {
    let mut resolved = Vec::with_capacity(headers.len() + 1);
    let mut has_referer = false;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("referer") {
            has_referer = true;
            if !value.is_empty() {
                resolved.push((name.clone(), value.clone()));
            }
            continue;
        }
        resolved.push((name.clone(), value.clone()));
    }

    if !has_referer {
        if let Some(value) = default_referer_header(current_url, request_url)? {
            resolved.push(("Referer".to_string(), value));
        }
    }

    Ok(resolved)
}

fn default_referer_header(
    current_url: Option<&str>,
    request_url: &str,
) -> OpenPageResult<Option<String>> {
    if let Some(current_url) = current_url.filter(|url| !url.is_empty()) {
        return Ok(Some(current_url.to_string()));
    }

    let parsed = Url::parse(request_url).map_err(|err| {
        OpenPageError::Http(invalid_url_message(request_url, Some(&err.to_string())))
    })?;
    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    let mut authority = host.to_string();
    if let Some(port) = parsed.port() {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    Ok(Some(format!("{}://{}", parsed.scheme(), authority)))
}

fn upsert_header_pair(headers: &mut Vec<(String, String)>, name: String, value: String) {
    if let Some(existing) = headers
        .iter_mut()
        .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(&name))
    {
        *existing = (name, value);
    } else {
        headers.push((name, value));
    }
}

fn remove_header_pairs(headers: &mut Vec<(String, String)>, name: &str) {
    headers.retain(|(existing_name, _)| !existing_name.eq_ignore_ascii_case(name));
}

fn upsert_header_map(headers: &mut HashMap<String, String>, name: String, value: String) {
    headers.retain(|existing_name, _| !existing_name.eq_ignore_ascii_case(&name));
    headers.insert(name, value);
}

fn append_query_params(target: &str, params: &[(String, String)]) -> OpenPageResult<String> {
    if params.is_empty() {
        return Ok(target.to_string());
    }

    let mut url = Url::parse(target)
        .map_err(|err| OpenPageError::Http(invalid_url_message(target, Some(&err.to_string()))))?;
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in params {
            query.append_pair(name, value);
        }
    }
    Ok(url.into())
}

fn resolve_local_file_path(target: &str) -> OpenPageResult<Option<std::path::PathBuf>> {
    if target.starts_with("file://") {
        let url = Url::parse(target).map_err(|err| {
            OpenPageError::Io(invalid_file_url_message(target, Some(&err.to_string())))
        })?;
        match url.host_str() {
            None | Some("localhost") => {}
            Some(_) => return Err(OpenPageError::Io(invalid_file_url_message(target, None))),
        }
        return url
            .to_file_path()
            .map(Some)
            .map_err(|_| OpenPageError::Io(invalid_file_url_message(target, None)));
    }

    let path = Path::new(target);
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

fn cookie_assignment(name: &str, value: &str, domain: Option<&str>, path: Option<&str>) -> String {
    let mut cookie = format!("{}={}", name.trim(), value.trim());
    if let Some(domain) = domain.filter(|item| !item.trim().is_empty()) {
        cookie.push_str("; Domain=");
        cookie.push_str(domain.trim());
    }
    if let Some(path) = path.filter(|item| !item.trim().is_empty()) {
        cookie.push_str("; Path=");
        cookie.push_str(path.trim());
    }
    cookie
}

fn remove_cookie_from_header(cookie_header: &str, name: &str) -> String {
    cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter(|item| {
            item.split_once('=')
                .map(|(cookie_name, _)| !cookie_name.trim().eq(name.trim()))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn detect_body_encoding(content_type: Option<&str>, body: &[u8]) -> Option<String> {
    if let Some(encoding) = declared_content_type_encoding(content_type) {
        return Some(encoding);
    }

    if std::str::from_utf8(body).is_ok() {
        return Some("utf-8".to_string());
    }

    None
}

fn declared_content_type_encoding(content_type: Option<&str>) -> Option<String> {
    if let Some(content_type) = content_type {
        for part in content_type.split(';').skip(1) {
            let trimmed = part.trim();
            let Some((name, value)) = trimmed.split_once('=') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("charset") {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_ascii_lowercase());
                }
            }
        }
    }
    None
}

fn decode_body(body: &[u8], encoding: Option<&str>) -> String {
    if let Some(encoding) = encoding {
        if let Some(decoder) = Encoding::for_label(encoding.as_bytes()) {
            let (text, _, _) = decoder.decode(body);
            return text.into_owned();
        }
    }

    String::from_utf8_lossy(body).into_owned()
}

fn resolve_effective_encoding(
    content_type: Option<&str>,
    body: &[u8],
    forced_encoding: Option<&str>,
) -> Option<String> {
    forced_encoding
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| detect_body_encoding(content_type, body))
}

fn refresh_state_body_encoding(state: &mut SessionState) {
    let Some(raw_data) = state.raw_data.as_ref() else {
        state.encoding = state
            .forced_encoding
            .as_ref()
            .map(|value| value.to_ascii_lowercase())
            .or_else(|| declared_content_type_encoding(state.response_content_type.as_deref()));
        state.body = None;
        state.json = None;
        return;
    };

    let encoding = resolve_effective_encoding(
        state.response_content_type.as_deref(),
        raw_data.as_ref(),
        state.forced_encoding.as_deref(),
    );
    let text = decode_body(raw_data.as_ref(), encoding.as_deref());
    let parsed_json = serde_json::from_str::<Value>(&text).ok();

    state.encoding = encoding;
    state.body = Some(Arc::new(text));
    state.json = parsed_json;
}

fn ensure_response_body_loaded(state: &mut SessionState) -> OpenPageResult<()> {
    if state.raw_data.is_some() || state.pending_response.is_none() {
        return Ok(());
    }

    let pending = state
        .pending_response
        .take()
        .expect("pending response checked");
    let raw_data = Arc::new(
        pending
            .response
            .bytes()
            .map_err(|err| OpenPageError::Http(format!("{err:?}")))?
            .to_vec(),
    );
    state.raw_data = Some(raw_data);
    refresh_state_body_encoding(state);
    Ok(())
}

pub fn snapshot_root(html: &str) -> OpenPageResult<SessionElement> {
    snapshot_root_arc(Arc::new(html.to_string()), None, None)
}

pub fn snapshot_find(html: &str, locator: &str) -> OpenPageResult<SessionElement> {
    snapshot_find_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_find_all(html: &str, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
    snapshot_find_all_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_query_xpath(
    html: &str,
    expression: &str,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_query_xpath_arc(Arc::new(html.to_string()), expression, None, None)
}

pub fn snapshot_fragment_root(html: &str) -> OpenPageResult<SessionElement> {
    snapshot_fragment_root_arc(Arc::new(html.to_string()), None, None)
}

pub fn snapshot_fragment_find(html: &str, locator: &str) -> OpenPageResult<SessionElement> {
    snapshot_fragment_find_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_fragment_find_all(
    html: &str,
    locator: &str,
) -> OpenPageResult<Vec<SessionElement>> {
    snapshot_fragment_find_all_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_fragment_query_xpath(
    html: &str,
    expression: &str,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_fragment_query_xpath_arc(Arc::new(html.to_string()), expression, None)
}

pub fn snapshot_root_with_base_url(
    html: &str,
    base_url: Option<&str>,
) -> OpenPageResult<SessionElement> {
    snapshot_root_arc(
        Arc::new(html.to_string()),
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_find_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<SessionElement> {
    snapshot_find_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_find_all_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionElement>> {
    snapshot_find_all_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_query_xpath_with_base_url(
    html: &str,
    expression: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_query_xpath_arc(
        Arc::new(html.to_string()),
        expression,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_root_with_base_url(
    html: &str,
    base_url: Option<&str>,
) -> OpenPageResult<SessionElement> {
    snapshot_fragment_root_arc(
        Arc::new(html.to_string()),
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_find_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<SessionElement> {
    snapshot_fragment_find_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_find_all_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionElement>> {
    snapshot_fragment_find_all_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_query_xpath_with_base_url(
    html: &str,
    expression: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_fragment_query_xpath_arc(
        Arc::new(html.to_string()),
        expression,
        base_url.map(|value| Arc::new(value.to_string())),
    )
}

fn snapshot_root_arc(
    html: Arc<String>,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionElement> {
    let parsed = Html::parse_document(html.as_ref());
    Ok(session_element_from_ref(
        &html,
        base_url.as_ref(),
        parsed.root_element(),
        none_element_config,
    ))
}

fn snapshot_find_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionElement> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let selector = parse_selector_query(locator.query())?;
            parsed
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &html,
            base_url.as_ref(),
            locator.query(),
            None,
            false,
            none_element_config,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string())),
    }
}

fn snapshot_find_all_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let selector = parse_selector_query(locator.query())?;
            Ok(parsed
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .collect())
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &html,
            base_url.as_ref(),
            locator.query(),
            None,
            false,
            none_element_config,
        ),
    }
}

fn snapshot_query_xpath_arc(
    html: Arc<String>,
    expression: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    xpath_query_with_scope(
        &html,
        base_url.as_ref(),
        expression,
        None,
        false,
        none_element_config,
    )
}

fn snapshot_fragment_root_arc(
    html: Arc<String>,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionElement> {
    let wrapped = wrap_fragment_html(html);
    let parsed = Html::parse_document(wrapped.as_ref());
    let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
        .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
    let mut element = parsed.select(&wrapper_selector).next().ok_or_else(|| {
        OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
    })?;
    while element.attr(FRAGMENT_WRAPPER_ATTR).is_some() {
        element = element.child_elements().next().ok_or_else(|| {
            OpenPageError::ElementNotFound(snapshot_fragment_root_not_found_message())
        })?;
    }
    Ok(session_element_from_ref(
        &wrapped,
        base_url.as_ref(),
        element,
        none_element_config,
    ))
}

fn snapshot_fragment_find_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionElement> {
    let wrapped = wrap_fragment_html(html);
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(wrapped.as_ref());
            let selector = parse_selector_query(locator.query())?;
            let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
                .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
            parsed
                .select(&wrapper_selector)
                .next()
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
                })?
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(
                        &wrapped,
                        base_url.as_ref(),
                        element,
                        none_element_config,
                    )
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &wrapped,
            base_url.as_ref(),
            locator.query(),
            None,
            true,
            none_element_config,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string())),
    }
}

fn snapshot_fragment_find_all_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let wrapped = wrap_fragment_html(html);
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(wrapped.as_ref());
            let selector = parse_selector_query(locator.query())?;
            let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
                .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
            let wrapper = parsed.select(&wrapper_selector).next().ok_or_else(|| {
                OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
            })?;
            Ok(wrapper
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(
                        &wrapped,
                        base_url.as_ref(),
                        element,
                        none_element_config,
                    )
                })
                .collect())
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &wrapped,
            base_url.as_ref(),
            locator.query(),
            None,
            true,
            none_element_config,
        ),
    }
}

fn snapshot_fragment_query_xpath_arc(
    html: Arc<String>,
    expression: &str,
    base_url: Option<Arc<String>>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let wrapped = wrap_fragment_html(html);
    xpath_query_with_scope(&wrapped, base_url.as_ref(), expression, None, true, None)
}

fn find_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionElement> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            let selector = parse_selector_query(locator.query())?;
            scope
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            xpath_find_all_from_scope_element(
                &html,
                base_url.as_ref(),
                scope,
                locator.query(),
                none_element_config,
            )?
            .into_iter()
            .next()
            .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
    }
}

fn find_all_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            let selector = parse_selector_query(locator.query())?;
            Ok(scope
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .collect())
        }
        LocatorKind::XPath => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            xpath_find_all_from_scope_element(
                &html,
                base_url.as_ref(),
                scope,
                locator.query(),
                none_element_config,
            )
        }
    }
}

fn parse_selector_query(query: &str) -> OpenPageResult<Selector> {
    Selector::parse(query).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))
}

fn session_element_from_ref(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> SessionElement {
    SessionElement {
        html: Arc::clone(html),
        node_id: element.id(),
        base_url: base_url.cloned(),
        none_element_config: none_element_config.cloned(),
    }
}

fn wrap_fragment_html(html: Arc<String>) -> Arc<String> {
    Arc::new(format!(
        "<!doctype html><html><body><div {FRAGMENT_WRAPPER_ATTR}=\"1\">{}</div></body></html>",
        html.as_ref()
    ))
}

fn element_from_document<'a>(
    document: &'a Html,
    node_id: NodeId,
) -> OpenPageResult<ElementRef<'a>> {
    document
        .tree
        .get(node_id)
        .and_then(ElementRef::wrap)
        .ok_or_else(|| OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message()))
}

#[derive(Clone, Copy)]
enum RelativeDirection {
    Before,
    After,
}

fn collect_matching_elements<'a, I>(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    elements: I,
    locator: Option<&str>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>>
where
    I: IntoIterator<Item = ElementRef<'a>>,
{
    let selector = parse_optional_selector(locator)?;
    Ok(elements
        .into_iter()
        .filter(|element| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(element))
        })
        .map(|element| session_element_from_ref(html, base_url, element, none_element_config))
        .collect())
}

fn document_relatives(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    direction: RelativeDirection,
    locator: Option<&str>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let selector = parse_optional_selector(locator)?;
    let root = element.tree().root();
    let elements: Vec<_> = root.descendants().filter_map(ElementRef::wrap).collect();
    let current_id = element.id();
    let current_index = elements
        .iter()
        .position(|candidate| candidate.id() == current_id)
        .ok_or_else(|| OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message()))?;

    let ancestor_ids: HashSet<_> = element.ancestors().map(|node| node.id()).collect();
    let descendant_ids: HashSet<_> = element
        .descendants()
        .skip(1)
        .map(|node| node.id())
        .collect();

    let iter: Box<dyn Iterator<Item = ElementRef<'_>> + '_> = match direction {
        RelativeDirection::Before => Box::new(
            elements[..current_index]
                .iter()
                .copied()
                .filter(|candidate| !ancestor_ids.contains(&candidate.id())),
        ),
        RelativeDirection::After => Box::new(
            elements[current_index + 1..]
                .iter()
                .copied()
                .filter(|candidate| !descendant_ids.contains(&candidate.id())),
        ),
    };

    Ok(iter
        .filter(|candidate| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(candidate))
        })
        .map(|candidate| session_element_from_ref(html, base_url, candidate, none_element_config))
        .collect())
}

fn collect_document_relative_nodes(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    direction: RelativeDirection,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let mut path: Vec<_> = element.ancestors().collect();
    path.reverse();
    path.push(*element);
    let mut candidates = Vec::new();

    match direction {
        RelativeDirection::Before => {
            for window in path.windows(2) {
                append_atomic_siblings(&mut candidates, window[1], SiblingDirection::Before);
            }
        }
        RelativeDirection::After => {
            for window in path.windows(2).rev() {
                append_atomic_siblings(&mut candidates, window[1], SiblingDirection::After);
            }
        }
    }

    filter_relative_node_results(
        candidates
            .into_iter()
            .map(|node| {
                scraper_node_to_session_xpath_result(html, base_url, node, none_element_config)
            })
            .collect::<OpenPageResult<Vec<_>>>()?,
        false,
    )
}

#[derive(Clone, Copy, Debug)]
enum SiblingDirection {
    Before,
    After,
}

fn append_atomic_siblings<'a>(
    output: &mut Vec<NodeRef<'a, Node>>,
    node: NodeRef<'a, Node>,
    direction: SiblingDirection,
) {
    let mut siblings: Vec<_> = match direction {
        SiblingDirection::Before => node.prev_siblings().collect(),
        SiblingDirection::After => node.next_siblings().collect(),
    };
    if matches!(direction, SiblingDirection::Before) {
        siblings.reverse();
    }
    output.extend(siblings);
}

fn scraper_node_to_session_xpath_result(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    node: NodeRef<'_, Node>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionXPathResult> {
    match node.value() {
        Node::Element(_) => Ok(SessionXPathResult::Element(session_element_from_ref(
            html,
            base_url,
            ElementRef::wrap(node).ok_or_else(|| {
                OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message())
            })?,
            none_element_config,
        ))),
        Node::Text(text) => Ok(SessionXPathResult::Text(text.to_string())),
        Node::Comment(comment) => Ok(SessionXPathResult::Comment(comment.to_string())),
        Node::Doctype(doctype) => Ok(SessionXPathResult::Doctype {
            name: doctype.name.to_string(),
            public_id: (!doctype.public_id.is_empty()).then(|| doctype.public_id.to_string()),
            system_id: (!doctype.system_id.is_empty()).then(|| doctype.system_id.to_string()),
        }),
        _ => Err(OpenPageError::ElementNotFound(
            unsupported_snapshot_node_kind_message(),
        )),
    }
}

fn resolve_href_attr(value: &str, base_url: Option<&String>) -> Option<String> {
    if value.is_empty()
        || value.to_ascii_lowercase().starts_with("javascript:")
        || value.to_ascii_lowercase().starts_with("mailto:")
    {
        return Some(value.to_string());
    }
    resolve_src_attr(value, base_url)
}

fn resolve_src_attr(value: &str, base_url: Option<&String>) -> Option<String> {
    if value.is_empty() {
        return Some(String::new());
    }
    Some(make_absolute_url(value, base_url))
}

fn make_absolute_url(value: &str, base_url: Option<&String>) -> String {
    let Some(base_url) = base_url else {
        return value.to_string();
    };
    Url::parse(base_url)
        .and_then(|base| base.join(value))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn normalize_text_item(value: &str) -> Option<String> {
    let normalized = value.replace('\u{a0}', " ").trim().to_string();
    if normalized.chars().any(|char| !char.is_whitespace()) {
        Some(normalized)
    } else {
        None
    }
}

fn xpath_for_element(element: ElementRef<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = Some(element);
    while let Some(node) = current {
        if node.value().attr(FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let tag = node.value().name();
        let index = node
            .prev_siblings()
            .filter_map(ElementRef::wrap)
            .filter(|sibling| sibling.value().name() == tag)
            .count()
            + 1;
        parts.push(format!("/{tag}[{index}]"));
        current = nearest_parent_element(node);
    }
    parts.reverse();
    parts.join("")
}

fn css_path_for_element(element: ElementRef<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = Some(element);
    while let Some(node) = current {
        if node.value().attr(FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let tag = node.value().name();
        if let Some(id) = node.attr("id") {
            parts.push(format!(">{tag}#{id}"));
            current = nearest_parent_element(node);
            continue;
        }
        let index = node.prev_siblings().filter_map(ElementRef::wrap).count() + 1;
        parts.push(format!(">{tag}:nth-child({index})"));
        current = nearest_parent_element(node);
    }
    parts.reverse();
    parts.join("").trim_start_matches('>').to_string()
}

fn nth_from_start<T>(elements: Vec<T>, index: usize, error_message: &str) -> OpenPageResult<T> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    elements
        .into_iter()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn nth_from_end<T>(elements: Vec<T>, index: usize, error_message: &str) -> OpenPageResult<T> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    let len = elements.len();
    if index > len {
        return Err(OpenPageError::ElementNotFound(error_message.to_string()));
    }
    elements
        .into_iter()
        .nth(len - index)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn parse_optional_locator(locator: Option<&str>) -> OpenPageResult<Option<Locator>> {
    locator
        .map(str::trim)
        .filter(|locator| !locator.is_empty())
        .map(Locator::parse)
        .transpose()
}

fn parse_optional_xpath_locator(locator: Option<&str>) -> OpenPageResult<Option<Locator>> {
    let locator = parse_optional_locator(locator)?;
    match locator {
        Some(locator) if locator.kind() == LocatorKind::Css => {
            Err(OpenPageError::UnsupportedLocator(
                "css locator is not supported for node queries".to_string(),
            ))
        }
        other => Ok(other),
    }
}

fn parse_optional_xpath_locator_input<'a, L>(locator: Option<L>) -> OpenPageResult<Option<Locator>>
where
    L: Into<LocatorInput<'a>>,
{
    let locator = parse_optional_locator_input(locator)?;
    match locator {
        Some(locator) if locator.kind() == LocatorKind::Css => {
            Err(OpenPageError::UnsupportedLocator(
                "css locator is not supported for node queries".to_string(),
            ))
        }
        other => Ok(other),
    }
}

fn parse_optional_selector(locator: Option<&str>) -> OpenPageResult<Option<Selector>> {
    let locator = parse_optional_locator(locator)?;
    locator
        .map(|locator| match locator.kind() {
            LocatorKind::Css => parse_selector_query(locator.query()),
            LocatorKind::XPath => Err(OpenPageError::UnsupportedLocator(
                "xpath locator is not valid for CSS filtering".to_string(),
            )),
        })
        .transpose()
}

fn direct_child_xpath_query(query: &str) -> String {
    format!("./{}", trim_xpath_axis_target(query))
}

fn relative_axis_xpath_query(axis: &str, query: &str) -> String {
    format!("./{axis}::{}", trim_xpath_axis_target(query))
}

fn trim_xpath_axis_target(query: &str) -> &str {
    let trimmed = query.trim().trim_start_matches(['.', '/']);
    if trimmed.is_empty() { "*" } else { trimmed }
}

fn normalize_scoped_xpath_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.starts_with('/') {
        format!(".{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn xpath_find_all_from_scope_element(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    query: &str,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let scope_path = xpath_for_element(scope);
    let scope_at_fragment_root = nearest_fragment_wrapper(scope).is_some();
    xpath_find_all_with_scope(
        html,
        base_url,
        query,
        Some(scope_path.as_str()),
        scope_at_fragment_root,
        none_element_config,
    )
}

fn xpath_query_from_scope_element(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    query: &str,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let scope_path = xpath_for_element(scope);
    let scope_at_fragment_root = nearest_fragment_wrapper(scope).is_some();
    xpath_query_with_scope(
        html,
        base_url,
        query,
        Some(scope_path.as_str()),
        scope_at_fragment_root,
        none_element_config,
    )
}

fn relative_node_xpath_query<F>(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    locator: Option<&str>,
    default_query: &str,
    query_builder: F,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>>
where
    F: FnOnce(&str) -> String,
{
    let (query, keep_attributes) = match parse_optional_xpath_locator(locator)? {
        Some(locator) => (
            query_builder(locator.query()),
            xpath_query_requests_attributes(locator.query()),
        ),
        None => (default_query.to_string(), false),
    };
    filter_relative_node_results(
        xpath_query_from_scope_element(html, base_url, scope, &query, none_element_config)?,
        keep_attributes,
    )
}

fn relative_node_xpath_query_with_locator<F>(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    locator: Option<&Locator>,
    default_query: &str,
    query_builder: F,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>>
where
    F: FnOnce(&str) -> String,
{
    let (query, keep_attributes) = match locator {
        Some(locator) => (
            query_builder(locator.query()),
            xpath_query_requests_attributes(locator.query()),
        ),
        None => (default_query.to_string(), false),
    };
    filter_relative_node_results(
        xpath_query_from_scope_element(html, base_url, scope, &query, none_element_config)?,
        keep_attributes,
    )
}

fn filter_relative_node_results(
    items: Vec<SessionXPathResult>,
    keep_attributes: bool,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    Ok(items
        .into_iter()
        .filter(|item| match item {
            SessionXPathResult::Text(value) => normalize_text_item(value).is_some(),
            SessionXPathResult::Attribute { .. } => keep_attributes,
            _ => true,
        })
        .collect())
}

fn xpath_query_requests_attributes(query: &str) -> bool {
    let query = query.trim();
    query.contains('@') || query.contains("attribute::")
}

fn xpath_find_all_with_scope(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    query: &str,
    scope_path: Option<&str>,
    scope_at_fragment_root: bool,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionElement>> {
    let parsed = Html::parse_document(html.as_ref());
    let xpath_tree = xpath_html::parse(html.as_ref()).map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_html_message(&err.to_string()))
    })?;
    let xpath = xpath_engine::parse(&if scope_path.is_some() || scope_at_fragment_root {
        normalize_scoped_xpath_query(query)
    } else {
        query.trim().to_string()
    })
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let wrapper_scraper = if scope_at_fragment_root {
        Some(fragment_wrapper_from_document(&parsed)?)
    } else {
        None
    };
    let wrapper_xpath = if scope_at_fragment_root {
        Some(fragment_wrapper_from_xpath_tree(&xpath_tree)?)
    } else {
        None
    };

    let xpath_matches = match scope_path {
        Some(scope_path) => {
            let scope = find_xpath_element_by_path(
                &xpath_tree,
                wrapper_xpath,
                scope_path,
                scope_at_fragment_root,
            )?;
            xpath.find_elements_from_element(&xpath_tree, scope)
        }
        None => match wrapper_xpath {
            Some(wrapper) => xpath.find_elements_from_element(&xpath_tree, wrapper),
            None => xpath.find_elements(&xpath_tree),
        },
    }
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let mapping_root = wrapper_scraper.map_or_else(|| parsed.tree.root(), |wrapper| *wrapper);
    xpath_matches
        .into_iter()
        .filter(|element| {
            !scope_at_fragment_root || xpath_element_is_within_fragment_root(&xpath_tree, element)
        })
        .map(|element| {
            let path = xpath_path_for_xpath_element(&xpath_tree, element, scope_at_fragment_root)?;
            let mapped = find_scraper_element_by_path(mapping_root, &path)?;
            Ok(session_element_from_ref(
                html,
                base_url,
                mapped,
                none_element_config,
            ))
        })
        .collect()
}

fn xpath_query_with_scope(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    query: &str,
    scope_path: Option<&str>,
    scope_at_fragment_root: bool,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let parsed = Html::parse_document(html.as_ref());
    let xpath_tree = xpath_html::parse(html.as_ref()).map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_html_message(&err.to_string()))
    })?;
    let xpath = xpath_engine::parse(&if scope_path.is_some() || scope_at_fragment_root {
        normalize_scoped_xpath_query(query)
    } else {
        query.trim().to_string()
    })
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let wrapper_scraper = if scope_at_fragment_root {
        Some(fragment_wrapper_from_document(&parsed)?)
    } else {
        None
    };
    let wrapper_xpath = if scope_at_fragment_root {
        Some(fragment_wrapper_from_xpath_tree(&xpath_tree)?)
    } else {
        None
    };

    let xpath_items = match scope_path {
        Some(scope_path) => {
            let scope = find_xpath_element_by_path(
                &xpath_tree,
                wrapper_xpath,
                scope_path,
                scope_at_fragment_root,
            )?;
            xpath.apply_to_element(&xpath_tree, scope)
        }
        None => match wrapper_xpath {
            Some(wrapper) => xpath.apply_to_element(&xpath_tree, wrapper),
            None => xpath.apply(&xpath_tree),
        },
    }
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let mapping_root = wrapper_scraper.map_or_else(|| parsed.tree.root(), |wrapper| *wrapper);
    xpath_items
        .into_iter()
        .filter(|item| {
            !scope_at_fragment_root || xpath_item_is_within_fragment_root(&xpath_tree, item)
        })
        .map(|item| {
            xpath_item_to_session_result(
                &xpath_tree,
                html,
                base_url,
                mapping_root,
                scope_at_fragment_root,
                item,
                none_element_config,
            )
        })
        .collect()
}

fn xpath_item_to_session_result(
    tree: &XpathItemTree,
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    mapping_root: NodeRef<'_, Node>,
    stop_at_fragment_root: bool,
    item: XpathItem<'_>,
    none_element_config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<SessionXPathResult> {
    match item {
        XpathItem::Node(node) => match node {
            XpathItemTreeNode::DocumentNode(_) => Ok(SessionXPathResult::Document),
            XpathItemTreeNode::ElementNode(element) => {
                let path = xpath_path_for_xpath_element(tree, element, stop_at_fragment_root)?;
                let mapped = find_scraper_element_by_path(mapping_root, &path)?;
                Ok(SessionXPathResult::Element(session_element_from_ref(
                    html,
                    base_url,
                    mapped,
                    none_element_config,
                )))
            }
            XpathItemTreeNode::TextNode(text) => Ok(SessionXPathResult::Text(text.content.clone())),
            XpathItemTreeNode::CommentNode(comment) => {
                Ok(SessionXPathResult::Comment(comment.content.clone()))
            }
            XpathItemTreeNode::AttributeNode(attribute) => Ok(SessionXPathResult::Attribute {
                name: attribute.name.clone(),
                value: attribute.value.clone(),
            }),
            XpathItemTreeNode::PINode(node) => Ok(SessionXPathResult::ProcessingInstruction {
                target: node.target.clone(),
                data: node.data.clone(),
            }),
            XpathItemTreeNode::DoctypeNode(node) => Ok(SessionXPathResult::Doctype {
                name: node.name.clone(),
                public_id: node.public_id.clone(),
                system_id: node.system_id.clone(),
            }),
        },
        XpathItem::AnyAtomicType(value) => Ok(xpath_atomic_to_session_result(value)),
        XpathItem::Function(function) => Ok(SessionXPathResult::Function(function.to_string())),
    }
}

fn xpath_atomic_to_session_result(value: AnyAtomicType) -> SessionXPathResult {
    match value {
        AnyAtomicType::Boolean(value) => SessionXPathResult::Boolean(value),
        AnyAtomicType::Integer(value) => SessionXPathResult::Integer(value),
        AnyAtomicType::Float(value) => SessionXPathResult::Number(value.0 as f64),
        AnyAtomicType::Double(value) => SessionXPathResult::Number(value.0),
        AnyAtomicType::String(value) => SessionXPathResult::String(value),
        AnyAtomicType::QName {
            namespace_uri,
            local_name,
            prefix,
        } => SessionXPathResult::QName {
            namespace_uri,
            local_name,
            prefix,
        },
    }
}

fn fragment_wrapper_from_document(document: &Html) -> OpenPageResult<ElementRef<'_>> {
    let selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
        .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
    document.select(&selector).next().ok_or_else(|| {
        OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
    })
}

fn fragment_wrapper_from_xpath_tree(tree: &XpathItemTree) -> OpenPageResult<&XpathElementNode> {
    tree.iter()
        .filter_map(|node| node.as_element_node().ok())
        .find(|element| element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some())
        .ok_or_else(
            || OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message()),
        )
}

fn nearest_fragment_wrapper(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    element
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|candidate| candidate.attr(FRAGMENT_WRAPPER_ATTR).is_some())
}

fn xpath_path_for_xpath_element(
    tree: &XpathItemTree,
    element: &XpathElementNode,
    stop_at_fragment_root: bool,
) -> OpenPageResult<String> {
    let mut parts = Vec::new();
    let mut current = Some(element);

    while let Some(node) = current {
        if stop_at_fragment_root && node.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let parent = node.parent(tree);
        let index = xpath_element_index_in_parent(tree, node)?;
        parts.push(format!("/{}[{index}]", node.name));
        current = match parent.and_then(|parent| parent.as_element_node().ok()) {
            Some(parent)
                if stop_at_fragment_root
                    && parent.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() =>
            {
                None
            }
            other => other,
        };
    }

    parts.reverse();
    Ok(parts.join(""))
}

fn xpath_element_index_in_parent(
    tree: &XpathItemTree,
    element: &XpathElementNode,
) -> OpenPageResult<usize> {
    let Some(parent) = element.parent(tree) else {
        return Ok(1);
    };

    let mut index = 0;
    for child in parent.children(tree) {
        let Ok(child_element) = child.as_element_node() else {
            continue;
        };
        if child_element.name != element.name {
            continue;
        }
        index += 1;
        if std::ptr::eq(child_element, element) {
            return Ok(index);
        }
    }

    Err(OpenPageError::ElementNotFound(
        xpath_node_no_longer_exists_message(),
    ))
}

fn find_xpath_element_by_path<'a>(
    tree: &'a XpathItemTree,
    wrapper: Option<&'a XpathElementNode>,
    path: &str,
    scope_at_fragment_root: bool,
) -> OpenPageResult<&'a XpathElementNode> {
    let segments = parse_xpath_path(path)?;
    let mut current_children = if let Some(wrapper) = wrapper {
        wrapper.children(tree).collect::<Vec<_>>()
    } else {
        tree.root().children(tree)
    };
    let mut current = None;

    for (index, segment) in segments.iter().enumerate() {
        let next = nth_xpath_child_by_tag(&current_children, &segment.tag, segment.index)?;
        current = Some(next);
        if index + 1 < segments.len() {
            current_children = next.children(tree).collect();
        }
    }

    let element = current.ok_or_else(|| OpenPageError::ElementNotFound(path.to_string()))?;
    if scope_at_fragment_root && element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
        return Err(OpenPageError::ElementNotFound(path.to_string()));
    }
    Ok(element)
}

fn nth_xpath_child_by_tag<'a>(
    children: &[&'a XpathItemTreeNode],
    tag: &str,
    index: usize,
) -> OpenPageResult<&'a XpathElementNode> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            invalid_xpath_segment_index_message(tag),
        ));
    }
    children
        .iter()
        .filter_map(|child| child.as_element_node().ok())
        .filter(|child| child.name == tag)
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_segment_not_found_message(tag, index)))
}

fn find_scraper_element_by_path<'a>(
    start: NodeRef<'a, Node>,
    path: &str,
) -> OpenPageResult<ElementRef<'a>> {
    let root = top_scraper_node(start);
    let start_id = start.id();
    match find_scraper_element_by_path_from(start, path) {
        Ok(element) => Ok(element),
        Err(err) if path.starts_with('/') && root.id() != start_id => {
            let element = find_scraper_element_by_path_from(root, path)?;
            if element.id() == start_id || element.ancestors().any(|node| node.id() == start_id) {
                Ok(element)
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err),
    }
}

fn find_scraper_element_by_path_from<'a>(
    start: NodeRef<'a, Node>,
    path: &str,
) -> OpenPageResult<ElementRef<'a>> {
    let segments = parse_xpath_path(path)?;
    let mut current = start;

    for segment in segments {
        current = nth_scraper_child_by_tag(current, &segment.tag, segment.index)?;
    }

    ElementRef::wrap(current)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_path_not_found_message(path)))
}

fn nth_scraper_child_by_tag<'a>(
    node: NodeRef<'a, Node>,
    tag: &str,
    index: usize,
) -> OpenPageResult<NodeRef<'a, Node>> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            invalid_xpath_segment_index_message(tag),
        ));
    }
    node.children()
        .filter_map(ElementRef::wrap)
        .filter(|child| child.value().name() == tag)
        .nth(index - 1)
        .map(|child| *child)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_segment_not_found_message(tag, index)))
}

fn top_scraper_node(node: NodeRef<'_, Node>) -> NodeRef<'_, Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        current = parent;
    }
    current
}

fn xpath_element_is_within_fragment_root(tree: &XpathItemTree, element: &XpathElementNode) -> bool {
    let mut current = Some(element);
    while let Some(node) = current {
        if node.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
            return true;
        }
        current = node
            .parent(tree)
            .and_then(|parent| parent.as_element_node().ok());
    }
    false
}

fn xpath_item_is_within_fragment_root(tree: &XpathItemTree, item: &XpathItem<'_>) -> bool {
    let XpathItem::Node(node) = item else {
        return true;
    };
    let mut current = Some(*node);
    while let Some(node) = current {
        if let Ok(element) = node.as_element_node() {
            if element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
                return true;
            }
        }
        current = node.parent(tree);
    }
    false
}

#[derive(Debug)]
struct XPathPathSegment {
    tag: String,
    index: usize,
}

fn parse_xpath_path(path: &str) -> OpenPageResult<Vec<XPathPathSegment>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let (tag, index) = segment
                .rsplit_once('[')
                .and_then(|(tag, rest)| rest.strip_suffix(']').map(|index| (tag, index)))
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound(unsupported_xpath_path_message(path))
                })?;
            let index = index.parse::<usize>().map_err(|_| {
                OpenPageError::ElementNotFound(unsupported_xpath_path_message(path))
            })?;
            Ok(XPathPathSegment {
                tag: tag.to_string(),
                index,
            })
        })
        .collect()
}

fn nearest_parent_element(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut current = element.parent();
    while let Some(node) = current {
        if let Some(parent) = ElementRef::wrap(node) {
            return Some(parent);
        }
        current = node.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        CookieInput, SessionAdapter, SessionAdapterMount, SessionCert, SessionCookieParam,
        SessionElement, SessionHandle, SessionHooks, SessionOptions, SessionPage,
        SessionRequestOptions, SessionXPathResult, append_query_params, cookie_assignment,
        cookie_input_to_params, default_referer_header, nth_scraper_child_by_tag, parse_xpath_path,
        remove_cookie_from_header, resolve_local_file_path, resolve_session_options_ini_path,
        snapshot_find, snapshot_find_all, snapshot_fragment_find, snapshot_fragment_root,
        snapshot_fragment_root_with_base_url, snapshot_root,
    };
    use crate::settings::scoped_test_settings;
    use crate::{By, ElementsListExt, LocatorInput, OpenPageError, Settings};
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use scraper::Html;
    use serde_json::json;
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{LazyLock, Mutex};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use url::Url;

    static CURRENT_DIR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    const HTML: &str = r#"
<!doctype html>
<html>
  <body>
    <section id="root">
      <h1>OpenPage</h1>
      <input id="name" value="openpage" />
      <button id="submit">Go</button>
      <div id="out"></div>
      <ul class="items">
        <li class="item" data-kind="a">alpha</li>
        <li class="item" data-kind="b">beta</li>
      </ul>
    </section>
  </body>
</html>
"#;

    fn load_session_html_for_test(
        page: &SessionPage,
        html: &str,
        url: Option<&str>,
    ) -> crate::OpenPageResult<()> {
        let mut state = page
            .inner
            .lock()
            .map_err(|_| OpenPageError::PageOperation("session state lock poisoned".to_string()))?;
        state.url = url.map(str::to_string);
        state.response_content_type = Some("text/html; charset=utf-8".to_string());
        state.pending_response = None;
        state.raw_data = Some(Arc::new(html.as_bytes().to_vec()));
        super::refresh_state_body_encoding(&mut state);
        Ok(())
    }

    fn poison_mutex<T: Send + 'static>(mutex: Arc<Mutex<T>>) {
        let join = thread::spawn(move || {
            let _guard = mutex.lock().expect("lock poisoned test mutex");
            panic!("poison mutex");
        })
        .join();
        assert!(join.is_err(), "poison helper thread should panic");
    }

    #[test]
    fn snapshot_find_supports_nested_queries() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let heading = root.find("h1").expect("nested heading should exist");
        let first_item = root.find(".item").expect("nested item should exist");

        assert_eq!(
            heading.text().expect("heading text"),
            Some("OpenPage".to_string())
        );
        assert_eq!(heading.tag().expect("heading tag"), "h1".to_string());
        assert_eq!(
            first_item.attr("data-kind").expect("item attr"),
            Some("a".to_string())
        );
        let attrs = first_item.attrs().expect("item attrs");
        assert!(attrs.contains(&("class".to_string(), "item".to_string())));
        assert!(attrs.contains(&("data-kind".to_string(), "a".to_string())));
    }

    #[test]
    fn snapshot_find_all_keeps_match_order() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let items = root.find_all(".item").expect("items should exist");

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].text().expect("first item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            items[1].text().expect("second item text"),
            Some("beta".to_string())
        );

        let top_level = snapshot_find_all(HTML, ".item").expect("top-level items should exist");
        assert_eq!(top_level.len(), 2);
    }

    #[test]
    fn session_elements_one_runtime_config_supports_none_value_and_raise() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <div class="item" data-role="keep">Alpha</div>
                <div class="item" data-role="other">Beta</div>
              </body>
            </html>
            "#,
            Some("https://example.com/items"),
        )
        .expect("load session html");

        page.set_none_element_value(Some("missing"), true)
            .expect("set session none element value");
        let items = page.find_all(".item").expect("session items");
        let missing = items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect("session missing filter");
        assert_eq!(
            missing.text().expect("missing text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.attr("id").expect("missing attr"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.texts(false).expect("missing texts"),
            Some(vec!["missing".to_string()])
        );
        assert_eq!(
            missing.comments().expect("missing comments"),
            Some(vec!["missing".to_string()])
        );

        page.set_raise_when_ele_not_found(true)
            .expect("set session raise when missing");
        let error = items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("session missing filter should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session filter error: {error}"
        );
    }

    #[test]
    fn session_page_inherits_global_raise_when_ele_not_found_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_raise_when_ele_not_found(true);

        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <div class="item" data-role="keep">Alpha</div>
                <div class="item" data-role="other">Beta</div>
              </body>
            </html>
            "#,
            Some("https://example.com/items"),
        )
        .expect("load session html");

        let error = page
            .find_all(".item")
            .expect("session items")
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("session missing filter should use global raise setting");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session filter error: {error}"
        );
    }

    #[test]
    fn session_ele_runtime_config_supports_none_value_and_nested_queries() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <section id="card">
                  <span class="name">Alpha</span>
                  <span class="phone">10086</span>
                </section>
                <section id="tail">Omega</section>
              </body>
            </html>
            "#,
            Some("https://example.com/card"),
        )
        .expect("load session html");

        assert_eq!(page.eles(".missing").expect("missing list").len(), 0);

        let card = page.ele("#card").expect("card ele");
        assert!(card.is_some());
        assert_eq!(
            card.ele(".name")
                .expect("name ele")
                .text()
                .expect("name text"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            card.child()
                .expect("card child")
                .text()
                .expect("card child text"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            page.ele(".name")
                .expect("name ele")
                .parent()
                .expect("name parent")
                .attr("id")
                .expect("name parent attr"),
            Some("card".to_string())
        );
        assert_eq!(
            page.ele(".name")
                .expect("name ele")
                .next()
                .expect("name next")
                .text()
                .expect("name next text"),
            Some("10086".to_string())
        );
        assert_eq!(
            page.ele(".phone")
                .expect("phone ele")
                .after()
                .expect("phone after")
                .text()
                .expect("phone after text"),
            Some("Omega".to_string())
        );

        let missing_default = page.ele(".missing").expect("missing ele");
        assert!(missing_default.is_none());
        assert_eq!(missing_default.text().expect("missing default text"), None);

        page.set_none_element_value(Some("missing"), true)
            .expect("set session none element value");

        let missing = page.ele(".missing").expect("missing ele after none value");
        assert_eq!(
            missing.text().expect("missing text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.attr("id").expect("missing attr"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .ele(".child")
                .expect("missing child ele")
                .text()
                .expect("missing child text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .child()
                .expect("missing child")
                .text()
                .expect("missing child text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .parent()
                .expect("missing parent")
                .text()
                .expect("missing parent text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .next()
                .expect("missing next")
                .text()
                .expect("missing next text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .before()
                .expect("missing before")
                .text()
                .expect("missing before text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .after()
                .expect("missing after")
                .text()
                .expect("missing after text"),
            Some("missing".to_string())
        );
        assert_eq!(
            page.ele("#card")
                .expect("card ele after none value")
                .ele(".phone")
                .expect("missing phone ele")
                .text()
                .expect("missing phone text"),
            Some("10086".to_string())
        );
        assert_eq!(
            page.ele("#card")
                .expect("card ele after none value")
                .child_with(Some(".missing"), 1)
                .expect("missing child wrapper")
                .text()
                .expect("missing child wrapper text"),
            Some("missing".to_string())
        );
        assert_eq!(
            page.ele(".phone")
                .expect("phone ele after none value")
                .next_with(Some(".missing"), 1)
                .expect("missing next wrapper")
                .text()
                .expect("missing next wrapper text"),
            Some("missing".to_string())
        );

        page.set_raise_when_ele_not_found(true)
            .expect("set session raise when missing");
        let error = page.ele(".missing").expect_err("session ele should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session ele error: {error}"
        );
        let error = page
            .ele("#card")
            .expect("card ele after raise")
            .child_with(Some(".missing"), 1)
            .expect_err("missing child should raise after toggle");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session child error: {error}"
        );
    }

    #[test]
    fn cookie_assignment_includes_optional_scope_fields() {
        let cookie = cookie_assignment("foo", "bar", Some("example.com"), Some("/demo"));
        assert_eq!(cookie, "foo=bar; Domain=example.com; Path=/demo");
    }

    #[test]
    fn remove_cookie_from_header_drops_matching_cookie_names() {
        let header = remove_cookie_from_header("foo=bar; baz=qux; foo=next", "foo");
        assert_eq!(header, "baz=qux");
    }

    fn spawn_retry_server(
        statuses: &'static [&'static str],
        bodies: &'static [&'static str],
        attempts: Arc<AtomicUsize>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind retry server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let mut methods = Vec::new();
            for (index, status) in statuses.iter().enumerate() {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                methods.push(
                    request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().next())
                        .unwrap_or_default()
                        .to_string(),
                );
                attempts.fetch_add(1, Ordering::SeqCst);

                let body = bodies[index];
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
            methods
        });
        (address, handle)
    }

    fn make_temp_file(name: &str, content: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("openpage-{name}-{suffix}.html"));
        fs::write(&path, content).expect("write temp file");
        path
    }

    fn make_temp_bytes(name: &str, content: &[u8]) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("openpage-{name}-{suffix}.bin"));
        fs::write(&path, content).expect("write temp bytes");
        path
    }

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("openpage-{name}-{suffix}"))
    }

    struct RestoreFileGuard {
        path: std::path::PathBuf,
        original: Option<Vec<u8>>,
    }

    impl RestoreFileGuard {
        fn new(path: std::path::PathBuf) -> Self {
            Self {
                original: fs::read(&path).ok(),
                path,
            }
        }
    }

    impl Drop for RestoreFileGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                let _ = fs::write(&self.path, original);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    struct CurrentDirGuard {
        original: std::path::PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurrentDirGuard {
        fn change_to(path: &std::path::Path) -> Self {
            let lock = CURRENT_DIR_TEST_LOCK.lock().expect("lock current dir");
            let original = env::current_dir().expect("read current dir");
            env::set_current_dir(path).expect("set current dir");
            Self {
                original,
                _lock: lock,
            }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }

    fn spawn_capture_server(
        status: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind capture server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write response");
            request
        });
        (address, handle)
    }

    fn spawn_redirect_server(max_requests: usize) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let redirect_target = format!("{address}/final");
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..max_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let first_line = request.lines().next().unwrap_or_default().to_string();
                requests.push(first_line.clone());

                if first_line.contains("/final ") {
                    let body = "done";
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .expect("write final response");
                } else {
                    let body = "redirect";
                    write!(
                        stream,
                        "HTTP/1.1 302 Found\r\nLocation: {redirect_target}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .expect("write redirect response");
                }
            }
            requests
        });
        (address, handle)
    }

    fn spawn_delayed_server(delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind delayed server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            thread::sleep(delay);
            let body = "slow";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        (address, handle)
    }

    fn spawn_truncated_body_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let body = "short";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 999\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
            );
        });
        (address, handle)
    }

    #[test]
    fn session_options_default_http_settings_match_reference_behavior() {
        let options = SessionOptions::default();

        assert_eq!(options.timeout_secs, 10);
        assert!(options.verify);
        assert!(options.trust_env);
        assert_eq!(options.max_redirects, Some(30));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert!(options.headers.is_empty());
        assert!(options.cookies.is_empty());
        assert!(options.params.is_empty());
        assert!(options.auth.is_none());
        assert!(options.hooks().is_empty());
        assert!(!options.stream);
        assert!(options.http_proxy.is_none());
        assert!(options.https_proxy.is_none());
        assert!(options.adapters().is_empty());
    }

    #[test]
    fn session_options_response_hooks_fire_for_page_requests() {
        let captured = Arc::new(Mutex::new(Vec::<(
            String,
            Option<String>,
            Option<u16>,
            String,
        )>::new()));
        let captured_for_hook = Arc::clone(&captured);
        let mut hooks = SessionHooks::new();
        hooks.add_response(move |event| {
            let body = String::from_utf8(event.raw_data.as_ref().clone()).expect("hook body utf8");
            captured_for_hook.lock().expect("lock hook capture").push((
                event.requested_url,
                event.response.url.clone(),
                event.response.status_code,
                body,
            ));
        });

        let mut options = SessionOptions::default();
        options.set_hooks(hooks);
        let page = SessionPage::new(options).expect("session page");
        let (address, handle) = spawn_capture_server("200 OK", "hooked");
        let url = format!("{address}/hook");

        assert!(page.get(&url).expect("request with response hook"));
        handle.join().expect("server thread");

        let records = captured.lock().expect("lock hook records");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, url);
        assert_eq!(records[0].1.as_deref(), Some(records[0].0.as_str()));
        assert_eq!(records[0].2, Some(200));
        assert_eq!(records[0].3, "hooked");
    }

    #[test]
    fn session_request_options_response_hooks_extend_runtime_hooks() {
        let labels = Arc::new(Mutex::new(Vec::<String>::new()));
        let labels_for_runtime = Arc::clone(&labels);
        let labels_for_request = Arc::clone(&labels);

        let mut runtime_hooks = SessionHooks::new();
        runtime_hooks.add_response(move |_| {
            labels_for_runtime
                .lock()
                .expect("lock runtime labels")
                .push("runtime".to_string());
        });

        let mut request_hooks = SessionHooks::new();
        request_hooks.add_response(move |_| {
            labels_for_request
                .lock()
                .expect("lock request labels")
                .push("request".to_string());
        });

        let mut options = SessionOptions::default();
        options.set_hooks(runtime_hooks);
        let page = SessionPage::new(options).expect("session page");
        let request_options = SessionRequestOptions {
            hooks: Some(request_hooks),
            ..SessionRequestOptions::default()
        };
        let (address, handle) = spawn_capture_server("200 OK", "hooks");
        let url = format!("{address}/extend");

        assert!(
            page.get_with_options(&url, &request_options)
                .expect("request with runtime + request hooks")
        );
        handle.join().expect("server thread");

        let labels = labels.lock().expect("lock response hook labels");
        assert_eq!(labels.as_slice(), ["runtime", "request"]);
    }

    #[test]
    fn session_options_save_does_not_persist_hooks() {
        let dir = make_temp_dir("session-hooks-save");
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("session.ini");

        let mut hooks = SessionHooks::new();
        hooks.add_response(|_| {});
        let mut adapter = SessionAdapter::new();
        adapter.set_timeout(27).set_verify(false);

        let mut options = SessionOptions::default();
        options
            .set_user_agent(Some("OpenPage/HookSave".to_string()))
            .set_hooks(hooks)
            .set_stream(true)
            .add_adapter("http://example.test/api/", adapter);

        let saved_path = options
            .save(Some(path.as_path()))
            .expect("save session options with hooks");
        let loaded = SessionOptions::from_ini(Some(saved_path.as_path()))
            .expect("reload session options without persisted hooks");

        assert_eq!(loaded.user_agent.as_deref(), Some("OpenPage/HookSave"));
        assert!(loaded.hooks().is_empty());
        assert!(loaded.stream);
        assert!(loaded.adapters().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_builder_methods_update_runtime_configuration_fields() {
        let mut options = SessionOptions::default();
        let mut adapter = SessionAdapter::new();
        adapter
            .set_timeout(17)
            .set_verify(false)
            .set_max_redirects(Some(1));
        let cookies = vec![SessionCookieParam {
            name: "sid".to_string(),
            value: "abc".to_string(),
            url: Some("https://example.test/".to_string()),
            domain: None,
            path: Some("/".to_string()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".to_string()),
        }];

        options
            .set_timeout(21)
            .set_user_agent(Some("OpenPage/Test".to_string()))
            .set_headers(&[("Accept".to_string(), "text/html".to_string())])
            .set_a_header("accept", "application/json")
            .set_a_header("X-Test", "1")
            .remove_a_header("x-test");
        options
            .set_cookies(&cookies)
            .expect("set session option cookies");
        options
            .set_retry(Some(6), Some(125))
            .set_proxies(Some("http://127.0.0.1:8080".to_string()), None)
            .set_download_path("downloads")
            .set_auth(Some(("alice".to_string(), "secret".to_string())))
            .set_params(&[("page".to_string(), "2".to_string())])
            .set_cert(Some(SessionCert::Pem(std::path::PathBuf::from(
                "client.pem",
            ))))
            .set_verify(false)
            .set_stream(true)
            .set_trust_env(false)
            .set_max_redirects(Some(5))
            .add_adapter("http://example.test/api/", adapter.clone());

        assert_eq!(options.timeout_secs, 21);
        assert_eq!(options.user_agent.as_deref(), Some("OpenPage/Test"));
        assert_eq!(
            options.headers,
            vec![("accept".to_string(), "application/json".to_string())]
        );
        assert_eq!(options.cookies, cookies);
        assert_eq!(options.retry_times, 6);
        assert_eq!(options.retry_interval_millis, 125);
        assert_eq!(options.http_proxy.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(options.https_proxy, None);
        assert_eq!(options.download_path, std::path::PathBuf::from("downloads"));
        assert_eq!(
            options.auth,
            Some(("alice".to_string(), "secret".to_string()))
        );
        assert_eq!(options.params, vec![("page".to_string(), "2".to_string())]);
        assert_eq!(
            options.cert,
            Some(SessionCert::Pem(std::path::PathBuf::from("client.pem")))
        );
        assert!(!options.verify);
        assert!(options.stream);
        assert!(!options.trust_env);
        assert_eq!(options.max_redirects, Some(5));
        assert_eq!(
            options.adapters(),
            &[SessionAdapterMount {
                url_prefix: "http://example.test/api/".to_string(),
                adapter,
            }]
        );

        options.clear_headers();
        assert!(options.headers.is_empty());
    }

    #[test]
    fn cookie_input_parser_accepts_text_and_json_formats() {
        let from_text = cookie_input_to_params(
            CookieInput::from("host=1; path=/shared"),
            Some("https://www.example.test/shared/page"),
        )
        .expect("parse single cookie text");
        assert_eq!(from_text.len(), 1);
        assert_eq!(from_text[0].name, "host");
        assert_eq!(from_text[0].value, "1");
        assert_eq!(
            from_text[0].url.as_deref(),
            Some("https://www.example.test/shared/page")
        );
        assert_eq!(from_text[0].path.as_deref(), Some("/shared"));

        let multi_json = json!({
            "sid": "abc",
            "token": "xyz",
            "domain": ".example.test",
            "path": "/",
            "secure": true,
            "httpOnly": true,
            "sameSite": "Strict"
        });
        let from_json =
            cookie_input_to_params(CookieInput::from(&multi_json), None).expect("parse json");
        assert_eq!(from_json.len(), 2);
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.domain.as_deref() == Some(".example.test"))
        );
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.path.as_deref() == Some("/"))
        );
        assert!(from_json.iter().all(|cookie| cookie.secure));
        assert!(from_json.iter().all(|cookie| cookie.http_only));
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.same_site.as_deref() == Some("Strict"))
        );

        let mixed_list = json!([
            "alpha=1; domain=alpha.test; path=/",
            {"name": "beta", "value": "2", "url": "https://beta.test/", "same_site": "Lax"}
        ]);
        let list_cookies = cookie_input_to_params(CookieInput::from(&mixed_list), None)
            .expect("parse mixed cookie list");
        assert_eq!(list_cookies.len(), 2);
        assert!(list_cookies.iter().any(|cookie| cookie.name == "alpha"));
        assert!(
            list_cookies.iter().any(|cookie| {
                cookie.name == "beta" && cookie.same_site.as_deref() == Some("Lax")
            })
        );
    }

    #[test]
    fn cookie_input_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let missing_name = [SessionCookieParam {
            name: " ".to_string(),
            value: "1".to_string(),
            url: None,
            domain: Some("example.test".to_string()),
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        }];
        let english_empty_name =
            cookie_input_to_params(CookieInput::from(missing_name.as_slice()), None)
                .expect_err("cookie name validation should fail")
                .to_string();
        assert!(english_empty_name.contains("cookie name cannot be empty"));

        let english_scope = cookie_input_to_params(CookieInput::from("sid=1"), None)
            .expect_err("cookie scope validation should fail")
            .to_string();
        assert!(english_scope.contains("cookie `sid` requires either url or domain"));

        let english_separators = cookie_input_to_params(CookieInput::from("a=1; b=2, c=3"), None)
            .expect_err("cookie separator validation should fail")
            .to_string();
        assert!(english_separators.contains("cookie text cannot mix ';' and ',' separators"));

        let english_missing_text_value = cookie_input_to_params(
            CookieInput::from("sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text missing value validation should fail")
        .to_string();
        assert!(
            english_missing_text_value.contains("invalid cookie text: `sid` is missing a value")
        );

        let english_text_without_cookie = cookie_input_to_params(
            CookieInput::from("domain=example.test; path=/"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text assignment validation should fail")
        .to_string();
        assert!(
            english_text_without_cookie
                .contains("cookie text must contain at least one cookie assignment")
        );

        let english_list_item = cookie_input_to_params(
            CookieInput::from(&json!([{"sid": "1", "token": "2", "domain": "example.test"}])),
            None,
        )
        .expect_err("cookie list item validation should fail")
        .to_string();
        assert!(
            english_list_item.contains("cookie list items must each describe exactly one cookie")
        );

        let english_object_without_cookie =
            cookie_input_to_params(CookieInput::from(&json!({"domain": "example.test"})), None)
                .expect_err("cookie object assignment validation should fail")
                .to_string();
        assert!(
            english_object_without_cookie
                .contains("cookie object must contain at least one cookie assignment")
        );

        let english_type = cookie_input_to_params(CookieInput::from(&json!(true)), None)
            .expect_err("cookie input type validation should fail")
            .to_string();
        assert!(english_type.contains("cookie input must be null, string, object, or array"));

        let english_name_value = cookie_input_to_params(
            CookieInput::from("name=sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie name/value validation should fail")
        .to_string();
        assert!(english_name_value.contains("cookie text must contain `name` and `value`"));

        let english_bool = cookie_input_to_params(
            CookieInput::from("sid=1; secure=None; domain=example.test"),
            None,
        )
        .expect_err("cookie boolean validation should fail")
        .to_string();
        assert!(english_bool.contains("invalid cookie text.secure: expected boolean"));

        Settings::set_language("cn");

        let chinese_empty_name =
            cookie_input_to_params(CookieInput::from(missing_name.as_slice()), None)
                .expect_err("cookie name validation should fail in Chinese")
                .to_string();
        assert!(chinese_empty_name.contains("cookie 名称不能为空"));
        assert!(chinese_empty_name.contains("HTTP 操作失败"));

        let chinese_scope = cookie_input_to_params(CookieInput::from("sid=1"), None)
            .expect_err("cookie scope validation should fail in Chinese")
            .to_string();
        assert!(chinese_scope.contains("cookie `sid` 必须设置 url 或 domain"));

        let chinese_separators = cookie_input_to_params(CookieInput::from("a=1; b=2, c=3"), None)
            .expect_err("cookie separator validation should fail in Chinese")
            .to_string();
        assert!(chinese_separators.contains("cookie 文本不能同时混用 ';' 和 ',' 分隔符"));

        let chinese_missing_text_value = cookie_input_to_params(
            CookieInput::from("sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text missing value validation should fail in Chinese")
        .to_string();
        assert!(chinese_missing_text_value.contains("cookie 文本中的 `sid` 缺少值"));

        let chinese_text_without_cookie = cookie_input_to_params(
            CookieInput::from("domain=example.test; path=/"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text assignment validation should fail in Chinese")
        .to_string();
        assert!(chinese_text_without_cookie.contains("cookie 文本必须至少包含一个 cookie 赋值"));

        let chinese_list_item = cookie_input_to_params(
            CookieInput::from(&json!([{"sid": "1", "token": "2", "domain": "example.test"}])),
            None,
        )
        .expect_err("cookie list item validation should fail in Chinese")
        .to_string();
        assert!(chinese_list_item.contains("cookie 列表中的每一项必须只描述一个 cookie"));

        let chinese_object_without_cookie =
            cookie_input_to_params(CookieInput::from(&json!({"domain": "example.test"})), None)
                .expect_err("cookie object assignment validation should fail in Chinese")
                .to_string();
        assert!(chinese_object_without_cookie.contains("cookie 对象必须至少包含一个 cookie 赋值"));

        let chinese_type = cookie_input_to_params(CookieInput::from(&json!(true)), None)
            .expect_err("cookie input type validation should fail in Chinese")
            .to_string();
        assert!(chinese_type.contains("cookie 输入必须是 null、字符串、对象或数组"));

        let chinese_name_value = cookie_input_to_params(
            CookieInput::from("name=sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie name/value validation should fail in Chinese")
        .to_string();
        assert!(chinese_name_value.contains("cookie text 必须包含 `name` 和 `value`"));

        let chinese_bool = cookie_input_to_params(
            CookieInput::from("sid=1; secure=None; domain=example.test"),
            None,
        )
        .expect_err("cookie boolean validation should fail in Chinese")
        .to_string();
        assert!(chinese_bool.contains("cookie text.secure 无效: 期望 boolean"));
    }

    #[test]
    fn session_url_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = SessionPage::new(SessionOptions::default()).expect("create session page");

        let english_cookie_header = page
            .cookie_header("not a url")
            .expect_err("cookie_header() should reject invalid url")
            .to_string();
        assert!(english_cookie_header.contains("invalid url `not a url`"));

        let english_set_cookie_header = page
            .set_cookie_header("not a url", "sid=1")
            .expect_err("set_cookie_header() should reject invalid url")
            .to_string();
        assert!(english_set_cookie_header.contains("invalid url `not a url`"));

        let english_referer = default_referer_header(None, "not a url")
            .expect_err("default_referer_header() should reject invalid url")
            .to_string();
        assert!(english_referer.contains("invalid url `not a url`"));

        let english_query = append_query_params("not a url", &[("q".to_string(), "1".to_string())])
            .expect_err("append_query_params() should reject invalid url")
            .to_string();
        assert!(english_query.contains("invalid url `not a url`"));

        let english_file = resolve_local_file_path("file://example.com/path")
            .expect_err("resolve_local_file_path() should reject invalid file url")
            .to_string();
        assert!(english_file.contains("invalid file url: file://example.com/path"));

        Settings::set_language("cn");

        let chinese_cookie_header = page
            .cookie_header("not a url")
            .expect_err("cookie_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_cookie_header.contains("无效的 url `not a url`"));
        assert!(chinese_cookie_header.contains("HTTP 操作失败"));

        let chinese_set_cookie_header = page
            .set_cookie_header("not a url", "sid=1")
            .expect_err("set_cookie_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_set_cookie_header.contains("无效的 url `not a url`"));

        let chinese_referer = default_referer_header(None, "not a url")
            .expect_err("default_referer_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_referer.contains("无效的 url `not a url`"));

        let chinese_query = append_query_params("not a url", &[("q".to_string(), "1".to_string())])
            .expect_err("append_query_params() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_query.contains("无效的 url `not a url`"));

        let chinese_file = resolve_local_file_path("file://example.com/path")
            .expect_err("resolve_local_file_path() should reject invalid file url in Chinese")
            .to_string();
        assert!(chinese_file.contains("无效的 file url: file://example.com/path"));
    }

    #[test]
    fn session_host_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = SessionPage::new(SessionOptions::default()).expect("create session page");

        let english_title = page
            .title()
            .expect_err("title() should fail before any document is loaded")
            .to_string();
        assert!(english_title.contains("session page has no loaded document"));

        let english_set_cookie = page
            .set_cookie("sid", "1", None, None, None)
            .expect_err("set_cookie() should require an explicit scope before navigation")
            .to_string();
        assert!(
            english_set_cookie.contains("session page has no current url; provide url explicitly")
        );

        Settings::set_language("cn");

        let chinese_title = page
            .title()
            .expect_err("title() should fail in Chinese before any document is loaded")
            .to_string();
        assert!(chinese_title.contains("session 页面还没有已加载文档"));
        assert!(chinese_title.contains("HTTP 操作失败"));

        let chinese_set_cookie = page
            .set_cookie("sid", "1", None, None, None)
            .expect_err("set_cookie() should require an explicit scope in Chinese")
            .to_string();
        assert!(chinese_set_cookie.contains("session 页面没有当前 url；请显式传入 url"));
        assert!(chinese_set_cookie.contains("HTTP 操作失败"));
    }

    #[test]
    fn session_lock_poisoned_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = SessionPage::new(SessionOptions::default()).expect("create session page");
        poison_mutex(Arc::clone(&page.none_element_config));

        let english = page
            .set_none_element_value(Some("missing"), true)
            .expect_err("set_none_element_value() should surface poisoned config")
            .to_string();
        assert!(english.contains("none element runtime config lock poisoned"));

        Settings::set_language("cn");

        let chinese = page
            .set_raise_when_ele_not_found(true)
            .expect_err("set_raise_when_ele_not_found() should localize poisoned config")
            .to_string();
        assert!(chinese.contains("未找到元素运行时配置锁已损坏"));
    }

    #[test]
    fn session_options_set_cookies_accepts_multi_format_inputs() {
        let mut options = SessionOptions::default();
        options
            .set_cookies("sid=abc; domain=.example.test; path=/shared; secure; httpOnly")
            .expect("set cookies from text");
        assert_eq!(options.cookies.len(), 1);
        assert_eq!(options.cookies[0].name, "sid");
        assert_eq!(options.cookies[0].domain.as_deref(), Some(".example.test"));
        assert!(options.cookies[0].secure);
        assert!(options.cookies[0].http_only);

        let cookies = json!({
            "api": "1",
            "domain": "api.example.test",
            "path": "/"
        });
        options
            .set_cookies(&cookies)
            .expect("replace cookies from json");
        assert_eq!(options.cookies.len(), 1);
        assert_eq!(options.cookies[0].name, "api");
        assert_eq!(
            options.cookies[0].domain.as_deref(),
            Some("api.example.test")
        );
        assert_eq!(options.cookies[0].path.as_deref(), Some("/"));

        options.clear_cookies();
        assert!(options.cookies.is_empty());
    }

    #[test]
    fn session_options_from_ini_loads_reference_drissionpage_configs_file() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .expect("repository root")
            .to_path_buf();
        let config_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");

        let options = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect("load DrissionPage reference session options ini");

        assert_eq!(options.timeout_secs, 10);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert!(options.http_proxy.is_none());
        assert!(options.https_proxy.is_none());
        assert!(options.verify);
        assert!(options.trust_env);
        assert_eq!(options.max_redirects, Some(30));
        assert_eq!(
            options.source_ini_path.as_deref(),
            Some(config_path.as_path())
        );
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "accept" && value.contains("text/html"))
        );
    }

    #[test]
    fn session_options_from_ini_none_loads_default_configs_file() {
        let options = SessionOptions::from_ini(None).expect("load default session options ini");

        assert_eq!(options.timeout_secs, 10);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert!(options.source_ini_path.is_some());
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "accept-charset" && value.contains("utf-8"))
        );
    }

    #[test]
    fn session_options_from_ini_none_prefers_project_dp_configs_file() {
        let dir = make_temp_dir("session-options-project-config");
        fs::create_dir_all(&dir).expect("create temp dir");
        let project_ini = dir.join("dp_configs.ini");
        fs::write(
            &project_ini,
            "[session_options]\nheaders = {'user-agent': 'ProjectSession/1.0'}\n\n[timeouts]\nbase = 7\n",
        )
        .expect("write project session configs ini");
        let _guard = CurrentDirGuard::change_to(dir.as_path());

        let resolved = resolve_session_options_ini_path(None).expect("resolve project session ini");
        let options = SessionOptions::from_ini(None).expect("load project session configs ini");

        assert_eq!(
            fs::canonicalize(&resolved).expect("canonicalize resolved project session ini"),
            fs::canonicalize(&project_ini).expect("canonicalize expected project session ini")
        );
        assert_eq!(options.timeout_secs, 7);
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value == "ProjectSession/1.0")
        );
        assert_eq!(
            options
                .source_ini_path
                .as_ref()
                .and_then(|path| fs::canonicalize(path).ok()),
            Some(fs::canonicalize(&project_ini).expect("canonicalize source project session ini"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_from_ini_parse_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let dir = make_temp_dir("session-options-parse-error");
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("session.ini");
        fs::write(&config_path, "[session_options]\nheaders = {bad}\n")
            .expect("write invalid session ini");

        let english = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect_err("invalid session ini should fail")
            .to_string();
        assert!(english.contains("invalid headers in session options ini:"));

        Settings::set_language("cn");

        let chinese = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect_err("invalid session ini should fail in Chinese")
            .to_string();
        assert!(chinese.contains("session options ini 中的 headers 无效"));
        assert!(chinese.contains("HTTP 操作失败"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_from_ini_options_false_returns_defaults_without_reading_file() {
        let dir = make_temp_dir("session-options-read-file-false");
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("session.ini");
        let mut options = SessionOptions::default();
        options
            .set_user_agent(Some("OpenPage/ReadFalse".to_string()))
            .set_auth(Some(("alice".to_string(), "secret".to_string())));
        options
            .save(Some(config_path.as_path()))
            .expect("write session options ini");

        let loaded = SessionOptions::from_ini_options(false, Some(config_path.as_path()))
            .expect("create default session options");

        assert_eq!(loaded.timeout_secs, 10);
        assert_eq!(loaded.download_path, std::path::PathBuf::from("."));
        assert_eq!(loaded.retry_times, 3);
        assert_eq!(loaded.retry_interval_millis, 2_000);
        assert!(
            loaded
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(loaded.user_agent.is_none());
        assert!(loaded.auth.is_none());
        assert!(loaded.source_ini_path.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_new_matches_from_ini_options_semantics() {
        let dir = make_temp_dir("session-options-new-wrapper");
        let config_path = dir.join("session.ini");
        let mut options = SessionOptions::default();
        options
            .set_user_agent(Some("OpenPage/SessionNew".to_string()))
            .set_auth(Some(("alice".to_string(), "secret".to_string())));
        options
            .save(Some(config_path.as_path()))
            .expect("write wrapped session ini");

        let loaded = SessionOptions::new(true, Some(config_path.as_path()))
            .expect("load session options via new()");
        let defaults = SessionOptions::new(false, Some(config_path.as_path()))
            .expect("default session options");

        assert_eq!(loaded.user_agent.as_deref(), Some("OpenPage/SessionNew"));
        assert_eq!(
            loaded.auth.as_ref(),
            Some(&("alice".to_string(), "secret".to_string()))
        );
        assert!(defaults.user_agent.is_none());
        assert!(defaults.auth.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_save_preserves_browser_sections_from_template_ini() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .expect("repository root")
            .to_path_buf();
        let source_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");
        let dir = make_temp_dir("session-options-save-template");
        fs::create_dir_all(&dir).expect("create temp dir");
        let target_path = dir.join("session.ini");
        let cookies = vec![SessionCookieParam {
            name: "sid".to_string(),
            value: "abc".to_string(),
            url: Some("https://example.test/".to_string()),
            domain: None,
            path: Some("/".to_string()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".to_string()),
        }];

        let mut options = SessionOptions::from_ini(Some(source_path.as_path()))
            .expect("load DrissionPage reference session options ini");
        options
            .set_download_path("downloads")
            .set_user_agent(Some("OpenPage/SessionIni".to_string()));
        options
            .set_cookies(&cookies)
            .expect("set ini session cookies");
        options
            .set_auth(Some(("alice".to_string(), "secret".to_string())))
            .set_params(&[("page".to_string(), "2".to_string())])
            .set_cert(Some(SessionCert::PemPair {
                cert: std::path::PathBuf::from("client.pem"),
                key: std::path::PathBuf::from("client.key"),
            }))
            .set_verify(false)
            .set_trust_env(false)
            .set_max_redirects(None)
            .set_proxies(Some("http://127.0.0.1:7890".to_string()), None)
            .set_retry(Some(4), Some(250));

        let saved_path = options
            .save(Some(target_path.as_path()))
            .expect("save session options ini");
        let saved = fs::read_to_string(&saved_path).expect("read saved session ini");
        let loaded = SessionOptions::from_ini(Some(saved_path.as_path()))
            .expect("reload saved session options ini");

        assert_eq!(saved_path, target_path);
        assert!(saved.contains("[chromium_options]"));
        assert!(saved.contains("address = 127.0.0.1:9222"));
        assert!(saved.contains("[session_options]"));
        assert!(saved.contains("OpenPage/SessionIni"));
        assert_eq!(loaded.download_path, std::path::PathBuf::from("downloads"));
        assert_eq!(loaded.user_agent.as_deref(), Some("OpenPage/SessionIni"));
        assert_eq!(
            loaded.auth,
            Some(("alice".to_string(), "secret".to_string()))
        );
        assert_eq!(loaded.params, vec![("page".to_string(), "2".to_string())]);
        assert_eq!(loaded.cookies, cookies);
        assert_eq!(
            loaded.cert,
            Some(SessionCert::PemPair {
                cert: std::path::PathBuf::from("client.pem"),
                key: std::path::PathBuf::from("client.key"),
            })
        );
        assert!(!loaded.verify);
        assert!(!loaded.trust_env);
        assert_eq!(loaded.max_redirects, None);
        assert_eq!(loaded.http_proxy.as_deref(), Some("http://127.0.0.1:7890"));
        assert!(loaded.https_proxy.is_none());
        assert_eq!(loaded.retry_times, 4);
        assert_eq!(loaded.retry_interval_millis, 250);
        assert!(
            loaded
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_save_to_default_writes_default_configs_ini() {
        let saved_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini");
        let _guard = RestoreFileGuard::new(saved_path.clone());
        let mut options = SessionOptions::default();
        options.set_user_agent(Some("OpenPageDefaultSession/1.0".to_string()));

        let returned = options
            .save_to_default()
            .expect("save session options to default ini");

        assert_eq!(returned, saved_path);

        let saved = fs::read_to_string(&saved_path).expect("read saved default session ini");
        assert!(saved.contains("[session_options]"));
        assert!(saved.contains("OpenPageDefaultSession/1.0"));
    }

    #[test]
    fn session_runtime_timeout_retry_and_close_match_reference_behavior() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        assert_eq!(page.timeout_secs().expect("default timeout"), 10);
        assert_eq!(page.retry_times().expect("default retry times"), 3);
        assert_eq!(
            page.download_path().expect("default download path"),
            env::current_dir()
                .expect("current dir")
                .display()
                .to_string()
        );
        assert_eq!(
            page.retry_interval_millis()
                .expect("default retry interval"),
            2_000
        );

        page.set_timeout(7).expect("set timeout");
        page.set_retry(Some(5), Some(250)).expect("set retry");
        page.close().expect("close session");

        assert_eq!(page.timeout_secs().expect("updated timeout"), 7);
        assert_eq!(page.retry_times().expect("updated retry times"), 5);
        assert_eq!(
            page.retry_interval_millis()
                .expect("updated retry interval"),
            250
        );
        assert_eq!(page.forced_encoding().expect("forced encoding"), None);
    }

    #[test]
    fn session_download_uses_runtime_download_path_and_tracks_last_download() {
        let download_dir = make_temp_dir("session-download");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        page.set_download_path(&download_dir)
            .expect("set download path");
        assert_eq!(
            page.download_path().expect("download path"),
            download_dir.display().to_string()
        );

        let body = "downloaded body";
        let (address, handle) = spawn_capture_server("200 OK", body);
        let url = format!("{address}/files/openpage.txt");

        let saved_path = page.download(&url).expect("download file");
        assert_eq!(
            saved_path,
            download_dir.join("openpage.txt").display().to_string()
        );
        assert_eq!(
            fs::read_to_string(&saved_path).expect("downloaded contents"),
            body
        );

        let last_download = page
            .last_download()
            .expect("last download result")
            .expect("download record");
        assert_eq!(last_download.url, url);
        assert_eq!(last_download.final_url, url);
        assert_eq!(last_download.path, saved_path);
        assert_eq!(last_download.filename, "openpage.txt".to_string());
        assert_eq!(last_download.status_code, 200);
        assert_eq!(last_download.total_bytes, body.len() as u64);
        assert_eq!(
            last_download.content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );

        let _ = handle.join().expect("server thread");
        let _ = fs::remove_file(&saved_path);
        let _ = fs::remove_dir_all(&download_dir);
    }

    #[test]
    fn session_download_to_writes_explicit_target_path() {
        let target_dir = make_temp_dir("session-download-explicit");
        let target_path = target_dir.join("custom.bin");
        let body = "explicit target";
        let (address, handle) = spawn_capture_server("200 OK", body);
        let url = format!("{address}/payload.bin");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        let saved_path = page
            .download_to(&url, &target_path)
            .expect("download explicit path");
        assert_eq!(saved_path, target_path.display().to_string());
        assert_eq!(
            fs::read_to_string(&target_path).expect("downloaded contents"),
            body
        );
        assert_eq!(
            page.last_download()
                .expect("last download")
                .expect("record")
                .filename,
            "custom.bin".to_string()
        );

        let _ = handle.join().expect("server thread");
        let _ = fs::remove_file(&target_path);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn session_download_status_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = SessionPage::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (english_address, english_handle) = spawn_capture_server("404 Not Found", "missing");
        let english_url = format!("{english_address}/missing.bin");
        let english_error = page
            .download(&english_url)
            .expect_err("download status validation should fail")
            .to_string();
        assert!(english_error.contains(&format!(
            "download request returned status 404 for {english_url}"
        )));
        let _ = english_handle.join().expect("english server thread");

        Settings::set_language("cn");

        let (chinese_address, chinese_handle) = spawn_capture_server("404 Not Found", "missing");
        let chinese_url = format!("{chinese_address}/missing.bin");
        let chinese_error = page
            .download(&chinese_url)
            .expect_err("download status validation should fail in Chinese")
            .to_string();
        assert!(chinese_error.contains(&format!("下载请求 {chinese_url} 返回状态码 404")));
        assert!(chinese_error.contains("HTTP 操作失败"));
        let _ = chinese_handle.join().expect("chinese server thread");
    }

    #[test]
    fn session_cookie_queries_cover_all_domains_and_metadata() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let current_url =
            Url::parse("https://www.example.test/shared/page").expect("current cookie url");
        let other_url = Url::parse("https://other.test/").expect("other cookie url");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some(current_url.as_str().to_string());
        }

        let cookie_jar = page.lock_state().expect("lock state").cookie_jar.clone();
        cookie_jar.add_cookie_str("host=1; Path=/shared", &current_url);
        cookie_jar.add_cookie_str(
            "shared=2; Domain=example.test; Path=/shared; Secure; HttpOnly; SameSite=Strict",
            &current_url,
        );
        cookie_jar.add_cookie_str("other=3; Domain=other.test; Path=/", &other_url);

        let current = page.cookies().expect("current cookies");
        assert_eq!(current.len(), 2);
        assert!(current.iter().any(|cookie| {
            cookie.name == "host" && cookie.domain.as_deref() == Some("www.example.test")
        }));
        assert!(current.iter().any(|cookie| {
            cookie.name == "shared" && cookie.domain.as_deref() == Some("example.test")
        }));

        let all = page.cookies_all_domains().expect("all-domain cookies");
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|cookie| cookie.name == "other"));

        let detailed_current = page
            .cookies_detailed(false)
            .expect("current detailed cookies");
        assert_eq!(detailed_current.len(), 2);
        let host = detailed_current
            .iter()
            .find(|cookie| cookie.name == "host")
            .expect("host cookie");
        assert!(host.host_only);
        assert_eq!(host.path.as_deref(), Some("/shared"));
        assert!(!host.secure);
        assert!(!host.http_only);
        assert_eq!(host.same_site, None);
        assert!(!host.persistent);

        let shared = detailed_current
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert!(!shared.host_only);
        assert_eq!(shared.path.as_deref(), Some("/shared"));
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Strict"));
        assert!(!shared.persistent);

        let detailed_all = page.cookies_detailed(true).expect("all detailed cookies");
        assert_eq!(detailed_all.len(), 3);
        assert!(detailed_all.iter().any(|cookie| cookie.name == "other"));
    }

    #[test]
    fn session_page_set_cookies_accepts_text_and_json_inputs() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some("https://www.example.test/shared/page".to_string());
        }

        page.set_cookies("host=1; path=/shared")
            .expect("set host cookie from text");
        let shared = json!({
            "shared": "2",
            "domain": "example.test",
            "path": "/shared",
            "secure": true,
            "httpOnly": true,
            "sameSite": "Strict"
        });
        page.set_cookies(&shared)
            .expect("set shared cookie from json");

        let cookies = page.cookies().expect("current cookies");
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|cookie| {
            cookie.name == "host" && cookie.domain.as_deref() == Some("www.example.test")
        }));
        assert!(cookies.iter().any(|cookie| {
            cookie.name == "shared" && cookie.domain.as_deref() == Some("example.test")
        }));

        let detailed = page.cookies_detailed(false).expect("detailed cookies");
        let host = detailed
            .iter()
            .find(|cookie| cookie.name == "host")
            .expect("host cookie");
        assert_eq!(host.path.as_deref(), Some("/shared"));
        assert!(!host.secure);
        assert!(!host.http_only);

        let shared = detailed
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert_eq!(shared.path.as_deref(), Some("/shared"));
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Strict"));
    }

    #[test]
    fn session_get_retries_failed_responses_until_success() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (url, handle) = spawn_retry_server(
            &["500 Internal Server Error", "200 OK"],
            &["retry", "done"],
            Arc::clone(&attempts),
        );
        let page = SessionPage::new(SessionOptions {
            retry_times: 1,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page should initialize");

        assert!(page.get(&url).expect("get should retry then succeed"));
        assert_eq!(page.status_code().expect("status code"), Some(200));
        assert_eq!(page.html().expect("html body"), "done".to_string());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(handle.join().expect("server thread"), vec!["GET", "GET"]);
    }

    #[test]
    fn session_post_retries_failed_responses_until_success() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (url, handle) = spawn_retry_server(
            &["502 Bad Gateway", "200 OK"],
            &["retry", "posted"],
            Arc::clone(&attempts),
        );
        let page = SessionPage::new(SessionOptions {
            retry_times: 1,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page should initialize");

        assert!(page.post(&url).expect("post should retry then succeed"));
        assert_eq!(page.status_code().expect("status code"), Some(200));
        assert_eq!(page.html().expect("html body"), "posted".to_string());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(handle.join().expect("server thread"), vec!["POST", "POST"]);
    }

    #[test]
    fn session_get_loads_local_file_paths() {
        let path = make_temp_file(
            "session-local",
            "<html><head><title>Local File</title></head><body>openpage</body></html>",
        );
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let file_url = Url::from_file_path(&path)
            .expect("build local file url")
            .to_string();

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
        );
        assert_eq!(page.status_code().expect("status"), Some(200));
        assert!(page.url_available().expect("url available"));
        assert_eq!(page.title().expect("title"), Some("Local File".to_string()));
        assert!(page.html().expect("html").contains("openpage"));
        assert!(
            page.url()
                .expect("url")
                .expect("current url")
                .starts_with("file://")
        );
        assert!(page.get(&file_url).expect("load file url"));
        let current_file_url = page.url().expect("url").expect("current url");
        let current_path = Url::parse(&current_file_url)
            .expect("parse current file url")
            .to_file_path()
            .expect("current file url path");
        assert_eq!(
            fs::canonicalize(current_path).expect("canonical current file path"),
            fs::canonicalize(&path).expect("canonical expected file path")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_response_snapshot_exposes_latest_response_metadata() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let (address, handle) = spawn_capture_server("200 OK", "snapshot");

        assert!(page.get(&address).expect("request snapshot"));

        let expected_url = format!("{address}/");
        let response = page
            .response()
            .expect("response snapshot result")
            .expect("response snapshot");
        assert_eq!(response.status_code, Some(200));
        assert_eq!(response.url.as_deref(), Some(expected_url.as_str()));
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(response.encoding.as_deref(), Some("utf-8"));
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| name == "content-type"
                    && value == "text/plain; charset=utf-8")
        );

        let _ = handle.join().expect("server thread");
    }

    #[test]
    fn session_runtime_snapshot_exposes_current_configuration_and_cookies() {
        let mut adapter = SessionAdapter::new();
        adapter
            .set_timeout(7)
            .set_verify(false)
            .set_max_redirects(Some(2));
        let page = SessionPage::new(SessionOptions {
            timeout_secs: 21,
            user_agent: Some("OpenPage/TestAgent".to_string()),
            headers: vec![
                ("x-one".to_string(), "1".to_string()),
                ("accept".to_string(), "text/html".to_string()),
            ],
            cookies: vec![SessionCookieParam {
                name: "sid".to_string(),
                value: "abc".to_string(),
                url: Some("https://example.test/".to_string()),
                domain: None,
                path: Some("/".to_string()),
                secure: true,
                http_only: true,
                same_site: Some("Lax".to_string()),
            }],
            download_path: std::path::PathBuf::from("downloads"),
            retry_times: 4,
            retry_interval_millis: 250,
            http_proxy: Some("http://127.0.0.1:7890".to_string()),
            https_proxy: Some("http://127.0.0.1:7891".to_string()),
            params: vec![("page".to_string(), "2".to_string())],
            verify: false,
            auth: Some(("alice".to_string(), "secret".to_string())),
            stream: true,
            trust_env: false,
            max_redirects: Some(5),
            adapters: vec![SessionAdapterMount {
                url_prefix: "http://example.test/api/".to_string(),
                adapter: adapter.clone(),
            }],
            ..SessionOptions::default()
        })
        .expect("session page");

        let snapshot = page.session().expect("session runtime snapshot");
        let expected_download_path = std::env::current_dir()
            .expect("current dir")
            .join("downloads")
            .display()
            .to_string();

        assert_eq!(snapshot.timeout_secs, 21);
        assert_eq!(snapshot.user_agent.as_deref(), Some("OpenPage/TestAgent"));
        assert_eq!(snapshot.download_path, expected_download_path);
        assert_eq!(snapshot.retry_times, 4);
        assert_eq!(snapshot.retry_interval_millis, 250);
        assert_eq!(
            snapshot.http_proxy.as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            snapshot.https_proxy.as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(snapshot.params, vec![("page".to_string(), "2".to_string())]);
        assert!(!snapshot.verify);
        assert_eq!(
            snapshot.auth,
            Some(("alice".to_string(), "secret".to_string()))
        );
        assert!(snapshot.stream);
        assert!(snapshot.cert.is_none());
        assert!(!snapshot.trust_env);
        assert_eq!(snapshot.max_redirects, Some(5));
        assert!(snapshot.current_url.is_none());
        assert!(
            snapshot
                .headers
                .contains(&("accept".to_string(), "text/html".to_string()))
        );
        assert!(
            snapshot
                .headers
                .contains(&("x-one".to_string(), "1".to_string()))
        );
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(snapshot.cookies[0].name, "sid".to_string());
        assert_eq!(snapshot.cookies[0].value, "abc".to_string());
        assert_eq!(snapshot.cookies[0].same_site.as_deref(), Some("Lax"));
        assert_eq!(
            snapshot.adapters,
            vec![SessionAdapterMount {
                url_prefix: "http://example.test/api/".to_string(),
                adapter,
            }]
        );
    }

    #[test]
    fn session_handle_can_spawn_new_page_sharing_runtime_state() {
        let page1 = SessionPage::new(SessionOptions::default()).expect("session page");
        page1
            .set_header("x-shared", "page1")
            .expect("set shared header");
        page1
            .set_cookies("sid=abc; url=http://example.test/")
            .expect("set shared cookie");

        let handle = page1.session_handle();
        let page2 = SessionPage::from_session_handle(handle.clone());

        assert_eq!(page2.stream().expect("initial stream"), false);
        page2.set_stream(true).expect("enable shared stream");
        assert!(page1.stream().expect("stream visible across pages"));

        let snapshot = handle.snapshot().expect("session snapshot");
        assert!(
            snapshot
                .headers
                .iter()
                .any(|(name, value)| name == "x-shared" && value == "page1")
        );
        assert!(
            snapshot
                .cookies
                .iter()
                .any(|cookie| cookie.name == "sid" && cookie.value == "abc")
        );

        let (address, server) = spawn_capture_server("200 OK", "shared session");
        assert!(page1.get(&address).expect("shared request"));

        let response = page2
            .response()
            .expect("shared response result")
            .expect("shared response");
        assert_eq!(response.status_code, Some(200));
        assert_eq!(handle.response().expect("handle response"), Some(response));

        let _ = server.join().expect("server thread");
    }

    #[test]
    fn session_handle_page_roundtrip_preserves_identity() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let handle = page.session_handle();
        let cloned_page = handle.page();
        let second_handle: SessionHandle = cloned_page.session_handle();

        cloned_page.set_timeout(27).expect("set shared timeout");

        assert_eq!(second_handle.snapshot().expect("snapshot").timeout_secs, 27);
        assert_eq!(page.timeout_secs().expect("page timeout"), 27);
    }

    #[test]
    fn session_set_params_and_auth_apply_to_requests() {
        let (address, handle) = spawn_capture_server("200 OK", "secured");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let url = format!("{address}/items");

        page.set_params(&[
            ("foo".to_string(), "bar baz".to_string()),
            ("x".to_string(), "1".to_string()),
        ])
        .expect("set params");
        page.set_auth(Some(("alice".to_string(), "secret".to_string())))
            .expect("set auth");

        assert!(page.get(&url).expect("request with params and auth"));
        assert_eq!(page.html().expect("html body"), "secured".to_string());

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .next()
                .expect("request line")
                .contains("/items?foo=bar+baz&x=1 HTTP/1.1")
        );
        let auth_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
            .expect("authorization header");
        let (_, value) = auth_header
            .split_once(':')
            .expect("authorization separator");
        let expected = BASE64_STANDARD.encode("alice:secret");
        assert_eq!(value.trim(), format!("Basic {expected}"));
    }

    #[test]
    fn session_stream_option_defers_body_loading_until_needed() {
        let (address, handle) = spawn_capture_server("200 OK", "streamed");
        let page = SessionPage::new(SessionOptions {
            stream: true,
            ..SessionOptions::default()
        })
        .expect("session page");

        assert!(page.get(&address).expect("streaming request"));
        {
            let state = page.lock_state().expect("lock session state");
            assert_eq!(state.status_code, Some(200));
            assert!(state.pending_response.is_some());
            assert!(state.raw_data.is_none());
            assert!(state.body.is_none());
            assert_eq!(state.encoding.as_deref(), Some("utf-8"));
        }
        assert_eq!(
            page.response().expect("response").and_then(|r| r.encoding),
            Some("utf-8".to_string())
        );
        assert_eq!(
            page.html().expect("load streamed body"),
            "streamed".to_string()
        );
        {
            let state = page.lock_state().expect("lock session state");
            assert!(state.pending_response.is_none());
            assert!(state.raw_data.is_some());
            assert!(state.body.is_some());
        }

        handle.join().expect("server thread");
    }

    #[test]
    fn session_request_options_stream_override_disables_lazy_loading() {
        let (address, handle) = spawn_capture_server("200 OK", "override");
        let page = SessionPage::new(SessionOptions {
            stream: true,
            ..SessionOptions::default()
        })
        .expect("session page");
        let request_options = SessionRequestOptions {
            stream: Some(false),
            ..SessionRequestOptions::default()
        };

        assert!(
            page.get_with_options(&address, &request_options)
                .expect("request with stream override")
        );
        {
            let state = page.lock_state().expect("lock session state");
            assert!(state.pending_response.is_none());
            assert!(state.raw_data.is_some());
            assert_eq!(state.body.as_deref().map(AsRef::as_ref), Some("override"));
        }

        handle.join().expect("server thread");
    }

    #[test]
    fn session_page_set_stream_updates_runtime_stream_behavior() {
        let (address, handle) = spawn_capture_server("200 OK", "runtime-stream");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        page.set_stream(true).expect("enable runtime stream");
        assert!(page.stream().expect("runtime stream getter"));
        assert!(page.get(&address).expect("runtime streaming request"));
        assert!(
            page.lock_state()
                .expect("lock session state")
                .pending_response
                .is_some()
        );

        handle.join().expect("server thread");
    }

    #[test]
    fn session_options_initial_headers_apply_to_requests() {
        let (address, handle) = spawn_capture_server("200 OK", "init");
        let page = SessionPage::new(SessionOptions {
            headers: vec![
                ("X-Init".to_string(), "present".to_string()),
                ("Referer".to_string(), "".to_string()),
            ],
            ..SessionOptions::default()
        })
        .expect("session page");
        let url = format!("{address}/headers");

        assert!(page.get(&url).expect("request with initial headers"));

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-init: present"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_page_set_header_replaces_existing_header_case_insensitively() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        page.set_headers(&[
            ("Accept".to_string(), "text/html".to_string()),
            ("Referer".to_string(), "".to_string()),
        ])
        .expect("set initial headers");
        page.set_header("accept", "application/json")
            .expect("replace accept header");
        page.set_header("X-Test", "1").expect("set x-test");
        page.set_header("x-test", "2").expect("replace x-test");

        let (address, handle) = spawn_capture_server("200 OK", "headers");
        let url = format!("{address}/headers");
        assert!(page.get(&url).expect("request with updated headers"));

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("accept: application/json"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("accept: text/html"))
        );
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-test: 2"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-test: 1"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_options_initial_cookies_seed_session_cookie_store() {
        let current_url =
            Url::parse("https://www.example.test/shared/page").expect("current cookie url");
        let page = SessionPage::new(SessionOptions {
            cookies: vec![
                SessionCookieParam {
                    name: "host".to_string(),
                    value: "1".to_string(),
                    url: Some(current_url.as_str().to_string()),
                    domain: None,
                    path: Some("/shared".to_string()),
                    secure: false,
                    http_only: false,
                    same_site: None,
                },
                SessionCookieParam {
                    name: "shared".to_string(),
                    value: "2".to_string(),
                    url: None,
                    domain: Some("example.test".to_string()),
                    path: Some("/shared".to_string()),
                    secure: true,
                    http_only: true,
                    same_site: Some("Lax".to_string()),
                },
            ],
            ..SessionOptions::default()
        })
        .expect("session page");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some(current_url.as_str().to_string());
        }

        let cookies = page.cookies().expect("current cookies");
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|cookie| cookie.name == "host"));
        assert!(cookies.iter().any(|cookie| cookie.name == "shared"));

        let detailed = page.cookies_detailed(false).expect("detailed cookies");
        let shared = detailed
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Lax"));
    }

    #[test]
    fn session_request_options_override_runtime_request_settings() {
        let page = SessionPage::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 999,
            ..SessionOptions::default()
        })
        .expect("session page");
        page.set_params(&[("base".to_string(), "1".to_string())])
            .expect("set base params");
        page.set_headers(&[
            ("X-Base".to_string(), "base".to_string()),
            ("Referer".to_string(), "http://default.test/".to_string()),
        ])
        .expect("set base headers");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind request options server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (index, status) in ["500 Internal Server Error", "200 OK"].iter().enumerate() {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                requests.push(request);
                let body = if index == 0 { "retry" } else { "done" };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
            requests
        });
        let url = format!("{address}/items");
        let request_options = SessionRequestOptions {
            retry_times: Some(1),
            retry_interval_millis: Some(0),
            headers: vec![
                ("X-Base".to_string(), "override".to_string()),
                ("Referer".to_string(), "".to_string()),
                ("X-Req".to_string(), "request".to_string()),
            ],
            params: vec![("req".to_string(), "2".to_string())],
            ..SessionRequestOptions::default()
        };

        assert!(
            page.get_with_options(&url, &request_options)
                .expect("request with overrides")
        );
        let requests = handle.join().expect("server thread");
        assert_eq!(requests.len(), 2);
        let second_request = &requests[1];
        assert!(
            second_request
                .lines()
                .next()
                .expect("request line")
                .contains("/items?base=1&req=2 HTTP/1.1")
        );
        assert!(
            second_request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-base: override"))
        );
        assert!(
            second_request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-req: request"))
        );
        assert!(
            !second_request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_request_options_timeout_overrides_session_default() {
        let page = SessionPage::new(SessionOptions {
            timeout_secs: 5,
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (url, handle) = spawn_delayed_server(Duration::from_millis(1200));
        let request_options = SessionRequestOptions {
            timeout_secs: Some(1),
            ..SessionRequestOptions::default()
        };

        let result = page.get_with_options(&url, &request_options);
        assert!(result.is_err());
        handle.join().expect("server thread");
    }

    #[test]
    fn session_request_send_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = SessionPage::new(SessionOptions {
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let request_options = SessionRequestOptions {
            timeout_secs: Some(1),
            ..SessionRequestOptions::default()
        };

        let (english_url, english_handle) = spawn_delayed_server(Duration::from_millis(1200));
        let english = page
            .get_with_options(&english_url, &request_options)
            .expect_err("timeout should fail")
            .to_string();
        assert!(english.contains(&format!("session GET request failed for {english_url}")));
        let _ = english_handle.join();

        Settings::set_language("cn");

        let (chinese_url, chinese_handle) = spawn_delayed_server(Duration::from_millis(1200));
        let chinese = page
            .get_with_options(&chinese_url, &request_options)
            .expect_err("timeout should fail in Chinese")
            .to_string();
        assert!(chinese.contains(&format!("session GET 请求 {chinese_url} 失败")));
        assert!(chinese.contains("HTTP 操作失败"));
        let _ = chinese_handle.join();
    }

    #[test]
    fn session_response_body_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = SessionPage::new(SessionOptions {
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");

        let (english_url, english_handle) = spawn_truncated_body_server();
        let english = page
            .get(&english_url)
            .expect_err("truncated response body should fail")
            .to_string();
        assert!(english.contains(&format!(
            "failed to read session response body for {english_url}"
        )));
        let _ = english_handle.join();

        Settings::set_language("cn");

        let (chinese_url, chinese_handle) = spawn_truncated_body_server();
        let chinese = page
            .get(&chinese_url)
            .expect_err("truncated response body should fail in Chinese")
            .to_string();
        assert!(chinese.contains(&format!("读取 session 响应体 {chinese_url} 失败")));
        assert!(chinese.contains("HTTP 操作失败"));
        let _ = chinese_handle.join();
    }

    #[test]
    fn session_requests_set_default_referer_from_current_url() {
        let (first_address, first_handle) = spawn_capture_server("200 OK", "first");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let first_url = format!("{first_address}/first");

        assert!(page.get(&first_url).expect("first request"));
        let first_request = first_handle.join().expect("server thread");
        let first_referer = first_request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
            .expect("first referer header");
        assert_eq!(
            first_referer.trim().to_ascii_lowercase(),
            format!("referer: {first_address}")
        );

        let (second_address, second_handle) = spawn_capture_server("200 OK", "second");
        let second_url = format!("{second_address}/next");
        assert!(page.get(&second_url).expect("second request"));
        let second_request = second_handle.join().expect("server thread");
        let second_referer = second_request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
            .expect("second referer header");
        assert_eq!(
            second_referer.trim().to_ascii_lowercase(),
            format!("referer: {first_url}")
        );
    }

    #[test]
    fn session_set_encoding_updates_current_and_future_documents() {
        let path = make_temp_bytes("session-encoding", b"caf\xe9");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load bytes file")
        );
        assert_eq!(
            page.html().expect("default html"),
            "caf\u{fffd}".to_string()
        );

        page.set_encoding(Some("windows-1252".to_string()))
            .expect("set encoding");
        assert_eq!(
            page.forced_encoding().expect("forced encoding"),
            Some("windows-1252".to_string())
        );
        assert_eq!(page.html().expect("decoded html"), "caf\u{e9}".to_string());
        assert_eq!(
            page.encoding().expect("current encoding"),
            Some("windows-1252".to_string())
        );

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("reload bytes file")
        );
        assert_eq!(page.html().expect("reloaded html"), "caf\u{e9}".to_string());

        page.set_encoding(None).expect("clear encoding");
        assert_eq!(page.forced_encoding().expect("forced encoding"), None);
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("reload bytes file after clear")
        );
        assert_eq!(
            page.html().expect("auto decoded html"),
            "caf\u{fffd}".to_string()
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_set_proxies_routes_http_requests_through_proxy() {
        let (proxy_url, handle) = spawn_capture_server("200 OK", "proxied");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        page.set_proxies(Some(proxy_url.clone()), None)
            .expect("set proxy");
        assert!(
            page.get("http://example.test/proxy-path")
                .expect("request through proxy")
        );
        assert_eq!(page.html().expect("html body"), "proxied".to_string());

        let request = handle.join().expect("server thread");
        assert_eq!(
            request.lines().next().expect("request line"),
            "GET http://example.test/proxy-path HTTP/1.1"
        );
    }

    #[test]
    fn session_add_adapter_routes_matching_urls_through_proxy_and_updates_snapshot() {
        let (proxy_url, handle) = spawn_capture_server("200 OK", "adapter");
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        let mut adapter = SessionAdapter::new();
        adapter.set_proxies(Some(proxy_url.clone()), None);

        page.add_adapter("http://example.test/api/", adapter.clone())
            .expect("add runtime adapter");

        assert_eq!(
            page.adapters().expect("runtime adapters"),
            vec![SessionAdapterMount {
                url_prefix: "http://example.test/api/".to_string(),
                adapter: adapter.clone(),
            }]
        );
        assert_eq!(
            page.session().expect("session snapshot").adapters,
            vec![SessionAdapterMount {
                url_prefix: "http://example.test/api/".to_string(),
                adapter,
            }]
        );

        assert!(
            page.get("http://example.test/api/items")
                .expect("request through mounted adapter")
        );
        assert_eq!(page.html().expect("response body"), "adapter".to_string());

        let request = handle.join().expect("server thread");
        assert_eq!(
            request.lines().next().expect("request line"),
            "GET http://example.test/api/items HTTP/1.1"
        );
    }

    #[test]
    fn session_adapter_uses_longest_matching_url_prefix() {
        let (address, handle) = spawn_delayed_server(Duration::from_millis(1500));
        let mut broad_adapter = SessionAdapter::new();
        broad_adapter.set_timeout(1);
        let mut specific_adapter = SessionAdapter::new();
        specific_adapter.set_timeout(3);
        let mut options = SessionOptions::default();
        options
            .add_adapter(address.clone(), broad_adapter)
            .add_adapter(format!("{address}/api/"), specific_adapter);
        let page = SessionPage::new(options).expect("session page");

        assert!(
            page.get(&format!("{address}/api/items"))
                .expect("request should use most specific adapter timeout")
        );
        assert_eq!(page.html().expect("response body"), "slow".to_string());

        handle.join().expect("server thread");
    }

    #[test]
    fn session_set_max_redirects_controls_follow_behavior() {
        let page = SessionPage::new(SessionOptions::default()).expect("session page");

        page.set_max_redirects(Some(0)).expect("disable redirects");
        let (first_address, first_handle) = spawn_redirect_server(1);
        assert!(
            page.get(&format!("{first_address}/first"))
                .expect("request without redirects")
        );
        assert_eq!(page.status_code().expect("status"), Some(302));
        assert!(page.url_available().expect("url available"));
        assert_eq!(
            page.url().expect("url").expect("current url"),
            format!("{first_address}/first")
        );
        assert_eq!(
            first_handle.join().expect("server thread"),
            vec!["GET /first HTTP/1.1".to_string()]
        );

        page.set_max_redirects(Some(1)).expect("allow one redirect");
        let (second_address, second_handle) = spawn_redirect_server(2);
        assert!(
            page.get(&format!("{second_address}/first"))
                .expect("request with redirect")
        );
        assert_eq!(page.status_code().expect("status"), Some(200));
        assert!(page.url_available().expect("url available"));
        assert_eq!(page.html().expect("html body"), "done".to_string());
        assert_eq!(
            page.url().expect("url").expect("current url"),
            format!("{second_address}/final")
        );
        assert_eq!(
            second_handle.join().expect("server thread"),
            vec![
                "GET /first HTTP/1.1".to_string(),
                "GET /final HTTP/1.1".to_string()
            ]
        );
    }

    #[test]
    fn session_url_available_is_false_for_unsuccessful_status() {
        let page = SessionPage::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (address, handle) = spawn_capture_server("404 Not Found", "missing");

        assert!(!page.get(&address).expect("request 404"));
        assert_eq!(page.status_code().expect("status"), Some(404));
        assert!(!page.url_available().expect("url available"));

        let _ = handle.join().expect("server thread");
    }

    #[test]
    fn session_new_reports_missing_client_cert_files() {
        let missing = env::temp_dir().join(format!(
            "openpage-missing-cert-{}.pem",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let error = SessionPage::new(SessionOptions {
            cert: Some(SessionCert::Pem(missing.clone())),
            ..SessionOptions::default()
        })
        .expect_err("missing cert should fail");

        match error {
            OpenPageError::Io(message) => {
                assert!(message.contains("failed to read cert"));
                assert!(message.contains(&missing.display().to_string()));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn session_new_proxy_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_error = SessionPage::new(SessionOptions {
            http_proxy: Some("://bad-proxy".to_string()),
            ..SessionOptions::default()
        })
        .expect_err("invalid proxy should fail")
        .to_string();
        assert!(english_error.contains("invalid session http proxy `://bad-proxy`"));

        Settings::set_language("cn");

        let chinese_error = SessionPage::new(SessionOptions {
            https_proxy: Some("://bad-proxy".to_string()),
            ..SessionOptions::default()
        })
        .expect_err("invalid proxy should fail in Chinese")
        .to_string();
        assert!(chinese_error.contains("session https 代理 `://bad-proxy` 无效"));
        assert!(chinese_error.contains("HTTP 操作失败"));
    }

    #[test]
    fn session_new_identity_parse_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let dir = make_temp_dir("session-invalid-cert");
        fs::create_dir_all(&dir).expect("create invalid cert dir");
        let cert_path = dir.join("client.pem");
        fs::write(&cert_path, "not a pem").expect("write invalid cert");

        let english_error = SessionPage::new(SessionOptions {
            cert: Some(SessionCert::Pem(cert_path.clone())),
            ..SessionOptions::default()
        })
        .expect_err("invalid cert should fail")
        .to_string();
        assert!(english_error.contains("failed to parse session identity"));

        Settings::set_language("cn");

        let chinese_error = SessionPage::new(SessionOptions {
            cert: Some(SessionCert::Pem(cert_path.clone())),
            ..SessionOptions::default()
        })
        .expect_err("invalid cert should fail in Chinese")
        .to_string();
        assert!(chinese_error.contains("解析 session identity 失败"));
        assert!(chinese_error.contains("HTTP 操作失败"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_find_supports_xpath_queries() {
        let root = snapshot_find(HTML, "xpath://section[@id='root']").expect("root should exist");
        let second_item = root
            .find("xpath:./ul/li[@data-kind='b']")
            .expect("second item should exist");
        let all_items = root
            .find_all("xpath:.//li")
            .expect("all xpath items should exist");

        assert_eq!(second_item.tag().expect("tag"), "li".to_string());
        assert_eq!(
            second_item.text().expect("second item text"),
            Some("beta".to_string())
        );
        assert_eq!(all_items.len(), 2);
    }

    #[test]
    fn snapshot_xpath_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = snapshot_find(HTML, "xpath://[")
            .expect_err("invalid xpath should fail")
            .to_string();
        assert!(english.contains("unsupported locator"));
        assert!(english.contains("invalid xpath `//[`"));

        Settings::set_language("cn");

        let chinese = snapshot_find(HTML, "xpath://[")
            .expect_err("invalid xpath should localize")
            .to_string();
        assert!(chinese.contains("定位符语法不受支持"));
        assert!(chinese.contains("无效的 xpath `//[`"));
    }

    #[test]
    fn xpath_path_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = parse_xpath_path("broken")
            .expect_err("unsupported xpath path should fail")
            .to_string();
        assert!(english.contains("element not found"));
        assert!(english.contains("unsupported xpath path `broken`"));

        let parsed = Html::parse_document("<html><body><div></div></body></html>");
        let missing = nth_scraper_child_by_tag(parsed.tree.root(), "section", 2)
            .expect_err("missing xpath segment should fail")
            .to_string();
        assert!(missing.contains("xpath segment `section[2]` not found"));

        Settings::set_language("cn");

        let chinese = parse_xpath_path("broken")
            .expect_err("unsupported xpath path should localize")
            .to_string();
        assert!(chinese.contains("没有找到元素"));
        assert!(chinese.contains("不支持的 xpath 路径 `broken`"));

        let invalid_index = nth_scraper_child_by_tag(parsed.tree.root(), "section", 0)
            .expect_err("invalid xpath segment index should localize")
            .to_string();
        assert!(invalid_index.contains("xpath 片段 `section` 的序号无效"));

        let missing = nth_scraper_child_by_tag(parsed.tree.root(), "section", 2)
            .expect_err("missing xpath segment should localize")
            .to_string();
        assert!(missing.contains("没有找到 xpath 片段 `section[2]`"));
    }

    #[test]
    fn session_element_find_by_supports_by_mappings() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let item = root
            .find_by("class name", "item")
            .expect("first class match should exist");
        let items = root
            .find_all_by("tag name", "li")
            .expect("tag matches should exist");

        assert_eq!(item.tag().expect("item tag"), "li".to_string());
        assert_eq!(item.text().expect("item text"), Some("alpha".to_string()));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn session_element_find_accepts_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let item = root
            .find((By::CLASS_NAME, "item"))
            .expect("first class match should exist");
        let items = root
            .find_all((By::TAG_NAME, "li"))
            .expect("tag matches should exist");

        assert_eq!(item.tag().expect("item tag"), "li".to_string());
        assert_eq!(item.text().expect("item text"), Some("alpha".to_string()));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn session_page_find_by_supports_by_mappings() {
        let path = make_temp_file("session-find-by", HTML);
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
        );

        let submit = page.find_by("id", "submit").expect("submit button");
        let items = page
            .find_all_by("class name", "item")
            .expect("item matches");

        assert_eq!(submit.tag().expect("submit tag"), "button".to_string());
        assert_eq!(submit.text().expect("submit text"), Some("Go".to_string()));
        assert_eq!(items.len(), 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_page_find_accepts_by_locator_tuples() {
        let path = make_temp_file("session-find", HTML);
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
        );

        let submit = page.find((By::ID, "submit")).expect("submit button");
        let items = page
            .find_all((By::CLASS_NAME, "item"))
            .expect("item matches");

        assert_eq!(submit.tag().expect("submit tag"), "button".to_string());
        assert_eq!(submit.text().expect("submit text"), Some("Go".to_string()));
        assert_eq!(items.len(), 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_page_and_element_find_locators_accept_by_locator_inputs() {
        fn assert_calls(page: &SessionPage, element: &SessionElement) {
            let locators = vec!["#root".to_string(), ".item".to_string()];
            let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
            let mixed_locators = [
                LocatorInput::from("#root"),
                LocatorInput::from((By::CLASS_NAME, "item")),
            ];

            let _ = page.find_locators((By::ID, "root"), true, true);
            let _ = page.find_locators(&locators, false, false);
            let _ = page.find_locators(&tuple_locators, false, false);
            let _ = page.find_locators(&mixed_locators, false, false);
            let _ = element.find_locators((By::CLASS_NAME, "item"), true, true);
            let _ = element.find_locators(&locators, false, false);
            let _ = element.find_locators(&tuple_locators, false, false);
            let _ = element.find_locators(&mixed_locators, false, false);
        }

        let _ = assert_calls as fn(&SessionPage, &SessionElement);
    }

    #[test]
    fn snapshot_traversal_supports_root_parent_siblings_and_children() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items list should exist");

        assert_eq!(root.tag().expect("root tag"), "html".to_string());
        assert_eq!(
            submit
                .parent()
                .expect("submit parent")
                .tag()
                .expect("parent tag"),
            "section"
        );
        assert_eq!(
            submit
                .next()
                .expect("next sibling")
                .attr("id")
                .expect("next id"),
            Some("out".to_string())
        );
        assert_eq!(
            submit
                .prev()
                .expect("prev sibling")
                .attr("id")
                .expect("prev id"),
            Some("name".to_string())
        );

        let children = items.children().expect("direct children");
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0].text().expect("first child text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            children[1].text().expect("second child text"),
            Some("beta".to_string())
        );
        assert_eq!(
            submit.raw_text().expect("submit raw text"),
            Some("Go".to_string())
        );
    }

    #[test]
    fn snapshot_relative_lists_cover_before_after_and_filtered_siblings() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let prevs = submit.prevs().expect("previous siblings");
        assert_eq!(prevs.len(), 2);
        assert_eq!(prevs[0].tag().expect("first prev tag"), "h1".to_string());
        assert_eq!(
            prevs[1].attr("id").expect("second prev id"),
            Some("name".to_string())
        );

        let nexts = submit.nexts().expect("next siblings");
        assert_eq!(nexts.len(), 2);
        assert_eq!(
            nexts[0].attr("id").expect("first next id"),
            Some("out".to_string())
        );
        assert_eq!(nexts[1].tag().expect("second next tag"), "ul".to_string());

        let afters = submit.afters_with(Some(".item")).expect("following items");
        assert_eq!(afters.len(), 2);
        assert_eq!(
            afters[0].text().expect("first after text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            submit
                .after_with(Some(".item"), 2)
                .expect("second matching after")
                .text()
                .expect("second matching after text"),
            Some("beta".to_string())
        );

        let befores = second_item
            .befores_with(Some(".item"))
            .expect("preceding items");
        assert_eq!(befores.len(), 1);
        assert_eq!(
            befores[0].text().expect("preceding item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            second_item
                .before()
                .expect("nearest preceding element")
                .text()
                .expect("nearest preceding text"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn snapshot_parent_supports_level_and_xpath_filter() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        assert_eq!(
            second_item
                .parent_level(1)
                .expect("list parent")
                .tag()
                .expect("list parent tag"),
            "ul".to_string()
        );
        assert_eq!(
            second_item
                .parent_level(3)
                .expect("document parent")
                .tag()
                .expect("document parent tag"),
            "body".to_string()
        );
        assert_eq!(
            second_item
                .parent_with("xpath:section[@id='root']", 1)
                .expect("section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );
        assert_eq!(
            second_item
                .parent_with("section#root", 1)
                .expect("css section parent")
                .attr("id")
                .expect("css section id"),
            Some("root".to_string())
        );
    }

    #[test]
    fn snapshot_parent_and_child_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let items = root.find(".items").expect("items should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        assert_eq!(
            second_item
                .parent_with((By::XPATH, "section[@id='root']"), 1)
                .expect("xpath section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );
        assert_eq!(
            second_item
                .parent_with((By::CSS_SELECTOR, "section#root"), 1)
                .expect("css section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );

        let direct_child = items
            .child_with(Some((By::XPATH, "li[@data-kind='b']")), 1)
            .expect("matching child");
        assert_eq!(
            direct_child.text().expect("child text"),
            Some("beta".to_string())
        );

        let tag_children = items
            .children_with(Some((By::TAG_NAME, "li")))
            .expect("matching tag children");
        assert_eq!(tag_children.len(), 2);
    }

    #[test]
    fn snapshot_prev_and_next_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");

        let prev_input = submit
            .prev_with(Some((By::TAG_NAME, "input")), 1)
            .expect("matching previous input");
        assert_eq!(
            prev_input.attr("id").expect("input id"),
            Some("name".to_string())
        );

        let next_div = submit
            .next_with(Some((By::XPATH, "div[@id='out']")), 1)
            .expect("matching next div");
        assert_eq!(
            next_div.attr("id").expect("div id"),
            Some("out".to_string())
        );

        let next_divs = submit
            .nexts_with(Some((By::TAG_NAME, "div")))
            .expect("matching next divs");
        assert_eq!(next_divs.len(), 1);

        let prev_inputs = submit
            .prevs_with(Some((By::XPATH, "input")))
            .expect("matching previous inputs");
        assert_eq!(prev_inputs.len(), 1);
    }

    #[test]
    fn snapshot_before_and_after_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let after_item = submit
            .after_with(Some((By::CLASS_NAME, "item")), 2)
            .expect("second matching after");
        assert_eq!(
            after_item.text().expect("after item text"),
            Some("beta".to_string())
        );

        let after_items = submit
            .afters_with(Some((By::TAG_NAME, "li")))
            .expect("matching after items");
        assert_eq!(after_items.len(), 2);

        let before_item = second_item
            .before_with(Some((By::XPATH, "li")), 1)
            .expect("matching before item");
        assert_eq!(
            before_item.text().expect("before item text"),
            Some("alpha".to_string())
        );

        let before_items = second_item
            .befores_with(Some((By::CLASS_NAME, "item")))
            .expect("matching before items");
        assert_eq!(before_items.len(), 1);
    }

    #[test]
    fn snapshot_relative_lists_support_xpath_filters() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let direct_child = items
            .children_with(Some("xpath:li[@data-kind='b']"))
            .expect("matching child");
        assert_eq!(direct_child.len(), 1);
        assert_eq!(
            direct_child[0].text().expect("child text"),
            Some("beta".to_string())
        );

        let prev_inputs = submit
            .prevs_with(Some("xpath:input"))
            .expect("previous matching inputs");
        assert_eq!(prev_inputs.len(), 1);
        assert_eq!(
            prev_inputs[0].attr("id").expect("input id"),
            Some("name".to_string())
        );

        let before_items = second_item
            .befores_with(Some("xpath:li"))
            .expect("preceding items");
        assert_eq!(before_items.len(), 1);
        assert_eq!(
            before_items[0].text().expect("preceding item text"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_root_uses_first_element_in_fragment() {
        let root = snapshot_fragment_root(r#"<section id="root"><span>demo</span></section>"#)
            .expect("fragment root should exist");
        assert_eq!(
            root.tag().expect("fragment root tag"),
            "section".to_string()
        );
        assert_eq!(
            root.attr("id").expect("fragment root id"),
            Some("root".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_find_supports_xpath_queries() {
        let span = snapshot_fragment_find(
            r#"<section id="root"><span class="value">demo</span></section>"#,
            "xpath:/section/span",
        )
        .expect("fragment span should exist");
        let root = snapshot_fragment_root(
            r#"<section id="root"><span class="value">demo</span></section>"#,
        )
        .expect("fragment root should exist");
        let nested = root
            .find("xpath:/span")
            .expect("root-relative xpath child should exist");

        assert_eq!(
            span.attr("class").expect("span class"),
            Some("value".to_string())
        );
        assert_eq!(
            nested.text().expect("nested text"),
            Some("demo".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_paths_ignore_internal_wrapper() {
        let root = snapshot_fragment_root(r#"<section id="root"><span>demo</span></section>"#)
            .expect("fragment root should exist");
        assert_eq!(
            root.xpath().expect("fragment xpath"),
            "/section[1]".to_string()
        );
        assert_eq!(
            root.css_path().expect("fragment css path"),
            "section#root".to_string()
        );
    }

    #[test]
    fn snapshot_helpers_resolve_special_attrs_with_base_url() {
        let root = snapshot_fragment_root_with_base_url(
            r#"<div><a id="doc" href="/docs">Docs</a><img id="logo" src="img/logo.png" /></div>"#,
            Some("https://example.com/start"),
        )
        .expect("fragment root should exist");
        let link = root.find("#doc").expect("link should exist");
        let image = root.find("#logo").expect("image should exist");

        assert_eq!(
            link.attr("href").expect("href attr"),
            Some("https://example.com/docs".to_string())
        );
        assert_eq!(
            link.attr("text").expect("text attr"),
            Some("Docs".to_string())
        );
        assert_eq!(
            image.attr("src").expect("src attr"),
            Some("https://example.com/img/logo.png".to_string())
        );
        assert_eq!(
            image.link().expect("image link"),
            Some("https://example.com/img/logo.png".to_string())
        );
    }

    #[test]
    fn snapshot_helpers_cover_comments_texts_and_child_count() {
        let root = snapshot_fragment_root(
            r#"<div id="root"> alpha <!--note--><span>beta</span> <em>gamma</em></div>"#,
        )
        .expect("fragment root should exist");

        assert_eq!(root.child_count().expect("child count"), 2);
        assert_eq!(root.comments().expect("comments"), vec!["note".to_string()]);
        assert_eq!(
            root.texts(true).expect("direct texts"),
            vec!["alpha".to_string()]
        );
        assert_eq!(
            root.texts(false).expect("texts"),
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn snapshot_query_xpath_supports_non_element_results() {
        let root = snapshot_fragment_root_with_base_url(
            r#"<div id="root"> alpha <!--note--><a href="/docs">Docs</a><span>beta</span></div>"#,
            Some("https://example.com/start"),
        )
        .expect("fragment root should exist");

        let text_and_element = root
            .query_xpath("./text() | *")
            .expect("text and element query");
        assert!(matches!(text_and_element[0], SessionXPathResult::Text(_)));
        assert!(matches!(
            text_and_element[1],
            SessionXPathResult::Element(_)
        ));
        assert!(matches!(
            text_and_element[2],
            SessionXPathResult::Element(_)
        ));

        let comments = root.query_xpath(".//comment()").expect("comments query");
        match &comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment result, got {other:?}"),
        }

        let attrs = root.query_xpath("./a/@href").expect("attribute query");
        match &attrs[0] {
            SessionXPathResult::Attribute { name, value } => {
                assert_eq!(name, "href");
                assert_eq!(value, "/docs");
            }
            other => panic!("expected attribute result, got {other:?}"),
        }

        let counts = root.query_xpath("count(./*)").expect("count query");
        match counts[0] {
            SessionXPathResult::Integer(value) => assert_eq!(value, 2),
            SessionXPathResult::Number(value) => assert_eq!(value, 2.0),
            ref other => panic!("expected numeric result, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_supports_non_element_nodes() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let first = root.find("#a").expect("first span should exist");
        let second = root.find("#b").expect("second span should exist");

        let children = root.children_nodes().expect("child nodes");
        assert_eq!(children.len(), 6);
        match &children[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child, got {other:?}"),
        }
        match &children[1] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child, got {other:?}"),
        }
        assert!(matches!(children[2], SessionXPathResult::Element(_)));
        assert!(matches!(children[3], SessionXPathResult::Element(_)));
        assert!(matches!(children[4], SessionXPathResult::Element(_)));
        match &children[5] {
            SessionXPathResult::Text(value) => assert_eq!(value, "tail"),
            other => panic!("expected trailing text child, got {other:?}"),
        }

        match first
            .next_node_with(None::<&str>, 1)
            .expect("next sibling node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("next id"), Some("b".to_string()))
            }
            other => panic!("expected next element node, got {other:?}"),
        }
        match second
            .prev_node_with(None::<&str>, 1)
            .expect("previous sibling node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("prev id"), Some("a".to_string()))
            }
            other => panic!("expected previous element node, got {other:?}"),
        }
        match second
            .before_node_with(None::<&str>, 1)
            .expect("preceding node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("before id"), Some("a".to_string()))
            }
            other => panic!("expected preceding element node, got {other:?}"),
        }
        match second
            .after_node_with(None::<&str>, 1)
            .expect("following node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("after id"), Some("c".to_string()))
            }
            other => panic!("expected following element node, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_supports_xpath_filters_and_rejects_css() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let second = root.find("#b").expect("second span should exist");

        let text_nodes = root
            .children_nodes_with(Some("xpath:text()"))
            .expect("text child nodes");
        assert_eq!(text_nodes.len(), 2);
        match &text_nodes[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected first text child, got {other:?}"),
        }
        match &text_nodes[1] {
            SessionXPathResult::Text(value) => assert_eq!(value, "tail"),
            other => panic!("expected second text child, got {other:?}"),
        }

        let comments = second
            .prev_nodes_with(Some("xpath:comment()"))
            .expect("comment siblings");
        assert_eq!(comments.len(), 1);
        match &comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment sibling, got {other:?}"),
        }

        match root.children_nodes_with(Some("span")) {
            Err(OpenPageError::UnsupportedLocator(message)) => {
                assert_eq!(message, "css locator is not supported for node queries")
            }
            other => panic!("expected css node query rejection, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_accepts_by_locator_tuples() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let second = root.find("#b").expect("second span should exist");

        match second
            .prev_node_with(Some((By::XPATH, "comment()")), 1)
            .expect("previous comment node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }

        match second
            .next_node_with(Some((By::XPATH, "strong[@id='c']")), 1)
            .expect("next strong node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("next id"), Some("c".to_string()))
            }
            other => panic!("expected next strong node, got {other:?}"),
        }

        match second
            .before_node_with(Some((By::XPATH, "span[@id='a']")), 1)
            .expect("preceding span node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("before id"), Some("a".to_string()))
            }
            other => panic!("expected preceding span node, got {other:?}"),
        }

        match second
            .after_node_with(Some((By::XPATH, "strong[@id='c']")), 1)
            .expect("following strong node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("after id"), Some("c".to_string()))
            }
            other => panic!("expected following strong node, got {other:?}"),
        }

        match second.prev_nodes_with(Some((By::CSS_SELECTOR, "span"))) {
            Err(OpenPageError::UnsupportedLocator(message)) => {
                assert_eq!(message, "css locator is not supported for node queries")
            }
            other => panic!("expected css tuple node query rejection, got {other:?}"),
        }
    }

    #[test]
    fn session_find_locators_supports_any_one_and_first_match_only() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let locators = vec![".missing".to_string(), ".item".to_string()];
        let tuple_locators = [(By::ID, "submit"), (By::CLASS_NAME, "item")];
        let mixed_locators = [
            LocatorInput::from(".missing"),
            LocatorInput::from((By::CLASS_NAME, "item")),
        ];
        let single = root
            .find_locators((By::CLASS_NAME, "item"), true, true)
            .expect("find single by locator");

        assert_eq!(single.len(), 1);
        assert_eq!(single[0].locator, "@class=item".to_string());
        assert_eq!(single[0].elements.len(), 1);

        let any_one = root
            .find_locators(&locators, true, true)
            .expect("find any locator");
        assert_eq!(any_one.len(), 1);
        assert_eq!(any_one[0].locator, ".item".to_string());
        assert_eq!(any_one[0].elements.len(), 1);
        assert_eq!(
            any_one[0].elements[0].text().expect("matched text"),
            Some("alpha".to_string())
        );

        let all = root
            .find_locators(&locators, false, false)
            .expect("find all locators");
        assert_eq!(all.len(), 2);
        assert!(all[0].elements.is_empty());
        assert_eq!(all[1].locator, ".item".to_string());
        assert_eq!(all[1].elements.len(), 2);

        let tuples = root
            .find_locators(&tuple_locators, false, true)
            .expect("find tuple locator list");
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].locator, "@id=submit".to_string());
        assert_eq!(tuples[0].elements.len(), 1);
        assert_eq!(tuples[1].locator, "@class=item".to_string());
        assert_eq!(tuples[1].elements.len(), 1);

        let mixed = root
            .find_locators(&mixed_locators, true, true)
            .expect("find mixed locator list");
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].locator, "@class=item".to_string());
        assert_eq!(mixed[0].elements.len(), 1);
    }
}
