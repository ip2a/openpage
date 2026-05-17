use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::network::{
    EventLoadingFailed, EventLoadingFinished, EventRequestWillBeSent, EventResponseReceived,
    GetResponseBodyParams, Headers, ResourceType, Response,
};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::error::{OpenPageError, OpenPageResult};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub post_data: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerResponse {
    pub url: String,
    pub status: i64,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub mime_type: String,
    pub body: Option<String>,
    pub body_base64: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerFailInfo {
    pub error_text: String,
    pub canceled: Option<bool>,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListenerPacket {
    pub matched_target: Option<String>,
    pub url: String,
    pub method: String,
    pub resource_type: Option<String>,
    pub is_failed: bool,
    pub request: ListenerRequest,
    pub response: Option<ListenerResponse>,
    pub fail_info: Option<ListenerFailInfo>,
}

#[derive(Clone, Debug)]
enum TargetMatcher {
    All,
    Substrings(Vec<String>),
    Regexes(Vec<Regex>),
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
            methods: None,
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
    matched_target: Option<String>,
    request: ListenerRequest,
    response: Option<ListenerResponse>,
    resource_type: Option<String>,
    finished: bool,
    response_body: Option<String>,
    response_body_base64: bool,
}

impl PendingPacket {
    fn from_request(event: &EventRequestWillBeSent, matched_target: Option<String>) -> Self {
        Self {
            matched_target,
            request: ListenerRequest {
                url: event.request.url.clone(),
                method: event.request.method.clone(),
                headers: headers_to_map(&event.request.headers),
                post_data: None,
            },
            response: None,
            resource_type: event.r#type.as_ref().map(resource_type_to_string),
            finished: false,
            response_body: None,
            response_body_base64: false,
        }
    }

    fn into_packet(self, fail_info: Option<ListenerFailInfo>) -> ListenerPacket {
        ListenerPacket {
            matched_target: self.matched_target,
            url: self.request.url.clone(),
            method: self.request.method.clone(),
            resource_type: self.resource_type,
            is_failed: fail_info.is_some(),
            request: self.request,
            response: self.response,
            fail_info,
        }
    }
}

#[derive(Debug)]
struct ListenerState {
    queue: VecDeque<ListenerPacket>,
    inflight: HashMap<String, PendingPacket>,
    filters: ListenerFilters,
    listening: bool,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

impl Default for ListenerState {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            inflight: HashMap::new(),
            filters: ListenerFilters::default(),
            listening: false,
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
    fn new() -> Self {
        Self {
            state: StdMutex::new(ListenerState::default()),
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

impl Listener {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        Self {
            runtime,
            page,
            shared: Arc::new(ListenerShared::new()),
        }
    }

    pub fn start(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> OpenPageResult<()> {
        let filters = ListenerFilters::new(targets, is_regex, methods, resource_types)?;
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;

        state.queue.clear();
        state.inflight.clear();
        state.last_error = None;
        state.filters = filters;

        if state.listening {
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

        if !state.listening {
            return Err(listener_not_running_error(&state));
        }

        loop {
            if state.queue.len() >= needed {
                return Ok(pop_packets(&mut state.queue, needed));
            }

            if !state.listening {
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

    pub fn clear(&self) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        state.queue.clear();
        state.inflight.clear();
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn stop(&self) -> OpenPageResult<()> {
        let handle = {
            let mut state = self.shared.state.lock().map_err(|_| {
                OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
            })?;
            state.queue.clear();
            state.inflight.clear();
            state.last_error = None;
            state.listening = false;
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
            .map(|state| state.listening)
            .map_err(|_| {
                OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
            })
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
    let resource_type = event.r#type.as_ref().map(resource_type_to_string);
    let matched_target = {
        let state = shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("listener state lock poisoned".to_string())
        })?;
        if !state.listening {
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
    if !state.listening {
        return Ok(());
    }

    if let Some(redirect_response) = &event.redirect_response {
        if let Some(mut pending) = state.inflight.remove(&request_id) {
            pending.response = Some(listener_response_from_cdp(redirect_response));
            state.queue.push_back(pending.into_packet(None));
            shared.condvar.notify_all();
        }
    }

    if let Some(matched_target) = matched_target {
        state.inflight.insert(
            request_id,
            PendingPacket::from_request(event, matched_target),
        );
    } else {
        state.inflight.remove(&request_id);
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
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        let mut response = listener_response_from_cdp(&event.response);
        if let Some(body) = &packet.response_body {
            response.body = Some(body.clone());
            response.body_base64 = packet.response_body_base64;
        }
        packet.response = Some(response);
        packet.resource_type = Some(resource_type_to_string(&event.r#type));
    }
    let should_finalize = state
        .inflight
        .get(&request_id)
        .is_some_and(|packet| packet.finished && packet.response.is_some());
    if should_finalize {
        if let Some(packet) = state.inflight.remove(&request_id) {
            state.queue.push_back(packet.into_packet(None));
            shared.condvar.notify_all();
        }
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
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("listener state lock poisoned".to_string()))?;
    if let Some(packet) = state.inflight.get_mut(&request_id) {
        packet.finished = true;
        if let Some((body, body_base64)) = response_body {
            packet.response_body = Some(body.clone());
            packet.response_body_base64 = body_base64;
            if let Some(response) = packet.response.as_mut() {
                response.body = Some(body);
                response.body_base64 = body_base64;
            }
        }
    }
    let should_finalize = state
        .inflight
        .get(&request_id)
        .is_some_and(|packet| packet.response.is_some());
    if should_finalize {
        if let Some(packet) = state.inflight.remove(&request_id) {
            state.queue.push_back(packet.into_packet(None));
            shared.condvar.notify_all();
        }
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
    if let Some(mut packet) = state.inflight.remove(&request_id) {
        packet.resource_type = Some(resource_type_to_string(&event.r#type));
        state.queue.push_back(
            packet.into_packet(Some(ListenerFailInfo {
                error_text: event.error_text.clone(),
                canceled: event.canceled,
                blocked_reason: event
                    .blocked_reason
                    .as_ref()
                    .map(|reason| reason.as_ref().to_string()),
            })),
        );
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
    state.task = None;
    state.last_error = error;
    shared.condvar.notify_all();
    Ok(())
}

fn normalize_set(values: Option<Vec<String>>) -> Option<HashSet<String>> {
    values.map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_uppercase())
            .collect()
    })
}

fn listener_not_running_error(state: &ListenerState) -> OpenPageError {
    if let Some(error) = &state.last_error {
        OpenPageError::BrowserOperation(format!("listener is not running: {error}"))
    } else {
        OpenPageError::BrowserOperation("listener is not running".to_string())
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
    ListenerResponse {
        url: response.url.clone(),
        status: response.status,
        status_text: response.status_text.clone(),
        headers: headers_to_map(&response.headers),
        mime_type: response.mime_type.clone(),
        body: None,
        body_base64: false,
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

#[cfg(test)]
mod tests {
    use super::{ListenerFilters, headers_to_map};
    use chromiumoxide::cdp::browser_protocol::network::Headers;

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
}
