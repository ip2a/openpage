use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::{OpenPageError, OpenPageResult};

const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(60);

struct CliOutcome {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
}

pub fn serve_stdio<R: BufRead, W: Write>(
    session: &str,
    input: R,
    mut output: W,
) -> OpenPageResult<()> {
    for line in input.lines() {
        let line = line.map_err(|err| OpenPageError::Io(err.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(session, &line)? {
            serde_json::to_writer(&mut output, &response)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

fn handle_line(session: &str, line: &str) -> OpenPageResult<Option<Value>> {
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
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "openpage", "version": env!("CARGO_PKG_VERSION")}
            }
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"tools": [
                {
                    "name": "help",
                    "description": "Show OpenPage CLI help. Omit `topic` for the top-level command catalog; pass a topic like \"click\" or \"browser start\" to get that command's full usage, flags, and examples exactly as `openpage <topic> --help` would print.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "topic": {
                                "type": "string",
                                "description": "Command or subcommand path, e.g. \"click\", \"browser start\", \"wait-for-navigation\". Omit for the top-level command catalog."
                            }
                        }
                    }
                },
                {
                    "name": "openpage",
                    "description": "Execute one or more OpenPage CLI commands against the bound session. Provide `command` for a single command, or `commands` for a sequence executed via `openpage batch`. `mcp` and `serve` are rejected. Use the `help` tool first if unsure of a command's syntax.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "string", "description": "A single OpenPage CLI command line, e.g. \"click @e2 --wait-navigation\". Session is injected automatically if `--session` is not present."},
                            "commands": {"type": "array", "items": {"type": "string"}, "description": "Multiple CLI command lines executed in order via `openpage batch`. Session is injected into each one automatically if absent."},
                            "bail": {"type": "boolean", "description": "Only used with `commands`: stop after the first failing command. Default false."}
                        }
                    }
                },
                {
                    "name": "snapshot",
                    "description": "Take an accessibility-style snapshot of the current page: refs, roles, names, and text. Equivalent to `openpage snapshot`.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "session": {"type": "string"},
                            "mode": {"type": "string", "enum": ["interactive", "semantic", "all"]},
                            "format": {"type": "string", "enum": ["text", "json"]},
                            "compact": {"type": "boolean"},
                            "raw": {"type": "boolean"},
                            "depth": {"type": "integer"},
                            "selector": {"type": "string"}
                        }
                    }
                },
                {
                    "name": "navigate",
                    "description": "Navigate the current page to a URL, bootstrapping the session if needed. Equivalent to `openpage goto`.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["url"],
                        "properties": {
                            "url": {"type": "string"},
                            "session": {"type": "string"},
                            "wait": {"type": "boolean"}
                        }
                    }
                },
                {
                    "name": "click",
                    "description": "Click an element by ref (e.g. \"@e2\") or CSS/text locator. Equivalent to `openpage click`.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["locator"],
                        "properties": {
                            "locator": {"type": "string"},
                            "session": {"type": "string"},
                            "wait_navigation": {"type": "boolean"}
                        }
                    }
                },
                {
                    "name": "fill",
                    "description": "Type text into an element by ref or locator. Equivalent to `openpage fill` (without `--stdin`, which is unavailable over MCP).",
                    "inputSchema": {
                        "type": "object",
                        "required": ["locator", "text"],
                        "properties": {
                            "locator": {"type": "string"},
                            "text": {"type": "string"},
                            "session": {"type": "string"}
                        }
                    }
                }
            ]}
        })),
        "tools/call" => Some(call_tool(session, id, request.get("params"))),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("method not found: {method}")}
        })),
    };
    Ok(response)
}

fn call_tool(session: &str, id: Option<Value>, params: Option<&Value>) -> Value {
    let name = params
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let args = params
        .and_then(|value| value.get("arguments"))
        .unwrap_or(&Value::Null);

    let result = match name {
        "help" => call_help(args),
        "openpage" => build_openpage_args(args, session).and_then(run_tool_args),
        "snapshot" => build_snapshot_args(args, session).and_then(run_tool_args),
        "navigate" => build_navigate_args(args, session).and_then(run_tool_args),
        "click" => build_click_args(args, session).and_then(run_tool_args),
        "fill" => build_fill_args(args, session).and_then(run_tool_args),
        _ => Err(format!("unknown tool: {name}")),
    };

    let tool_result = result.unwrap_or_else(tool_error);
    json!({"jsonrpc": "2.0", "id": id, "result": tool_result})
}

fn call_help(args: &Value) -> Result<Value, String> {
    let topic = args.get("topic").and_then(Value::as_str);
    let mut cli_args = build_help_topic_args(topic)?;
    cli_args.push("--help".into());
    let outcome = execute_cli(&cli_args)?;
    if outcome.timed_out {
        return Err("OpenPage help command timed out after 60 seconds".into());
    }
    let mut text = outcome.stdout;
    text.push_str(&outcome.stderr);
    Ok(tool_content(text, false))
}

fn run_tool_args(args: Vec<String>) -> Result<Value, String> {
    let outcome = execute_cli(&args)?;
    if outcome.timed_out {
        return Err("OpenPage command timed out after 60 seconds".into());
    }
    let failed_without_stdout = outcome.exit_code != Some(0) && outcome.stdout.is_empty();
    let text = if failed_without_stdout {
        outcome.stderr
    } else {
        outcome.stdout
    };
    Ok(tool_content(text, failed_without_stdout))
}

