use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::fetch::{
    ContinueRequestParams, DisableParams, EnableParams, EventRequestPaused, FailRequestParams,
    FulfillRequestParams, HeaderEntry, RequestPattern,
};
use chromiumoxide::cdp::browser_protocol::network::{ErrorReason, Headers, ResourceType};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio::time::timeout as tokio_timeout;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::{execute_page_command_async, execute_page_command_blocking};
use crate::settings::{
    cdp_timeout_duration, component_not_active_start_message, component_not_running_message,
    component_not_running_with_error_message, component_state_lock_poisoned_message,
    component_stopped_while_waiting_message, intercepted_request_no_longer_pending_message,
    interceptor_setup_operation_failed_message, invalid_regex_message, timeout_duration_millis,
    timeout_error,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterceptedRequestInfo {
    pub request_id: String,
    pub frame_id: String,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub resource_type: String,
    pub has_post_data: bool,
    pub post_data_entries: usize,
}

#[derive(Clone, Debug)]
enum TargetMatcher {
    All,
    Substrings(Vec<String>),
    Regexes(Vec<Regex>),
}

#[derive(Clone, Debug)]
struct InterceptFilters {
    targets: TargetMatcher,
    methods: Option<HashSet<String>>,
    resource_types: Option<HashSet<String>>,
}

impl Default for InterceptFilters {
    fn default() -> Self {
        Self {
            targets: TargetMatcher::All,
            methods: None,
            resource_types: None,
        }
    }
}

impl InterceptFilters {
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
                        OpenPageError::BrowserOperation(invalid_regex_message(
                            "intercept",
                            "拦截规则",
                            &pattern,
                            &err.to_string(),
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

    fn matches(&self, url: &str, method: &str, resource_type: &str) -> bool {
        if self
            .methods
            .as_ref()
            .is_some_and(|methods| !methods.contains(&method.to_ascii_uppercase()))
        {
            return false;
        }
        if self.resource_types.as_ref().is_some_and(|resource_types| {
            !resource_types.contains(&resource_type.to_ascii_uppercase())
        }) {
            return false;
        }
        match &self.targets {
            TargetMatcher::All => true,
            TargetMatcher::Substrings(targets) => {
                targets.iter().any(|target| url.contains(target.as_str()))
            }
            TargetMatcher::Regexes(patterns) => {
                patterns.iter().any(|pattern| pattern.is_match(url))
            }
        }
    }
}

#[derive(Debug, Default)]
struct InterceptState {
    queue: VecDeque<InterceptedRequestInfo>,
    pending_request_ids: HashSet<String>,
    filters: InterceptFilters,
    listening: bool,
    paused: bool,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

#[derive(Debug)]
struct InterceptShared {
    state: StdMutex<InterceptState>,
    condvar: Condvar,
}

impl InterceptShared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(InterceptState::default()),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interceptor {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<InterceptShared>,
}

#[derive(Clone, Debug)]
pub struct InterceptedRequest {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<InterceptShared>,
    info: InterceptedRequestInfo,
}

impl Interceptor {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        Self {
            runtime,
            page,
            shared: Arc::new(InterceptShared::new()),
        }
    }

    pub fn start(
        &self,
        targets: Option<Vec<String>>,
        is_regex: bool,
        methods: Option<Vec<String>>,
        resource_types: Option<Vec<String>>,
    ) -> OpenPageResult<()> {
        let filters = InterceptFilters::new(targets, is_regex, methods, resource_types)?;
        {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| intercept_state_lock_poisoned_error())?;
            state.queue.clear();
            state.pending_request_ids.clear();
            state.last_error = None;
            state.filters = filters;
            if state.task.is_some() {
                state.listening = true;
                self.shared.condvar.notify_all();
                return Ok(());
            }
        }

        let mut paused_events =
            self.runtime
                .block_on(register_interceptor_listener_with_cdp_timeout(
                    self.page.event_listener::<EventRequestPaused>(),
                    "register request paused listener",
                ))?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            EnableParams::builder()
                .pattern(RequestPattern::builder().url_pattern("*").build())
                .build(),
            "Interceptor::start()",
        )?;

        {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| intercept_state_lock_poisoned_error())?;
            state.listening = true;
        }

        let page = self.page.clone();
        let shared = Arc::clone(&self.shared);
        let shared_for_task = Arc::clone(&shared);
        let handle = self.runtime.spawn(async move {
            let result = async {
                while let Some(event) = paused_events.next().await {
                    on_request_paused(&page, &shared_for_task, &event).await?;
                }
                Ok(())
            }
            .await;
            let error = result.err().map(|err: OpenPageError| err.to_string());
            let _ = set_interceptor_stopped(&shared_for_task, error);
        });
        self.shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?
            .task = Some(handle);
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn wait(&self, timeout_ms: Option<u64>) -> OpenPageResult<Option<InterceptedRequest>> {
        let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;

        if state.task.is_none() {
            return Err(interceptor_not_running_error(&state));
        }
        if !state.listening {
            return Err(interceptor_not_active_error());
        }

        loop {
            if let Some(info) = state.queue.pop_front() {
                return Ok(Some(InterceptedRequest {
                    runtime: Arc::clone(&self.runtime),
                    page: self.page.clone(),
                    shared: Arc::clone(&self.shared),
                    info,
                }));
            }

            if state.task.is_none() {
                return Err(interceptor_not_running_error(&state));
            }
            if !state.listening {
                return Err(interceptor_stopped_while_waiting_error());
            }

            match deadline {
                None => {
                    state = self
                        .shared
                        .condvar
                        .wait(state)
                        .map_err(|_| intercept_state_lock_poisoned_error())?;
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(None);
                    }
                    let wait_for = deadline.saturating_duration_since(now);
                    let result = self
                        .shared
                        .condvar
                        .wait_timeout(state, wait_for)
                        .map_err(|_| intercept_state_lock_poisoned_error())?;
                    state = result.0;
                    if result.1.timed_out() {
                        return Ok(None);
                    }
                }
            }
        }
    }

    pub fn stop(&self) -> OpenPageResult<()> {
        let pending_ids = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| intercept_state_lock_poisoned_error())?;
            let ids = state
                .pending_request_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            state.queue.clear();
            state.pending_request_ids.clear();
            state.last_error = None;
            state.listening = false;
            ids
        };

        for request_id in pending_ids {
            let _ = continue_request(
                &self.runtime,
                &self.page,
                &request_id,
                None,
                None,
                None,
                None,
            );
        }
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            DisableParams::default(),
            "Interceptor::stop()",
        )?;
        if let Some(task) = self
            .shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?
            .task
            .take()
        {
            task.abort();
        }

        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn is_listening(&self) -> OpenPageResult<bool> {
        self.shared
            .state
            .lock()
            .map(|state| state.listening)
            .map_err(|_| intercept_state_lock_poisoned_error())
    }

    pub fn pause(&self) -> OpenPageResult<()> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;
        state.paused = true;
        Ok(())
    }

    pub fn resume(&self) -> OpenPageResult<()> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;
        state.paused = false;
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn is_paused(&self) -> OpenPageResult<bool> {
        self.shared
            .state
            .lock()
            .map(|state| state.paused)
            .map_err(|_| intercept_state_lock_poisoned_error())
    }
}

