use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use tauri::State;

struct Session(String);

#[tauri::command]
fn recorder_call(session: State<'_, Session>, op: String, params: Value) -> Result<Value, String> {
    let home = std::env::var_os("OPENPAGE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".openpage")
        });
    let port = fs::read_to_string(home.join("daemon").join(format!("{}.port", session.0)))
        .map_err(|e| format!("无法连接 OpenPage daemon: {e}"))?
        .trim()
        .parse::<u16>()
        .map_err(|e| format!("无效 daemon 端口: {e}"))?;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let request = json!({"id": 1, "op": op, "target": session.0, "params": params});
    writeln!(stream, "{}", request).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let response: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if response["ok"] == true {
        Ok(response["result"].clone())
    } else {
        Err(response["error"]["message"]
            .as_str()
            .unwrap_or("daemon error")
            .to_string())
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(Session("default".to_string()))
        .invoke_handler(tauri::generate_handler![recorder_call])
        .run(tauri::generate_context!())
        .expect("error while running OpenPage desktop");
}
