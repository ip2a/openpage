use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use serde_json::{Value, json};

use crate::capture::{Capture, CaptureCreate, CaptureKind, CaptureRegistry};
use crate::error::{OpenPageError, OpenPageResult};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_OUTPUT_CHARS: u64 = 20_000;
const RESOURCE_PAGE_SIZE: usize = 50;

struct CliOutcome {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
}

struct McpState {
    session: String,
    captures: CaptureRegistry,
}

impl McpState {
    fn new(session: &str) -> Self {
        Self {
            session: session.to_string(),
            captures: CaptureRegistry::default(),
        }
    }

    fn handle_line(&self, line: &str) -> OpenPageResult<Option<Value>> {
        let request: Value = serde_json::from_str(line)
            .map_err(|err| OpenPageError::Serialization(format!("invalid MCP JSON: {err}")))?;
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let response = match method {
            "notifications/initialized" => None,
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {"listChanged": false},
                        "resources": {"listChanged": false, "subscribe": false},
                        "prompts": {"listChanged": false}
                    },
                    "serverInfo": {"name": "openpage", "version": env!("CARGO_PKG_VERSION")}
                }
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"tools": tool_definitions()}
            })),
            "tools/call" => Some(self.call_tool(id, request.get("params"))),
            "resources/list" => Some(self.list_resources(id, request.get("params"))),
            "resources/read" => Some(self.read_resource(id, request.get("params"))),
            "prompts/list" => Some(self.list_prompts(id, request.get("params"))),
            "prompts/get" => Some(self.get_prompt(id, request.get("params"))),
            _ => Some(method_not_found(id, method)),
        };
        Ok(response)
    }

    fn call_tool(&self, id: Option<Value>, params: Option<&Value>) -> Value {
        let name = params
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let args = params
            .and_then(|value| value.get("arguments"))
            .unwrap_or(&Value::Null);

        let result = match name {
            "help" => call_help(args),
            "openpage" => build_openpage_args(args, &self.session).and_then(run_tool_args),
            "snapshot" => build_snapshot_args(args, &self.session)
                .and_then(run_tool_args)
                .map(|result| self.record_capture(CaptureKind::Snapshot, result)),
            "screenshot" => build_screenshot_args(args, &self.session)
                .and_then(run_tool_args)
                .map(|result| self.record_capture(CaptureKind::Screenshot, result)),
            "navigate" => build_navigate_args(args, &self.session).and_then(run_tool_args),
            "click" => build_click_args(args, &self.session).and_then(run_tool_args),
            "fill" => build_fill_args(args, &self.session).and_then(run_tool_args),
            _ => Err(format!("unknown tool: {name}")),
        };
        let tool_result =
            result.unwrap_or_else(|message| structured_error("invalid_input", message));
        json!({"jsonrpc": "2.0", "id": id, "result": tool_result})
    }

    fn record_capture(&self, kind: CaptureKind, mut tool_result: Value) -> Value {
        let Some(result) = tool_result
            .get("structuredContent")
            .filter(|value| value.get("ok").and_then(Value::as_bool) == Some(true))
            .and_then(|value| value.get("result"))
            .cloned()
        else {
            return tool_result;
        };
        let file_path = if kind == CaptureKind::Screenshot {
            result
                .get("output")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        let summary = tool_result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| content.first())
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("OpenPage capture")
            .to_string();
        let capture = self.captures.record(
            &self.session,
            CaptureCreate {
                kind,
                target: result
                    .get("target")
                    .or_else(|| result.get("target_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                revision: result
                    .get("revision")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                mime_type: if kind == CaptureKind::Screenshot {
                    result
                        .get("mime_type")
                        .and_then(Value::as_str)
                        .unwrap_or("image/png")
                        .to_string()
                } else {
                    "application/json".to_string()
                },
                content: (kind != CaptureKind::Screenshot).then_some(result),
                file_path,
                summary,
            },
        );
        if let Some(content) = tool_result.get_mut("content").and_then(Value::as_array_mut) {
            content.push(resource_link(&capture));
        }
        tool_result
    }

    fn list_resources(&self, id: Option<Value>, params: Option<&Value>) -> Value {
        let start = match parse_cursor(params) {
            Ok(start) => start,
            Err(message) => return invalid_params(id, message),
        };
        let captures = self.captures.list(&self.session, None);
        if start > captures.len() {
            return invalid_params(id, "resource cursor is out of range");
        }
        let end = start.saturating_add(RESOURCE_PAGE_SIZE).min(captures.len());
        let resources = captures[start..end]
            .iter()
            .map(resource_descriptor)
            .collect::<Vec<_>>();
        let mut result = json!({"resources": resources});
        if end < captures.len() {
            result["nextCursor"] = json!(end.to_string());
        }
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }

    fn read_resource(&self, id: Option<Value>, params: Option<&Value>) -> Value {
        let Some(uri) = params
            .and_then(|value| value.get("uri"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            return invalid_params(id, "`uri` is required and must be a string");
        };
        let Some(capture_id) = parse_capture_uri(uri, &self.session) else {
            return invalid_params(id, format!("invalid capture URI: {uri}"));
        };
        let Some(capture) = self.captures.get(&self.session, capture_id) else {
            return invalid_params(id, format!("capture not found: {uri}"));
        };
        match capture_contents(&capture, uri) {
            Ok(contents) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"contents": [contents]}
            }),
            Err(message) => invalid_params(id, message),
        }
    }

    fn list_prompts(&self, id: Option<Value>, params: Option<&Value>) -> Value {
        if let Err(message) = parse_empty_cursor(params) {
            return invalid_params(id, message);
        }
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"prompts": prompt_definitions()}
        })
    }

    fn get_prompt(&self, id: Option<Value>, params: Option<&Value>) -> Value {
        let Some(name) = params
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            return invalid_params(id, "`name` is required and must be a string");
        };
        let args = params
            .and_then(|value| value.get("arguments"))
            .unwrap_or(&Value::Null);
        let prompt = match prompt_messages(name, args) {
            Ok(prompt) => prompt,
            Err(message) => return invalid_params(id, message),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": prompt})
    }
}

