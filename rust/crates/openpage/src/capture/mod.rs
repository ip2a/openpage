//! Immutable captures exposed by long-lived protocol servers.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Snapshot,
    Screenshot,
    History,
    Console,
    Network,
    Dom,
}

impl CaptureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Screenshot => "screenshot",
            Self::History => "history",
            Self::Console => "console",
            Self::Network => "network",
            Self::Dom => "dom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: u64,
    pub kind: CaptureKind,
    pub session: String,
    pub target: Option<String>,
    pub revision: Option<String>,
    pub timestamp_ms: u128,
    pub mime_type: String,
    pub content: Option<Value>,
    pub file_path: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureCreate {
    pub kind: CaptureKind,
    pub target: Option<String>,
    pub revision: Option<String>,
    pub mime_type: String,
    pub content: Option<Value>,
    pub file_path: Option<String>,
    pub summary: String,
}

#[derive(Debug, Default)]
pub struct CaptureRegistry {
    inner: Mutex<BTreeMap<(String, u64), Capture>>,
    next_id: Mutex<BTreeMap<String, u64>>,
}

impl CaptureRegistry {
    pub fn record(&self, session: &str, capture: CaptureCreate) -> Capture {
        let id = {
            let mut next_ids = self.next_id.lock().expect("capture id lock poisoned");
            let next_id = next_ids.entry(session.to_string()).or_default();
            *next_id += 1;
            *next_id
        };
        let captured = Capture {
            id,
            kind: capture.kind,
            session: session.to_string(),
            target: capture.target,
            revision: capture.revision,
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            mime_type: capture.mime_type,
            content: capture.content,
            file_path: capture.file_path,
            summary: capture.summary,
        };
        self.inner
            .lock()
            .expect("capture registry lock poisoned")
            .insert((session.to_string(), id), captured.clone());
        captured
    }

    pub fn get(&self, session: &str, id: u64) -> Option<Capture> {
        self.inner
            .lock()
            .expect("capture registry lock poisoned")
            .get(&(session.to_string(), id))
            .cloned()
    }

    pub fn list(&self, session: &str, kind: Option<CaptureKind>) -> Vec<Capture> {
        self.inner
            .lock()
            .expect("capture registry lock poisoned")
            .range((session.to_string(), 0)..=(session.to_string(), u64::MAX))
            .filter(|(_, capture)| kind.is_none_or(|kind| capture.kind == kind))
            .map(|(_, capture)| capture.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(summary: &str) -> CaptureCreate {
        CaptureCreate {
            kind: CaptureKind::Snapshot,
            target: Some("target-1".to_string()),
            revision: Some("r_1".to_string()),
            mime_type: "application/json".to_string(),
            content: Some(json!({"summary": summary})),
            file_path: None,
            summary: summary.to_string(),
        }
    }

    #[test]
    fn records_gets_and_lists_captures_in_id_order() {
        let registry = CaptureRegistry::default();
        let first = registry.record("session-a", snapshot("first"));
        let second = registry.record("session-a", snapshot("second"));
        registry.record(
            "session-a",
            CaptureCreate {
                kind: CaptureKind::Screenshot,
                target: None,
                revision: Some("r_1".to_string()),
                mime_type: "image/png".to_string(),
                content: None,
                file_path: Some("/tmp/page.png".to_string()),
                summary: "image".to_string(),
            },
        );
        registry.record("session-b", snapshot("other session"));

        assert_eq!(first.id, 1);
        assert_eq!(second.id, 2);
        assert_eq!(registry.get("session-a", 2), Some(second));
        assert_eq!(
            registry
                .list("session-a", Some(CaptureKind::Snapshot))
                .into_iter()
                .map(|capture| capture.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            registry
                .list("session-a", None)
                .into_iter()
                .map(|capture| capture.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
}
