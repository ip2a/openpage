use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::OnceLock;

use crate::browser::OPENPAGE_BROWSER_PATH_ENV;
use crate::error::{ErrorDiagnostic, OpenPageError};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
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
                operation: None,
                locator: None,
                url: None,
                timeout: None,
                matched_count: None,
                element_state: None,
                failure_reason: None,
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

pub fn known_invalid_input_fix(detail: &str) -> Option<&'static str> {
    if detail.contains("unrecognized subcommand 'page'") {
        Some(
            "Use the active top-level TCP CLI instead: `openpage goto <url> --session <name>`, `openpage url --session <name>`, `openpage title --session <name>`, or `openpage html --session <name>`. The old `page ...` surface was removed.",
        )
    } else if detail.contains("unexpected argument '--stdio'") {
        Some(
            "Use `openpage serve --session <name>` for the TCP daemon workflow. The removed `serve --stdio` surface is intentionally rejected.",
        )
    } else if detail.starts_with("zoom step must ") {
        Some("Provide a positive finite `--step <number>` value before retrying.")
    } else if detail.starts_with("zoom factor must ") {
        Some("Provide a positive finite zoom factor before retrying.")
    } else if detail.starts_with("history index must be >= 1") {
        Some("Use a history index of 1 or greater before retrying.")
    } else if detail.starts_with("history index out of range:") {
        Some(
            "Run `openpage history list --session <name>` to inspect valid entries, then retry with an in-range history index.",
        )
    } else if detail.starts_with("find-in-page text must not be empty") {
        Some("Provide a non-empty search string before retrying `find-in-page`.")
    } else if detail.starts_with("drag-in requires --text or --files")
        || detail.starts_with("drag-in requires text or files")
    {
        Some(
            "Provide either `--text <content>` or one or more `--files <path>` values before retrying `drag-in`.",
        )
    } else if detail.starts_with("select requires one of:") {
        Some(
            "Provide exactly one selector family for `select`: `--text <value>`, `--value <value>`, or `--index <n>`.",
        )
    } else if detail.starts_with("unsupported snapshot mode:") {
        Some("Use one of the supported snapshot modes: `interactive`, `semantic`, or `all`.")
    } else if detail.starts_with("unsupported snapshot format:") {
        Some("Use one of the supported snapshot formats: `text` or `json`.")
    } else if detail.starts_with("missing target") {
        Some("Provide the required target identifier before retrying.")
    } else if detail.starts_with("missing string param:") {
        Some("Provide the required string param before retrying.")
    } else if detail.starts_with("missing number param:")
        || detail.starts_with("missing numeric param:")
    {
        Some("Provide the required numeric param before retrying.")
    } else if detail.starts_with("missing array param:") {
        Some("Provide the required array param before retrying.")
    } else if detail.starts_with("missing headers param:") {
        Some("Provide the required headers object before retrying.")
    } else if detail.starts_with("missing param:") {
        Some("Provide the required param before retrying.")
    } else if detail.contains(" must be a string or string array") {
        Some("Provide this field as a string or string array before retrying.")
    } else if detail.contains(" must be an integer or integer array") {
        Some("Provide this field as an integer or integer array before retrying.")
    } else if detail.contains(" must be an object") {
        Some("Provide this field as an object before retrying.")
    } else if detail.starts_with("array param must contain only strings:") {
        Some("Provide only string values in this array before retrying.")
    } else if detail.starts_with("array param must contain only integers:") {
        Some("Provide only integer values in this array before retrying.")
    } else if detail.starts_with("header values must be strings:") {
        Some("Provide only string header values before retrying.")
    } else {
        None
    }
}

fn known_unsupported_operation_fix(detail: &str) -> Option<&'static str> {
    match detail {
        "batch cannot execute `doctor`; run `openpage doctor` separately" => Some(
            "Run `openpage doctor [--quick] [--fix]` as a separate top-level command outside `batch`.",
        ),
        "batch cannot execute nested batch commands" => Some(
            "Flatten the command list into a single top-level `openpage batch ...` invocation instead of nesting `batch` inside `batch`.",
        ),
        "batch cannot execute `serve`; use top-level `serve` separately" => Some(
            "Run `openpage serve --session <name>` as a separate top-level command, then invoke follow-up commands outside `batch`.",
        ),
        "no recently closed tab recorded for this session" => Some(
            "Close a tab in this session with `openpage tab close ... --session <name>` before using `openpage tab reopen --session <name>`, or open a replacement tab directly with `openpage tab new <url> --session <name>`.",
        ),
        _ => None,
    }
}