impl InterceptedRequest {
    pub fn request_id(&self) -> String {
        self.info.request_id.clone()
    }

    pub fn frame_id(&self) -> String {
        self.info.frame_id.clone()
    }

    pub fn url(&self) -> String {
        self.info.url.clone()
    }

    pub fn method(&self) -> String {
        self.info.method.clone()
    }

    pub fn headers(&self) -> HashMap<String, String> {
        self.info.headers.clone()
    }

    pub fn resource_type(&self) -> String {
        self.info.resource_type.clone()
    }

    pub fn has_post_data(&self) -> bool {
        self.info.has_post_data
    }

    pub fn post_data_entries(&self) -> usize {
        self.info.post_data_entries
    }

    pub fn continue_request(
        &self,
        url: Option<&str>,
        method: Option<&str>,
        headers: Option<HashMap<String, String>>,
        post_data: Option<&str>,
    ) -> OpenPageResult<()> {
        with_pending_request(&self.shared, &self.info.request_id, || {
            continue_request(
                &self.runtime,
                &self.page,
                &self.info.request_id,
                url,
                method,
                headers,
                post_data,
            )
        })
    }

    pub fn abort(&self) -> OpenPageResult<()> {
        self.fail(ErrorReason::Aborted)
    }

    pub fn fail(&self, reason: ErrorReason) -> OpenPageResult<()> {
        with_pending_request(&self.shared, &self.info.request_id, || {
            let request_id = self.info.request_id.clone();
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.page,
                FailRequestParams::new(request_id, reason),
                "InterceptedRequest::fail()",
            )?;
            Ok(())
        })
    }

    pub fn fulfill(
        &self,
        response_code: i64,
        body: Option<&[u8]>,
        headers: Option<HashMap<String, String>>,
        response_phrase: Option<&str>,
    ) -> OpenPageResult<()> {
        with_pending_request(&self.shared, &self.info.request_id, || {
            let request_id = self.info.request_id.clone();
            let response_headers = headers.map(header_entries_from_map);
            let body = body.map(|value| BASE64_STANDARD.encode(value));
            let response_phrase = response_phrase.map(ToOwned::to_owned);
            let mut params = FulfillRequestParams::builder()
                .request_id(request_id)
                .response_code(response_code);
            if let Some(response_headers) = response_headers {
                params = params.response_headers(response_headers);
            }
            if let Some(body) = body {
                params = params.body(body);
            }
            if let Some(response_phrase) = response_phrase {
                params = params.response_phrase(response_phrase);
            }
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.page,
                params.build().map_err(OpenPageError::BrowserOperation)?,
                "InterceptedRequest::fulfill()",
            )?;
            Ok(())
        })
    }
}

