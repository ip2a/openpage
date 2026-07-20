use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{OpenPageError, OpenPageResult};

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
}

#[derive(Clone, Debug, Default)]
pub struct Recorder {
    state: Arc<Mutex<RecorderState>>,
}

impl Recorder {
    pub fn start(&self) -> OpenPageResult<()> {
        let mut state = self.lock()?;
        state.recording = true;
        state.started_at_ms = Some(now_ms());
        state.flow = RecordedFlow::default();
        Ok(())
    }

    pub fn stop(&self) -> OpenPageResult<RecordedFlow> {
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

#[cfg(test)]
mod tests {
    use super::{RECORDED_FLOW_VERSION, RecordedAction, RecordedTarget, RecordedValue, Recorder};

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
