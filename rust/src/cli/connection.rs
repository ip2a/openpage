#![allow(dead_code)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use crate::cli::protocol::{Request, Response};
use crate::error::{OpenPageError, OpenPageResult};

const CONNECT_TIMEOUT_MS: u64 = 200;
const READ_TIMEOUT_SECS: u64 = 30;
const WRITE_TIMEOUT_SECS: u64 = 5;
const MAX_RETRIES: u32 = 3;
const STARTUP_POLL_ATTEMPTS: u32 = 30;
const STARTUP_POLL_DELAY_MS: u64 = 100;

pub(crate) fn openpage_home() -> OpenPageResult<PathBuf> {
    if let Some(value) = std::env::var_os("OPENPAGE_HOME") {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(value).join(".openpage"));
    }
    Err(OpenPageError::Io(
        "OPENPAGE_HOME or HOME must be set".to_string(),
    ))
}

pub(crate) fn daemon_dir() -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?.join("daemon"))
}

pub(crate) fn port_path(session: &str) -> OpenPageResult<PathBuf> {
    Ok(daemon_dir()?.join(format!("{session}.port")))
}

pub(crate) fn pid_path(session: &str) -> OpenPageResult<PathBuf> {
    Ok(daemon_dir()?.join(format!("{session}.pid")))
}

pub(crate) fn version_path(session: &str) -> OpenPageResult<PathBuf> {
    Ok(daemon_dir()?.join(format!("{session}.version")))
}

pub(crate) fn log_path(session: &str) -> OpenPageResult<PathBuf> {
    Ok(daemon_dir()?.join(format!("{session}.log")))
}

pub(crate) fn write_tcp_sidecars(session: &str, port: u16) -> OpenPageResult<SidecarGuard> {
    let dir = daemon_dir()?;
    fs::create_dir_all(&dir)?;

    let port_path = port_path(session)?;
    let pid_path = pid_path(session)?;
    let version_path = version_path(session)?;

    fs::write(&port_path, port.to_string())?;
    fs::write(&pid_path, std::process::id().to_string())?;
    fs::write(&version_path, env!("CARGO_PKG_VERSION"))?;

    Ok(SidecarGuard {
        paths: vec![port_path, pid_path, version_path],
    })
}

pub(crate) fn cleanup_sidecars(session: &str) -> OpenPageResult<()> {
    for path in [
        port_path(session)?,
        pid_path(session)?,
        version_path(session)?,
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(OpenPageError::Io(err.to_string())),
        }
    }
    Ok(())
}

