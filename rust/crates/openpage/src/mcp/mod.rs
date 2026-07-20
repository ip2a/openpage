use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::daemon::client::send_request;
use crate::error::{OpenPageError, OpenPageResult};
use crate::protocol::Request;

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
            "result": {"tools": [{
                "name": "openpage",
                "description": "Execute an OpenPage daemon protocol operation.",
                "inputSchema": {
                    "type": "object",
                    "required": ["op"],
                    "properties": {
                        "op": {"type": "string"},
                        "target": {"type": "string"},
                        "params": {"type": "object"}
                    }
                }
            }]}
        })),
        "tools/call" => Some(call_tool(session, id, request.get("params"))?),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("method not found: {method}")}
        })),
    };
    Ok(response)
}

fn call_tool(session: &str, id: Option<Value>, params: Option<&Value>) -> OpenPageResult<Value> {
    let args = params
        .and_then(|value| value.get("arguments"))
        .unwrap_or(&Value::Null);
    let op = args.get("op").and_then(Value::as_str).ok_or_else(|| {
        OpenPageError::UnsupportedOperation("MCP openpage tool requires string `op`".into())
    })?;
    let request = Request {
        id: Some(json!(id.clone().unwrap_or(Value::Null))),
        op: op.to_string(),
        target: args
            .get("target")
            .and_then(Value::as_str)
            .map(str::to_owned),
        params: args.get("params").cloned().unwrap_or_else(|| json!({})),
    };
    let result = send_request(session, &request)?;
    let payload = serde_json::to_string(&result)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": payload}],
            "isError": !result.ok
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::handle_line;

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
        assert_eq!(response["result"]["tools"][0]["name"], "openpage");
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