async fn on_request_paused(
    page: &OxPage,
    shared: &Arc<InterceptShared>,
    event: &EventRequestPaused,
) -> OpenPageResult<()> {
    if event.response_status_code.is_some() {
        execute_page_command_async(
            page,
            ContinueRequestParams::new(event.request_id.clone()),
            "Interceptor::on_request_paused()",
        )
        .await?;
        return Ok(());
    }

    let info = {
        let state = shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;
        if !state.listening || state.paused {
            None
        } else {
            let resource_type = resource_type_to_string(&event.resource_type);
            let matches =
                state
                    .filters
                    .matches(&event.request.url, &event.request.method, &resource_type);
            if !matches {
                None
            } else {
                Some(InterceptedRequestInfo {
                    request_id: event.request_id.as_ref().to_string(),
                    frame_id: event.frame_id.as_ref().to_string(),
                    url: event.request.url.clone(),
                    method: event.request.method.clone(),
                    headers: headers_to_map(&event.request.headers),
                    resource_type,
                    has_post_data: event.request.has_post_data.unwrap_or(false),
                    post_data_entries: event
                        .request
                        .post_data_entries
                        .as_ref()
                        .map(|items| items.len())
                        .unwrap_or(0),
                })
            }
        }
    };

    let Some(info) = info else {
        execute_page_command_async(
            page,
            ContinueRequestParams::new(event.request_id.clone()),
            "Interceptor::on_request_paused()",
        )
        .await?;
        return Ok(());
    };

    let is_listening = {
        let state = shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;
        state.listening
    };
    if !is_listening {
        execute_page_command_async(
            page,
            ContinueRequestParams::new(event.request_id.clone()),
            "Interceptor::on_request_paused()",
        )
        .await?;
        return Ok(());
    }
    let mut state = shared
        .state
        .lock()
        .map_err(|_| intercept_state_lock_poisoned_error())?;
    state.pending_request_ids.insert(info.request_id.clone());
    state.queue.push_back(info);
    shared.condvar.notify_all();
    Ok(())
}

fn continue_request(
    runtime: &Arc<Runtime>,
    page: &OxPage,
    request_id: &str,
    url: Option<&str>,
    method: Option<&str>,
    headers: Option<HashMap<String, String>>,
    post_data: Option<&str>,
) -> OpenPageResult<()> {
    let request_id = request_id.to_string();
    let url = url.map(ToOwned::to_owned);
    let method = method.map(ToOwned::to_owned);
    let headers = headers.map(header_entries_from_map);
    let post_data = post_data.map(|value| BASE64_STANDARD.encode(value.as_bytes()));
    let mut params = ContinueRequestParams::builder().request_id(request_id);
    if let Some(url) = url {
        params = params.url(url);
    }
    if let Some(method) = method {
        params = params.method(method);
    }
    if let Some(headers) = headers {
        params = params.headers(headers);
    }
    if let Some(post_data) = post_data {
        params = params.post_data(post_data);
    }
    execute_page_command_blocking(
        runtime.as_ref(),
        page,
        params.build().map_err(OpenPageError::BrowserOperation)?,
        "Interceptor::continue_request()",
    )?;
    Ok(())
}