pub(crate) fn read_port(session: &str) -> OpenPageResult<Option<u16>> {
    let path = port_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => content
            .trim()
            .parse::<u16>()
            .map(Some)
            .map_err(|err| OpenPageError::Io(format!("invalid daemon port file: {err}"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

fn read_pid(session: &str) -> OpenPageResult<Option<u32>> {
    let path = pid_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => content
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|err| OpenPageError::Io(format!("invalid daemon pid file: {err}"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

pub(crate) fn read_version(session: &str) -> OpenPageResult<Option<String>> {
    let path = version_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => {
            let version = content.trim().to_string();
            if version.is_empty() {
                Ok(None)
            } else {
                Ok(Some(version))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DaemonSessionInfo {
    pub session: String,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub version: Option<String>,
    pub alive: bool,
    pub ready: bool,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct IncompleteDaemonSession {
    pub session: String,
    pub pid_present: bool,
    pub port_present: bool,
    pub version_present: bool,
    pub pid_valid: bool,
    pub port_valid: bool,
    pub alive: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CleanedDaemonSession {
    pub session: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct DaemonInventory {
    pub sessions: Vec<DaemonSessionInfo>,
    pub incomplete: Vec<IncompleteDaemonSession>,
    pub cleaned: Vec<CleanedDaemonSession>,
}

enum SidecarState<T> {
    Missing,
    Present(T),
    Invalid,
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn daemon_version_matches(session: &str) -> bool {
    let Ok(path) = version_path(session) else {
        return false;
    };
    match fs::read_to_string(path) {
        Ok(content) => content.trim() == env!("CARGO_PKG_VERSION"),
        Err(_) => false,
    }
}

fn kill_stale_daemon(session: &str) -> OpenPageResult<()> {
    if let Some(pid) = read_pid(session)? {
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            std::thread::sleep(Duration::from_millis(300));
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    cleanup_sidecars(session)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingDaemonAction {
    Reuse,
    Kill,
    Ignore,
}

fn wait_for_daemon_ready(session: &str, attempts: u32, delay_ms: u64) -> bool {
    for _ in 0..attempts {
        if daemon_ready(session) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    daemon_ready(session)
}

fn existing_daemon_action_with_retry(
    session: &str,
    attempts: u32,
    delay_ms: u64,
) -> OpenPageResult<ExistingDaemonAction> {
    if daemon_ready(session) {
        return Ok(if daemon_version_matches(session) {
            ExistingDaemonAction::Reuse
        } else {
            ExistingDaemonAction::Kill
        });
    }

    let pid_alive = read_pid(session)?.map(is_pid_alive).unwrap_or(false);
    if !pid_alive {
        return Ok(ExistingDaemonAction::Ignore);
    }

    if wait_for_daemon_ready(session, attempts, delay_ms) {
        return Ok(if daemon_version_matches(session) {
            ExistingDaemonAction::Reuse
        } else {
            ExistingDaemonAction::Kill
        });
    }

    Ok(ExistingDaemonAction::Kill)
}

fn existing_daemon_action(session: &str) -> OpenPageResult<ExistingDaemonAction> {
    existing_daemon_action_with_retry(session, STARTUP_POLL_ATTEMPTS, STARTUP_POLL_DELAY_MS)
}

pub(crate) fn daemon_ready(session: &str) -> bool {
    let Ok(Some(port)) = read_port(session) else {
        return false;
    };
    let socket = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&socket, Duration::from_millis(CONNECT_TIMEOUT_MS)).is_ok()
}

pub(crate) fn daemon_status(session: &str) -> OpenPageResult<DaemonSessionInfo> {
    let port = read_port(session)?;
    let pid = read_pid(session)?;
    let version = read_version(session)?;
    let ready = daemon_ready(session);
    let pid_alive = pid.map(is_pid_alive).unwrap_or(false);
    Ok(DaemonSessionInfo {
        session: session.to_string(),
        port,
        pid,
        version,
        alive: ready || pid_alive,
        ready,
        log_path: log_path(session)?.display().to_string(),
    })
}

pub(crate) fn list_daemons() -> OpenPageResult<Vec<DaemonSessionInfo>> {
    Ok(daemon_inventory()?.sessions)
}

pub(crate) fn daemon_inventory() -> OpenPageResult<DaemonInventory> {
    let dir = daemon_dir()?;
    if !dir.exists() {
        return Ok(DaemonInventory::default());
    }

    let mut sessions = std::collections::BTreeSet::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        for suffix in [".port", ".pid", ".version"] {
            if let Some(session) = name.strip_suffix(suffix) {
                if !session.is_empty() {
                    sessions.insert(session.to_string());
                }
            }
        }
    }

    let mut inventory = DaemonInventory::default();
    for session in sessions {
        let pid_state = pid_sidecar_state(&session)?;
        let port_state = port_sidecar_state(&session)?;
        let version_state = version_sidecar_state(&session)?;

        let pid_value = match &pid_state {
            SidecarState::Present(value) => Some(*value),
            _ => None,
        };
        let port_value = match &port_state {
            SidecarState::Present(value) => Some(*value),
            _ => None,
        };

        let ready = match port_value {
            Some(port) => {
                let socket = SocketAddr::from(([127, 0, 0, 1], port));
                TcpStream::connect_timeout(&socket, Duration::from_millis(CONNECT_TIMEOUT_MS))
                    .is_ok()
            }
            None => false,
        };
        let pid_alive = pid_value.map(is_pid_alive).unwrap_or(false);
        let alive = ready || pid_alive;

        let pid_present = !matches!(pid_state, SidecarState::Missing);
        let port_present = !matches!(port_state, SidecarState::Missing);
        let version_present = !matches!(version_state, SidecarState::Missing);
        let pid_valid = !matches!(pid_state, SidecarState::Invalid);
        let port_valid = !matches!(port_state, SidecarState::Invalid);
        let complete = pid_present && port_present && version_present && pid_valid && port_valid;

        if alive && complete {
            inventory.sessions.push(DaemonSessionInfo {
                session: session.clone(),
                port: port_value,
                pid: pid_value,
                version: match version_state {
                    SidecarState::Present(version) => Some(version),
                    SidecarState::Missing | SidecarState::Invalid => None,
                },
                alive,
                ready,
                log_path: log_path(&session)?.display().to_string(),
            });
        } else if alive {
            inventory.incomplete.push(IncompleteDaemonSession {
                session: session.clone(),
                pid_present,
                port_present,
                version_present,
                pid_valid,
                port_valid,
                alive,
                ready,
            });
        } else {
            let _ = cleanup_sidecars(&session);
            inventory.cleaned.push(CleanedDaemonSession {
                session: session.clone(),
                reason: cleaned_reason(&pid_state, &port_state, version_present),
            });
        }
    }

    Ok(inventory)
}

fn pid_sidecar_state(session: &str) -> OpenPageResult<SidecarState<u32>> {
    let path = pid_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => match content.trim().parse::<u32>() {
            Ok(value) => Ok(SidecarState::Present(value)),
            Err(_) => Ok(SidecarState::Invalid),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SidecarState::Missing),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

fn port_sidecar_state(session: &str) -> OpenPageResult<SidecarState<u16>> {
    let path = port_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => match content.trim().parse::<u16>() {
            Ok(value) => Ok(SidecarState::Present(value)),
            Err(_) => Ok(SidecarState::Invalid),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SidecarState::Missing),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

fn version_sidecar_state(session: &str) -> OpenPageResult<SidecarState<String>> {
    let path = version_path(session)?;
    match fs::read_to_string(&path) {
        Ok(content) => {
            let version = content.trim().to_string();
            if version.is_empty() {
                Ok(SidecarState::Missing)
            } else {
                Ok(SidecarState::Present(version))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SidecarState::Missing),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

fn cleaned_reason(
    pid_state: &SidecarState<u32>,
    port_state: &SidecarState<u16>,
    version_present: bool,
) -> String {
    let mut problems = Vec::new();
    match pid_state {
        SidecarState::Missing => problems.push("missing pid"),
        SidecarState::Invalid => problems.push("invalid pid"),
        SidecarState::Present(_) => {}
    }
    match port_state {
        SidecarState::Missing => problems.push("missing port"),
        SidecarState::Invalid => problems.push("invalid port"),
        SidecarState::Present(_) => {}
    }
    if !version_present {
        problems.push("missing version");
    }
    if problems.is_empty() {
        "not alive".to_string()
    } else {
        problems.join(", ")
    }
}

pub(crate) fn ensure_daemon(session: &str) -> OpenPageResult<DaemonStatus> {
    match existing_daemon_action(session)? {
        ExistingDaemonAction::Reuse => {
            return Ok(DaemonStatus {
                already_running: true,
            });
        }
        ExistingDaemonAction::Kill => {
            kill_stale_daemon(session)?;
        }
        ExistingDaemonAction::Ignore => {}
    }

    cleanup_sidecars(session)?;

    let dir = daemon_dir()?;
    fs::create_dir_all(&dir)?;
    let log_path = dir.join(format!("{session}.log"));

    let exe = std::env::current_exe().map_err(|err| OpenPageError::Io(err.to_string()))?;
    let mut command = Command::new(exe);
    command
        .arg("serve")
        .arg("--port")
        .arg("0")
        .arg("--session")
        .arg(session)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&log_path)?));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|err| OpenPageError::Io(format!("failed to start daemon: {err}")))?;

    for _ in 0..STARTUP_POLL_ATTEMPTS {
        std::thread::sleep(Duration::from_millis(STARTUP_POLL_DELAY_MS));
        if daemon_ready(session) {
            return Ok(DaemonStatus {
                already_running: false,
            });
        }
        if let Ok(Some(_)) = child.try_wait() {
            let log = fs::read_to_string(&log_path).unwrap_or_default();
            let log = log.trim();
            if log.is_empty() {
                return Err(OpenPageError::Io(format!(
                    "daemon for session '{session}' exited during startup"
                )));
            }
            return Err(OpenPageError::Io(format!(
                "daemon for session '{session}' exited during startup: {log}"
            )));
        }
    }

    Err(OpenPageError::Io(format!(
        "daemon for session '{session}' failed to become ready; see {}",
        log_path.display()
    )))
}

pub(crate) fn send_request(session: &str, request: &Request) -> OpenPageResult<Response> {
    send_request_with_retry(
        session,
        request,
        |session| ensure_daemon(session).map(|_| ()),
        send_request_once,
    )
}

fn send_request_once(session: &str, request: &Request) -> OpenPageResult<Response> {
    let port = read_port(session)?.ok_or_else(|| {
        OpenPageError::Io(format!("daemon port not found for session '{session}'"))
    })?;
    let socket = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream =
        TcpStream::connect_timeout(&socket, Duration::from_millis(CONNECT_TIMEOUT_MS))?;
    stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECS)))?;

    let mut payload = serde_json::to_string(request)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    payload.push('\n');
    stream.write_all(payload.as_bytes())?;
    stream.flush()?;

    let mut response_line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut response_line)?;
    if response_line.trim().is_empty() {
        return Err(OpenPageError::Io("empty daemon response".to_string()));
    }

    serde_json::from_str(&response_line)
        .map_err(|err| OpenPageError::Serialization(format!("invalid daemon response: {err}")))
}

fn is_transient_error(error: &OpenPageError) -> bool {
    let message = error.to_string();
    message.contains("Connection refused")
        || message.contains("connection refused")
        || message.contains("Connection reset")
        || message.contains("connection reset")
        || message.contains("Broken pipe")
        || message.contains("broken pipe")
        || message.contains("WouldBlock")
        || message.contains("would block")
        || message.contains("Resource temporarily unavailable")
        || message.contains("No such file or directory")
        || message.contains("os error 11")
        || message.contains("os error 35")
        || message.contains("os error 54")
        || message.contains("os error 61")
        || message.contains("os error 104")
        || message.contains("os error 111")
        || message.contains("os error 10054")
        || message.contains("os error 10061")
        || message.contains("timed out")
        || message.contains("empty daemon response")
}

fn send_request_with_retry<F, G>(
    session: &str,
    request: &Request,
    mut ensure: F,
    mut send_once: G,
) -> OpenPageResult<Response>
where
    F: FnMut(&str) -> OpenPageResult<()>,
    G: FnMut(&str, &Request) -> OpenPageResult<Response>,
{
    let mut last_error: Option<OpenPageError> = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100 * attempt as u64));
        }

        ensure(session)?;

        match send_once(session, request) {
            Ok(response) => return Ok(response),
            Err(err) if is_transient_error(&err) => {
                last_error = Some(err);
                continue;
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        OpenPageError::Io("daemon request failed with no captured error".to_string())
    }))
}

pub(crate) struct SidecarGuard {
    paths: Vec<PathBuf>,
}

pub(crate) struct DaemonStatus {
    pub already_running: bool,
}

impl Drop for SidecarGuard {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExistingDaemonAction, daemon_dir, existing_daemon_action_with_retry, pid_path, port_path,
        send_request_with_retry, version_path,
    };
    use crate::cli::protocol::{Request, Response};
    use crate::error::OpenPageError;
    use serde_json::{Value, json};
    use std::fs;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &PathBuf) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn unique_openpage_home(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openpage-connection-test-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn existing_daemon_action_reuses_ready_matching_daemon() {
        let home = unique_openpage_home("reuse");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener addr").port();
        let session = "reuse";

        fs::write(port_path(session).expect("port path"), port.to_string()).expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            std::process::id().to_string(),
        )
        .expect("write pid");
        fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        let action =
            existing_daemon_action_with_retry(session, 0, 0).expect("existing daemon action");
        assert_eq!(action, ExistingDaemonAction::Reuse);

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn existing_daemon_action_kills_ready_version_mismatch() {
        let home = unique_openpage_home("mismatch");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener addr").port();
        let session = "mismatch";

        fs::write(port_path(session).expect("port path"), port.to_string()).expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            std::process::id().to_string(),
        )
        .expect("write pid");
        fs::write(version_path(session).expect("version path"), "0.0.1").expect("write version");

        let action =
            existing_daemon_action_with_retry(session, 0, 0).expect("existing daemon action");
        assert_eq!(action, ExistingDaemonAction::Kill);

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn existing_daemon_action_kills_alive_unready_daemon_after_grace() {
        let home = unique_openpage_home("stale");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let session = "stale";
        fs::write(port_path(session).expect("port path"), "9").expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            std::process::id().to_string(),
        )
        .expect("write pid");
        fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        let action =
            existing_daemon_action_with_retry(session, 1, 0).expect("existing daemon action");
        assert_eq!(action, ExistingDaemonAction::Kill);

        let _ = fs::remove_dir_all(home);
    }

    fn test_request() -> Request {
        Request {
            id: Some(json!("test")),
            op: "daemon.status".to_string(),
            target: None,
            params: Value::Null,
        }
    }

    #[test]
    fn send_request_with_retry_reensures_after_transient_error() {
        let request = test_request();
        let mut ensure_calls = 0;
        let mut send_calls = 0;

        let response = send_request_with_retry(
            "retry-session",
            &request,
            |_| {
                ensure_calls += 1;
                Ok(())
            },
            |_, _| {
                send_calls += 1;
                if send_calls == 1 {
                    Err(OpenPageError::Io("connection reset by peer".to_string()))
                } else {
                    Ok(Response::ok(
                        Some(json!("test")),
                        json!({"recovered": true}),
                    ))
                }
            },
        )
        .expect("recover after transient error");

        assert_eq!(ensure_calls, 2);
        assert_eq!(send_calls, 2);
        assert_eq!(response.result, Some(json!({"recovered": true})));
    }

    #[test]
    fn send_request_with_retry_stops_after_non_transient_error() {
        let request = test_request();
        let mut ensure_calls = 0;
        let mut send_calls = 0;

        let error = send_request_with_retry(
            "non-transient-session",
            &request,
            |_| {
                ensure_calls += 1;
                Ok(())
            },
            |_, _| {
                send_calls += 1;
                Err(OpenPageError::Serialization(
                    "invalid daemon response".to_string(),
                ))
            },
        )
        .expect_err("stop after non-transient error");

        assert_eq!(ensure_calls, 1);
        assert_eq!(send_calls, 1);
        assert!(matches!(error, OpenPageError::Serialization(_)));
    }
}
