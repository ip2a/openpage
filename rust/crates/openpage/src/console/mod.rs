use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

#[cfg(test)]
use chromiumoxide::cdp::browser_protocol::log::LogEntry;
use chromiumoxide::cdp::js_protocol::runtime::{
    EnableParams as RuntimeEnableParams, EventConsoleApiCalled, RemoteObject,
};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio::time::timeout as tokio_timeout;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::execute_page_command_blocking;
use crate::settings::{
    cdp_timeout_duration, component_not_active_start_message, component_not_running_message,
    component_not_running_with_error_message, component_state_lock_poisoned_message,
    component_stopped_while_waiting_message, console_setup_operation_failed_message,
    timeout_duration_millis, timeout_error,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsoleMessage {
    pub all_info: Value,
    pub source: String,
    pub level: String,
    pub text: String,
    pub url: Option<String>,
    pub line: Option<i64>,
    pub column: Option<i64>,
    pub args: Vec<Value>,
}

impl ConsoleMessage {
    pub fn body(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|_| Value::String(self.text.clone()))
    }
}

#[derive(Debug, Default)]
struct ConsoleState {
    queue: VecDeque<ConsoleMessage>,
    listening: bool,
    enabled: bool,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

#[derive(Debug)]
struct ConsoleShared {
    state: StdMutex<ConsoleState>,
    condvar: Condvar,
}

impl ConsoleShared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(ConsoleState::default()),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Console {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<ConsoleShared>,
}

#[derive(Clone, Debug)]
pub struct ConsoleSteps {
    console: Console,
    timeout_ms: Option<u64>,
    finished: bool,
}

impl Console {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        let shared = Arc::new(ConsoleShared::new());
        let console = Self {
            runtime: Arc::clone(&runtime),
            page: page.clone(),
            shared: Arc::clone(&shared),
        };

        let task_shared = Arc::clone(&shared);
        let handle = runtime.spawn(async move {
            if let Err(err) = run_console(page, Arc::clone(&task_shared)).await {
                let _ = set_console_stopped(&task_shared, Some(err.to_string()));
            } else {
                let _ = set_console_stopped(&task_shared, None);
            }
        });

        if let Ok(mut state) = console.shared.state.lock() {
            state.task = Some(handle);
        }

        console
    }

    pub fn start(&self) -> OpenPageResult<()> {
        let should_enable = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| console_state_lock_poisoned_error())?;
            state.queue.clear();
            state.last_error = None;
            if state.task.is_none() {
                return Err(console_not_running_error(&state));
            }
            state.listening = true;
            self.shared.condvar.notify_all();
            !state.enabled
        };

        if should_enable {
            if let Err(err) = execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.page,
                RuntimeEnableParams::default(),
                "Console::start()",
            ) {
                if let Ok(mut state) = self.shared.state.lock() {
                    state.listening = false;
                    state.last_error = Some(err.to_string());
                    self.shared.condvar.notify_all();
                }
                return Err(err);
            }

            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| console_state_lock_poisoned_error())?;
            state.enabled = true;
            state.last_error = None;
            self.shared.condvar.notify_all();
        }

        Ok(())
    }

    pub fn stop(&self) -> OpenPageResult<()> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| console_state_lock_poisoned_error())?;
        state.listening = false;
        state.last_error = None;
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| console_state_lock_poisoned_error())?;
        state.queue.clear();
        self.shared.condvar.notify_all();
        Ok(())
    }

    pub fn wait(&self, timeout_ms: Option<u64>) -> OpenPageResult<Option<ConsoleMessage>> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| console_state_lock_poisoned_error())?;

        if state.task.is_none() {
            return Err(console_not_running_error(&state));
        }
        if !state.listening {
            return Err(console_not_active_error());
        }
        drop(state);

        wait_for_console_message(&self.shared, timeout_ms)
    }

    pub fn messages(&self) -> OpenPageResult<Vec<ConsoleMessage>> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| console_state_lock_poisoned_error())?;
        Ok(drain_console_messages(&mut state.queue))
    }

    pub fn steps(&self, timeout_ms: Option<u64>) -> ConsoleSteps {
        ConsoleSteps {
            console: self.clone(),
            timeout_ms,
            finished: false,
        }
    }

    pub fn is_listening(&self) -> OpenPageResult<bool> {
        self.shared
            .state
            .lock()
            .map(|state| state.listening)
            .map_err(|_| console_state_lock_poisoned_error())
    }
}

impl Iterator for ConsoleSteps {
    type Item = ConsoleMessage;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.console.wait(self.timeout_ms) {
            Ok(Some(message)) => Some(message),
            Ok(None) | Err(_) => {
                self.finished = true;
                None
            }
        }
    }
}

async fn run_console(page: OxPage, shared: Arc<ConsoleShared>) -> OpenPageResult<()> {
    let mut events = register_console_listener_with_cdp_timeout(
        page.event_listener::<EventConsoleApiCalled>(),
        "register console api listener",
    )
    .await?;

    while let Some(event) = events.next().await {
        push_console_message(&shared, console_message_from_api_call(&event))?;
    }

    Ok(())
}