pub fn serve_stdio<R: BufRead, W: Write>(
    session: &str,
    input: R,
    mut output: W,
) -> OpenPageResult<()> {
    let state = McpState::new(session);
    for line in input.lines() {
        let line = line.map_err(|err| OpenPageError::Io(err.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = state.handle_line(&line)? {
            serde_json::to_writer(&mut output, &response)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

fn invalid_params(id: Option<Value>, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": -32602, "message": message.into()}
    })
}

fn capture_uri(capture: &Capture) -> String {
    format!(
        "openpage://sessions/{}/captures/{}",
        capture.session, capture.id
    )
}

fn parse_capture_uri(uri: &str, session: &str) -> Option<u64> {
    uri.strip_prefix(&format!("openpage://sessions/{session}/captures/"))?
        .parse()
        .ok()
}

fn resource_descriptor(capture: &Capture) -> Value {
    json!({
        "uri": capture_uri(capture),
        "name": format!("{} #{}", capture.kind.as_str(), capture.id),
        "description": capture.summary,
        "mimeType": capture.mime_type
    })
}

fn resource_link(capture: &Capture) -> Value {
    let descriptor = resource_descriptor(capture);
    json!({
        "type": "resource_link",
        "uri": descriptor["uri"],
        "name": descriptor["name"],
        "description": descriptor["description"],
        "mimeType": descriptor["mimeType"]
    })
}

fn capture_contents(capture: &Capture, uri: &str) -> Result<Value, String> {
    if let Some(content) = &capture.content {
        let text = serde_json::to_string(content)
            .map_err(|err| format!("failed to serialize capture: {err}"))?;
        return Ok(json!({
            "uri": uri,
            "mimeType": capture.mime_type,
            "text": text
        }));
    }
    let path = capture
        .file_path
        .as_deref()
        .ok_or_else(|| format!("capture has no readable content: {uri}"))?;
    let bytes = fs::read(path).map_err(|err| format!("failed to read capture file: {err}"))?;
    Ok(json!({
        "uri": uri,
        "mimeType": capture.mime_type,
        "blob": BASE64_STANDARD.encode(bytes)
    }))
}

fn parse_cursor(params: Option<&Value>) -> Result<usize, String> {
    let Some(cursor) = params
        .and_then(|value| value.get("cursor"))
        .filter(|value| !value.is_null())
    else {
        return Ok(0);
    };
    cursor
        .as_str()
        .ok_or_else(|| "resource cursor must be a string".to_string())?
        .parse()
        .map_err(|_| "resource cursor must be a non-negative integer string".to_string())
}

fn parse_empty_cursor(params: Option<&Value>) -> Result<(), String> {
    match params
        .and_then(|value| value.get("cursor"))
        .filter(|value| !value.is_null())
    {
        Some(Value::String(cursor)) if cursor.is_empty() => Ok(()),
        Some(Value::String(_)) => Err("prompt cursor is not supported".to_string()),
        Some(_) => Err("prompt cursor must be a string".to_string()),
        None => Ok(()),
    }
}

fn prompt_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "review_page",
            "description": "Review the current page for accessibility, forms, and runtime errors.",
            "arguments": [{
                "name": "focus",
                "description": "Optional focus: a11y, forms, errors, or all.",
                "required": false
            }]
        }),
        json!({
            "name": "collect_paginated_data",
            "description": "Collect structured data across paginated results with a hard page limit.",
            "arguments": [
                {
                    "name": "goal",
                    "description": "The data to collect.",
                    "required": true
                },
                {
                    "name": "max_pages",
                    "description": "Maximum pages to visit. Defaults to 10.",
                    "required": false
                },
                {
                    "name": "next_page_locator",
                    "description": "Optional locator for the next-page control.",
                    "required": false
                }
            ]
        }),
        json!({
            "name": "guided_login",
            "description": "Guide a login while leaving credentials and verification to the user.",
            "arguments": [{
                "name": "site",
                "description": "Site URL or name.",
                "required": true
            }]
        }),
    ]
}