fn execute_cli(args: &[String]) -> Result<CliOutcome, String> {
    let exe = openpage_binary_path().map_err(|err| err.to_string())?;
    run_cli(&exe, args, TOOL_CALL_TIMEOUT).map_err(|err| err.to_string())
}

fn tool_content(text: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

fn tool_error(message: String) -> Value {
    tool_content(message, true)
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
            if args.get("bail").and_then(Value::as_bool) == Some(true) {
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
    inject_session(&mut tokens, session);
    Ok(tokens)
}

fn inject_session(args: &mut Vec<String>, session: &str) {
    if !args.iter().any(|arg| arg == "--session") {
        args.push("--session".into());
        args.push(session.into());
    }
}

fn selected_session(args: &Value, bound_session: &str) -> String {
    args.get("session")
        .and_then(Value::as_str)
        .unwrap_or(bound_session)
        .to_string()
}

fn build_snapshot_args(args: &Value, bound_session: &str) -> Result<Vec<String>, String> {
    let mut result = vec!["snapshot".into()];
    push_string_option(&mut result, args, "mode", "--mode")?;
    push_string_option(&mut result, args, "format", "--format")?;
    if args.get("compact").and_then(Value::as_bool) == Some(true) {
        result.push("--compact".into());
    }
    if args.get("raw").and_then(Value::as_bool) == Some(true) {
        result.push("--raw".into());
    }
    if let Some(depth) = args.get("depth").and_then(Value::as_i64) {
        result.extend(["--depth".into(), depth.to_string()]);
    }
    push_string_option(&mut result, args, "selector", "--selector")?;
    result.extend(["--session".into(), selected_session(args, bound_session)]);
    Ok(result)
}

fn build_navigate_args(args: &Value, bound_session: &str) -> Result<Vec<String>, String> {
    let url = required_nonempty_string(args, "url")?;
    let mut result = vec!["goto".into(), url.into()];
    if args.get("wait").and_then(Value::as_bool) == Some(true) {
        result.push("--wait".into());
    }
    result.extend(["--session".into(), selected_session(args, bound_session)]);
    Ok(result)
}

fn build_click_args(args: &Value, bound_session: &str) -> Result<Vec<String>, String> {
    let locator = required_nonempty_string(args, "locator")?;
    let mut result = vec!["click".into(), locator.into()];
    if args.get("wait_navigation").and_then(Value::as_bool) == Some(true) {
        result.push("--wait-navigation".into());
    }
    result.extend(["--session".into(), selected_session(args, bound_session)]);
    Ok(result)
}

fn build_fill_args(args: &Value, bound_session: &str) -> Result<Vec<String>, String> {
    let locator = required_nonempty_string(args, "locator")?;
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid input: `text` is required and must be a string".to_string())?;
    Ok(vec![
        "fill".into(),
        locator.into(),
        text.into(),
        "--session".into(),
        selected_session(args, bound_session),
    ])
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

    use super::{
        build_click_args, build_fill_args, build_help_topic_args, build_navigate_args,
        build_openpage_args, build_snapshot_args, handle_line,
    };

    #[test]
    fn initialize_and_tools_list_are_machine_readable() {
        let response = handle_line(
            "default",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(response["result"]["serverInfo"]["name"], "openpage");

        let response = handle_line(
            "default",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .unwrap()
        .unwrap();
        let names: HashSet<_> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 6);
        assert!(names.contains("help"));
        assert!(names.contains("openpage"));
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
    fn openpage_rejects_server_commands() {
        for command in ["mcp", "serve --port 9000"] {
            assert!(build_openpage_args(&json!({"command": command}), "bound").is_err());
            assert!(build_openpage_args(&json!({"commands": [command]}), "bound").is_err());
        }
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
    }

    #[test]
    fn batch_commands_roundtrip_after_session_injection() {
        let args = build_openpage_args(
            &json!({"commands": ["fill @e2 'value with space'", "snapshot"], "bail": true}),
            "bound",
        )
        .unwrap();
        assert_eq!(&args[..2], ["batch", "--bail"]);
        assert_eq!(
            shlex::split(&args[2]).unwrap(),
            vec!["fill", "@e2", "value with space", "--session", "bound"]
        );
        assert_eq!(
            shlex::split(&args[3]).unwrap(),
            vec!["snapshot", "--session", "bound"]
        );
    }

    #[test]
    fn typed_tool_argv_builders() {
        assert_eq!(
            build_snapshot_args(
                &json!({"mode": "semantic", "format": "json", "compact": true, "raw": true, "depth": 3, "selector": "main", "session": "explicit"}),
                "bound"
            )
            .unwrap(),
            vec![
                "snapshot", "--mode", "semantic", "--format", "json", "--compact", "--raw",
                "--depth", "3", "--selector", "main", "--session", "explicit"
            ]
        );
        assert_eq!(
            build_snapshot_args(&json!({}), "bound").unwrap(),
            vec!["snapshot", "--session", "bound"]
        );
        assert_eq!(
            build_navigate_args(
                &json!({"url": "https://example.com", "wait": true}),
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
            build_click_args(&json!({"locator": "@e2", "session": "explicit"}), "bound").unwrap(),
            vec!["click", "@e2", "--session", "explicit"]
        );
        assert_eq!(
            build_fill_args(&json!({"locator": "#name", "text": ""}), "bound").unwrap(),
            vec!["fill", "#name", "", "--session", "bound"]
        );
    }

    #[test]
    fn notifications_do_not_write_a_response() {
        assert!(
            handle_line(
                "default",
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .unwrap()
            .is_none()
        );
    }
}