fn known_browser_launch_fix(detail: &str) -> Option<Cow<'static, str>> {
    if detail.contains("No such file or directory") || detail.contains("os error 2") {
        Some(Cow::Owned(format!(
            "Run `openpage doctor --quick` to confirm browser-path resolution, then retry with `{OPENPAGE_BROWSER_PATH_ENV}=<absolute-browser-path> openpage browser start ...` or pass `--browser-path <absolute-browser-path>`."
        )))
    } else if detail.contains("before websocket URL could be resolved")
        || detail.contains("while resolving websocket URL from browser process")
    {
        Some(Cow::Owned(format!(
            "Verify that the configured executable is a real Chromium-based browser, then run `openpage doctor` for a live launch smoke test and retry with `{OPENPAGE_BROWSER_PATH_ENV}=<absolute-browser-path>` or `--browser-path <absolute-browser-path>`. If this session was partially created, rerun `openpage browser start --session <name> --replace ...` after fixing the browser path."
        )))
    } else {
        None
    }
}

fn known_browser_operation_fix(detail: &str) -> Option<Cow<'static, str>> {
    let target = detail.strip_prefix("unknown target: ")?;
    if target.is_empty() {
        return None;
    }

    Some(Cow::Owned(format!(
        "If `{0}` should be the session page, run `openpage browser start --session {0} --replace` or `openpage goto --session {0} <url>` to recreate it. If `{0}` is a tab or frame target, refresh the target list before retrying.",
        target
    )))
}

fn known_io_fix(detail: &str) -> Option<Cow<'static, str>> {
    if detail.contains("Not a directory") || detail.contains("os error 20") {
        Some(Cow::Borrowed(
            "Set OPENPAGE_HOME to a writable directory path, not a file, then retry.",
        ))
    } else if detail.contains("Permission denied") || detail.contains("os error 13") {
        Some(Cow::Borrowed(
            "Set OPENPAGE_HOME to a writable directory path and fix parent directory permissions before retrying.",
        ))
    } else {
        None
    }
}

pub fn openpage_error_kind(error: &OpenPageError) -> &'static str {
    match error.root() {
        OpenPageError::BrowserLaunch(_) => "browser_launch",
        OpenPageError::BrowserOperation(detail)
            if detail.starts_with("daemon transient for session `") =>
        {
            "daemon_transient"
        }
        OpenPageError::BrowserOperation(detail) if detail_maps_to_invalid_input(detail) => {
            "invalid_input"
        }
        OpenPageError::BrowserOperation(_) => "browser_operation",
        OpenPageError::PageOperation(_) => "page_operation",
        OpenPageError::ElementNotFound(_) => "element_not_found",
        OpenPageError::ElementDetached(_) => "element_detached",
        OpenPageError::ElementAmbiguous(_) => "element_ambiguous",
        OpenPageError::UnsupportedLocator(_) => "unsupported_locator",
        OpenPageError::UnsupportedOperation(detail) if detail_maps_to_invalid_input(detail) => {
            "invalid_input"
        }
        OpenPageError::UnsupportedOperation(_) => "unsupported_operation",
        OpenPageError::JavaScript(_) => "javascript",
        OpenPageError::Http(_) => "http",
        OpenPageError::Io(_) => "io",
        OpenPageError::Timeout(_) => "timeout",
        OpenPageError::Serialization(_) => "serialization",
        OpenPageError::Diagnosed { .. } => unreachable!("root error cannot be diagnosed"),
    }
}

pub fn simple_openpage_error(error: &OpenPageError) -> Value {
    let context = openpage_error_context(error);
    let mut payload = simple_error_with_context(
        openpage_error_kind(error),
        shell_error_message(error),
        context.fix.map(|value| value.into_owned()),
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
    );
    if let (Some(diagnostic), Some(error_payload)) = (
        error.diagnostic(),
        payload.get_mut("error").and_then(Value::as_object_mut),
    ) {
        insert_error_diagnostic(error_payload, diagnostic);
    }
    payload
}

pub fn response_openpage_error(id: Option<Value>, error: &OpenPageError) -> Response {
    let context = openpage_error_context(error);
    let mut response = Response::error_with_context(
        id,
        openpage_error_kind(error),
        openpage_error_detail(error),
        context.fix.map(|value| value.into_owned()),
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
    );
    if let (Some(diagnostic), Some(response_error)) = (error.diagnostic(), response.error.as_mut())
    {
        response_error.operation = diagnostic.operation.clone();
        response_error.locator = diagnostic.locator.clone();
        response_error.url = diagnostic.url.clone();
        response_error.timeout = diagnostic.timeout_ms;
        response_error.matched_count = diagnostic.matched_count;
        response_error.element_state = diagnostic.element_state.clone();
        response_error.failure_reason = diagnostic.failure_reason.clone();
    }
    response
}

