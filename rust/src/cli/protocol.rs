use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

use crate::error::OpenPageError;

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

pub fn openpage_error_kind(error: &OpenPageError) -> &'static str {
    match error {
        OpenPageError::BrowserLaunch(_) => "browser_launch",
        OpenPageError::BrowserOperation(_) => "browser_operation",
        OpenPageError::PageOperation(_) => "page_operation",
        OpenPageError::ElementNotFound(_) => "element_not_found",
        OpenPageError::UnsupportedLocator(_) => "unsupported_locator",
        OpenPageError::UnsupportedOperation(_) => "unsupported_operation",
        OpenPageError::JavaScript(_) => "javascript",
        OpenPageError::Http(_) => "http",
        OpenPageError::Io(_) => "io",
        OpenPageError::Timeout(_) => "timeout",
        OpenPageError::Serialization(_) => "serialization",
    }
}

pub fn simple_openpage_error(error: &OpenPageError) -> Value {
    simple_error(openpage_error_kind(error), error.to_string())
}

pub fn response_openpage_error(id: Option<Value>, error: &OpenPageError) -> Response {
    Response::error(id, openpage_error_kind(error), error.to_string())
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
        let mut buf = [0u8; 16];
        getrandom::getrandom(&mut buf).expect("failed to generate output boundary nonce");
        buf.iter().map(|byte| format!("{byte:02x}")).collect()
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

fn wrap_content(key: &str, content: &str, origin: Option<&str>) -> String {
    let nonce = boundary_nonce();
    let origin_suffix = origin
        .filter(|value| !value.is_empty())
        .map(|value| format!(" origin={value}"))
        .unwrap_or_default();
    format!(
        "--- OPENPAGE_PAGE_CONTENT nonce={nonce} key={key}{origin_suffix} ---\n{content}\n--- END_OPENPAGE_PAGE_CONTENT nonce={nonce} ---"
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
    let origin = result_obj
        .get("origin")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

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
            next = wrap_content(key, &next, origin.as_deref());
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
                "origin": origin,
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

    #[test]
    fn boundary_nonce_is_process_stable_hex() {
        let nonce = boundary_nonce();
        assert_eq!(nonce, boundary_nonce());
        assert_eq!(nonce.len(), 32);
        assert!(nonce.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn format_output_json_includes_origin_in_boundary_metadata() {
        unsafe {
            std::env::set_var("OPENPAGE_CONTENT_BOUNDARIES", "1");
        }
        let formatted = format_output_json(&serde_json::json!({
            "ok": true,
            "result": {
                "origin": "https://example.com/path",
                "text": "hello"
            }
        }))
        .expect("format output");
        unsafe {
            std::env::remove_var("OPENPAGE_CONTENT_BOUNDARIES");
        }

        let parsed: Value = serde_json::from_str(&formatted).expect("parse formatted json");
        assert_eq!(
            parsed["result"]["_boundary"]["origin"],
            "https://example.com/path"
        );
        let wrapped_text = parsed["result"]["text"]
            .as_str()
            .expect("wrapped text should be a string");
        assert!(wrapped_text.contains("origin=https://example.com/path"));
    }

    #[test]
    fn format_output_json_omits_empty_origin_from_boundary_metadata() {
        unsafe {
            std::env::set_var("OPENPAGE_CONTENT_BOUNDARIES", "1");
        }
        let formatted = format_output_json(&serde_json::json!({
            "ok": true,
            "result": {
                "origin": "",
                "text": "hello"
            }
        }))
        .expect("format output");
        unsafe {
            std::env::remove_var("OPENPAGE_CONTENT_BOUNDARIES");
        }

        let parsed: Value = serde_json::from_str(&formatted).expect("parse formatted json");
        assert!(parsed["result"]["_boundary"]["origin"].is_null());
        let wrapped_text = parsed["result"]["text"]
            .as_str()
            .expect("wrapped text should be a string");
        assert!(!wrapped_text.contains("origin="));
    }

    #[test]
    fn openpage_error_kind_maps_variants_to_stable_strings() {
        let cases = vec![
            (
                OpenPageError::BrowserLaunch("x".to_string()),
                "browser_launch",
            ),
            (
                OpenPageError::BrowserOperation("x".to_string()),
                "browser_operation",
            ),
            (
                OpenPageError::PageOperation("x".to_string()),
                "page_operation",
            ),
            (
                OpenPageError::ElementNotFound("x".to_string()),
                "element_not_found",
            ),
            (
                OpenPageError::UnsupportedLocator("x".to_string()),
                "unsupported_locator",
            ),
            (
                OpenPageError::UnsupportedOperation("x".to_string()),
                "unsupported_operation",
            ),
            (OpenPageError::JavaScript("x".to_string()), "javascript"),
            (OpenPageError::Http("x".to_string()), "http"),
            (OpenPageError::Io("x".to_string()), "io"),
            (OpenPageError::Timeout("x".to_string()), "timeout"),
            (
                OpenPageError::Serialization("x".to_string()),
                "serialization",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(openpage_error_kind(&error), expected);
        }
    }

    #[test]
    fn simple_openpage_error_uses_stable_kind_and_message() {
        let error = OpenPageError::UnsupportedOperation("batch cannot execute serve".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert_eq!(
            payload["error"]["message"],
            "unsupported operation: batch cannot execute serve"
        );
    }
}