fn prompt_messages(name: &str, args: &Value) -> Result<Value, String> {
    if !args.is_null() && !args.is_object() {
        return Err("prompt `arguments` must be an object".to_string());
    }
    let text = match name {
        "review_page" => {
            let focus = optional_prompt_string(args, "focus")?.unwrap_or("all");
            format!(
                "Review the current page with focus: {focus}.\n\n\
                 1. Call `snapshot` and retain its revisioned refs.\n\
                 2. Inspect accessibility names, labels, forms, and controls as requested.\n\
                 3. For errors, use `openpage` to inspect console and failed network requests.\n\
                 4. Return sections for accessibility, forms, errors, and suggestions.\n\
                 Keep the review read-only and cite refs for applicable findings."
            )
        }
        "collect_paginated_data" => {
            let goal = required_prompt_string(args, "goal")?;
            let max_pages = optional_prompt_string(args, "max_pages")?.unwrap_or("10");
            let next = optional_prompt_string(args, "next_page_locator")?
                .map(|value| format!(" Use `{value}` as the next-page locator."))
                .unwrap_or_default();
            format!(
                "Collect: {goal}. Maximum pages: {max_pages}.{next}\n\n\
                 1. Snapshot and extract the requested items as structured JSON.\n\
                 2. Find and click the next-page control using the snapshot revision.\n\
                 3. Re-snapshot after each page and verify that the page or items changed.\n\
                 4. Stop when the goal is met, no next page exists, or the limit is reached.\n\
                 Return the aggregated JSON and the number of pages visited."
            )
        }
        "guided_login" => {
            let site = required_prompt_string(args, "site")?;
            format!(
                "Guide login for {site}.\n\n\
                 1. Navigate to the login page and snapshot the form.\n\
                 2. Identify the account, password, submit, and verification controls.\n\
                 3. Ask the user to enter credentials and verification values directly.\n\
                 4. After confirmation, click submit, wait for navigation, and snapshot again.\n\
                 5. Report the resulting URL and whether login succeeded.\n\
                 Do not place credentials or verification values in tools, captures, or logs."
            )
        }
        _ => return Err(format!("unknown prompt: {name}")),
    };
    Ok(json!({
        "description": prompt_definitions()
            .into_iter()
            .find(|prompt| prompt["name"] == name)
            .and_then(|prompt| prompt["description"].as_str().map(str::to_string))
            .unwrap_or_default(),
        "messages": [{
            "role": "user",
            "content": {"type": "text", "text": text}
        }]
    }))
}

fn required_prompt_string<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    optional_prompt_string(args, name)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("prompt argument `{name}` is required"))
}

fn optional_prompt_string<'a>(args: &'a Value, name: &str) -> Result<Option<&'a str>, String> {
    match args.get(name).filter(|value| !value.is_null()) {
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("prompt argument `{name}` must be a string")),
        None => Ok(None),
    }
}

fn method_not_found(id: Option<Value>, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": -32601, "message": format!("method not found: {method}")}
    })
}

fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ok": {"type": "boolean"},
            "result": {},
            "error": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string"},
                    "message": {"type": "string"},
                    "retryable": {"type": "boolean"},
                    "suggested_action": {"type": "string"},
                    "operation": {"type": "string"},
                    "locator": {"type": "string"},
                    "current_revision": {"type": "string"},
                    "expected_revision": {"type": "string"}
                },
                "required": ["kind", "message"]
            }
        },
        "required": ["ok"]
    })
}

