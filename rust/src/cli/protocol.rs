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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasons: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
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
        Self::error_with_fix(id, kind, message, None::<String>)
    }

    pub fn error_with_fix(
        id: Option<Value>,
        kind: impl Into<String>,
        message: impl Into<String>,
        fix: Option<String>,
    ) -> Self {
        Self::error_with_context(
            id,
            kind,
            message,
            fix,
            None::<String>,
            None::<String>,
            None,
            None,
            None::<String>,
        )
    }

    pub fn error_with_context(
        id: Option<Value>,
        kind: impl Into<String>,
        message: impl Into<String>,
        fix: Option<String>,
        session: Option<String>,
        state: Option<String>,
        reasons: Option<Vec<String>>,
        retryable: Option<bool>,
        suggested_action: Option<String>,
    ) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(ResponseError {
                kind: kind.into(),
                message: message.into(),
                fix,
                session,
                state,
                reasons,
                retryable,
                suggested_action,
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
    simple_error_with_fix(kind, message, None::<String>)
}

pub fn simple_error_with_fix(
    kind: impl Into<String>,
    message: impl Into<String>,
    fix: Option<String>,
) -> Value {
    simple_error_with_context(
        kind,
        message,
        fix,
        None::<String>,
        None::<String>,
        None,
        None,
        None::<String>,
    )
}

pub fn simple_error_with_context(
    kind: impl Into<String>,
    message: impl Into<String>,
    fix: Option<String>,
    session: Option<String>,
    state: Option<String>,
    reasons: Option<Vec<String>>,
    retryable: Option<bool>,
    suggested_action: Option<String>,
) -> Value {
    let mut error = serde_json::Map::new();
    error.insert("kind".to_string(), Value::String(kind.into()));
    error.insert("message".to_string(), Value::String(message.into()));
    if let Some(fix) = fix {
        error.insert("fix".to_string(), Value::String(fix));
    }
    if let Some(session) = session {
        error.insert("session".to_string(), Value::String(session));
    }
    if let Some(state) = state {
        error.insert("state".to_string(), Value::String(state));
    }
    if let Some(reasons) = reasons.filter(|value| !value.is_empty()) {
        error.insert("reasons".to_string(), serde_json::json!(reasons));
    }
    if let Some(retryable) = retryable {
        error.insert("retryable".to_string(), Value::Bool(retryable));
    }
    if let Some(suggested_action) = suggested_action {
        error.insert(
            "suggested_action".to_string(),
            Value::String(suggested_action),
        );
    }

    let mut payload = serde_json::Map::new();
    payload.insert("ok".to_string(), Value::Bool(false));
    payload.insert("error".to_string(), Value::Object(error));
    Value::Object(payload)
}

pub fn openpage_error_kind(error: &OpenPageError) -> &'static str {
    match error {
        OpenPageError::BrowserLaunch(_) => "browser_launch",
        OpenPageError::BrowserOperation(detail)
            if detail.starts_with("daemon transient for session `") =>
        {
            "daemon_transient"
        }
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
    let context = openpage_error_context(error);
    simple_error_with_context(
        openpage_error_kind(error),
        error.to_string(),
        context.fix.map(ToString::to_string),
        context.session.map(ToString::to_string),
        context.state.map(ToString::to_string),
        if context.reasons.is_empty() {
            None
        } else {
            Some(
                context
                    .reasons
                    .iter()
                    .map(|reason| reason.to_string())
                    .collect::<Vec<_>>(),
            )
        },
        context.retryable,
        context.suggested_action.map(ToString::to_string),
    )
}

pub fn response_openpage_error(id: Option<Value>, error: &OpenPageError) -> Response {
    let context = openpage_error_context(error);
    Response::error_with_context(
        id,
        openpage_error_kind(error),
        openpage_error_detail(error),
        context.fix.map(ToString::to_string),
        context.session.map(ToString::to_string),
        context.state.map(ToString::to_string),
        if context.reasons.is_empty() {
            None
        } else {
            Some(
                context
                    .reasons
                    .iter()
                    .map(|reason| reason.to_string())
                    .collect::<Vec<_>>(),
            )
        },
        context.retryable,
        context.suggested_action.map(ToString::to_string),
    )
}

pub fn openpage_error_from_kind(kind: &str, message: impl Into<String>) -> OpenPageError {
    let message = message.into();
    match kind {
        "browser_launch" => OpenPageError::BrowserLaunch(message),
        "browser_operation" | "daemon_transient" => OpenPageError::BrowserOperation(message),
        "page_operation" => OpenPageError::PageOperation(message),
        "element_not_found" => OpenPageError::ElementNotFound(message),
        "unsupported_locator" => OpenPageError::UnsupportedLocator(message),
        "unsupported_operation" => OpenPageError::UnsupportedOperation(message),
        "javascript" => OpenPageError::JavaScript(message),
        "http" => OpenPageError::Http(message),
        "io" | "tcp_error" => OpenPageError::Io(message),
        "timeout" => OpenPageError::Timeout(message),
        "serialization" | "invalid_json" => OpenPageError::Serialization(message),
        _ => OpenPageError::BrowserOperation(format!("{kind}: {message}")),
    }
}

pub fn openpage_error_from_structured(
    kind: &str,
    message: impl Into<String>,
    fix: Option<&str>,
) -> OpenPageError {
    let mut message = message.into();
    if let Some(fix) = fix.filter(|value| !value.is_empty()) {
        if !message.contains(fix) {
            if message.ends_with('.') {
                message.push(' ');
                message.push_str(fix);
            } else {
                message.push_str(". ");
                message.push_str(fix);
            }
        }
    }
    openpage_error_from_kind(kind, message)
}

fn openpage_error_detail(error: &OpenPageError) -> &str {
    match error {
        OpenPageError::BrowserLaunch(detail)
        | OpenPageError::BrowserOperation(detail)
        | OpenPageError::PageOperation(detail)
        | OpenPageError::ElementNotFound(detail)
        | OpenPageError::UnsupportedLocator(detail)
        | OpenPageError::UnsupportedOperation(detail)
        | OpenPageError::JavaScript(detail)
        | OpenPageError::Http(detail)
        | OpenPageError::Io(detail)
        | OpenPageError::Timeout(detail)
        | OpenPageError::Serialization(detail) => detail,
    }
}

fn openpage_error_fix(error: &OpenPageError) -> Option<&str> {
    let detail = match error {
        OpenPageError::BrowserOperation(detail) => detail,
        _ => return None,
    };

    if !detail.starts_with("session `") {
        return None;
    }

    let (_, fix) = detail.split_once(". ")?;
    if !fix.contains("`openpage ") {
        return None;
    }
    Some(fix)
}

#[derive(Default)]
struct ErrorContext<'a> {
    fix: Option<&'a str>,
    session: Option<&'a str>,
    state: Option<&'static str>,
    reasons: Vec<&'static str>,
    retryable: Option<bool>,
    suggested_action: Option<&'static str>,
}