fn insert_error_diagnostic(
    payload: &mut serde_json::Map<String, Value>,
    diagnostic: &ErrorDiagnostic,
) {
    if let Some(value) = &diagnostic.operation {
        payload.insert("operation".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = &diagnostic.locator {
        payload.insert("locator".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = &diagnostic.url {
        payload.insert("url".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = diagnostic.timeout_ms {
        payload.insert("timeout".to_string(), Value::from(value));
    }
    if let Some(value) = diagnostic.matched_count {
        payload.insert("matched_count".to_string(), Value::from(value));
    }
    if let Some(value) = &diagnostic.element_state {
        payload.insert("element_state".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = &diagnostic.failure_reason {
        payload.insert("failure_reason".to_string(), Value::String(value.clone()));
    }
}

pub fn openpage_error_from_kind(kind: &str, message: impl Into<String>) -> OpenPageError {
    let message = message.into();
    match kind {
        "browser_launch" => OpenPageError::BrowserLaunch(message),
        "browser_operation" | "daemon_transient" => OpenPageError::BrowserOperation(message),
        "page_operation" => OpenPageError::PageOperation(message),
        "element_not_found" => OpenPageError::ElementNotFound(message),
        "element_detached" => OpenPageError::ElementDetached(message),
        "element_ambiguous" => OpenPageError::ElementAmbiguous(message),
        "unsupported_locator" => OpenPageError::UnsupportedLocator(message),
        "invalid_input" | "unsupported_operation" => OpenPageError::UnsupportedOperation(message),
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
    openpage_error_from_structured_context(kind, message, fix, None, None, None, None, None)
}

pub fn openpage_error_from_response_error(error: ResponseError) -> OpenPageError {
    let diagnostic = ErrorDiagnostic {
        operation: error.operation,
        locator: error.locator,
        url: error.url,
        timeout_ms: error.timeout,
        matched_count: error.matched_count,
        element_state: error.element_state,
        failure_reason: error.failure_reason,
    };
    let result = openpage_error_from_structured_context(
        &error.kind,
        error.message,
        error.fix.as_deref(),
        error.session.as_deref(),
        error.state.as_deref(),
        error.reasons.as_deref(),
        error.retryable,
        error.suggested_action.as_deref(),
    );
    if diagnostic == ErrorDiagnostic::default() {
        result
    } else {
        result.diagnosed(diagnostic)
    }
}

pub fn openpage_error_from_structured_context(
    kind: &str,
    message: impl Into<String>,
    fix: Option<&str>,
    session: Option<&str>,
    state: Option<&str>,
    reasons: Option<&[String]>,
    retryable: Option<bool>,
    suggested_action: Option<&str>,
) -> OpenPageError {
    let mut message = message.into();
    let startup_fix = session
        .filter(|value| !value.is_empty())
        .and_then(|session| fix.map(|fix| (session, fix)))
        .filter(|(session, fix)| startup_failure_fix_matches_session(fix, session));

    if kind == "daemon_transient" {
        let retry_same_command =
            retryable == Some(true) || matches!(suggested_action, Some("retry_same_command"));
        if let Some(session) = session.filter(|value| !value.is_empty()) {
            if !message.starts_with("daemon transient for session `") {
                message = format!("daemon transient for session `{session}`: {message}");
            }
        } else if !message.starts_with("daemon transient") {
            message = format!("daemon transient: {message}");
        }
        if retry_same_command && !message.contains("Retry the same command.") {
            append_sentence(&mut message, "Retry the same command.");
        }
    } else if kind == "io" {
        if let Some((session, _)) = startup_fix {
            if !message.starts_with("daemon for session '") {
                message = format!("daemon for session '{session}' startup failure: {message}");
            }
        } else if let Some(session) = session.filter(|value| !value.is_empty()) {
            if !message.starts_with("daemon for session '") {
                message = format!("daemon for session '{session}': {message}");
            }
        }
    } else if let Some(session) = session.filter(|value| !value.is_empty()) {
        if !message.starts_with("session `") {
            let has_reason = |needle: &str| {
                reasons
                    .map(|values| values.iter().any(|value| value == needle))
                    .unwrap_or(false)
            };
            message = match state {
                Some("inactive") => format!("session `{session}` is not active"),
                Some("incomplete") if has_reason("daemon_unresponsive") => {
                    format!("session `{session}` is currently busy or unresponsive")
                }
                Some("incomplete") if has_reason("daemon_not_ready") => {
                    format!("session `{session}` exists but its daemon is not ready")
                }
                Some("incompatible") if has_reason("version_mismatch") => {
                    format!("session `{session}` has a daemon version mismatch")
                }
                _ => format!("session `{session}`: {message}"),
            };
        }
    }

    if let Some(fix) = fix.filter(|value| !value.is_empty()) {
        if !message.contains(fix) {
            append_sentence(&mut message, fix);
        }
    }

    openpage_error_from_kind(kind, message)
}

fn openpage_error_detail(error: &OpenPageError) -> &str {
    match error.root() {
        OpenPageError::BrowserLaunch(detail)
        | OpenPageError::BrowserOperation(detail)
        | OpenPageError::PageOperation(detail)
        | OpenPageError::ElementNotFound(detail)
        | OpenPageError::ElementDetached(detail)
        | OpenPageError::ElementAmbiguous(detail)
        | OpenPageError::UnsupportedLocator(detail)
        | OpenPageError::UnsupportedOperation(detail)
        | OpenPageError::JavaScript(detail)
        | OpenPageError::Http(detail)
        | OpenPageError::Io(detail)
        | OpenPageError::Timeout(detail)
        | OpenPageError::Serialization(detail) => detail,
        OpenPageError::Diagnosed { .. } => unreachable!("root error cannot be diagnosed"),
    }
}

fn shell_error_message(error: &OpenPageError) -> String {
    if openpage_error_kind(error) == "invalid_input" {
        openpage_error_detail(error).to_string()
    } else {
        error.to_string()
    }
}

fn detail_maps_to_invalid_input(detail: &str) -> bool {
    detail.starts_with("drag-in requires ")
        || detail.starts_with("zoom step must ")
        || detail.starts_with("zoom factor must ")
        || detail.starts_with("unsupported snapshot mode:")
        || detail.starts_with("unsupported snapshot format:")
        || detail.starts_with("invalid batch command `")
        || detail.starts_with("invalid batch command quoting:")
        || detail.starts_with("history index must be >= 1")
        || detail.starts_with("history index out of range:")
        || detail.starts_with("tab.close requires targets or others=true")
        || detail.starts_with("find-in-page text must not be empty")
        || detail.starts_with("unknown navigation token:")
        || (detail.starts_with("navigation token ")
            && detail.contains(" belongs to another page or frame"))
        || detail.starts_with("missing target")
        || detail.starts_with("missing string param:")
        || detail.starts_with("missing number param:")
        || detail.starts_with("missing array param:")
        || detail.starts_with("select-range requires end >= start")
        || detail.starts_with("select-text requires end >= start")
        || detail.starts_with("select requires one of:")
        || detail.starts_with("missing param:")
        || detail.starts_with("missing numeric param:")
        || detail.starts_with("missing headers param:")
        || detail.contains(" must be a string or string array")
        || detail.contains(" must be an integer or integer array")
        || detail.contains(" must be an object")
        || detail.starts_with("array param must contain only strings:")
        || detail.starts_with("array param must contain only integers:")
        || detail.starts_with("header values must be strings:")
}

fn openpage_error_fix(error: &OpenPageError) -> Option<&str> {
    let detail = match error {
        OpenPageError::BrowserOperation(detail) => detail,
        OpenPageError::UnsupportedOperation(detail) => {
            return known_unsupported_operation_fix(detail);
        }
        _ => return None,
    };

    if !detail.starts_with("session `") {
        return None;
    }

    let (_, fix) = detail.split_once(". ")?;
    let canonical_session_state = detail.contains(" is not active")
        || detail.contains(" is currently busy or unresponsive")
        || detail.contains(" exists but its daemon is not ready")
        || detail.contains(" has a daemon version mismatch");
    if !canonical_session_state && !fix.contains("`openpage ") {
        return None;
    }
    Some(fix)
}

#[derive(Default)]
struct ErrorContext<'a> {
    fix: Option<Cow<'a, str>>,
    session: Option<&'a str>,
    state: Option<&'static str>,
    reasons: Vec<&'static str>,
    retryable: Option<bool>,
    suggested_action: Option<&'static str>,
}

fn openpage_error_context(error: &OpenPageError) -> ErrorContext<'_> {
    let detail = match error.root() {
        OpenPageError::BrowserLaunch(detail) => {
            return ErrorContext {
                fix: known_browser_launch_fix(detail),
                ..ErrorContext::default()
            };
        }
        OpenPageError::BrowserOperation(detail) => detail,
        OpenPageError::UnsupportedOperation(detail) => {
            return ErrorContext {
                fix: known_unsupported_operation_fix(detail)
                    .or_else(|| known_invalid_input_fix(detail))
                    .map(Cow::Borrowed),
                ..ErrorContext::default()
            };
        }
        OpenPageError::Io(detail) => {
            let mut context = ErrorContext::default();
            if let Some(session) = startup_failure_session_from_detail(detail) {
                context.session = Some(session);
                context.fix = Some(Cow::Owned(startup_failure_fix(session)));
            } else if let Some(session) = daemon_session_from_io_detail(detail) {
                context.session = Some(session);
                context.fix = known_io_fix(detail);
            } else {
                context.fix = known_io_fix(detail);
            }
            return context;
        }
        _ => return ErrorContext::default(),
    };

    let mut context = ErrorContext {
        fix: openpage_error_fix(error).map(Cow::Borrowed),
        session: session_name_from_detail(detail),
        ..ErrorContext::default()
    };

    if context.fix.is_none() {
        context.fix = known_browser_operation_fix(detail);
    }

    if context.fix.is_none() {
        context.fix = known_invalid_input_fix(detail).map(Cow::Borrowed);
    }

    if !detail.starts_with("session `") {
        if detail.starts_with("daemon transient for session `") {
            context.session = detail
                .strip_prefix("daemon transient for session `")
                .and_then(|rest| rest.split_once('`').map(|(session, _)| session))
                .filter(|value| !value.is_empty());
            context.fix = Some(Cow::Borrowed("Retry the same command."));
            context.retryable = Some(true);
            context.suggested_action = Some("retry_same_command");
        }
        return context;
    }

    if detail.contains(" is backed by daemon version ") {
        context.state = Some("incompatible");
        context.reasons.push("version_mismatch");
    } else if detail.contains(" has a daemon version mismatch") {
        context.state = Some("incompatible");
        context.reasons.push("version_mismatch");
    } else if detail.contains(" is currently busy or unresponsive") {
        context.state = Some("incomplete");
        context.reasons.push("daemon_unresponsive");
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

fn startup_failure_session_from_detail(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix("daemon for session '")?;
    let (session, suffix) = rest.split_once('\'')?;
    if session.is_empty() {
        return None;
    }
    if suffix.starts_with(" exited during startup")
        || suffix.starts_with(" failed to become ready during startup")
        || suffix.starts_with(" startup failure:")
    {
        Some(session)
    } else {
        None
    }
}

fn daemon_session_from_io_detail(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix("daemon for session '")?;
    let (session, _) = rest.split_once('\'')?;
    if session.is_empty() {
        None
    } else {
        Some(session)
    }
}

fn startup_failure_fix(session: &str) -> String {
    format!(
        "Run `openpage browser logs --session {session} --tail 20` to inspect the persisted daemon log, then retry the start command."
    )
}

fn startup_failure_fix_matches_session(fix: &str, session: &str) -> bool {
    fix.contains(&format!("browser logs --session {session} --tail 20"))
}

fn append_sentence(message: &mut String, suffix: &str) {
    if message.ends_with('.') {
        message.push(' ');
        message.push_str(suffix);
    } else {
        message.push_str(". ");
        message.push_str(suffix);
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
        let req: Request = serde_json::from_str(r#"{"op":"page.title"}"#).unwrap();
        assert_eq!(req.op, "page.title");
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
                OpenPageError::ElementDetached("x".to_string()),
                "element_detached",
            ),
            (
                OpenPageError::ElementAmbiguous("x".to_string()),
                "element_ambiguous",
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
    fn diagnosed_errors_round_trip_through_protocol() {
        let error =
            OpenPageError::PageOperation("covered".to_string()).diagnosed(ErrorDiagnostic {
                operation: Some("click".to_string()),
                locator: Some("#submit".to_string()),
                url: Some("https://example.com".to_string()),
                timeout_ms: Some(10_000),
                matched_count: Some(1),
                element_state: Some("not actionable".to_string()),
                failure_reason: Some("covered".to_string()),
            });
        let response = response_openpage_error(None, &error);
        let serialized = serde_json::to_value(&response).expect("serialize response");
        assert_eq!(serialized["error"]["operation"], "click");
        assert_eq!(serialized["error"]["timeout"], 10_000);
        assert_eq!(serialized["error"]["matched_count"], 1);

        let reconstructed =
            openpage_error_from_response_error(response.error.expect("response error"));
        assert_eq!(reconstructed.diagnostic(), error.diagnostic());
        assert_eq!(openpage_error_kind(&reconstructed), "page_operation");
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
    fn simple_openpage_error_exposes_fix_for_batch_workflow_restriction() {
        let error = OpenPageError::UnsupportedOperation(
            "batch cannot execute `serve`; use top-level `serve` separately".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert_eq!(
            payload["error"]["fix"],
            "Run `openpage serve --session <name>` as a separate top-level command, then invoke follow-up commands outside `batch`."
        );
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
    fn simple_openpage_error_maps_invalid_value_unsupported_operation_to_invalid_input() {
        let error = OpenPageError::UnsupportedOperation(
            "zoom step must be a positive finite number".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "zoom step must be a positive finite number"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Provide a positive finite `--step <number>` value before retrying."
        );
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_drag_in_missing_payload() {
        let error =
            OpenPageError::UnsupportedOperation("drag-in requires --text or --files".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["fix"],
            "Provide either `--text <content>` or one or more `--files <path>` values before retrying `drag-in`."
        );
    }

    #[test]
    fn response_openpage_error_uses_invalid_input_kind_for_invalid_snapshot_mode() {
        let response = response_openpage_error(
            None,
            &OpenPageError::UnsupportedOperation("unsupported snapshot mode: weird".to_string()),
        );

        assert!(!response.ok);
        let error = response.error.expect("error payload");
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "unsupported snapshot mode: weird");
        assert_eq!(
            error.fix.as_deref(),
            Some("Use one of the supported snapshot modes: `interactive`, `semantic`, or `all`.")
        );
    }

    #[test]
    fn response_openpage_error_uses_invalid_input_fix_for_daemon_drag_in_validation() {
        let response = response_openpage_error(
            None,
            &OpenPageError::UnsupportedOperation("drag-in requires text or files".to_string()),
        );

        assert!(!response.ok);
        let error = response.error.expect("error payload");
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "drag-in requires text or files");
        assert_eq!(
            error.fix.as_deref(),
            Some(
                "Provide either `--text <content>` or one or more `--files <path>` values before retrying `drag-in`."
            )
        );
    }

    #[test]
    fn simple_openpage_error_maps_browser_operation_schema_validation_to_invalid_input() {
        let error = OpenPageError::BrowserOperation(
            "select requires one of: text, value, index".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "select requires one of: text, value, index"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Provide exactly one selector family for `select`: `--text <value>`, `--value <value>`, or `--index <n>`."
        );
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_invalid_snapshot_format() {
        let error =
            OpenPageError::UnsupportedOperation("unsupported snapshot format: xml".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["fix"],
            "Use one of the supported snapshot formats: `text` or `json`."
        );
    }

    #[test]
    fn response_openpage_error_uses_invalid_input_kind_for_browser_operation_param_validation() {
        let response = response_openpage_error(
            None,
            &OpenPageError::BrowserOperation("history index must be >= 1".to_string()),
        );

        assert!(!response.ok);
        let error = response.error.expect("error payload");
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "history index must be >= 1");
        assert_eq!(
            error.fix.as_deref(),
            Some("Use a history index of 1 or greater before retrying.")
        );
    }

    #[test]
    fn simple_openpage_error_maps_find_in_page_empty_text_to_invalid_input() {
        let error =
            OpenPageError::BrowserOperation("find-in-page text must not be empty".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "find-in-page text must not be empty"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Provide a non-empty search string before retrying `find-in-page`."
        );
    }

    #[test]
    fn response_openpage_error_exposes_fix_for_history_index_out_of_range() {
        let response = response_openpage_error(
            None,
            &OpenPageError::BrowserOperation("history index out of range: 42".to_string()),
        );

        assert!(!response.ok);
        let error = response.error.expect("error payload");
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "history index out of range: 42");
        assert_eq!(
            error.fix.as_deref(),
            Some(
                "Run `openpage history list --session <name>` to inspect valid entries, then retry with an in-range history index."
            )
        );
    }

    #[test]
    fn simple_openpage_error_maps_missing_string_param_to_invalid_input() {
        let error = OpenPageError::BrowserOperation("missing string param: locator".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(payload["error"]["message"], "missing string param: locator");
        assert_eq!(
            payload["error"]["fix"],
            "Provide the required string param before retrying."
        );
    }

    #[test]
    fn simple_openpage_error_maps_unknown_navigation_token_to_invalid_input() {
        let error =
            OpenPageError::BrowserOperation("unknown navigation token: definitely-bad".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "unknown navigation token: definitely-bad"
        );
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_missing_headers_param() {
        let error = OpenPageError::BrowserOperation("missing headers param: headers".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["fix"],
            "Provide the required headers object before retrying."
        );
    }

    #[test]
    fn response_openpage_error_exposes_fix_for_object_shape_validation() {
        let response = response_openpage_error(
            None,
            &OpenPageError::BrowserOperation("headers must be an object".to_string()),
        );

        assert!(!response.ok);
        let error = response.error.expect("error payload");
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "headers must be an object");
        assert_eq!(
            error.fix.as_deref(),
            Some("Provide this field as an object before retrying.")
        );
    }

    #[test]
    fn invalid_input_contract_covers_known_detail_taxonomy() {
        let invalid_input_cases = [
            OpenPageError::UnsupportedOperation(
                "zoom step must be a positive finite number".to_string(),
            ),
            OpenPageError::UnsupportedOperation("unsupported snapshot format: xml".to_string()),
            OpenPageError::BrowserOperation("history index must be >= 1".to_string()),
            OpenPageError::BrowserOperation("history index out of range: 42".to_string()),
            OpenPageError::BrowserOperation("find-in-page text must not be empty".to_string()),
            OpenPageError::BrowserOperation("missing target".to_string()),
            OpenPageError::BrowserOperation("missing string param: locator".to_string()),
            OpenPageError::BrowserOperation("missing number param: x".to_string()),
            OpenPageError::BrowserOperation("missing array param: files".to_string()),
            OpenPageError::BrowserOperation("missing param: value".to_string()),
            OpenPageError::BrowserOperation("missing headers param: headers".to_string()),
            OpenPageError::BrowserOperation("value must be a string or string array".to_string()),
            OpenPageError::BrowserOperation(
                "index must be an integer or integer array".to_string(),
            ),
            OpenPageError::BrowserOperation("headers must be an object".to_string()),
            OpenPageError::BrowserOperation(
                "array param must contain only strings: files".to_string(),
            ),
            OpenPageError::BrowserOperation(
                "array param must contain only integers: index".to_string(),
            ),
            OpenPageError::BrowserOperation("header values must be strings: X-Test".to_string()),
            OpenPageError::BrowserOperation(
                "navigation token bad-token belongs to another page or frame".to_string(),
            ),
        ];

        for error in invalid_input_cases {
            let payload = simple_openpage_error(&error);
            assert_eq!(
                payload["error"]["kind"], "invalid_input",
                "expected invalid_input for {:?}",
                error
            );
        }
    }

    #[test]
    fn invalid_input_contract_keeps_runtime_and_restriction_cases_outside_bucket() {
        let cases = [
            (
                OpenPageError::BrowserOperation("unknown target: popup-2".to_string()),
                "browser_operation",
            ),
            (
                OpenPageError::UnsupportedOperation(
                    "downloads open is unsupported on this platform".to_string(),
                ),
                "unsupported_operation",
            ),
            (
                OpenPageError::UnsupportedOperation(
                    "no recently closed tab recorded for this session".to_string(),
                ),
                "unsupported_operation",
            ),
        ];

        for (error, expected_kind) in cases {
            let payload = simple_openpage_error(&error);
            assert_eq!(
                payload["error"]["kind"], expected_kind,
                "unexpected shell kind for {:?}",
                error
            );
        }
    }

    #[test]
    fn simple_openpage_error_keeps_fix_absent_for_platform_unsupported_case() {
        let error = OpenPageError::UnsupportedOperation(
            "downloads open is unsupported on this platform".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert!(payload["error"].get("fix").is_none());
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_missing_recently_closed_tab_stack() {
        let error = OpenPageError::UnsupportedOperation(
            "no recently closed tab recorded for this session".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert_eq!(
            payload["error"]["fix"],
            "Close a tab in this session with `openpage tab close ... --session <name>` before using `openpage tab reopen --session <name>`, or open a replacement tab directly with `openpage tab new <url> --session <name>`."
        );
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
    fn simple_openpage_error_exposes_fix_for_missing_browser_executable_launch_failure() {
        let error = OpenPageError::BrowserLaunch("No such file or directory (os error 2)".into());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_launch");
        assert_eq!(
            payload["error"]["message"],
            "browser launch failed: No such file or directory (os error 2)"
        );
        let fix = payload["error"]["fix"]
            .as_str()
            .expect("fix should be present");
        assert!(fix.contains("openpage doctor --quick"));
        assert!(fix.contains("OPENPAGE_BROWSER_PATH=<absolute-browser-path>"));
        assert!(fix.contains("--browser-path <absolute-browser-path>"));
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_browser_process_exits_before_websocket() {
        let error = OpenPageError::BrowserLaunch(
            "Browser process exited with status ExitStatus(unix_wait_status(0)) before websocket URL could be resolved, stderr: BrowserStderr(\"\")"
                .into(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_launch");
        let fix = payload["error"]["fix"]
            .as_str()
            .expect("fix should be present");
        assert!(fix.contains("real Chromium-based browser"));
        assert!(fix.contains("openpage doctor"));
        assert!(fix.contains("OPENPAGE_BROWSER_PATH=<absolute-browser-path>"));
        assert!(fix.contains("browser start --session <name> --replace"));
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_browser_process_timeout_before_websocket() {
        let error = OpenPageError::BrowserLaunch(
            "Timeout while resolving websocket URL from browser process, stderr: BrowserStderr(\"\")"
                .into(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_launch");
        let fix = payload["error"]["fix"]
            .as_str()
            .expect("fix should be present");
        assert!(fix.contains("real Chromium-based browser"));
        assert!(fix.contains("openpage doctor"));
        assert!(fix.contains("--browser-path <absolute-browser-path>"));
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_browser_process_io_before_websocket() {
        let error = OpenPageError::BrowserLaunch(
            "Input/Output error while resolving websocket URL from browser process, stderr: BrowserStderr(\"\"): unexpected end of stream"
                .into(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_launch");
        let fix = payload["error"]["fix"]
            .as_str()
            .expect("fix should be present");
        assert!(fix.contains("real Chromium-based browser"));
        assert!(fix.contains("openpage doctor"));
        assert!(fix.contains("OPENPAGE_BROWSER_PATH=<absolute-browser-path>"));
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_unknown_target() {
        let error = OpenPageError::BrowserOperation("unknown target: badpath".into());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(
            payload["error"]["message"],
            "browser operation failed: unknown target: badpath"
        );
        let fix = payload["error"]["fix"]
            .as_str()
            .expect("fix should be present");
        assert!(fix.contains("browser start --session badpath --replace"));
        assert!(fix.contains("goto --session badpath <url>"));
        assert!(fix.contains("tab or frame target"));
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
    fn simple_openpage_error_exposes_session_and_fix_for_startup_timeout_io() {
        let error = OpenPageError::Io(
            "daemon for session 'review' failed to become ready during startup; startup daemon was stopped. See /tmp/review.log".to_string(),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(
            payload["error"]["fix"],
            "Run `openpage browser logs --session review --tail 20` to inspect the persisted daemon log, then retry the start command."
        );
        assert!(payload["error"].get("state").is_none());
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
    fn reconstructs_element_relocation_error_kinds() {
        assert!(matches!(
            openpage_error_from_kind("element_detached", "stale"),
            OpenPageError::ElementDetached(message) if message == "stale"
        ));
        assert!(matches!(
            openpage_error_from_kind("element_ambiguous", "two matches"),
            OpenPageError::ElementAmbiguous(message) if message == "two matches"
        ));
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
    fn reconstructs_openpage_error_from_structured_context_for_transient_retry() {
        let error = openpage_error_from_structured_context(
            "daemon_transient",
            "io error: connection reset by peer",
            Some("Retry the same command."),
            Some("review"),
            None,
            None,
            Some(true),
            Some("retry_same_command"),
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "daemon_transient");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["retryable"], true);
        assert_eq!(payload["error"]["suggested_action"], "retry_same_command");
        assert_eq!(payload["error"]["fix"], "Retry the same command.");
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_context_for_generic_incompatible_state() {
        let reasons = vec!["version_mismatch".to_string()];
        let error = openpage_error_from_structured_context(
            "browser_operation",
            "daemon reported version mismatch",
            Some("Stop and restart the session."),
            Some("review"),
            Some("incompatible"),
            Some(&reasons),
            None,
            None,
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["state"], "incompatible");
        assert_eq!(payload["error"]["reasons"], json!(["version_mismatch"]));
        assert_eq!(payload["error"]["fix"], "Stop and restart the session.");
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_context_for_busy_incomplete_state() {
        let reasons = vec!["daemon_unresponsive".to_string()];
        let error = openpage_error_from_structured_context(
            "browser_operation",
            "daemon reported unresponsive session",
            Some("Inspect session activity and retry."),
            Some("review"),
            Some("incomplete"),
            Some(&reasons),
            None,
            None,
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["state"], "incomplete");
        assert_eq!(payload["error"]["reasons"], json!(["daemon_unresponsive"]));
        assert_eq!(
            payload["error"]["fix"],
            "Inspect session activity and retry."
        );
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("session `review` is currently busy or unresponsive")
        );
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_context_for_generic_startup_failure_io() {
        let fix = startup_failure_fix("review");
        let error = openpage_error_from_structured_context(
            "io",
            "daemon startup timed out",
            Some(&fix),
            Some("review"),
            None,
            None,
            None,
            None,
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(payload["error"]["session"], "review");
        assert_eq!(payload["error"]["fix"], fix);
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("daemon for session 'review' startup failure")
        );
    }

    #[test]
    fn reconstructs_openpage_error_from_structured_context_for_generic_session_io() {
        let error = openpage_error_from_structured_context(
            "io",
            "permission denied",
            None,
            Some("review"),
            None,
            None,
            None,
            None,
        );
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(payload["error"]["session"], "review");
        assert!(payload["error"].get("fix").is_none());
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("daemon for session 'review': permission denied")
        );
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_not_a_directory_io() {
        let error = OpenPageError::Io("Not a directory (os error 20)".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(
            payload["error"]["message"],
            "io error: Not a directory (os error 20)"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Set OPENPAGE_HOME to a writable directory path, not a file, then retry."
        );
    }

    #[test]
    fn simple_openpage_error_exposes_fix_for_permission_denied_io() {
        let error = OpenPageError::Io("Permission denied (os error 13)".to_string());
        let payload = simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(
            payload["error"]["message"],
            "io error: Permission denied (os error 13)"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Set OPENPAGE_HOME to a writable directory path and fix parent directory permissions before retrying."
        );
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
    fn response_openpage_error_exposes_session_and_fix_for_startup_exit_io() {
        let error = OpenPageError::Io(
            "daemon for session 'review' exited during startup: bind failed".to_string(),
        );
        let response = response_openpage_error(Some(serde_json::json!("cli")), &error);

        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|error| error.kind.as_str()),
            Some("io")
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
                .and_then(|error| error.fix.as_deref()),
            Some(
                "Run `openpage browser logs --session review --tail 20` to inspect the persisted daemon log, then retry the start command."
            )
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