fn tool_definitions() -> Vec<Value> {
    let schema = output_schema();
    vec![
        json!({
            "name": "help",
            "description": "Discover OpenPage CLI capabilities. Omit topic for the command catalog, or pass a command path such as 'browser start'.",
            "inputSchema": {
                "type": "object",
                "properties": {"topic": {"type": "string"}}
            },
            "outputSchema": schema
        }),
        json!({
            "name": "openpage",
            "description": "Execute one command or a structured batch for operations not covered by typed tools. The bound MCP session is injected automatically.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "minLength": 1},
                    "commands": {"type": "array", "items": {"type": "string"}, "minItems": 1},
                    "bail": {"type": "boolean", "default": true}
                }
            },
            "outputSchema": schema
        }),
        json!({
            "name": "snapshot",
            "description": "Return a JSON accessibility snapshot with revisioned refs. Use offset to continue a truncated result.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["interactive", "semantic", "all"], "default": "interactive"},
                    "compact": {"type": "boolean", "default": false},
                    "depth": {"type": "integer", "minimum": 0, "maximum": 10},
                    "selector": {"type": "string"},
                    "max_length": {"type": "integer", "minimum": 1, "default": DEFAULT_MAX_OUTPUT_CHARS},
                    "offset": {"type": "integer", "minimum": 0, "default": 0}
                }
            },
            "outputSchema": schema
        }),
        json!({
            "name": "screenshot",
            "description": "Capture the page or one element as PNG and return saved-file metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "locator": {"type": "string", "minLength": 1},
                    "full_page": {"type": "boolean", "default": false},
                    "output_path": {"type": "string", "minLength": 1}
                }
            },
            "outputSchema": schema
        }),
        json!({
            "name": "navigate",
            "description": "Navigate the bound page and wait for the requested readiness state.",
            "inputSchema": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {"type": "string", "format": "uri"},
                    "wait_until": {"type": "string", "enum": ["load", "domcontentloaded", "networkidle"], "default": "load"}
                }
            },
            "outputSchema": schema
        }),
        json!({
            "name": "click",
            "description": "Click a locator or revisioned @ref. Pass expected_revision from the snapshot that produced the ref.",
            "inputSchema": {
                "type": "object",
                "required": ["locator"],
                "properties": {
                    "locator": {"type": "string", "minLength": 1},
                    "expected_revision": {"type": "string"},
                    "wait_until": {"type": ["string", "null"], "enum": ["load", "domcontentloaded", "networkidle", null], "default": null}
                }
            },
            "outputSchema": schema
        }),
        json!({
            "name": "fill",
            "description": "Fill an input by locator or revisioned @ref.",
            "inputSchema": {
                "type": "object",
                "required": ["locator", "text"],
                "properties": {
                    "locator": {"type": "string", "minLength": 1},
                    "text": {"type": "string"},
                    "expected_revision": {"type": "string"}
                }
            },
            "outputSchema": schema
        }),
    ]
}

fn call_help(args: &Value) -> Result<Value, String> {
    let topic = args.get("topic").and_then(Value::as_str);
    let mut cli_args = build_help_topic_args(topic)?;
    cli_args.push("--help".into());
    let outcome = execute_cli(&cli_args)?;
    if outcome.timed_out {
        return Ok(structured_error(
            "timeout",
            "OpenPage help command timed out after 60 seconds",
        ));
    }
    let mut text = outcome.stdout;
    text.push_str(&outcome.stderr);
    let structured = json!({"ok": true, "result": {"text": text}});
    Ok(structured_tool_result(structured))
}

fn run_tool_args(args: Vec<String>) -> Result<Value, String> {
    let outcome = execute_cli(&args)?;
    if outcome.timed_out {
        return Ok(structured_error(
            "timeout",
            "OpenPage command timed out after 60 seconds",
        ));
    }
    let stdout = outcome.stdout.trim();
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        return Ok(structured_tool_result(value));
    }
    let message = if stdout.is_empty() {
        outcome.stderr.trim()
    } else {
        stdout
    };
    let kind = if outcome.exit_code == Some(0) {
        "invalid_output"
    } else {
        "cli_failure"
    };
    Ok(structured_error(kind, message))
}

fn structured_tool_result(value: Value) -> Value {
    let is_error = value.get("ok").and_then(Value::as_bool) != Some(true);
    let summary = if is_error {
        summarize_error(value.get("error").unwrap_or(&Value::Null))
    } else {
        summarize_success(value.get("result").unwrap_or(&Value::Null))
    };
    json!({
        "content": [{"type": "text", "text": summary}],
        "structuredContent": value,
        "isError": is_error
    })
}

fn structured_error(kind: &str, message: impl Into<String>) -> Value {
    structured_tool_result(json!({
        "ok": false,
        "error": {
            "kind": kind,
            "message": message.into(),
            "retryable": false
        }
    }))
}

fn summarize_success(result: &Value) -> String {
    if let Some(revision) = result.get("revision").and_then(Value::as_str) {
        let count = result.get("count").and_then(Value::as_u64);
        return match count {
            Some(count) => format!("snapshot {revision}: {count} entries"),
            None => format!("completed at revision {revision}"),
        };
    }
    if let Some(commands) = result.get("commands").and_then(Value::as_array) {
        let failed = commands
            .iter()
            .filter(|entry| entry.get("ok").and_then(Value::as_bool) != Some(true))
            .count();
        return format!(
            "batch completed: {} commands, {failed} failed",
            commands.len()
        );
    }
    if let Some(output) = result.get("output").and_then(Value::as_str) {
        return format!("saved {output}");
    }
    "OpenPage command completed".to_string()
}

