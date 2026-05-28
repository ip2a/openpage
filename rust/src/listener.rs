use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chromiumoxide::cdp::browser_protocol::network::{
    EventLoadingFailed, EventLoadingFinished, EventRequestWillBeSent,
    EventRequestWillBeSentExtraInfo, EventResponseReceived, EventResponseReceivedExtraInfo,
    GetRequestPostDataParams, GetResponseBodyParams, Headers, ResourceType, Response,
};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use url::Url;

use crate::error::{OpenPageError, OpenPageResult};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerRequest {
    pub all_info: Value,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub post_data: Option<String>,
    pub extra_info: Option<ListenerRequestExtraInfo>,
}

impl ListenerRequest {
    pub fn params(&self) -> HashMap<String, String> {
        Url::parse(&self.url)
            .ok()
            .map(|url| url.query_pairs().into_owned().collect())
            .unwrap_or_default()
    }

    pub fn post_data_json(&self) -> Option<Value> {
        self.post_data
            .as_deref()
            .and_then(|post_data| serde_json::from_str(post_data).ok())
    }

    pub fn cookies(&self) -> Vec<Value> {
        self.extra_info
            .as_ref()
            .map(ListenerRequestExtraInfo::cookies)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerRequestExtraInfo {
    pub all_info: Value,
    pub headers: HashMap<String, String>,
    pub associated_cookies: Vec<ListenerAssociatedCookie>,
}

impl ListenerRequestExtraInfo {
    pub fn cookies(&self) -> Vec<Value> {
        self.associated_cookies
            .iter()
            .filter(|cookie| cookie.blocked_reasons.is_empty())
            .map(|cookie| cookie.cookie.clone())
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerAssociatedCookie {
    pub cookie: Value,
    pub blocked_reasons: Vec<String>,
    pub exemption_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerResponse {
    pub all_info: Value,
    pub url: String,
    pub status: i64,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub mime_type: String,
    pub body: Option<String>,
    pub body_base64: bool,
    pub extra_info: Option<ListenerResponseExtraInfo>,
}

impl ListenerResponse {
    pub fn raw_body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    pub fn body_bytes(&self) -> OpenPageResult<Option<Vec<u8>>> {
        match self.body.as_deref() {
            None => Ok(None),
            Some(body) if self.body_base64 => {
                BASE64_STANDARD.decode(body).map(Some).map_err(|err| {
                    OpenPageError::Serialization(format!(
                        "failed to decode listener response body: {err}"
                    ))
                })
            }
            Some(body) => Ok(Some(body.as_bytes().to_vec())),
        }
    }

    pub fn body_text(&self) -> OpenPageResult<Option<String>> {
        match self.body_bytes()? {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes).map(Some).map_err(|err| {
                OpenPageError::Serialization(format!(
                    "failed to decode listener response body as utf-8: {err}"
                ))
            }),
        }
    }

    pub fn body_json(&self) -> OpenPageResult<Option<Value>> {
        match self.body_text()? {
            None => Ok(None),
            Some(body) => serde_json::from_str(&body).map(Some).map_err(|err| {
                OpenPageError::Serialization(format!(
                    "failed to parse listener response body as json: {err}"
                ))
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerResponseExtraInfo {
    pub all_info: Value,
    pub headers: HashMap<String, String>,
    pub status_code: i64,
    pub headers_text: Option<String>,
    pub blocked_cookies: Vec<ListenerBlockedSetCookie>,
    pub exempted_cookies: Vec<ListenerExemptedSetCookie>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerBlockedSetCookie {
    pub blocked_reasons: Vec<String>,
    pub cookie_line: String,
    pub cookie: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerExemptedSetCookie {
    pub exemption_reason: String,
    pub cookie_line: String,
    pub cookie: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerFailInfo {
    pub all_info: Value,
    pub error_text: String,
    pub canceled: Option<bool>,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerPacket {
    pub tab_id: String,
    pub matched_target: Option<String>,
    pub frame_id: Option<String>,
    pub url: String,
    pub method: String,
    pub resource_type: Option<String>,
    pub is_failed: bool,
    pub request: ListenerRequest,
    pub response: Option<ListenerResponse>,
    pub fail_info: Option<ListenerFailInfo>,
    #[serde(skip, default)]
    late_extra_info: Option<Arc<ListenerPacketLateExtraInfo>>,
}

#[derive(Clone, Debug, Default)]
struct LatePacketExtraInfoState {
    request_extra_info: Option<ListenerRequestExtraInfo>,
    response_extra_info: Option<ListenerResponseExtraInfo>,
}

#[derive(Debug, Default)]
struct ListenerPacketLateExtraInfo {
    state: StdMutex<LatePacketExtraInfoState>,
    condvar: Condvar,
}

impl ListenerPacketLateExtraInfo {
    fn set_request_extra_info(&self, extra_info: ListenerRequestExtraInfo) -> OpenPageResult<()> {
        let mut state = self.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener packet state lock poisoned".to_string())
        })?;
        state.request_extra_info = Some(extra_info);
        self.condvar.notify_all();
        Ok(())
    }

    fn set_response_extra_info(&self, extra_info: ListenerResponseExtraInfo) -> OpenPageResult<()> {
        let mut state = self.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener packet state lock poisoned".to_string())
        })?;
        state.response_extra_info = Some(extra_info);
        self.condvar.notify_all();
        Ok(())
    }
}

impl ListenerPacket {
    pub fn wait_extra_info(&mut self, timeout_ms: Option<u64>) -> OpenPageResult<bool> {
        self.apply_late_extra_info()?;
        if self.has_response_extra_info() {
            return Ok(true);
        }

        let Some(shared) = self.late_extra_info.clone() else {
            return Ok(true);
        };

        let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
        let mut state = shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener packet state lock poisoned".to_string())
        })?;

        loop {
            if state.response_extra_info.is_some() {
                let late_state = state.clone();
                drop(state);
                self.apply_late_extra_info_state(late_state);
                return Ok(self.has_response_extra_info());
            }

            match deadline {
                None => {
                    state = shared.condvar.wait(state).map_err(|_| {
                        OpenPageError::BrowserOperation(
                            "listener packet state lock poisoned".to_string(),
                        )
                    })?;
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(false);
                    }
                    let wait_for = deadline.saturating_duration_since(now);
                    let result = shared.condvar.wait_timeout(state, wait_for).map_err(|_| {
                        OpenPageError::BrowserOperation(
                            "listener packet state lock poisoned".to_string(),
                        )
                    })?;
                    state = result.0;
                    if result.1.timed_out() && state.response_extra_info.is_none() {
                        return Ok(false);
                    }
                }
            }
        }
    }

    fn has_response_extra_info(&self) -> bool {
        self.response
            .as_ref()
            .and_then(|response| response.extra_info.as_ref())
            .is_some()
    }

    fn apply_late_extra_info(&mut self) -> OpenPageResult<()> {
        let Some(shared) = self.late_extra_info.clone() else {
            return Ok(());
        };
        let state = {
            let state = shared.state.lock().map_err(|_| {
                OpenPageError::BrowserOperation("listener packet state lock poisoned".to_string())
            })?;
            state.clone()
        };
        self.apply_late_extra_info_state(state);
        Ok(())
    }

    fn apply_late_extra_info_state(&mut self, state: LatePacketExtraInfoState) {
        if let Some(extra_info) = state.request_extra_info {
            merge_request_extra_info(&mut self.request, extra_info);
        }
        if let Some(extra_info) = state.response_extra_info {
            merge_response_extra_info(&mut self.response, &self.request.url, extra_info);
        }
    }
}

#[derive(Clone, Debug)]
enum TargetMatcher {
    All,
    Substrings(Vec<String>),
    Regexes(Vec<Regex>),
}

#[derive(Clone, Debug)]
struct ListenerFilterConfig {
    targets: Option<Vec<String>>,
    is_regex: bool,
    methods: Option<Vec<String>>,
    resource_types: Option<Vec<String>>,
}

impl Default for ListenerFilterConfig {
    fn default() -> Self {
        Self {
            targets: None,
            is_regex: false,
            methods: Some(default_listener_methods()),
            resource_types: None,
        }
    }
}

impl ListenerFilterConfig {
    fn compile(&self) -> OpenPageResult<ListenerFilters> {
        ListenerFilters::new(
            self.targets.clone(),
            self.is_regex,
            self.methods.clone(),
            self.resource_types.clone(),
        )
    }

    fn update(
        &mut self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) {
        if let Some(targets) = targets {
            self.targets = if targets.is_empty() {
                None
            } else {
                Some(targets)
            };
            self.is_regex = is_regex;
        }

        if let Some(methods) = methods {
            self.methods = if methods.is_empty() {
                None
            } else {
                Some(methods)
            };
        }

        if let Some(resource_types) = resource_types {
            self.resource_types = if resource_types.is_empty() {
                None
            } else {
                Some(resource_types)
            };
        }
    }
}

#[derive(Clone, Debug)]
struct ListenerFilters {
    targets: TargetMatcher,
    methods: Option<HashSet<String>>,
    resource_types: Option<HashSet<String>>,
}

impl Default for ListenerFilters {
    fn default() -> Self {
        Self {
            targets: TargetMatcher::All,
            methods: normalize_set(Some(default_listener_methods())),
            resource_types: None,
        }
    }
}

impl ListenerFilters {
    fn new(
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> OpenPageResult<Self> {
        let targets = match targets {
            None => TargetMatcher::All,
            Some(targets) if targets.is_empty() => TargetMatcher::All,
            Some(targets) if is_regex => {
                let mut compiled = Vec::with_capacity(targets.len());
                for pattern in targets {
                    compiled.push(Regex::new(&pattern).map_err(|err| {
                        OpenPageError::BrowserOperation(format!(
                            "invalid listener regex `{pattern}`: {err}"
                        ))
                    })?);
                }
                TargetMatcher::Regexes(compiled)
            }
            Some(targets) => TargetMatcher::Substrings(targets),
        };

        Ok(Self {
            targets,
            methods: normalize_set(methods),
            resource_types: normalize_set(resource_types),
        })
    }

    fn matches(
        &self,
        url: &str,
        method: &str,
        resource_type: Option<&str>,
    ) -> Option<Option<String>> {
        if self
            .methods
            .as_ref()
            .is_some_and(|methods| !methods.contains(&method.to_ascii_uppercase()))
        {
            return None;
        }

        let normalized_type = resource_type.map(|value| value.to_ascii_uppercase());
        if self.resource_types.as_ref().is_some_and(|resource_types| {
            normalized_type
                .as_ref()
                .is_none_or(|value| !resource_types.contains(value))
        }) {
            return None;
        }

        match &self.targets {
            TargetMatcher::All => Some(None),
            TargetMatcher::Substrings(targets) => targets
                .iter()
                .find(|target| url.contains(target.as_str()))
                .cloned()
                .map(Some),
            TargetMatcher::Regexes(patterns) => patterns
                .iter()
                .find(|pattern| pattern.is_match(url))
                .map(|pattern| Some(pattern.as_str().to_string())),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingPacket {
    tab_id: String,
    matched_target: Option<String>,
    frame_id: Option<String>,
    request: ListenerRequest,
    response: Option<ListenerResponse>,
    resource_type: Option<String>,
    finished: bool,
    response_body: Option<String>,
    response_body_base64: bool,
    awaiting_response_extra_info: bool,
}

impl PendingPacket {
    fn from_request(
        event: &EventRequestWillBeSent,
        matched_target: Option<String>,
        tab_id: &str,
    ) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            matched_target,
            frame_id: event
                .frame_id
                .as_ref()
                .map(|frame_id| frame_id.as_ref().to_string()),
            request: ListenerRequest {
                all_info: cdp_value(&event.request),
                url: event.request.url.clone(),
                method: event.request.method.clone(),
                headers: headers_to_map(&event.request.headers),
                post_data: None,
                extra_info: None,
            },
            response: None,
            resource_type: event.r#type.as_ref().map(resource_type_to_string),
            finished: false,
            response_body: None,
            response_body_base64: false,
            awaiting_response_extra_info: false,
        }
    }

    fn into_packet(self, fail_info: Option<ListenerFailInfo>) -> ListenerPacket {
        ListenerPacket {
            tab_id: self.tab_id,
            matched_target: self.matched_target,
            frame_id: self.frame_id,
            url: self.request.url.clone(),
            method: self.request.method.clone(),
            resource_type: self.resource_type,
            is_failed: fail_info.is_some(),
            request: self.request,
            response: self.response,
            fail_info,
            late_extra_info: None,
        }
    }
}

#[derive(Debug)]
struct ListenerState {
    queue: VecDeque<ListenerPacket>,
    inflight: HashMap<String, PendingPacket>,
    request_extra_infos: HashMap<String, ListenerRequestExtraInfo>,
    response_extra_infos: HashMap<String, ListenerResponseExtraInfo>,
    detached_extra_infos: HashMap<String, Arc<ListenerPacketLateExtraInfo>>,
    running_request_ids: HashSet<String>,
    filter_config: ListenerFilterConfig,
    filters: ListenerFilters,
    listening: bool,
    paused: bool,
    tab_id: String,
    scope_frame_id: Option<String>,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

impl ListenerState {
    fn new(scope_frame_id: Option<String>, tab_id: String) -> Self {
        let filter_config = ListenerFilterConfig::default();
        Self {
            queue: VecDeque::new(),
            inflight: HashMap::new(),
            request_extra_infos: HashMap::new(),
            response_extra_infos: HashMap::new(),
            detached_extra_infos: HashMap::new(),
            running_request_ids: HashSet::new(),
            filters: filter_config
                .compile()
                .expect("default listener filters should compile"),
            filter_config,
            listening: false,
            paused: false,
            tab_id,
            scope_frame_id,
            task: None,
            last_error: None,
        }
    }
}

#[derive(Debug)]
struct ListenerShared {
    state: StdMutex<ListenerState>,
    condvar: Condvar,
}

impl ListenerShared {
    fn new(scope_frame_id: Option<String>, tab_id: String) -> Self {
        Self {
            state: StdMutex::new(ListenerState::new(scope_frame_id, tab_id)),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Listener {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<ListenerShared>,
}

#[derive(Clone, Debug)]
pub struct ListenerSteps {
    listener: Listener,
    remaining: Option<usize>,
    timeout_ms: Option<u64>,
    gap: usize,
    finished: bool,
}

impl Listener {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        Self::new_with_scope(runtime, page, None)
    }

    pub fn new_for_frame(runtime: Arc<Runtime>, page: OxPage, frame_id: impl Into<String>) -> Self {
        Self::new_with_scope(runtime, page, Some(frame_id.into()))
    }

    fn new_with_scope(runtime: Arc<Runtime>, page: OxPage, scope_frame_id: Option<String>) -> Self {
        let tab_id = page.target_id().as_ref().to_string();
        Self {
            runtime,
            page,
            shared: Arc::new(ListenerShared::new(scope_frame_id, tab_id)),
        }
    }

    pub fn start(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;

        state.queue.clear();
        state.inflight.clear();
        state.request_extra_infos.clear();
        state.response_extra_infos.clear();
        state.detached_extra_infos.clear();
        state.running_request_ids.clear();
        state.last_error = None;
        update_listener_filters(&mut state, targets, is_regex, methods, resource_types)?;

        if state.listening {
            state.paused = false;
            self.shared.condvar.notify_all();
            return Ok(());
        }

        let page = self.page.clone();
        let shared = Arc::clone(&self.shared);
        let handle = self.runtime.spawn(async move {
            if let Err(err) = run_listener(page, Arc::clone(&shared)).await {
                let _ = set_listener_stopped(&shared, Some(err.to_string()));
            } else {
                let _ = set_listener_stopped(&shared, None);
            }
        });

        state.listening = true;
        state.task = Some(handle);
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn set_targets(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        update_listener_filters(&mut state, targets, is_regex, methods, resource_types)?;
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn wait(
        &self,
        count: usize,
        timeout_ms: Option<u64>,
        fit_count: bool,
    ) -> OpenPageResult<Vec<ListenerPacket>> {
        let needed = count.max(1);
        let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;

        if !listener_is_active(&state) {
            return Err(listener_not_running_error(&state));
        }

        loop {
            if state.queue.len() >= needed {
                return Ok(pop_packets(&mut state.queue, needed));
            }

            if !listener_is_active(&state) {
                break;
            }

            match deadline {
                None => {
                    state = self.shared.condvar.wait(state).map_err(|_| {
                        OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
                    })?;
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    let wait_for = deadline.saturating_duration_since(now);
                    let result =
                        self.shared
                            .condvar
                            .wait_timeout(state, wait_for)
                            .map_err(|_| {
                                OpenPageError::BrowserOperation(
                                    "listener state lock poisoned".to_string(),
                                )
                            })?;
                    state = result.0;
                    if result.1.timed_out() {
                        break;
                    }
                }
            }
        }

        if fit_count || state.queue.is_empty() {
            if !state.listening && state.last_error.is_some() {
                return Err(listener_not_running_error(&state));
            }
            Err(OpenPageError::Timeout(
                "listener did not capture enough packets in time".to_string(),
            ))
        } else {
            let available = state.queue.len();
            Ok(pop_packets(&mut state.queue, available))
        }
    }

    pub fn wait_one(&self, timeout_ms: Option<u64>) -> OpenPageResult<ListenerPacket> {
        let mut packets = self.wait(1, timeout_ms, true)?;
        Ok(packets.remove(0))
    }

    pub fn steps(
        &self,
        count: Option<usize>,
        timeout_ms: Option<u64>,
        gap: usize,
    ) -> ListenerSteps {
        ListenerSteps {
            listener: self.clone(),
            remaining: count,
            timeout_ms,
            gap: gap.max(1),
            finished: false,
        }
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        state.queue.clear();
        state.inflight.clear();
        state.request_extra_infos.clear();
        state.response_extra_infos.clear();
        state.detached_extra_infos.clear();
        state.running_request_ids.clear();
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn pause(&self, clear: bool) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        if !state.listening {
            return Ok(());
        }
        state.paused = true;
        if clear {
            state.queue.clear();
            state.inflight.clear();
            state.request_extra_infos.clear();
            state.response_extra_infos.clear();
            state.detached_extra_infos.clear();
            state.running_request_ids.clear();
        }
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn resume(&self) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        if !state.listening {
            return Err(listener_not_running_error(&state));
        }
        state.paused = false;
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn wait_until_idle(
        &self,
        timeout_ms: Option<u64>,
        targets_only: bool,
    ) -> OpenPageResult<bool> {
        let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;

        if !listener_is_active(&state) {
            return Err(listener_not_running_error(&state));
        }

        loop {
            if listener_is_idle(&state, targets_only) {
                return Ok(true);
            }

            if !listener_is_active(&state) {
                return Err(listener_not_running_error(&state));
            }

            match deadline {
                None => {
                    state = self.shared.condvar.wait(state).map_err(|_| {
                        OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
                    })?;
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(listener_is_idle(&state, targets_only));
                    }
                    let wait_for = deadline.saturating_duration_since(now);
                    let result =
                        self.shared
                            .condvar
                            .wait_timeout(state, wait_for)
                            .map_err(|_| {
                                OpenPageError::BrowserOperation(
                                    "listener state lock poisoned".to_string(),
                                )
                            })?;
                    state = result.0;
                    if result.1.timed_out() {
                        return Ok(listener_is_idle(&state, targets_only));
                    }
                }
            }
        }
    }

    pub fn wait_silent(
        &self,
        timeout_ms: Option<u64>,
        targets_only: bool,
        limit: usize,
    ) -> OpenPageResult<bool> {
        let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;

        if !listener_is_active(&state) {
            return Err(listener_not_running_error(&state));
        }

        loop {
            if listener_is_idle_with_limit(&state, targets_only, limit) {
                return Ok(true);
            }

            if !listener_is_active(&state) {
                return Err(listener_not_running_error(&state));
            }

            match deadline {
                None => {
                    state = self.shared.condvar.wait(state).map_err(|_| {
                        OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
                    })?;
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(listener_is_idle_with_limit(&state, targets_only, limit));
                    }
                    let wait_for = deadline.saturating_duration_since(now);
                    let result =
                        self.shared
                            .condvar
                            .wait_timeout(state, wait_for)
                            .map_err(|_| {
                                OpenPageError::BrowserOperation(
                                    "listener state lock poisoned".to_string(),
                                )
                            })?;
                    state = result.0;
                    if result.1.timed_out() {
                        return Ok(listener_is_idle_with_limit(&state, targets_only, limit));
                    }
                }
            }
        }
    }

    pub fn stop(&self) -> OpenPageResult<()> {
        let handle = {
            let mut state = self.shared.state.lock().map_err(|_| {
                OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
            })?;
            state.queue.clear();
            state.inflight.clear();
            state.request_extra_infos.clear();
            state.response_extra_infos.clear();
            state.detached_extra_infos.clear();
            state.running_request_ids.clear();
            state.last_error = None;
            state.listening = false;
            state.paused = false;
            state.task.take()
        };

        if let Some(handle) = handle {
            handle.abort();
        }
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn is_listening(&self) -> OpenPageResult<bool> {
        self.shared
            .state
            .lock()
            .map(|state| listener_is_active(&state))
            .map_err(|_| {
                OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
            })
    }
}

impl Iterator for ListenerSteps {
    type Item = OpenPageResult<Vec<ListenerPacket>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        if self.remaining == Some(0) {
            self.finished = true;
            return None;
        }

        let batch_size = self
            .remaining
            .map(|remaining| remaining.min(self.gap))
            .unwrap_or(self.gap)
            .max(1);
        let result = self.listener.wait(batch_size, self.timeout_ms, true);
        match result {
            Ok(packets) => {
                if let Some(remaining) = self.remaining.as_mut() {
                    *remaining = remaining.saturating_sub(packets.len());
                    if *remaining == 0 {
                        self.finished = true;
                    }
                }
                Some(Ok(packets))
            }
            Err(err) => {
                self.finished = true;
                Some(Err(err))
            }
        }
    }
}

async fn run_listener(page: OxPage, shared: Arc<ListenerShared>) -> OpenPageResult<()> {
    let mut request_events = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    let mut response_events = page
        .event_listener::<EventResponseReceived>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    let mut request_extra_events = page
        .event_listener::<EventRequestWillBeSentExtraInfo>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    let mut response_extra_events = page
        .event_listener::<EventResponseReceivedExtraInfo>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    let mut finished_events = page
        .event_listener::<EventLoadingFinished>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    let mut failed_events = page
        .event_listener::<EventLoadingFailed>()
        .await
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;

    loop {
        tokio::select! {
            event = request_events.next() => match event {
                Some(event) => on_request_will_be_sent(&shared, &event)?,
                None => break,
            },
            event = response_events.next() => match event {
                Some(event) => on_response_received(&shared, &event)?,
                None => break,
            },
            event = request_extra_events.next() => match event {
                Some(event) => on_request_will_be_sent_extra_info(&shared, &event)?,
                None => break,
            },
            event = response_extra_events.next() => match event {
                Some(event) => on_response_received_extra_info(&shared, &event)?,
                None => break,
            },
            event = finished_events.next() => match event {
                Some(event) => on_loading_finished(&shared, &page, &event).await?,
                None => break,
            },
            event = failed_events.next() => match event {
                Some(event) => on_loading_failed(&shared, &event)?,
                None => break,
            },
        }
    }

    Ok(())
}

fn on_request_will_be_sent(
    shared: &Arc<ListenerShared>,
    event: &EventRequestWillBeSent,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let event_frame_id = event
        .frame_id
        .as_ref()
        .map(|frame_id| frame_id.as_ref().to_string());
    let resource_type = event.r#type.as_ref().map(resource_type_to_string);
    let matched_target = {
        let state = shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        if !listener_is_active(&state) {
            return Ok(());
        }
        if !scope_matches_frame(state.scope_frame_id.as_deref(), event_frame_id.as_deref()) {
            return Ok(());
        }
        state.filters.matches(
            &event.request.url,
            &event.request.method,
            resource_type.as_deref(),
        )
    };

    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !listener_is_active(&state) {
        return Ok(());
    }
    state.running_request_ids.insert(request_id.clone());

    if let Some(redirect_response) = &event.redirect_response {
        if let Some(mut pending) = state.inflight.remove(&request_id) {
            pending.response = Some(listener_response_from_cdp(redirect_response));
            finalize_if_ready(&mut state, &request_id, pending);
            shared.condvar.notify_all();
        }
    }

    if let Some(matched_target) = matched_target {
        let mut packet = PendingPacket::from_request(event, matched_target, &state.tab_id);
        if let Some(extra_info) = state.request_extra_infos.remove(&request_id) {
            apply_request_extra_info(&mut packet, extra_info);
        }
        if let Some(extra_info) = state.response_extra_infos.remove(&request_id) {
            apply_response_extra_info(&mut packet, extra_info);
        }
        state.inflight.insert(request_id, packet);
    } else {
        state.inflight.remove(&request_id);
        state.request_extra_infos.remove(&request_id);
        state.response_extra_infos.remove(&request_id);
    }

    Ok(())
}

fn on_request_will_be_sent_extra_info(
    shared: &Arc<ListenerShared>,
    event: &EventRequestWillBeSentExtraInfo,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let extra_info = listener_request_extra_info_from_cdp(event);
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !listener_is_active(&state) {
        return Ok(());
    }
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        apply_request_extra_info(packet, extra_info);
    } else if let Some(packet) = state.detached_extra_infos.get(&request_id).cloned() {
        packet.set_request_extra_info(extra_info)?;
    } else {
        state.request_extra_infos.insert(request_id, extra_info);
    }
    Ok(())
}

fn on_response_received(
    shared: &Arc<ListenerShared>,
    event: &EventResponseReceived,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !listener_is_active(&state) {
        return Ok(());
    }
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        let mut response = listener_response_from_cdp(&event.response);
        let has_existing_extra_info = preserve_existing_response_extra_info(packet, &mut response);
        if let Some(body) = &packet.response_body {
            response.body = Some(body.clone());
            response.body_base64 = packet.response_body_base64;
        }
        packet.response = Some(response);
        packet.resource_type = Some(resource_type_to_string(&event.r#type));
        packet.awaiting_response_extra_info = event.has_extra_info && !has_existing_extra_info;
    }
    let should_finalize = take_ready_packet(&mut state, &request_id);
    if let Some(packet) = should_finalize {
        state.queue.push_back(packet.into_packet(None));
        shared.condvar.notify_all();
    }
    Ok(())
}

fn on_response_received_extra_info(
    shared: &Arc<ListenerShared>,
    event: &EventResponseReceivedExtraInfo,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let extra_info = listener_response_extra_info_from_cdp(event);
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !listener_is_active(&state) {
        return Ok(());
    }
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        apply_response_extra_info(packet, extra_info);
    } else if let Some(packet) = state.detached_extra_infos.remove(&request_id) {
        packet.set_response_extra_info(extra_info)?;
    } else {
        state
            .response_extra_infos
            .insert(request_id.clone(), extra_info);
    }

    let should_finalize = take_ready_packet(&mut state, &request_id);
    if let Some(packet) = should_finalize {
        state.queue.push_back(packet.into_packet(None));
        shared.condvar.notify_all();
    }
    Ok(())
}

async fn on_loading_finished(
    shared: &Arc<ListenerShared>,
    page: &OxPage,
    event: &EventLoadingFinished,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let response_body = fetch_response_body(page, event).await;
    let request_post_data = fetch_request_post_data(page, event).await;
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !state.listening {
        return Ok(());
    }
    state.running_request_ids.remove(&request_id);
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        packet.finished = true;
        if packet.request.post_data.is_none() {
            packet.request.post_data = request_post_data;
        }
        if let Some((body, body_base64)) = response_body {
            packet.response_body = Some(body.clone());
            packet.response_body_base64 = body_base64;
            if let Some(response) = packet.response.as_mut() {
                response.body = Some(body);
                response.body_base64 = body_base64;
            }
        }
    }
    let should_finalize = take_ready_packet(&mut state, &request_id);
    if let Some(packet) = should_finalize {
        state.queue.push_back(packet.into_packet(None));
        shared.condvar.notify_all();
    }
    Ok(())
}

fn on_loading_failed(
    shared: &Arc<ListenerShared>,
    event: &EventLoadingFailed,
) -> OpenPageResult<()> {
    let request_id = event.request_id.as_ref().to_string();
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if !state.listening {
        return Ok(());
    }
    state.running_request_ids.remove(&request_id);
    if let Some(mut packet) = state.inflight.remove(&request_id) {
        packet.resource_type = Some(resource_type_to_string(&event.r#type));
        let mut queued_packet = packet.into_packet(Some(listener_fail_info_from_cdp(event)));
        if !queued_packet.has_response_extra_info() {
            let late_extra_info = Arc::new(ListenerPacketLateExtraInfo::default());
            state
                .detached_extra_infos
                .insert(request_id.clone(), Arc::clone(&late_extra_info));
            queued_packet.late_extra_info = Some(late_extra_info);
        }
        state.queue.push_back(queued_packet);
        shared.condvar.notify_all();
    }
    Ok(())
}

fn set_listener_stopped(shared: &Arc<ListenerShared>, error: Option<String>) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    state.listening = false;
    state.paused = false;
    state.task = None;
    state.last_error = error;
    shared.condvar.notify_all();
    Ok(())
}

fn normalize_set(values: Option<Vec<String>>) -> Option<HashSet<String>> {
    values.and_then(|values| {
        if values.is_empty() {
            None
        } else {
            Some(
                values
                    .into_iter()
                    .map(|value| value.to_ascii_uppercase())
                    .collect(),
            )
        }
    })
}

fn default_listener_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string()]
}

fn update_listener_filters(
    state: &mut ListenerState,
    targets: Option<Vec<String>>,
    is_regex: bool,
    methods: Option<Vec<String>>,
    resource_types: Option<Vec<String>>,
) -> OpenPageResult<()> {
    if targets.is_none() && methods.is_none() && resource_types.is_none() {
        return Ok(());
    }

    state
        .filter_config
        .update(targets, is_regex, methods, resource_types);
    state.filters = state.filter_config.compile()?;
    Ok(())
}

fn scope_matches_frame(scope_frame_id: Option<&str>, event_frame_id: Option<&str>) -> bool {
    match scope_frame_id {
        Some(scope_frame_id) => event_frame_id == Some(scope_frame_id),
        None => true,
    }
}

fn listener_not_running_error(state: &ListenerState) -> OpenPageError {
    if let Some(error) = &state.last_error {
        OpenPageError::BrowserOperation(format!("listener is not running: {error}"))
    } else {
        OpenPageError::BrowserOperation("listener is not running".to_string())
    }
}

fn listener_is_active(state: &ListenerState) -> bool {
    state.listening && !state.paused
}

fn listener_is_idle(state: &ListenerState, targets_only: bool) -> bool {
    listener_is_idle_with_limit(state, targets_only, 0)
}

fn listener_is_idle_with_limit(state: &ListenerState, targets_only: bool, limit: usize) -> bool {
    if targets_only {
        state.inflight.len() <= limit
    } else {
        state.running_request_ids.len() <= limit
    }
}

fn pop_packets(queue: &mut VecDeque<ListenerPacket>, count: usize) -> Vec<ListenerPacket> {
    (0..count).filter_map(|_| queue.pop_front()).collect()
}

fn headers_to_map(headers: &Headers) -> HashMap<String, String> {
    let Some(object) = headers.inner().as_object() else {
        return HashMap::new();
    };

    object
        .iter()
        .map(|(key, value)| {
            let value = match value {
                serde_json::Value::String(value) => value.clone(),
                _ => value.to_string(),
            };
            (key.clone(), value)
        })
        .collect()
}

fn resource_type_to_string(value: &ResourceType) -> String {
    value.as_ref().to_string()
}

fn listener_response_from_cdp(response: &Response) -> ListenerResponse {
    let headers = headers_to_map(&response.headers);
    ListenerResponse {
        all_info: cdp_value(response),
        url: response.url.clone(),
        status: response.status,
        status_text: response.status_text.clone(),
        mime_type: normalize_mime_type(Some(response.mime_type.as_str()), &headers),
        headers,
        body: None,
        body_base64: false,
        extra_info: None,
    }
}

fn listener_request_extra_info_from_cdp(
    event: &EventRequestWillBeSentExtraInfo,
) -> ListenerRequestExtraInfo {
    ListenerRequestExtraInfo {
        all_info: cdp_value(event),
        headers: headers_to_map(&event.headers),
        associated_cookies: event
            .associated_cookies
            .iter()
            .map(listener_associated_cookie_from_cdp)
            .collect(),
    }
}

fn listener_response_extra_info_from_cdp(
    event: &EventResponseReceivedExtraInfo,
) -> ListenerResponseExtraInfo {
    ListenerResponseExtraInfo {
        all_info: cdp_value(event),
        headers: headers_to_map(&event.headers),
        status_code: event.status_code,
        headers_text: event.headers_text.clone(),
        blocked_cookies: event
            .blocked_cookies
            .iter()
            .map(listener_blocked_set_cookie_from_cdp)
            .collect(),
        exempted_cookies: event
            .exempted_cookies
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(listener_exempted_set_cookie_from_cdp)
            .collect(),
    }
}

fn listener_fail_info_from_cdp(event: &EventLoadingFailed) -> ListenerFailInfo {
    ListenerFailInfo {
        all_info: cdp_value(event),
        error_text: event.error_text.clone(),
        canceled: event.canceled,
        blocked_reason: event
            .blocked_reason
            .as_ref()
            .map(|reason| reason.as_ref().to_string()),
    }
}

fn listener_associated_cookie_from_cdp(
    cookie: &chromiumoxide::cdp::browser_protocol::network::AssociatedCookie,
) -> ListenerAssociatedCookie {
    ListenerAssociatedCookie {
        cookie: cdp_value(&cookie.cookie),
        blocked_reasons: cookie
            .blocked_reasons
            .iter()
            .map(|reason| reason.as_ref().to_string())
            .collect(),
        exemption_reason: cookie
            .exemption_reason
            .as_ref()
            .map(|reason| reason.as_ref().to_string()),
    }
}

fn listener_blocked_set_cookie_from_cdp(
    cookie: &chromiumoxide::cdp::browser_protocol::network::BlockedSetCookieWithReason,
) -> ListenerBlockedSetCookie {
    ListenerBlockedSetCookie {
        blocked_reasons: cookie
            .blocked_reasons
            .iter()
            .map(|reason| reason.as_ref().to_string())
            .collect(),
        cookie_line: cookie.cookie_line.clone(),
        cookie: cookie.cookie.as_ref().map(cdp_value),
    }
}

fn listener_exempted_set_cookie_from_cdp(
    cookie: &chromiumoxide::cdp::browser_protocol::network::ExemptedSetCookieWithReason,
) -> ListenerExemptedSetCookie {
    ListenerExemptedSetCookie {
        exemption_reason: cookie.exemption_reason.as_ref().to_string(),
        cookie_line: cookie.cookie_line.clone(),
        cookie: cdp_value(&cookie.cookie),
    }
}

fn apply_request_extra_info(packet: &mut PendingPacket, extra_info: ListenerRequestExtraInfo) {
    merge_request_extra_info(&mut packet.request, extra_info);
}

fn apply_response_extra_info(packet: &mut PendingPacket, extra_info: ListenerResponseExtraInfo) {
    merge_response_extra_info(&mut packet.response, &packet.request.url, extra_info);
    packet.awaiting_response_extra_info = false;
}

fn normalize_mime_type(raw_mime_type: Option<&str>, headers: &HashMap<String, String>) -> String {
    let mime_type = raw_mime_type.unwrap_or_default().trim();
    if !mime_type.is_empty() {
        return mime_type.to_string();
    }

    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-type"))
        .and_then(|(_, value)| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn cdp_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("cdp data should serialize to json")
}

fn preserve_existing_response_extra_info(
    packet: &PendingPacket,
    response: &mut ListenerResponse,
) -> bool {
    let Some(extra_info) = packet
        .response
        .as_ref()
        .and_then(|existing| existing.extra_info.clone())
    else {
        return false;
    };

    merge_headers(&mut response.headers, &extra_info.headers);
    response.status = extra_info.status_code;
    response.extra_info = Some(extra_info);
    true
}

fn merge_request_extra_info(request: &mut ListenerRequest, extra_info: ListenerRequestExtraInfo) {
    merge_headers(&mut request.headers, &extra_info.headers);
    request.extra_info = Some(extra_info);
}

fn merge_response_extra_info(
    response: &mut Option<ListenerResponse>,
    request_url: &str,
    extra_info: ListenerResponseExtraInfo,
) {
    if let Some(response) = response.as_mut() {
        merge_headers(&mut response.headers, &extra_info.headers);
        response.extra_info = Some(extra_info.clone());
        response.status = extra_info.status_code;
        response.mime_type =
            normalize_mime_type(Some(response.mime_type.as_str()), &response.headers);
        return;
    }

    let headers = extra_info.headers.clone();
    *response = Some(ListenerResponse {
        all_info: Value::Null,
        url: request_url.to_string(),
        status: extra_info.status_code,
        status_text: String::new(),
        mime_type: normalize_mime_type(None, &headers),
        headers,
        body: None,
        body_base64: false,
        extra_info: Some(extra_info),
    });
}

fn response_ready(packet: &PendingPacket) -> bool {
    packet.response.is_some() && !packet.awaiting_response_extra_info
}

fn merge_headers(base: &mut HashMap<String, String>, extra: &HashMap<String, String>) {
    let mut existing = base
        .keys()
        .map(|key| key.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for (key, value) in extra {
        let normalized = key.to_ascii_lowercase();
        if existing.insert(normalized) {
            base.insert(key.clone(), value.clone());
        }
    }
}

fn finalize_if_ready(state: &mut ListenerState, request_id: &str, packet: PendingPacket) {
    if response_ready(&packet) || !packet.awaiting_response_extra_info {
        state.queue.push_back(packet.into_packet(None));
    } else {
        state.inflight.insert(request_id.to_string(), packet);
    }
}

fn take_ready_packet(state: &mut ListenerState, request_id: &str) -> Option<PendingPacket> {
    let is_ready = state
        .inflight
        .get(request_id)
        .is_some_and(|packet| packet.finished && response_ready(packet));
    if is_ready {
        state.inflight.remove(request_id)
    } else {
        None
    }
}

async fn fetch_response_body(
    page: &OxPage,
    event: &EventLoadingFinished,
) -> Option<(String, bool)> {
    let response = page
        .execute(GetResponseBodyParams::new(event.request_id.clone()))
        .await
        .ok()?;
    Some((response.result.body, response.result.base64_encoded))
}

async fn fetch_request_post_data(page: &OxPage, event: &EventLoadingFinished) -> Option<String> {
    let response = page
        .execute(GetRequestPostDataParams::new(event.request_id.clone()))
        .await
        .ok()?;
    Some(response.result.post_data)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use super::{
        ListenerAssociatedCookie, ListenerFilters, ListenerRequest, ListenerRequestExtraInfo,
        ListenerResponse, ListenerResponseExtraInfo, ListenerShared, ListenerState, PendingPacket,
        apply_response_extra_info, headers_to_map, listener_request_extra_info_from_cdp,
        listener_response_extra_info_from_cdp, listener_response_from_cdp, on_loading_failed,
        on_request_will_be_sent, on_request_will_be_sent_extra_info,
        on_response_received_extra_info, preserve_existing_response_extra_info,
        update_listener_filters,
    };
    use chromiumoxide::cdp::browser_protocol::network::{
        EventLoadingFailed, EventRequestWillBeSent, EventRequestWillBeSentExtraInfo,
        EventResponseReceivedExtraInfo, Headers, Response,
    };
    use serde_json::json;

    #[test]
    fn headers_are_converted_into_plain_strings() {
        let headers = Headers::new(serde_json::json!({
            "Content-Type": "application/json",
            "X-Count": 3,
        }));

        let converted = headers_to_map(&headers);
        assert_eq!(
            converted.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(converted.get("X-Count"), Some(&"3".to_string()));
    }

    #[test]
    fn filters_match_substrings_and_normalize_method_and_resource_type() {
        let filters = ListenerFilters::new(
            Some(vec!["/api/data".to_string()]),
            false,
            Some(vec!["post".to_string()]),
            Some(vec!["fetch".to_string()]),
        )
        .expect("filters should compile");

        assert_eq!(
            filters.matches("http://127.0.0.1/api/data", "POST", Some("Fetch"),),
            Some(Some("/api/data".to_string()))
        );
        assert_eq!(
            filters.matches("http://127.0.0.1/other", "POST", Some("Fetch"),),
            None
        );
    }

    #[test]
    fn default_filters_only_match_get_and_post() {
        let filters = ListenerFilters::default();

        assert_eq!(
            filters.matches("http://127.0.0.1/api", "GET", None),
            Some(None)
        );
        assert_eq!(
            filters.matches("http://127.0.0.1/api", "POST", None),
            Some(None)
        );
        assert_eq!(filters.matches("http://127.0.0.1/api", "PUT", None), None);
    }

    #[test]
    fn filter_updates_preserve_existing_values_when_args_are_none() {
        let mut state = ListenerState::new(None, "tab-1".to_string());
        update_listener_filters(
            &mut state,
            Some(vec!["/api/data".to_string()]),
            true,
            Some(vec!["patch".to_string()]),
            Some(vec!["xhr".to_string()]),
        )
        .expect("filters should update");

        update_listener_filters(&mut state, None, false, None, None)
            .expect("empty update should preserve filters");

        assert_eq!(
            state
                .filters
                .matches("http://127.0.0.1/api/data", "PATCH", Some("XHR")),
            Some(Some("/api/data".to_string()))
        );
        assert_eq!(
            state
                .filters
                .matches("http://127.0.0.1/api/data", "GET", Some("XHR")),
            None
        );
    }

    #[test]
    fn request_helpers_parse_query_and_json_post_data() {
        let request = ListenerRequest {
            all_info: json!({}),
            url: "http://127.0.0.1/api/data?foo=1&bar=".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            post_data: Some("{\"name\":\"openpage\"}".to_string()),
            extra_info: None,
        };

        assert_eq!(
            request.params(),
            HashMap::from([
                ("foo".to_string(), "1".to_string()),
                ("bar".to_string(), "".to_string()),
            ])
        );
        assert_eq!(request.post_data_json(), Some(json!({"name": "openpage"})));
    }

    #[test]
    fn request_cookies_only_return_unblocked_associated_cookies() {
        let allowed_cookie = json!({"name": "sid", "value": "1"});
        let blocked_cookie = json!({"name": "blocked", "value": "0"});
        let request = ListenerRequest {
            all_info: json!({}),
            url: "http://127.0.0.1/api/data".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            post_data: None,
            extra_info: Some(ListenerRequestExtraInfo {
                all_info: json!({}),
                headers: HashMap::new(),
                associated_cookies: vec![
                    ListenerAssociatedCookie {
                        cookie: allowed_cookie.clone(),
                        blocked_reasons: Vec::new(),
                        exemption_reason: None,
                    },
                    ListenerAssociatedCookie {
                        cookie: blocked_cookie,
                        blocked_reasons: vec!["DomainMismatch".to_string()],
                        exemption_reason: None,
                    },
                ],
            }),
        };

        assert_eq!(request.cookies(), vec![allowed_cookie]);
    }

    #[test]
    fn response_helpers_decode_plain_text_and_json() {
        let response = ListenerResponse {
            all_info: json!({}),
            url: "http://127.0.0.1/api/data".to_string(),
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            mime_type: "application/json".to_string(),
            body: Some("{\"ok\":true}".to_string()),
            body_base64: false,
            extra_info: None,
        };

        assert_eq!(response.raw_body(), Some("{\"ok\":true}"));
        assert_eq!(
            response.body_text().expect("plain text body should decode"),
            Some("{\"ok\":true}".to_string())
        );
        assert_eq!(
            response.body_json().expect("json body should parse"),
            Some(json!({"ok": true}))
        );
    }

    #[test]
    fn response_helpers_decode_base64_body() {
        let response = ListenerResponse {
            all_info: json!({}),
            url: "http://127.0.0.1/file".to_string(),
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            mime_type: "application/octet-stream".to_string(),
            body: Some("aGVsbG8=".to_string()),
            body_base64: true,
            extra_info: None,
        };

        assert_eq!(
            response.body_bytes().expect("base64 body should decode"),
            Some(b"hello".to_vec())
        );
    }

    #[test]
    fn response_received_preserves_earlier_extra_info() {
        let mut packet = PendingPacket {
            tab_id: "tab-1".to_string(),
            matched_target: None,
            frame_id: Some("frame-1".to_string()),
            request: ListenerRequest {
                all_info: json!({}),
                url: "http://127.0.0.1/api/data".to_string(),
                method: "POST".to_string(),
                headers: HashMap::new(),
                post_data: None,
                extra_info: None,
            },
            response: None,
            resource_type: None,
            finished: false,
            response_body: None,
            response_body_base64: false,
            awaiting_response_extra_info: false,
        };
        let extra_info = ListenerResponseExtraInfo {
            all_info: json!({}),
            headers: HashMap::from([("X-OpenPage-Response".to_string(), "enabled".to_string())]),
            status_code: 200,
            headers_text: None,
            blocked_cookies: Vec::new(),
            exempted_cookies: Vec::new(),
        };
        apply_response_extra_info(&mut packet, extra_info.clone());

        let mut response = ListenerResponse {
            all_info: json!({}),
            url: packet.request.url.clone(),
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::from([("Content-Type".to_string(), "application/json".to_string())]),
            mime_type: "application/json".to_string(),
            body: None,
            body_base64: false,
            extra_info: None,
        };

        let has_extra_info = preserve_existing_response_extra_info(&packet, &mut response);
        assert!(has_extra_info);
        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("X-OpenPage-Response"),
            Some(&"enabled".to_string())
        );
        assert_eq!(
            response
                .extra_info
                .as_ref()
                .and_then(|info| info.headers.get("X-OpenPage-Response")),
            Some(&"enabled".to_string())
        );
    }

    #[test]
    fn scoped_listener_only_matches_same_frame() {
        assert!(super::scope_matches_frame(None, Some("frame-1")));
        assert!(super::scope_matches_frame(Some("frame-1"), Some("frame-1")));
        assert!(!super::scope_matches_frame(
            Some("frame-1"),
            Some("frame-2")
        ));
        assert!(!super::scope_matches_frame(Some("frame-1"), None));
    }

    #[test]
    fn request_extra_info_from_cdp_captures_associated_cookies() {
        let event: EventRequestWillBeSentExtraInfo = serde_json::from_value(json!({
            "requestId": "request-1",
            "associatedCookies": [
                {
                    "cookie": {
                        "name": "sid",
                        "value": "1",
                        "domain": "127.0.0.1",
                        "path": "/",
                        "expires": -1.0,
                        "size": 4,
                        "httpOnly": true,
                        "secure": false,
                        "session": true,
                        "priority": "Medium",
                        "sourceScheme": "NonSecure",
                        "sourcePort": 80
                    },
                    "blockedReasons": []
                },
                {
                    "cookie": {
                        "name": "blocked",
                        "value": "0",
                        "domain": "127.0.0.1",
                        "path": "/",
                        "expires": -1.0,
                        "size": 8,
                        "httpOnly": false,
                        "secure": false,
                        "session": true,
                        "priority": "Medium",
                        "sourceScheme": "NonSecure",
                        "sourcePort": 80
                    },
                    "blockedReasons": ["DomainMismatch"]
                }
            ],
            "headers": {
                "Cookie": "sid=1"
            },
            "connectTiming": {
                "requestTime": 1.0
            }
        }))
        .expect("request extra info event should deserialize");

        let extra_info = listener_request_extra_info_from_cdp(&event);
        assert_eq!(
            extra_info.all_info["connectTiming"]["requestTime"],
            json!(1.0)
        );
        assert_eq!(extra_info.associated_cookies.len(), 2);
        assert_eq!(
            extra_info.cookies(),
            vec![json!({
                "name": "sid",
                "value": "1",
                "domain": "127.0.0.1",
                "path": "/",
                "expires": -1.0,
                "size": 4,
                "httpOnly": true,
                "secure": false,
                "session": true,
                "priority": "Medium",
                "sourceScheme": "NonSecure",
                "sourcePort": 80
            })]
        );
        assert_eq!(
            extra_info.associated_cookies[1].blocked_reasons,
            vec!["DomainMismatch".to_string()]
        );
    }

    #[test]
    fn request_and_response_keep_raw_cdp_payloads() {
        let request_event: EventRequestWillBeSent = serde_json::from_value(json!({
            "requestId": "request-1",
            "loaderId": "loader-1",
            "documentURL": "http://127.0.0.1/api/data",
            "request": {
                "url": "http://127.0.0.1/api/data?foo=1",
                "method": "POST",
                "headers": {
                    "Content-Type": "application/json"
                },
                "initialPriority": "High",
                "referrerPolicy": "strict-origin-when-cross-origin"
            },
            "timestamp": 1.0,
            "wallTime": 1.0,
            "initiator": {
                "type": "script"
            },
            "redirectHasExtraInfo": false,
            "type": "XHR",
            "frameId": "frame-1",
            "hasUserGesture": false
        }))
        .expect("request event should deserialize");
        let packet =
            PendingPacket::from_request(&request_event, Some("/api/data".to_string()), "tab-1");
        assert_eq!(packet.request.all_info["initialPriority"], json!("High"));
        assert_eq!(
            packet.request.all_info["url"],
            json!("http://127.0.0.1/api/data?foo=1")
        );

        let response: Response = serde_json::from_value(json!({
            "url": "http://127.0.0.1/api/data?foo=1",
            "status": 200,
            "statusText": "OK",
            "headers": {
                "Content-Type": "application/json"
            },
            "mimeType": "application/json",
            "charset": "utf-8",
            "connectionReused": false,
            "connectionId": 1.0,
            "encodedDataLength": 12.0,
            "securityState": "secure",
            "remoteIPAddress": "127.0.0.1",
            "remotePort": 443,
            "fromDiskCache": false,
            "fromServiceWorker": false,
            "fromPrefetchCache": false,
            "timing": {
                "requestTime": 1.0,
                "proxyStart": -1.0,
                "proxyEnd": -1.0,
                "dnsStart": -1.0,
                "dnsEnd": -1.0,
                "connectStart": -1.0,
                "connectEnd": -1.0,
                "sslStart": -1.0,
                "sslEnd": -1.0,
                "workerStart": -1.0,
                "workerReady": -1.0,
                "workerFetchStart": -1.0,
                "workerRespondWithSettled": -1.0,
                "sendStart": 0.0,
                "sendEnd": 0.0,
                "pushStart": 0.0,
                "pushEnd": 0.0,
                "receiveHeadersStart": 1.0,
                "receiveHeadersEnd": 2.0
            }
        }))
        .expect("response should deserialize");
        let response = listener_response_from_cdp(&response);
        assert_eq!(response.all_info["remoteIPAddress"], json!("127.0.0.1"));
        assert_eq!(response.all_info["status"], json!(200));
    }

    #[test]
    fn response_extra_info_from_cdp_captures_cookie_details() {
        let event: EventResponseReceivedExtraInfo = serde_json::from_value(json!({
            "requestId": "request-1",
            "blockedCookies": [
                {
                    "blockedReasons": ["SecureOnly"],
                    "cookieLine": "sid=1; Secure",
                    "cookie": {
                        "name": "sid",
                        "value": "1",
                        "domain": "127.0.0.1",
                        "path": "/",
                        "expires": -1.0,
                        "size": 4,
                        "httpOnly": true,
                        "secure": true,
                        "session": true,
                        "priority": "Medium",
                        "sourceScheme": "Secure",
                        "sourcePort": 443
                    }
                }
            ],
            "headers": {
                "Set-Cookie": "sid=1"
            },
            "resourceIPAddressSpace": "Loopback",
            "statusCode": 200,
            "headersText": "HTTP/1.1 200 OK",
            "exemptedCookies": [
                {
                    "exemptionReason": "UserSetting",
                    "cookieLine": "pref=1; SameSite=None",
                    "cookie": {
                        "name": "pref",
                        "value": "1",
                        "domain": "127.0.0.1",
                        "path": "/",
                        "expires": -1.0,
                        "size": 5,
                        "httpOnly": false,
                        "secure": false,
                        "session": true,
                        "priority": "Medium",
                        "sourceScheme": "NonSecure",
                        "sourcePort": 80
                    }
                }
            ]
        }))
        .expect("response extra info event should deserialize");

        let extra_info = listener_response_extra_info_from_cdp(&event);
        assert_eq!(
            extra_info.all_info["resourceIPAddressSpace"],
            json!("Loopback")
        );
        assert_eq!(extra_info.blocked_cookies.len(), 1);
        assert_eq!(
            extra_info.blocked_cookies[0].blocked_reasons,
            vec!["SecureOnly".to_string()]
        );
        assert_eq!(
            extra_info.blocked_cookies[0].cookie_line,
            "sid=1; Secure".to_string()
        );
        assert_eq!(extra_info.exempted_cookies.len(), 1);
        assert_eq!(
            extra_info.exempted_cookies[0].exemption_reason,
            "UserSetting".to_string()
        );
    }

    #[test]
    fn fail_info_keeps_raw_loading_failed_payload() {
        let event: EventLoadingFailed = serde_json::from_value(json!({
            "requestId": "request-1",
            "timestamp": 1.0,
            "type": "Fetch",
            "errorText": "net::ERR_FAILED",
            "canceled": true,
            "blockedReason": "Inspector"
        }))
        .expect("loading failed event should deserialize");

        let fail_info = super::listener_fail_info_from_cdp(&event);
        assert_eq!(fail_info.error_text, "net::ERR_FAILED".to_string());
        assert_eq!(fail_info.blocked_reason, Some("inspector".to_string()));
        assert_eq!(fail_info.all_info["requestId"], json!("request-1"));
        assert_eq!(fail_info.all_info["type"], json!("Fetch"));
    }

    #[test]
    fn failed_packets_can_wait_for_late_extra_info() {
        let shared = Arc::new(ListenerShared::new(None, "tab-1".to_string()));
        {
            let mut state = shared.state.lock().expect("listener state should lock");
            state.listening = true;
        }

        let request_event: EventRequestWillBeSent = serde_json::from_value(json!({
            "requestId": "request-1",
            "loaderId": "loader-1",
            "documentURL": "http://127.0.0.1/api/data",
            "request": {
                "url": "http://127.0.0.1/api/data",
                "method": "GET",
                "headers": {
                    "Accept": "application/json"
                },
                "initialPriority": "High",
                "referrerPolicy": "strict-origin-when-cross-origin"
            },
            "timestamp": 1.0,
            "wallTime": 1.0,
            "initiator": {
                "type": "script"
            },
            "redirectHasExtraInfo": false,
            "type": "Fetch",
            "frameId": "frame-1",
            "hasUserGesture": false
        }))
        .expect("request event should deserialize");
        on_request_will_be_sent(&shared, &request_event).expect("request should be accepted");

        let fail_event: EventLoadingFailed = serde_json::from_value(json!({
            "requestId": "request-1",
            "timestamp": 1.0,
            "type": "Fetch",
            "errorText": "net::ERR_FAILED",
            "canceled": false
        }))
        .expect("failed event should deserialize");
        on_loading_failed(&shared, &fail_event).expect("failed event should queue packet");

        let mut packet = {
            let mut state = shared.state.lock().expect("listener state should lock");
            state
                .queue
                .pop_front()
                .expect("failed packet should be queued")
        };
        assert!(packet.request.extra_info.is_none());
        assert!(packet.response.is_none());

        let request_extra_event: EventRequestWillBeSentExtraInfo = serde_json::from_value(json!({
            "requestId": "request-1",
            "associatedCookies": [],
            "headers": {
                "Cookie": "sid=1"
            },
            "connectTiming": {
                "requestTime": 1.0
            }
        }))
        .expect("request extra event should deserialize");
        on_request_will_be_sent_extra_info(&shared, &request_extra_event)
            .expect("late request extra info should be stored");

        let response_extra_event: EventResponseReceivedExtraInfo = serde_json::from_value(json!({
            "requestId": "request-1",
            "blockedCookies": [],
            "headers": {
                "Content-Type": "application/json"
            },
            "resourceIPAddressSpace": "Loopback",
            "statusCode": 502
        }))
        .expect("response extra event should deserialize");
        on_response_received_extra_info(&shared, &response_extra_event)
            .expect("late response extra info should be stored");

        assert!(
            packet
                .wait_extra_info(Some(10))
                .expect("waiting for late extra info should succeed")
        );
        assert_eq!(
            packet
                .request
                .extra_info
                .as_ref()
                .and_then(|info| info.headers.get("Cookie")),
            Some(&"sid=1".to_string())
        );
        assert_eq!(
            packet
                .response
                .as_ref()
                .and_then(|response| response.extra_info.as_ref())
                .map(|info| info.status_code),
            Some(502)
        );
        assert_eq!(
            packet.response.as_ref().map(|response| response.status),
            Some(502)
        );
    }
}