fn with_pending_request<F>(
    shared: &Arc<InterceptShared>,
    request_id: &str,
    action: F,
) -> OpenPageResult<()>
where
    F: FnOnce() -> OpenPageResult<()>,
{
    {
        let mut state = shared
            .state
            .lock()
            .map_err(|_| intercept_state_lock_poisoned_error())?;
        if !state.pending_request_ids.remove(request_id) {
            return Err(OpenPageError::BrowserOperation(
                intercepted_request_no_longer_pending_message(request_id),
            ));
        }
    }

    match action() {
        Ok(()) => {
            shared.condvar.notify_all();
            Ok(())
        }
        Err(err) => {
            let mut state = shared
                .state
                .lock()
                .map_err(|_| intercept_state_lock_poisoned_error())?;
            state.pending_request_ids.insert(request_id.to_string());
            shared.condvar.notify_all();
            Err(err)
        }
    }
}

fn set_interceptor_stopped(
    shared: &Arc<InterceptShared>,
    error: Option<String>,
) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| intercept_state_lock_poisoned_error())?;
    state.listening = false;
    state.pending_request_ids.clear();
    state.task = None;
    state.last_error = error;
    shared.condvar.notify_all();
    Ok(())
}

fn interceptor_not_running_error(state: &InterceptState) -> OpenPageError {
    if let Some(error) = &state.last_error {
        OpenPageError::BrowserOperation(component_not_running_with_error_message(
            "interceptor",
            "拦截器",
            error,
        ))
    } else {
        OpenPageError::BrowserOperation(component_not_running_message("interceptor", "拦截器"))
    }
}

fn intercept_state_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "intercept state",
        "拦截状态",
    ))
}

fn interceptor_setup_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::BrowserOperation(interceptor_setup_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

async fn register_interceptor_listener_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| interceptor_setup_error(operation, err))
}

fn interceptor_not_active_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_not_active_start_message("interceptor", "拦截器"))
}

fn interceptor_stopped_while_waiting_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_stopped_while_waiting_message(
        "interceptor",
        "拦截器",
    ))
}

fn normalize_set(values: Option<Vec<String>>) -> Option<HashSet<String>> {
    values.map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_uppercase())
            .collect()
    })
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

fn header_entries_from_map(headers: HashMap<String, String>) -> Vec<HeaderEntry> {
    headers
        .into_iter()
        .map(|(name, value)| HeaderEntry::new(name, value))
        .collect()
}

fn resource_type_to_string(value: &ResourceType) -> String {
    value.as_ref().to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::runtime::Runtime;

    use crate::error::OpenPageError;
    use crate::settings::{Settings, scoped_test_settings};

    use super::{
        InterceptFilters, InterceptShared, InterceptState, interceptor_not_running_error,
        interceptor_setup_error, register_interceptor_listener_with_cdp_timeout,
        with_pending_request,
    };

    #[test]
    fn intercept_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_regex = InterceptFilters::new(Some(vec!["(".to_string()]), true, None, None)
            .expect_err("invalid regex should fail")
            .to_string();
        assert!(english_regex.contains("invalid intercept regex `(`"));

        let mut state = InterceptState::default();
        state.last_error = Some("boom".to_string());
        let english_not_running = interceptor_not_running_error(&state).to_string();
        assert!(english_not_running.contains("interceptor is not running: boom"));

        let shared = Arc::new(InterceptShared::new());
        let english_pending = with_pending_request(&shared, "req-1", || Ok(()))
            .expect_err("missing pending request should fail")
            .to_string();
        assert!(english_pending.contains("intercepted request `req-1` is no longer pending"));
        let english_setup = interceptor_setup_error("unit test setup", "boom").to_string();
        assert!(english_setup.contains("interceptor setup operation unit test setup failed: boom"));

        Settings::set_language("cn");

        let chinese_regex = InterceptFilters::new(Some(vec!["(".to_string()]), true, None, None)
            .expect_err("invalid regex should fail in Chinese")
            .to_string();
        assert!(chinese_regex.contains("无效的拦截规则正则 `(`"));

        let chinese_not_running = interceptor_not_running_error(&state).to_string();
        assert!(chinese_not_running.contains("拦截器未运行: boom"));

        let chinese_pending = with_pending_request(&shared, "req-1", || Ok(()))
            .expect_err("missing pending request should fail in Chinese")
            .to_string();
        assert!(chinese_pending.contains("被拦截请求 `req-1` 已不再等待处理"));
        let chinese_setup = interceptor_setup_error("unit test setup", "boom").to_string();
        assert!(chinese_setup.contains("拦截器初始化操作 unit test setup 失败: boom"));
    }

    #[test]
    fn interceptor_listener_registration_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");
        let result = runtime.block_on(async {
            register_interceptor_listener_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "register request paused listener",
            )
            .await
        });

        Settings::reset();

        let error = result.expect_err("interceptor listener registration should time out");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("register request paused listener")),
            "unexpected interceptor registration timeout error: {error}"
        );
    }
}