fn console_setup_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::PageOperation(console_setup_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

async fn register_console_listener_with_cdp_timeout<Fut, T, E>(
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
        .map_err(|err| console_setup_error(operation, err))
}

fn push_console_message(
    shared: &Arc<ConsoleShared>,
    message: ConsoleMessage,
) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| console_state_lock_poisoned_error())?;
    if state.listening {
        state.queue.push_back(message);
        shared.condvar.notify_all();
    }
    Ok(())
}

fn wait_for_console_message(
    shared: &Arc<ConsoleShared>,
    timeout_ms: Option<u64>,
) -> OpenPageResult<Option<ConsoleMessage>> {
    let deadline = timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
    let mut state = shared
        .state
        .lock()
        .map_err(|_| console_state_lock_poisoned_error())?;

    loop {
        if let Some(message) = state.queue.pop_front() {
            return Ok(Some(message));
        }

        if state.task.is_none() {
            return Err(console_not_running_error(&state));
        }
        if !state.listening {
            return Err(console_stopped_while_waiting_error());
        }

        match deadline {
            None => {
                state = shared
                    .condvar
                    .wait(state)
                    .map_err(|_| console_state_lock_poisoned_error())?;
            }
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return Ok(None);
                }
                let wait_for = deadline.saturating_duration_since(now);
                let result = shared
                    .condvar
                    .wait_timeout(state, wait_for)
                    .map_err(|_| console_state_lock_poisoned_error())?;
                state = result.0;
                if result.1.timed_out() {
                    return Ok(state.queue.pop_front());
                }
            }
        }
    }
}

fn drain_console_messages(queue: &mut VecDeque<ConsoleMessage>) -> Vec<ConsoleMessage> {
    queue.drain(..).collect()
}

fn console_message_from_api_call(event: &EventConsoleApiCalled) -> ConsoleMessage {
    let top_frame = event
        .stack_trace
        .as_ref()
        .and_then(|stack_trace| stack_trace.call_frames.first());

    ConsoleMessage {
        all_info: cdp_value(event),
        source: "console".to_string(),
        level: event.r#type.as_ref().to_string(),
        text: event
            .args
            .iter()
            .map(remote_object_text)
            .collect::<Vec<_>>()
            .join(" "),
        url: top_frame
            .map(|frame| frame.url.clone())
            .filter(|url| !url.is_empty()),
        line: top_frame.map(|frame| frame.line_number),
        column: top_frame.map(|frame| frame.column_number),
        args: event.args.iter().map(cdp_value).collect(),
    }
}

#[cfg(test)]
fn console_message_from_entry(entry: &LogEntry) -> ConsoleMessage {
    let top_frame = entry
        .stack_trace
        .as_ref()
        .and_then(|stack_trace| stack_trace.call_frames.first());
    let url = entry.url.clone().or_else(|| {
        top_frame
            .map(|frame| frame.url.clone())
            .filter(|url| !url.is_empty())
    });

    ConsoleMessage {
        all_info: cdp_value(entry),
        source: entry.source.as_ref().to_string(),
        level: entry.level.as_ref().to_string(),
        text: entry.text.clone(),
        url,
        line: entry
            .line_number
            .or_else(|| top_frame.map(|frame| frame.line_number)),
        column: top_frame.map(|frame| frame.column_number),
        args: entry
            .args
            .as_ref()
            .map(|args| args.iter().map(cdp_value).collect())
            .unwrap_or_default(),
    }
}

fn console_not_running_error(state: &ConsoleState) -> OpenPageError {
    OpenPageError::BrowserOperation(if let Some(error) = &state.last_error {
        component_not_running_with_error_message("console", "控制台", error)
    } else {
        component_not_running_message("console", "控制台")
    })
}

fn set_console_stopped(shared: &Arc<ConsoleShared>, error: Option<String>) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| console_state_lock_poisoned_error())?;
    state.listening = false;
    state.task = None;
    state.last_error = error;
    shared.condvar.notify_all();
    Ok(())
}

fn cdp_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn remote_object_text(value: &RemoteObject) -> String {
    if let Some(raw) = &value.value {
        return match raw {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
    }
    if let Some(raw) = &value.unserializable_value {
        return raw.as_ref().to_string();
    }
    if let Some(description) = &value.description {
        return description.clone();
    }
    value.r#type.as_ref().to_string()
}

fn console_state_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "console state",
        "控制台状态",
    ))
}

fn console_not_active_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_not_active_start_message("console", "控制台"))
}

