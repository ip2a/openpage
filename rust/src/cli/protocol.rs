use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<Value>,
    pub op: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Serialize)]
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