fn openpage_error_context(error: &OpenPageError) -> ErrorContext<'_> {
    let detail = match error {
        OpenPageError::BrowserOperation(detail) => detail,
        _ => return ErrorContext::default(),
    };

    let mut context = ErrorContext {
        fix: openpage_error_fix(error),
        session: session_name_from_detail(detail),
        ..ErrorContext::default()
    };

    if !detail.starts_with("session `") {
        if detail.starts_with("daemon transient for session `") {
            context.session = detail
                .strip_prefix("daemon transient for session `")
                .and_then(|rest| rest.split_once('`').map(|(session, _)| session))
                .filter(|value| !value.is_empty());
            context.fix = Some("Retry the same command.");
            context.retryable = Some(true);
            context.suggested_action = Some("retry_same_command");
        }
        return context;
    }

    if detail.contains(" is backed by daemon version ") {
        context.state = Some("incompatible");
        context.reasons.push("version_mismatch");
    } else if detail.contains(" exists but its daemon is not ready") {
        context.state = Some("incomplete");
        context.reasons.push("daemon_not_ready");
    } else if detail.contains(" is not active") {
        context.state = Some("inactive");
    }

    context
}

fn session_name_from_detail(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix("session `")?;
    let (session, _) = rest.split_once('`')?;
    if session.is_empty() {
        None
    } else {
        Some(session)
    }
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
    for key in ["html", "text", "value", "content"] {
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

pub fn serialize_output_json(value: &Value) -> String {
    format_output_json(value).unwrap_or_else(|_| {
        r#"{"ok":false,"error":{"kind":"serialization","message":"serialization error: failed to serialize CLI JSON output"}}"#
            .to_string()
    })
}

pub fn print_output_json(value: &Value) {
    println!("{}", serialize_output_json(value));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }
    use serde_json::json;

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
        let _guard = env_lock();
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
        let _guard = env_lock();
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
    fn format_output_json_wraps_content_field_with_boundaries() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("OPENPAGE_CONTENT_BOUNDARIES", "1");
        }
        let formatted = format_output_json(&serde_json::json!({
            "ok": true,
            "result": {
                "content": "daemon log line"
            }
        }))
        .expect("format output");
        unsafe {
            std::env::remove_var("OPENPAGE_CONTENT_BOUNDARIES");
        }

        let parsed: Value = serde_json::from_str(&formatted).expect("parse formatted json");
        assert_eq!(parsed["result"]["_boundary"]["keys"], json!(["content"]));
        let wrapped = parsed["result"]["content"]
            .as_str()
            .expect("wrapped content should be a string");
        assert!(wrapped.contains("key=content"));
        assert!(wrapped.contains("daemon log line"));
    }

    #[test]
    fn format_output_json_truncates_content_field() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("OPENPAGE_MAX_OUTPUT_CHARS", "5");
        }
        let formatted = format_output_json(&serde_json::json!({
            "ok": true,
            "result": {
                "content": "abcdefghij"
            }
        }))
        .expect("format output");
        unsafe {
            std::env::remove_var("OPENPAGE_MAX_OUTPUT_CHARS");
        }

        let parsed: Value = serde_json::from_str(&formatted).expect("parse formatted json");
        let truncated = parsed["result"]["content"]
            .as_str()
            .expect("truncated content should be a string");
        assert!(truncated.starts_with("abcde"));
        assert!(truncated.contains("[truncated: showing 5 of 10 chars."));
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
        assert!(payload["error"].get("fix").is_none());
    }

    #[test]
    fn simple_error_omits_fix_when_absent() {
        let payload = simple_error("invalid_input", "unexpected argument");

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(payload["error"]["message"], "unexpected argument");
        assert!(payload["error"].get("fix").is_none());
        assert!(payload["error"].get("session").is_none());
        assert!(payload["error"].get("state").is_none());
        assert!(payload["error"].get("reasons").is_none());
    }

    #[test]
    fn simple_openpage_error_exposes_structured_fix_for_session_guidance() {
        let error = OpenPageError::BrowserOperation(
            "session `review` is not active. Start it with `openpage browser start --session review` before retrying.".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["state"], "inactive");
        assert!(payload["error"].get("reasons").is_none());
        assert_eq!(
            payload["error"]["fix"],
            "Start it with `openpage browser start --session review` before retrying."
        );
        assert_eq!(
            payload["error"]["message"],
            "browser operation failed: session `review` is not active. Start it with `openpage browser start --session review` before retrying."
        );
    }

    #[test]
    fn simple_openpage_error_exposes_retryable_daemon_transient_fields() {
        let error = OpenPageError::BrowserOperation(
            "daemon transient for session `review`: io error: connection reset by peer. Retry the same command.".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "daemon_transient");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["retryable"], true);
        assert_eq!(payload["error"]["suggested_action"], "retry_same_command");
        assert_eq!(payload["error"]["fix"], "Retry the same command.");
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_kind() {
        let error = openpage_error_from_kind("element_not_found", "missing #submit");
        match error {
            OpenPageError::ElementNotFound(message) => {
                assert_eq!(message, "missing #submit");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn reconstructs_unknown_kind_as_browser_operation() {
        let error = openpage_error_from_kind("daemon_state", "not ready");
        match error {
            OpenPageError::BrowserOperation(message) => {
                assert_eq!(message, "daemon_state: not ready");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_fix() {
        let error = openpage_error_from_structured(
            "browser_operation",
            "session `review` is not active",
            Some("Start it with `openpage browser start --session review` before retrying."),
        );
        match error {
            OpenPageError::BrowserOperation(message) => {
                assert_eq!(
                    message,
                    "session `review` is not active. Start it with `openpage browser start --session review` before retrying."
                );
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn response_openpage_error_uses_raw_detail_and_structured_fix() {
        let error = OpenPageError::BrowserOperation(
            "session `review` is not active. Start it with `openpage browser start --session review` before retrying.".to_string(),
        );
        let response = response_openpage_error(Some(serde_json::json!("cli")), &error);

        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|error| error.kind.as_str()),
            Some("browser_operation")
        );
        assert_eq!(
            response.error.as_ref().map(|error| error.message.as_str()),
            Some(
                "session `review` is not active. Start it with `openpage browser start --session review` before retrying."
            )
        );
        assert_eq!(
            response
                .error
                .as_ref()
                .and_then(|error| error.fix.as_deref()),
            Some("Start it with `openpage browser start --session review` before retrying.")
        );
        assert_eq!(
            response
                .error
                .as_ref()
                .and_then(|error| error.session.as_deref()),
            Some("review")
        );
        assert_eq!(
            response
                .error
                .as_ref()
                .and_then(|error| error.state.as_deref()),
            Some("inactive")
        );
        assert!(
            response
                .error
                .as_ref()
                .and_then(|error| error.reasons.as_ref())
                .is_none()
        );
    }

    #[test]
    fn simple_openpage_error_exposes_state_and_reasons_for_version_mismatch() {
        let error = OpenPageError::BrowserOperation(
            "session `review` is backed by daemon version 0.0.1 but the current CLI expects 0.1.0. Run `openpage browser stop --session review` and then restart that session with the current CLI so its daemon sidecars are recreated with version 0.1.0. Or run `openpage doctor --quick --fix` if you want the CLI to stop the stale daemon for you.".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["state"], "incompatible");
        assert_eq!(payload["error"]["reasons"], json!(["version_mismatch"]));
        assert!(
            payload["error"]["fix"]
                .as_str()
                .expect("fix should be present")
                .contains("browser stop --session review")
        );
    }

    #[test]
    fn serialize_output_json_matches_format_output_json_for_normal_payloads() {
        let _guard = env_lock();
        let value = serde_json::json!({
            "ok": true,
            "result": {
                "text": "hello"
            }
        });

        assert_eq!(
            serialize_output_json(&value),
            format_output_json(&value).expect("format output")
        );
    }
}
