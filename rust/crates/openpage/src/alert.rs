use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::page::{
    EventJavascriptDialogClosed, EventJavascriptDialogOpening, HandleJavaScriptDialogParams,
};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::execute_page_command_async;
use crate::settings::{
    alert_operation_failed_message, component_state_lock_poisoned_message,
    default_auto_handle_alert,
};

fn alert_operation_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::PageOperation(alert_operation_failed_message(operation, &err.to_string()))
}

#[derive(Clone, Debug)]
struct PendingAlertAction {
    accept: bool,
    prompt_text: Option<String>,
}

#[derive(Debug, Default)]
struct AlertState {
    has_alert: bool,
    message: Option<String>,
    auto_action: Option<PendingAlertAction>,
    pending_next: Option<PendingAlertAction>,
    last_error: Option<String>,
    opening_task: Option<JoinHandle<()>>,
    closed_task: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct AlertShared {
    state: StdMutex<AlertState>,
    condvar: Condvar,
}

impl AlertShared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(AlertState::default()),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AlertTracker {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<AlertShared>,
}

impl AlertTracker {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        let shared = Arc::new(AlertShared::new());
        if let Some(accept) = default_auto_handle_alert()
            && let Ok(mut state) = shared.state.lock()
        {
            state.auto_action = Some(PendingAlertAction {
                accept,
                prompt_text: None,
            });
        }
        let tracker = Self {
            runtime: Arc::clone(&runtime),
            page: page.clone(),
            shared: Arc::clone(&shared),
        };

        let opening_shared = Arc::clone(&shared);
        let opening_page = page.clone();
        let opening_task = runtime.spawn(async move {
            let mut events = match opening_page
                .event_listener::<EventJavascriptDialogOpening>()
                .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_last_error(&opening_shared, err.to_string());
                    return;
                }
            };

            while let Some(event) = events.next().await {
                let pending = {
                    let mut state = match opening_shared.state.lock() {
                        Ok(state) => state,
                        Err(_) => return,
                    };
                    state.has_alert = true;
                    state.message = Some(event.message.clone());
                    state.last_error = None;
                    let pending = state
                        .auto_action
                        .clone()
                        .or_else(|| state.pending_next.take());
                    opening_shared.condvar.notify_all();
                    pending
                };

                if let Some(action) = pending {
                    if let Err(err) =
                        handle_dialog(&opening_page, action.accept, action.prompt_text.as_deref())
                            .await
                    {
                        set_last_error(&opening_shared, err.to_string());
                    }
                }
            }
        });

        let closed_shared = Arc::clone(&shared);
        let closed_page = page.clone();
        let closed_task = runtime.spawn(async move {
            let mut events = match closed_page
                .event_listener::<EventJavascriptDialogClosed>()
                .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_last_error(&closed_shared, err.to_string());
                    return;
                }
            };

            while events.next().await.is_some() {
                if let Ok(mut state) = closed_shared.state.lock() {
                    state.has_alert = false;
                    state.message = None;
                    closed_shared.condvar.notify_all();
                } else {
                    return;
                }
            }
        });

        if let Ok(mut state) = tracker.shared.state.lock() {
            state.opening_task = Some(opening_task);
            state.closed_task = Some(closed_task);
        }

        tracker
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.shared
            .state
            .lock()
            .map(|state| state.has_alert)
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "alert state",
                    "弹窗状态",
                ))
            })
    }

    pub fn alert_text(&self) -> OpenPageResult<Option<String>> {
        self.shared
            .state
            .lock()
            .map(|state| state.message.clone())
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "alert state",
                    "弹窗状态",
                ))
            })
    }

    pub fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "alert state",
                "弹窗状态",
            ))
        })?;

        loop {
            if let Some(error) = &state.last_error {
                return Err(OpenPageError::PageOperation(error.clone()));
            }
            if state.has_alert {
                let message = state.message.clone().unwrap_or_default();
                drop(state);
                self.runtime.block_on(async {
                    handle_dialog(&self.page, accept, prompt_text)
                        .await
                        .map_err(|err| alert_operation_error("handle dialog", err))
                })?;
                return Ok(Some(message));
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let wait_for = deadline.saturating_duration_since(now);
            let result = self
                .shared
                .condvar
                .wait_timeout(state, wait_for)
                .map_err(|_| {
                    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                        "alert state",
                        "弹窗状态",
                    ))
                })?;
            state = result.0;
            if result.1.timed_out() {
                return Ok(None);
            }
        }
    }

    pub fn set_next_alert_action(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "alert state",
                "弹窗状态",
            ))
        })?;
        state.pending_next = Some(PendingAlertAction {
            accept,
            prompt_text: prompt_text.map(str::to_string),
        });
        Ok(())
    }

    pub fn set_auto_alert_action(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "alert state",
                "弹窗状态",
            ))
        })?;
        state.auto_action = accept.map(|accept| PendingAlertAction {
            accept,
            prompt_text: prompt_text.map(str::to_string),
        });
        Ok(())
    }

    pub fn wait_for_alert_closed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "alert state",
                "弹窗状态",
            ))
        })?;
        let mut seen_open = state.has_alert;

        loop {
            if let Some(error) = &state.last_error {
                return Err(OpenPageError::PageOperation(error.clone()));
            }
            if !seen_open && state.has_alert {
                seen_open = true;
            }
            if seen_open && !state.has_alert {
                return Ok(true);
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            let wait_for = deadline.saturating_duration_since(now);
            let result = self
                .shared
                .condvar
                .wait_timeout(state, wait_for)
                .map_err(|_| {
                    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                        "alert state",
                        "弹窗状态",
                    ))
                })?;
            state = result.0;
            if result.1.timed_out() {
                return Ok(seen_open && !state.has_alert);
            }
        }
    }
}

async fn handle_dialog(
    page: &OxPage,
    accept: bool,
    prompt_text: Option<&str>,
) -> Result<(), String> {
    let mut params = HandleJavaScriptDialogParams::new(accept);
    if let Some(prompt_text) = prompt_text {
        params.prompt_text = Some(prompt_text.to_string());
    }
    execute_page_command_async(page, params, "AlertTracker::handle_dialog()")
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn set_last_error(shared: &AlertShared, error: String) {
    if let Ok(mut state) = shared.state.lock() {
        state.last_error = Some(error);
        shared.condvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::alert_operation_error;
    use crate::Settings;
    use crate::settings::scoped_test_settings;

    #[test]
    fn alert_operation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english = alert_operation_error("handle dialog", "boom").to_string();
        assert_eq!(
            english,
            "page operation failed: alert operation handle dialog failed: boom"
        );

        Settings::set_language("cn");

        let chinese = alert_operation_error("handle dialog", "boom").to_string();
        assert_eq!(chinese, "页面操作失败: 弹窗操作 handle dialog 失败: boom");
    }
}
