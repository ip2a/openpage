use std::sync::{Arc, Mutex};

use chromiumoxide::cdp::browser_protocol::page::EventFrameNavigated;
use chromiumoxide::cdp::js_protocol::runtime::{AddBindingParams, EventBindingCalled};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use serde::{Deserialize, Serialize};

use crate::browser::Browser;
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::execute_page_command_async;

pub const RECORDED_FLOW_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordedFlow {
    pub version: u32,
    pub steps: Vec<RecordedStep>,
}

impl Default for RecordedFlow {
    fn default() -> Self {
        Self {
            version: RECORDED_FLOW_VERSION,
            steps: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordedStep {
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub action: RecordedAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_after: Option<RecordedWait>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RecordedAction {
    Goto {
        url: String,
    },
    Click {
        target: RecordedTarget,
    },
    Fill {
        target: RecordedTarget,
        value: RecordedValue,
    },
    Select {
        target: RecordedTarget,
        values: Vec<String>,
    },
    Check {
        target: RecordedTarget,
        checked: bool,
    },
    Press {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<RecordedTarget>,
        key: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordedTarget {
    pub locator: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<String>,
}

impl RecordedTarget {
    pub fn new(locator: impl Into<String>) -> Self {
        Self {
            locator: locator.into(),
            fallbacks: Vec::new(),
            frames: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RecordedValue {
    Text(String),
    Secret { secret: String },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordedWait {
    Navigation,
    NewTab,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecorderStatus {
    pub recording: bool,
    pub step_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
}

#[derive(Debug, Default)]
struct RecorderState {
    recording: bool,
    started_at_ms: Option<u64>,
    flow: RecordedFlow,
    task: Option<JoinHandle<()>>,
    stop: Option<oneshot::Sender<()>>,
}

#[derive(Clone, Debug, Default)]
pub struct Recorder {
    runtime: Option<Arc<Runtime>>,
    page: Option<OxPage>,
    browser: Option<Browser>,
    state: Arc<Mutex<RecorderState>>,
}

impl Recorder {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        Self {
            runtime: Some(runtime),
            page: Some(page),
            browser: None,
            state: Arc::new(Mutex::new(RecorderState::default())),
        }
    }

    pub(crate) fn with_browser(mut self, browser: Browser) -> Self {
        self.browser = Some(browser);
        self
    }

    pub fn start(&self) -> OpenPageResult<()> {
        let mut state = self.lock()?;
        if state.recording {
            return Ok(());
        }
        state.recording = true;
        state.started_at_ms = Some(now_ms());
        state.flow = RecordedFlow::default();
        drop(state);
        if let Err(error) = self.start_page_listener() {
            let mut state = self.lock()?;
            state.recording = false;
            state.started_at_ms = None;
            state.flow = RecordedFlow::default();
            return Err(error);
        }
        Ok(())
    }

    pub fn stop(&self) -> OpenPageResult<RecordedFlow> {
        let (stop, task) = {
            let mut state = self.lock()?;
            (state.stop.take(), state.task.take())
        };
        if let Some(stop) = stop {
            let _ = stop.send(());
        }
        if let (Some(runtime), Some(mut task)) = (&self.runtime, task) {
            runtime.block_on(async {
                if tokio::time::timeout(std::time::Duration::from_millis(250), &mut task)
                    .await
                    .is_err()
                {
                    task.abort();
                }
            });
        }
        let mut state = self.lock()?;
        state.recording = false;
        Ok(state.flow.clone())
    }

    pub fn flow(&self) -> OpenPageResult<RecordedFlow> {
        Ok(self.lock()?.flow.clone())
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.lock()?.flow.steps.clear();
        Ok(())
    }

    pub fn status(&self) -> OpenPageResult<RecorderStatus> {
        let state = self.lock()?;
        Ok(RecorderStatus {
            recording: state.recording,
            step_count: state.flow.steps.len(),
            started_at_ms: state.started_at_ms,
        })
    }

    pub(crate) fn record(&self, action: RecordedAction) -> OpenPageResult<bool> {
        let mut state = self.lock()?;
        if !state.recording {
            return Ok(false);
        }

        let step = RecordedStep {
            timestamp_ms: now_ms(),
            action,
            wait_after: None,
        };

        if let Some(previous) = state.flow.steps.last_mut()
            && merge_step(previous, &step)
        {
            return Ok(true);
        }

        state.flow.steps.push(step);
        Ok(true)
    }

    fn record_new_tab(&self) -> OpenPageResult<()> {
        let mut state = self.lock()?;
        if !state.recording {
            return Ok(());
        }
        if let Some(step) = state.flow.steps.last_mut() {
            step.wait_after = Some(RecordedWait::NewTab);
        }
        Ok(())
    }

    fn start_page_listener(&self) -> OpenPageResult<()> {
        let (runtime, page) = match (&self.runtime, &self.page) {
            (Some(runtime), Some(page)) => (Arc::clone(runtime), page.clone()),
            _ => return Ok(()),
        };
        let browser = self.browser.clone();
        let initial_tabs = browser.as_ref().and_then(|browser| browser.tab_ids().ok());
        let (mut events, mut navigations) = runtime.block_on(async {
            let events = page.event_listener::<EventBindingCalled>().await
                .map_err(|err| OpenPageError::PageOperation(format!("register recorder listener: {err}")))?;
            execute_page_command_async(&page, AddBindingParams::new(RECORDER_BINDING), "add recorder binding").await?;
            execute_page_command_async(
                &page,
                chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(RECORDER_SCRIPT),
                "install recorder script",
            ).await?;
            page.evaluate(RECORDER_SCRIPT).await
                .map_err(|err| OpenPageError::PageOperation(format!("install recorder script: {err}")))?;
            let navigations = page.event_listener::<EventFrameNavigated>().await
                .map_err(|err| OpenPageError::PageOperation(format!("register navigation listener: {err}")))?;
            Ok::<_, OpenPageError>((events, navigations))
        })?;
        let recorder = self.clone();
        let (stop, mut stopped) = oneshot::channel();
        let task = runtime.spawn(async move {
            let mut tab_poll = tokio::time::interval(std::time::Duration::from_millis(100));
            let mut known_tabs = initial_tabs.unwrap_or_default();
            loop {
                tokio::select! {
                    _ = &mut stopped => {
                        loop {
                            let pending = tokio::time::timeout(
                                std::time::Duration::from_millis(10),
                                async {
                                    tokio::select! {
                                        event = events.next() => Some((event, None)),
                                        navigation = navigations.next() => Some((None, navigation)),
                                    }
                                },
                            ).await;
                            let Ok(Some((event, navigation))) = pending else { break };
                            if event.is_none() && navigation.is_none() { break; }
                            if let Some(event) = event
                                && event.name == RECORDER_BINDING
                                && let Ok(action) = serde_json::from_str::<RecordedAction>(&event.payload)
                            {
                                let _ = recorder.record(action);
                            }
                            if let Some(navigation) = navigation
                                && navigation.frame.parent_id.is_none()
                                && !navigation.frame.url.is_empty()
                            {
                                let _ = recorder.record(RecordedAction::Goto {
                                    url: navigation.frame.url.clone(),
                                });
                            }
                        }
                        break;
                    }
                    _ = tab_poll.tick(), if browser.is_some() => {
                        if let Some(browser) = browser.as_ref()
                            && let Ok(tabs) = browser.tab_ids()
                            && tabs.iter().any(|tab| !known_tabs.contains(tab))
                        {
                            let _ = recorder.record_new_tab();
                            known_tabs = tabs;
                        }
                    }
                    event = events.next() => {
                        let Some(event) = event else { break };
                        if event.name == RECORDER_BINDING {
                            if let Ok(action) = serde_json::from_str::<RecordedAction>(&event.payload) {
                                let _ = recorder.record(action);
                            }
                        }
                    }
                    navigation = navigations.next() => {
                        let Some(navigation) = navigation else { break };
                        if navigation.frame.parent_id.is_none() && !navigation.frame.url.is_empty() {
                            let _ = recorder.record(RecordedAction::Goto { url: navigation.frame.url.clone() });
                        }
                    }
                }
            }
        });
        let mut state = self.lock()?;
        state.task = Some(task);
        state.stop = Some(stop);
        Ok(())
    }

    fn lock(&self) -> OpenPageResult<std::sync::MutexGuard<'_, RecorderState>> {
        self.state.lock().map_err(|_| {
            OpenPageError::PageOperation("recorder state lock is poisoned".to_string())
        })
    }
}

fn merge_step(previous: &mut RecordedStep, next: &RecordedStep) -> bool {
    match (&mut previous.action, &next.action) {
        (
            RecordedAction::Fill {
                target: previous_target,
                value: previous_value,
            },
            RecordedAction::Fill {
                target: next_target,
                value: next_value,
            },
        ) if previous_target == next_target => {
            *previous_value = next_value.clone();
            previous.timestamp_ms = next.timestamp_ms;
            true
        }
        (
            RecordedAction::Click {
                target: previous_target,
            },
            RecordedAction::Check {
                target: next_target,
                ..
            },
        ) if previous_target == next_target => {
            previous.action = next.action.clone();
            true
        }
        (RecordedAction::Click { .. }, RecordedAction::Goto { .. }) => {
            previous.wait_after = Some(RecordedWait::Navigation);
            true
        }
        _ => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

const RECORDER_BINDING: &str = "__openpage_record";

const RECORDER_SCRIPT: &str = r#"(() => {
  if (window.__openpageRecorderInstalled) return;
  window.__openpageRecorderInstalled = true;
  const send = (value) => window.__openpage_record(JSON.stringify(value));
  const escape = (value) => String(value).replaceAll("\\", "\\\\").replaceAll('"', '\\"');
  const locator = (element) => {
    if (element.id) return `css:#${CSS.escape(element.id)}`;
    for (const name of ["data-testid", "name", "aria-label", "placeholder"]) {
      if (element.getAttribute(name)) return `css:[${name}="${escape(element.getAttribute(name))}"]`;
    }
    let node = element;
    const parts = [];
    while (node && node.nodeType === 1 && node !== document.body && parts.length < 6) {
      let part = node.tagName.toLowerCase();
      if (node.parentElement) {
        const same = [...node.parentElement.children].filter((item) => item.tagName === node.tagName);
        if (same.length > 1) part += `:nth-of-type(${same.indexOf(node) + 1})`;
      }
      parts.unshift(part);
      node = node.parentElement;
    }
    return `css:${parts.join(" > ")}`;
  };
  const frameContext = () => {
    const frames = [];
    let current = window;
    while (current !== current.parent) {
      try {
        const frame = current.frameElement;
        if (!frame) break;
        frames.unshift(locator(frame));
        current = current.parent;
      } catch (_) {
        break;
      }
    }
    return frames;
  };
  const target = (element) => ({ locator: locator(element), frames: frameContext() });
  const sensitiveKey = (element) => {
    const metadata = [element.type, element.autocomplete, element.name, element.id,
      element.getAttribute("aria-label"), element.getAttribute("placeholder")]
      .filter(Boolean).join(" ").toLowerCase();
    if (element.type === "password" || /pass(word)?/.test(metadata)) return "PASSWORD";
    if (/otp|one[-_ ]?time|verification/.test(metadata)) return "OTP";
    if (/token|secret|api[-_ ]?key/.test(metadata)) return "TOKEN";
    if (/credit|card[-_ ]?number/.test(metadata)) return "CARD_NUMBER";
    return null;
  };
  document.addEventListener("click", (event) => {
    const element = event.target instanceof Element ? event.target : null;
    if (element) send({ action: "click", target: target(element) });
  }, true);
  document.addEventListener("input", (event) => {
    const element = event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement || event.target instanceof HTMLSelectElement ? event.target : null;
    if (!element) return;
    if (element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")) return;
    if (element instanceof HTMLSelectElement) return;
    const secret = element instanceof HTMLInputElement ? sensitiveKey(element) : null;
    const value = secret ? { secret } : String(element.value);
    send({ action: "fill", target: target(element), value });
  }, true);
  document.addEventListener("change", (event) => {
    const element = event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement ? event.target : null;
    if (!element) return;
    if (element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")) send({ action: "check", target: target(element), checked: element.checked });
    if (element instanceof HTMLSelectElement) send({ action: "select", target: target(element), values: [...element.selectedOptions].map((option) => option.value) });
  }, true);
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Tab" && event.key !== "Enter" && event.key !== "Escape") return;
    const element = event.target instanceof Element ? event.target : null;
    send({ action: "press", target: element ? target(element) : null, key: event.key });
  }, true);
})();"#;

#[cfg(test)]
mod tests {
    use super::{
        RECORDED_FLOW_VERSION, RecordedAction, RecordedTarget, RecordedValue, RecordedWait,
        Recorder,
    };

    #[test]
    fn recorder_merges_consecutive_fill_steps_for_same_target() {
        let recorder = Recorder::default();
        let target = RecordedTarget::new("css:input[name=email]");

        recorder.start().unwrap();
        recorder
            .record(RecordedAction::Fill {
                target: target.clone(),
                value: RecordedValue::Text("a".to_string()),
            })
            .unwrap();
        recorder
            .record(RecordedAction::Fill {
                target,
                value: RecordedValue::Text("alice@example.com".to_string()),
            })
            .unwrap();

        let flow = recorder.stop().unwrap();
        assert_eq!(flow.version, RECORDED_FLOW_VERSION);
        assert_eq!(flow.steps.len(), 1);
        assert!(matches!(
            &flow.steps[0].action,
            RecordedAction::Fill {
                value: RecordedValue::Text(value),
                ..
            } if value == "alice@example.com"
        ));
    }

    #[test]
    fn recorder_merges_checkbox_click_into_check() {
        let recorder = Recorder::default();
        let target = RecordedTarget::new("css:#ok");

        recorder.start().unwrap();
        recorder
            .record(RecordedAction::Click {
                target: target.clone(),
            })
            .unwrap();
        recorder
            .record(RecordedAction::Check {
                target,
                checked: true,
            })
            .unwrap();

        let flow = recorder.stop().unwrap();
        assert_eq!(flow.steps.len(), 1);
        assert!(matches!(
            flow.steps[0].action,
            RecordedAction::Check { checked: true, .. }
        ));
    }

    #[test]
    fn recorder_marks_navigation_after_click_without_adding_goto() {
        let recorder = Recorder::default();
        let target = RecordedTarget::new("css:#login");

        recorder.start().unwrap();
        recorder.record(RecordedAction::Click { target }).unwrap();
        recorder
            .record(RecordedAction::Goto {
                url: "https://example.com/login".to_string(),
            })
            .unwrap();

        let flow = recorder.stop().unwrap();
        assert_eq!(flow.steps.len(), 1);
        assert_eq!(flow.steps[0].wait_after, Some(RecordedWait::Navigation));
        assert!(matches!(flow.steps[0].action, RecordedAction::Click { .. }));
    }

    #[test]
    fn recorder_ignores_events_until_started_and_never_serializes_secret_values_as_text() {
        let recorder = Recorder::default();
        let target = RecordedTarget::new("css:input[type=password]");
        assert!(
            !recorder
                .record(RecordedAction::Click {
                    target: target.clone()
                })
                .unwrap()
        );

        recorder.start().unwrap();
        recorder
            .record(RecordedAction::Fill {
                target,
                value: RecordedValue::Secret {
                    secret: "PASSWORD".to_string(),
                },
            })
            .unwrap();

        let json = serde_json::to_value(recorder.stop().unwrap()).unwrap();
        assert_eq!(json["steps"][0]["value"]["secret"], "PASSWORD");
        assert!(json.to_string().find("hunter2").is_none());
    }
}