fn console_stopped_while_waiting_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_stopped_while_waiting_message("console", "控制台"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use chromiumoxide::cdp::browser_protocol::log::{LogEntry, LogEntryLevel, LogEntrySource};
    use chromiumoxide::cdp::js_protocol::runtime::{
        CallFrame, RemoteObject, RemoteObjectType, StackTrace, Timestamp,
    };
    use serde_json::json;
    use tokio::runtime::Runtime;

    use crate::settings::{Settings, scoped_test_settings};

    use super::{
        ConsoleMessage, ConsoleShared, console_message_from_entry, console_not_running_error,
        console_setup_error, drain_console_messages, push_console_message,
        register_console_listener_with_cdp_timeout, wait_for_console_message,
    };

    fn sample_message(text: &str) -> ConsoleMessage {
        ConsoleMessage {
            all_info: json!({ "text": text }),
            source: "javascript".to_string(),
            level: "info".to_string(),
            text: text.to_string(),
            url: None,
            line: None,
            column: None,
            args: Vec::new(),
        }
    }

    #[test]
    fn console_message_keeps_raw_cdp_payloads() {
        let mut entry = LogEntry::new(
            LogEntrySource::Javascript,
            LogEntryLevel::Warning,
            r#"{"ok":true}"#,
            Timestamp::new(1.0),
        );
        entry.stack_trace = Some(StackTrace::new(vec![
            CallFrame::builder()
                .function_name("test")
                .script_id("1".to_string())
                .url("https://example.com/app.js")
                .line_number(12)
                .column_number(34)
                .build()
                .unwrap(),
        ]));
        entry.args = Some(vec![
            RemoteObject::builder()
                .r#type(RemoteObjectType::String)
                .value(json!("hello"))
                .build()
                .unwrap(),
        ]);

        let message = console_message_from_entry(&entry);

        assert_eq!(message.source, "javascript");
        assert_eq!(message.level, "warning");
        assert_eq!(message.url.as_deref(), Some("https://example.com/app.js"));
        assert_eq!(message.line, Some(12));
        assert_eq!(message.column, Some(34));
        assert_eq!(message.args[0]["value"], json!("hello"));
        assert_eq!(message.all_info["text"], json!(r#"{"ok":true}"#));
        assert_eq!(message.body(), json!({ "ok": true }));
    }

    #[test]
    fn console_messages_drain_queue() {
        let mut queue = std::collections::VecDeque::from(vec![
            sample_message("first"),
            sample_message("second"),
        ]);

        let messages = drain_console_messages(&mut queue);

        assert_eq!(messages.len(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn console_wait_receives_async_messages() {
        let runtime = Runtime::new().unwrap();
        let shared = Arc::new(ConsoleShared::new());
        {
            let mut state = shared.state.lock().unwrap();
            state.listening = true;
            state.task = Some(runtime.spawn(async {}));
        }

        let shared_for_thread = Arc::clone(&shared);
        let sender = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            push_console_message(&shared_for_thread, sample_message("later")).unwrap();
        });

        let message = wait_for_console_message(&shared, Some(100)).unwrap();
        sender.join().unwrap();

        assert_eq!(message.unwrap().text, "later");
    }

    #[test]
    fn console_wait_times_out_without_messages() {
        let runtime = Runtime::new().unwrap();
        let shared = Arc::new(ConsoleShared::new());
        {
            let mut state = shared.state.lock().unwrap();
            state.listening = true;
            state.task = Some(runtime.spawn(async {}));
        }

        assert!(
            wait_for_console_message(&shared, Some(10))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn console_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let runtime = Runtime::new().unwrap();
        let shared = Arc::new(ConsoleShared::new());
        {
            let mut state = shared.state.lock().unwrap();
            state.task = Some(runtime.spawn(async {}));
            state.listening = false;
            state.last_error = Some("boom".to_string());
        }

        let english_wait = wait_for_console_message(&shared, Some(10))
            .expect_err("console wait should fail when listener is stopped")
            .to_string();
        assert!(english_wait.contains("console stopped while waiting"));

        let english_not_running = {
            let mut state = shared.state.lock().unwrap();
            state.task = None;
            console_not_running_error(&state).to_string()
        };
        assert!(english_not_running.contains("console is not running: boom"));
        let english_setup = console_setup_error("unit test setup", "boom").to_string();
        assert!(english_setup.contains("console setup operation unit test setup failed: boom"));

        {
            let mut state = shared.state.lock().unwrap();
            state.task = Some(runtime.spawn(async {}));
        }

        Settings::set_language("cn");

        let chinese_wait = wait_for_console_message(&shared, Some(10))
            .expect_err("console wait should fail in Chinese when listener is stopped")
            .to_string();
        assert!(chinese_wait.contains("等待期间控制台已停止"));

        let chinese_not_running = {
            let state = shared.state.lock().unwrap();
            console_not_running_error(&state).to_string()
        };
        assert!(chinese_not_running.contains("控制台未运行: boom"));
        let chinese_setup = console_setup_error("unit test setup", "boom").to_string();
        assert!(chinese_setup.contains("控制台初始化操作 unit test setup 失败: boom"));
    }

    #[test]
    fn console_listener_registration_respects_cdp_timeout() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);
        let runtime = Runtime::new().expect("runtime");

        let error = runtime
            .block_on(register_console_listener_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok::<(), &str>(())
                },
                "register console api listener",
            ))
            .expect_err("console listener registration should time out")
            .to_string();
        assert!(error.contains("register console api listener"));
    }
}