fn summarize_error(error: &Value) -> String {
    let kind = error
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("command failed");
    match error.get("suggested_action").and_then(Value::as_str) {
        Some(action) => format!("[{kind}] {message} (suggested: {action})"),
        None => format!("[{kind}] {message}"),
    }
}

fn execute_cli(args: &[String]) -> Result<CliOutcome, String> {
    let exe = openpage_binary_path().map_err(|err| err.to_string())?;
    run_cli(&exe, args, TOOL_CALL_TIMEOUT).map_err(|err| err.to_string())
}

fn build_help_topic_args(topic: Option<&str>) -> Result<Vec<String>, String> {
    let Some(topic) = topic.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    shlex::split(topic).ok_or_else(|| "invalid help topic: unbalanced quoting".into())
}

fn build_openpage_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let command = args.get("command").filter(|value| !value.is_null());
    let commands = args.get("commands").filter(|value| !value.is_null());
    match (command, commands) {
        (Some(_), Some(_)) | (None, None) => {
            Err("invalid input: provide exactly one of `command` or `commands`".into())
        }
        (Some(command), None) => {
            let command = command
                .as_str()
                .ok_or_else(|| "invalid input: `command` must be a string".to_string())?;
            parse_command(command, session, None)
        }
        (None, Some(commands)) => {
            let commands = commands
                .as_array()
                .ok_or_else(|| "invalid input: `commands` must be an array".to_string())?;
            if commands.is_empty() {
                return Err("invalid input: `commands` must not be empty".into());
            }
            let mut result = vec!["batch".into()];
            if args.get("bail").and_then(Value::as_bool).unwrap_or(true) {
                result.push("--bail".into());
            }
            for (index, command) in commands.iter().enumerate() {
                let command = command.as_str().ok_or_else(|| {
                    format!("invalid input: command at index {index} must be a string")
                })?;
                let tokens = parse_command(command, session, Some(index))?;
                let joined = shlex::try_join(tokens.iter().map(String::as_str)).map_err(|err| {
                    format!("invalid input: command at index {index} cannot be composed: {err}")
                })?;
                result.push(joined);
            }
            Ok(result)
        }
    }
}

fn parse_command(
    command: &str,
    session: &str,
    index: Option<usize>,
) -> Result<Vec<String>, String> {
    let label = index
        .map(|value| format!("command at index {value}"))
        .unwrap_or_else(|| "command".into());
    let mut tokens = shlex::split(command)
        .ok_or_else(|| format!("invalid input: {label} has unbalanced quoting"))?;
    if tokens.is_empty() {
        return Err(format!("invalid input: {label} must not be empty"));
    }
    if matches!(tokens[0].as_str(), "mcp" | "serve") {
        return Err(format!(
            "invalid input: `{}` is not allowed via the openpage tool ({label})",
            tokens[0]
        ));
    }
    if tokens[0] != "diff" {
        inject_session(&mut tokens, session);
    }
    Ok(tokens)
}

fn inject_session(args: &mut Vec<String>, session: &str) {
    if !args.iter().any(|arg| arg == "--session") {
        args.extend(["--session".into(), session.into()]);
    }
}

fn build_snapshot_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let mut result = vec!["snapshot".into(), "--format".into(), "json".into()];
    push_string_option(&mut result, args, "mode", "--mode")?;
    if args.get("compact").and_then(Value::as_bool) == Some(true) {
        result.push("--compact".into());
    }
    if let Some(depth) = optional_u64(args, "depth")? {
        result.extend(["--depth".into(), depth.to_string()]);
    }
    push_string_option(&mut result, args, "selector", "--selector")?;
    let max_length = optional_u64(args, "max_length")?.unwrap_or(DEFAULT_MAX_OUTPUT_CHARS);
    let offset = optional_u64(args, "offset")?.unwrap_or(0);
    result.extend(["--max-output".into(), max_length.to_string()]);
    result.extend(["--offset".into(), offset.to_string()]);
    result.extend(["--session".into(), session.into()]);
    Ok(result)
}

fn build_screenshot_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let output = match args.get("output_path").filter(|value| !value.is_null()) {
        Some(value) => value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "invalid input: `output_path` must be a non-empty string".to_string())?
            .to_string(),
        None => temporary_screenshot_path().to_string_lossy().into_owned(),
    };
    let mut result = if let Some(locator) = args.get("locator").filter(|value| !value.is_null()) {
        vec![
            "screenshot-element".into(),
            locator
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "invalid input: `locator` must be a non-empty string".to_string())?
                .into(),
        ]
    } else {
        let mut result = vec!["screenshot".into()];
        if args.get("full_page").and_then(Value::as_bool) == Some(true) {
            result.push("--full-page".into());
        }
        result
    };
    result.extend([
        "--output".into(),
        output,
        "--session".into(),
        session.into(),
    ]);
    Ok(result)
}

