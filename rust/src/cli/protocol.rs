use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<Value>,
    pub op: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseError {
    pub kind: String,
    pub message: String,
}

impl Response {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(ResponseError {
                kind: kind.into(),
                message: message.into(),
            }),
        }
    }
}

pub fn simple_ok(result: Value) -> Value {
    serde_json::json!({
        "ok": true,
        "result": result,
    })
}

pub fn simple_error(kind: impl Into<String>, message: impl Into<String>) -> Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "kind": kind.into(),
            "message": message.into(),
        },
    })
}

#[derive(Debug, Clone, Copy)]
struct OutputOptions {
    content_boundaries: bool,
    max_output_chars: Option<usize>,
}

impl OutputOptions {
    fn from_env() -> Self {
        Self {
            content_boundaries: env_truthy("OPENPAGE_CONTENT_BOUNDARIES"),
            max_output_chars: std::env::var("OPENPAGE_MAX_OUTPUT_CHARS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0),
        }
    }
}

fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no"
        ),
        Err(_) => false,
    }
}

fn boundary_nonce() -> &'static str {
    static NONCE: OnceLock<String> = OnceLock::new();
    NONCE.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{:x}{:x}", std::process::id(), nanos)
    })
}

fn truncate_string(content: &str, limit: usize) -> String {
    if content.chars().count() <= limit {
        return content.to_string();
    }
    let byte_limit = content
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(content.len());
    let shown = content[..byte_limit].chars().count();
    let total = content.chars().count();
    format!(
        "{}\n[truncated: showing {} of {} chars. Set OPENPAGE_MAX_OUTPUT_CHARS to adjust]",
        &content[..byte_limit],
        shown,
        total
    )
}

fn wrap_content(key: &str, content: &str) -> String {
    let nonce = boundary_nonce();
    format!(
        "--- OPENPAGE_PAGE_CONTENT nonce={nonce} key={key} ---\n{content}\n--- END_OPENPAGE_PAGE_CONTENT nonce={nonce} ---"
    )
}

fn apply_output_filters(value: &mut Value) {
    let opts = OutputOptions::from_env();
    if !opts.content_boundaries && opts.max_output_chars.is_none() {
        return;
    }

    let Some(result) = value.get_mut("result") else {
        return;
    };
    let Some(result_obj) = result.as_object_mut() else {
        return;
    };

    let mut wrapped_keys = Vec::new();
    for key in ["html", "text", "value"] {
        let Some(current) = result_obj.get_mut(key) else {
            continue;
        };
        let Some(content) = current.as_str() else {
            continue;
        };

        let mut next = content.to_string();
        if let Some(limit) = opts.max_output_chars {
            next = truncate_string(&next, limit);
        }
        if opts.content_boundaries {
            next = wrap_content(key, &next);
            wrapped_keys.push(key.to_string());
        }
        *current = Value::String(next);
    }

    if opts.content_boundaries && !wrapped_keys.is_empty() {
        result_obj.insert(
            "_boundary".to_string(),
            serde_json::json!({
                "nonce": boundary_nonce(),
                "keys": wrapped_keys,
            }),
        );
    }
}

pub fn format_output_json(value: &Value) -> Result<String, serde_json::Error> {
    let mut filtered = value.clone();
    apply_output_filters(&mut filtered);
    serde_json::to_string(&filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_with_defaults() {
        let req: Request = serde_json::from_str(r#"{"op":"webpage.title"}"#).unwrap();
        assert_eq!(req.op, "webpage.title");
        assert!(req.id.is_none());
        assert!(req.target.is_none());
        assert!(req.params.is_null());
    }

    #[test]
    fn serializes_success_response() {
        let value = serde_json::to_value(Response::ok(
            Some(serde_json::json!("1")),
            serde_json::json!({"title":"Example"}),
        ))
        .unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["id"], "1");
        assert_eq!(value["result"]["title"], "Example");
    }
}