fn temporary_screenshot_path() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openpage-mcp-{}-{timestamp}-{}.png",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn build_navigate_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let url = required_nonempty_string(args, "url")?;
    validate_wait_until(args, "wait_until", false)?;
    Ok(vec![
        "goto".into(),
        url.into(),
        "--wait".into(),
        "--session".into(),
        session.into(),
    ])
}

fn build_click_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let locator = required_nonempty_string(args, "locator")?;
    let mut result = vec!["click".into(), locator.into()];
    push_string_option(
        &mut result,
        args,
        "expected_revision",
        "--expected-revision",
    )?;
    if validate_wait_until(args, "wait_until", true)?.is_some() {
        result.push("--wait-navigation".into());
    }
    result.extend(["--session".into(), session.into()]);
    Ok(result)
}

fn build_fill_args(args: &Value, session: &str) -> Result<Vec<String>, String> {
    let locator = required_nonempty_string(args, "locator")?;
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid input: `text` is required and must be a string".to_string())?;
    let mut result = vec!["fill".into(), locator.into(), text.into()];
    push_string_option(
        &mut result,
        args,
        "expected_revision",
        "--expected-revision",
    )?;
    result.extend(["--session".into(), session.into()]);
    Ok(result)
}

fn validate_wait_until<'a>(
    args: &'a Value,
    name: &str,
    nullable: bool,
) -> Result<Option<&'a str>, String> {
    let Some(value) = args.get(name) else {
        return Ok(if nullable { None } else { Some("load") });
    };
    if value.is_null() && nullable {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| format!("invalid input: `{name}` must be a string"))?;
    if matches!(value, "load" | "domcontentloaded" | "networkidle") {
        Ok(Some(value))
    } else {
        Err(format!(
            "invalid input: `{name}` must be load, domcontentloaded, or networkidle"
        ))
    }
}

fn optional_u64(args: &Value, name: &str) -> Result<Option<u64>, String> {
    match args.get(name).filter(|value| !value.is_null()) {
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("invalid input: `{name}` must be a non-negative integer")),
        None => Ok(None),
    }
}

fn required_nonempty_string<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("invalid input: `{name}` is required and must not be empty"))
}

fn push_string_option(
    result: &mut Vec<String>,
    args: &Value,
    name: &str,
    flag: &str,
) -> Result<(), String> {
    if let Some(value) = args.get(name).filter(|value| !value.is_null()) {
        let value = value
            .as_str()
            .ok_or_else(|| format!("invalid input: `{name}` must be a string"))?;
        result.extend([flag.into(), value.into()]);
    }
    Ok(())
}

fn openpage_binary_path() -> OpenPageResult<PathBuf> {
    std::env::current_exe()
        .map_err(|err| OpenPageError::Io(format!("failed to resolve openpage binary path: {err}")))
}

fn run_cli(exe: &Path, args: &[String], timeout: Duration) -> OpenPageResult<CliOutcome> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| OpenPageError::Io(format!("failed to spawn openpage command: {err}")))?;

    let stdout = child.stdout.take().expect("piped stdout must be available");
    let stderr = child.stderr.take().expect("piped stderr must be available");
    let stdout_reader = thread::spawn(move || read_output(stdout));
    let stderr_reader = thread::spawn(move || read_output(stderr));
    let started = Instant::now();

    let (exit_code, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|err| {
            OpenPageError::Io(format!("failed to wait for openpage command: {err}"))
        })? {
            break (status.code(), false);
        }
        if started.elapsed() >= timeout {
            child.kill().map_err(|err| {
                OpenPageError::Io(format!("failed to kill timed out openpage command: {err}"))
            })?;
            let status = child.wait().map_err(|err| {
                OpenPageError::Io(format!("failed to reap timed out openpage command: {err}"))
            })?;
            break (status.code(), true);
        }
        thread::sleep(Duration::from_millis(25));
    };

    let stdout = join_output(stdout_reader, "stdout")?;
    let stderr = join_output(stderr_reader, "stderr")?;
    Ok(CliOutcome {
        stdout,
        stderr,
        exit_code,
        timed_out,
    })
}

fn read_output<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn join_output(
    handle: thread::JoinHandle<std::io::Result<String>>,
    stream: &str,
) -> OpenPageResult<String> {
    handle
        .join()
        .map_err(|_| OpenPageError::Io(format!("openpage {stream} reader thread panicked")))?
        .map_err(|err| OpenPageError::Io(format!("failed to read openpage {stream}: {err}")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;

    #[test]
    fn initialize_and_tools_list_use_current_structured_contract() {
        let state = McpState::new("default");
        let response = state
            .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .unwrap()
            .unwrap();
        assert_eq!(response["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(
            response["result"]["capabilities"]["resources"]["subscribe"],
            false
        );
        assert_eq!(
            response["result"]["capabilities"]["prompts"]["listChanged"],
            false
        );

        let response = state
            .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .unwrap()
            .unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        let names: HashSet<_> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 7);
        assert!(names.contains("screenshot"));
        assert!(tools.iter().all(|tool| tool["outputSchema"].is_object()));
        for name in ["snapshot", "navigate", "click", "fill"] {
            let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
            assert!(tool["inputSchema"]["properties"].get("session").is_none());
        }
        let navigate = tools
            .iter()
            .find(|tool| tool["name"] == "navigate")
            .unwrap();
        assert!(
            navigate["inputSchema"]["properties"]
                .get("wait_until")
                .is_some()
        );
        assert!(navigate["inputSchema"]["properties"].get("wait").is_none());
    }

    #[test]
    fn structured_results_preserve_success_and_diagnostic_errors() {
        let success = structured_tool_result(json!({
            "ok": true,
            "result": {"revision": "r_2", "count": 3, "refs": {}}
        }));
        assert_eq!(success["structuredContent"]["result"]["revision"], "r_2");
        assert_eq!(success["isError"], false);

        let error = structured_tool_result(json!({
            "ok": false,
            "error": {
                "kind": "stale_ref",
                "message": "stale",
                "retryable": true,
                "suggested_action": "re-snapshot",
                "current_revision": "r_2",
                "expected_revision": "r_1"
            }
        }));
        assert_eq!(error["structuredContent"]["error"]["kind"], "stale_ref");
        assert_eq!(
            error["structuredContent"]["error"]["suggested_action"],
            "re-snapshot"
        );
        assert_eq!(error["isError"], true);
    }

    #[test]
    fn help_topic_tokenization() {
        assert_eq!(build_help_topic_args(None).unwrap(), Vec::<String>::new());
        assert_eq!(
            build_help_topic_args(Some("  ")).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            build_help_topic_args(Some("browser start")).unwrap(),
            vec!["browser", "start"]
        );
    }

    #[test]
    fn openpage_requires_exactly_one_command_form() {
        assert!(build_openpage_args(&json!({}), "bound").is_err());
        assert!(
            build_openpage_args(
                &json!({"command": "snapshot", "commands": ["click @e2"]}),
                "bound"
            )
            .is_err()
        );
    }

    #[test]
    fn openpage_rejects_server_commands_and_defaults_batch_bail() {
        for command in ["mcp", "serve --port 9000"] {
            assert!(build_openpage_args(&json!({"command": command}), "bound").is_err());
            assert!(build_openpage_args(&json!({"commands": [command]}), "bound").is_err());
        }
        let args = build_openpage_args(&json!({"commands": ["snapshot"]}), "bound").unwrap();
        assert_eq!(&args[..2], ["batch", "--bail"]);
    }

    #[test]
    fn openpage_injects_session_only_when_absent() {
        assert_eq!(
            build_openpage_args(&json!({"command": "click @e2"}), "bound").unwrap(),
            vec!["click", "@e2", "--session", "bound"]
        );
        assert_eq!(
            build_openpage_args(&json!({"command": "click @e2 --session explicit"}), "bound")
                .unwrap(),
            vec!["click", "@e2", "--session", "explicit"]
        );
        assert_eq!(
            build_openpage_args(
                &json!({"command": "diff snapshot --before a --after b"}),
                "bound"
            )
            .unwrap(),
            vec!["diff", "snapshot", "--before", "a", "--after", "b"]
        );
    }

    #[test]
    fn typed_tool_argv_builders_bind_session_and_revision() {
        assert_eq!(
            build_snapshot_args(
                &json!({"mode": "semantic", "compact": true, "depth": 3, "selector": "main", "max_length": 1000, "offset": 2}),
                "bound"
            )
            .unwrap(),
            vec![
                "snapshot", "--format", "json", "--mode", "semantic", "--compact", "--depth",
                "3", "--selector", "main", "--max-output", "1000", "--offset", "2",
                "--session", "bound"
            ]
        );
        assert_eq!(
            build_navigate_args(
                &json!({"url": "https://example.com", "wait_until": "networkidle"}),
                "bound"
            )
            .unwrap(),
            vec![
                "goto",
                "https://example.com",
                "--wait",
                "--session",
                "bound"
            ]
        );
        assert_eq!(
            build_click_args(
                &json!({"locator": "@e2", "expected_revision": "r_2", "wait_until": "load"}),
                "bound"
            )
            .unwrap(),
            vec![
                "click",
                "@e2",
                "--expected-revision",
                "r_2",
                "--wait-navigation",
                "--session",
                "bound"
            ]
        );
        assert_eq!(
            build_fill_args(
                &json!({"locator": "#name", "text": "", "expected_revision": "r_3"}),
                "bound"
            )
            .unwrap(),
            vec![
                "fill",
                "#name",
                "",
                "--expected-revision",
                "r_3",
                "--session",
                "bound"
            ]
        );
    }

    #[test]
    fn screenshot_builder_selects_page_or_element_command() {
        assert_eq!(
            build_screenshot_args(
                &json!({"full_page": true, "output_path": "/tmp/page.png"}),
                "bound"
            )
            .unwrap(),
            vec![
                "screenshot",
                "--full-page",
                "--output",
                "/tmp/page.png",
                "--session",
                "bound"
            ]
        );
        assert_eq!(
            build_screenshot_args(
                &json!({"locator": "@e2", "output_path": "/tmp/element.png"}),
                "bound"
            )
            .unwrap(),
            vec![
                "screenshot-element",
                "@e2",
                "--output",
                "/tmp/element.png",
                "--session",
                "bound"
            ]
        );
    }

    #[test]
    fn resources_list_and_read_recorded_snapshot() {
        let state = McpState::new("session-a");
        let tool_result = state.record_capture(
            CaptureKind::Snapshot,
            structured_tool_result(json!({
                "ok": true,
                "result": {
                    "revision": "r_4",
                    "count": 1,
                    "snapshot": [{"ref": "@e1", "role": "button"}],
                    "refs": {"@e1": {"revision": "r_4"}}
                }
            })),
        );
        assert_eq!(tool_result["content"].as_array().unwrap().len(), 2);
        assert_eq!(tool_result["content"][1]["type"], "resource_link");

        let listed = state
            .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#)
            .unwrap()
            .unwrap();
        let resources = listed["result"]["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 1);
        let uri = resources[0]["uri"].as_str().unwrap();
        assert_eq!(uri, "openpage://sessions/session-a/captures/1");

        let read = state
            .handle_line(&format!(
                r#"{{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{{"uri":"{uri}"}}}}"#
            ))
            .unwrap()
            .unwrap();
        let content = &read["result"]["contents"][0];
        assert_eq!(content["mimeType"], "application/json");
        let captured: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
        assert_eq!(captured["revision"], "r_4");
        assert_eq!(captured["snapshot"][0]["ref"], "@e1");
    }

    #[test]
    fn screenshot_resources_are_read_as_base64_blobs() {
        let state = McpState::new("session-a");
        let path = temporary_screenshot_path();
        fs::write(&path, b"png-bytes").unwrap();
        let capture = state.captures.record(
            "session-a",
            CaptureCreate {
                kind: CaptureKind::Screenshot,
                target: None,
                revision: Some("r_5".to_string()),
                mime_type: "image/png".to_string(),
                content: None,
                file_path: Some(path.to_string_lossy().into_owned()),
                summary: "screenshot".to_string(),
            },
        );
        let content = capture_contents(&capture, &capture_uri(&capture)).unwrap();
        assert_eq!(content["mimeType"], "image/png");
        assert_eq!(content["blob"], BASE64_STANDARD.encode(b"png-bytes"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn prompts_list_and_get_return_three_nonempty_templates() {
        let state = McpState::new("default");
        let listed = state
            .handle_line(r#"{"jsonrpc":"2.0","id":4,"method":"prompts/list"}"#)
            .unwrap()
            .unwrap();
        let prompts = listed["result"]["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 3);
        assert_eq!(
            prompts
                .iter()
                .map(|prompt| prompt["name"].as_str().unwrap())
                .collect::<HashSet<_>>(),
            HashSet::from(["review_page", "collect_paginated_data", "guided_login"])
        );

        for (name, arguments) in [
            ("review_page", json!({"focus": "a11y"})),
            (
                "collect_paginated_data",
                json!({"goal": "titles", "max_pages": "3"}),
            ),
            ("guided_login", json!({"site": "example.test"})),
        ] {
            let response = state.get_prompt(
                Some(json!(5)),
                Some(&json!({"name": name, "arguments": arguments})),
            );
            assert!(response["result"]["messages"].as_array().unwrap().len() > 0);
            assert!(
                !response["result"]["messages"][0]["content"]["text"]
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn invalid_resource_and_prompt_arguments_return_json_rpc_errors() {
        let state = McpState::new("default");
        for response in [
            state.read_resource(Some(json!(1)), Some(&json!({"uri": "invalid"}))),
            state.list_resources(Some(json!(2)), Some(&json!({"cursor": "bad"}))),
            state.get_prompt(Some(json!(3)), Some(&json!({"name": "unknown"}))),
            state.get_prompt(
                Some(json!(4)),
                Some(&json!({"name": "collect_paginated_data", "arguments": {}})),
            ),
        ] {
            assert_eq!(response["error"]["code"], -32602);
        }
    }

    #[test]
    fn notifications_do_not_write_a_response() {
        let state = McpState::new("default");
        assert!(
            state
                .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .unwrap()
                .is_none()
        );
    }
}
