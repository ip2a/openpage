#![allow(dead_code)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

#[cfg(windows)]
use windows_sys::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use crate::cli::protocol::{Request, Response};
use crate::error::{OpenPageError, OpenPageResult};

const CONNECT_TIMEOUT_MS: u64 = 200;
const READ_TIMEOUT_SECS: u64 = 30;
const WRITE_TIMEOUT_SECS: u64 = 5;
const MAX_RETRIES: u32 = 3;
const STARTUP_POLL_ATTEMPTS: u32 = 30;
const STARTUP_POLL_DELAY_MS: u64 = 100;
const SHUTDOWN_POLL_ATTEMPTS: u32 = 20;
const SHUTDOWN_POLL_DELAY_MS: u64 = 100;

pub(crate) fn openpage_home() -> OpenPageResult<PathBuf> {
    crate::config::openpage_home()
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
    pub log_path: String,
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

pub(crate) fn incomplete_daemon_reasons(
    incomplete: &IncompleteDaemonSession,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !incomplete.pid_present {
        reasons.push("missing_pid");
    } else if !incomplete.pid_valid {
        reasons.push("invalid_pid");
    }
    if !incomplete.port_present {
        reasons.push("missing_port");
    } else if !incomplete.port_valid {
        reasons.push("invalid_port");
    }
    if !incomplete.version_present {
        reasons.push("missing_version");
    }
    if incomplete.alive && !incomplete.ready {
        reasons.push("daemon_not_ready");
    }
    reasons
}

fn version_matches_current_cli(version: Option<&str>) -> bool {
    matches!(version, Some(value) if value == env!("CARGO_PKG_VERSION"))
}

pub(crate) fn daemon_session_state(session: &DaemonSessionInfo) -> &'static str {
    if version_matches_current_cli(session.version.as_deref()) {
        "healthy"
    } else {
        "incompatible"
    }
}

pub(crate) fn daemon_session_reasons(session: &DaemonSessionInfo) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !version_matches_current_cli(session.version.as_deref()) {
        reasons.push("version_mismatch");
    }
    reasons
}

pub(crate) fn daemon_session_fix(session: &DaemonSessionInfo) -> Option<String> {
    if !version_matches_current_cli(session.version.as_deref()) {
        return Some(format!(
            "Run `openpage browser stop --session {0}` and then restart that session with the current CLI so its daemon sidecars are recreated with version {1}. Or run `openpage doctor --quick --fix` if you want the CLI to stop the stale daemon for you.",
            session.session,
            env!("CARGO_PKG_VERSION")
        ));
    }

    if !session.ready {
        return Some(format!(
            "Run `openpage browser status --session {0}` and inspect {1}. If the daemon is stale or unhealthy, stop it with `openpage browser stop --session {0}` and restart it.",
            session.session, session.log_path
        ));
    }

    None
}

pub(crate) fn incomplete_daemon_fix(incomplete: &IncompleteDaemonSession) -> String {
    if incomplete.ready {
        format!(
            "Run `openpage browser status --session {0}` to inspect the session. If it is no longer needed, run `openpage browser stop --session {0}` and rerun `openpage doctor --quick`.",
            incomplete.session
        )
    } else {
        format!(
            "Run `openpage doctor --quick --fix` to stop and clean this incomplete unready daemon session, or run `openpage browser stop --session {0}` yourself if you want explicit control.",
            incomplete.session
        )
    }
}

fn inactive_daemon_fix(session: &str) -> String {
    format!(
        "Start it with `openpage browser start --session {session}` before retrying."
    )
}

fn alive_but_unready_fix(session: &str) -> String {
    format!(
        "Run `openpage browser status --session {0}` and inspect the daemon. If it is stale or no longer needed, stop it with `openpage browser stop --session {0}` before retrying.",
        session
    )
}

fn daemon_status_fix(
    status: &DaemonSessionInfo,
    inventory: &DaemonInventory,
) -> Option<String> {
    if let Some(incomplete) = inventory
        .incomplete
        .iter()
        .find(|entry| entry.session == status.session)
    {
        return Some(incomplete_daemon_fix(incomplete));
    }

    if inventory
        .sessions
        .iter()
        .any(|entry| entry.session == status.session)
    {
        return daemon_session_fix(status);
    }

    if status.alive {
        return Some(alive_but_unready_fix(&status.session));
    }

    Some(inactive_daemon_fix(&status.session))
}

pub(crate) fn daemon_inventory_summary_json(inventory: &DaemonInventory) -> Value {
    let healthy = inventory
        .sessions
        .iter()
        .filter(|session| daemon_session_state(session) == "healthy")
        .count();
    let incompatible = inventory.sessions.len().saturating_sub(healthy);
    json!({
        "healthy": healthy,
        "incompatible": incompatible,
        "incomplete": inventory.incomplete.len(),
        "cleaned": inventory.cleaned.len(),
        "total": inventory.sessions.len() + inventory.incomplete.len() + inventory.cleaned.len(),
    })
}

pub(crate) fn daemon_inventory_payload_json(inventory: &DaemonInventory) -> Value {
    json!({
        "summary": daemon_inventory_summary_json(inventory),
        "sessions": inventory.sessions.iter().map(|session| {
            let state = daemon_session_state(session);
            let reasons = daemon_session_reasons(session);
            let mut entry = json!({
                "session": session.session,
                "port": session.port,
                "pid": session.pid,
                "version": session.version,
                "version_matches_current_cli": version_matches_current_cli(session.version.as_deref()),
                "alive": session.alive,
                "ready": session.ready,
                "log_path": session.log_path,
                "state": state,
                "reasons": reasons,
            });
            if let Some(fix) = daemon_session_fix(session) {
                entry["fix"] = json!(fix);
            }
            entry
        }).collect::<Vec<_>>(),
        "incomplete": inventory.incomplete.iter().map(|session| {
            let mut entry = json!({
                "session": session.session,
                "pid_present": session.pid_present,
                "port_present": session.port_present,
                "version_present": session.version_present,
                "pid_valid": session.pid_valid,
                "port_valid": session.port_valid,
                "alive": session.alive,
                "ready": session.ready,
                "log_path": session.log_path,
                "state": "incomplete",
                "reasons": incomplete_daemon_reasons(session),
            });
            entry["fix"] = json!(incomplete_daemon_fix(session));
            entry
        }).collect::<Vec<_>>(),
        "cleaned": inventory.cleaned.iter().map(|session| {
            json!({
                "session": session.session,
                "reason": session.reason,
                "state": "cleaned",
            })
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn daemon_status_payload_json(
    status: &DaemonSessionInfo,
    inventory: &DaemonInventory,
) -> Value {
    let mut payload = json!(status);

    if let Some(incomplete) = inventory
        .incomplete
        .iter()
        .find(|entry| entry.session == status.session)
    {
        payload["state"] = Value::from("incomplete");
        payload["incomplete"] = json!(incomplete);
        payload["reasons"] = json!(incomplete_daemon_reasons(incomplete));
        if let Some(fix) = daemon_status_fix(status, inventory) {
            payload["fix"] = json!(fix);
        }
        return payload;
    }

    if inventory
        .sessions
        .iter()
        .any(|entry| entry.session == status.session)
    {
        let state = daemon_session_state(status);
        let reasons = daemon_session_reasons(status);
        payload["version_matches_current_cli"] =
            json!(version_matches_current_cli(status.version.as_deref()));
        payload["state"] = Value::from(state);
        if !reasons.is_empty() {
            payload["reasons"] = json!(reasons);
        }
        if let Some(fix) = daemon_status_fix(status, inventory) {
            payload["fix"] = json!(fix);
        }
        return payload;
    }

    payload["state"] = Value::from(if status.alive { "incomplete" } else { "inactive" });
    if status.alive {
        payload["reasons"] = json!(["daemon_not_ready"]);
    }
    if let Some(fix) = daemon_status_fix(status, inventory) {
        payload["fix"] = json!(fix);
    }
    payload
}

pub(crate) fn daemon_status_payload_for_session(session: &str) -> OpenPageResult<Value> {
    let status = daemon_status(session)?;
    let inventory = daemon_inventory()?;
    Ok(daemon_status_payload_json(&status, &inventory))
}

enum SidecarState<T> {
    Missing,
    Present(T),
    Invalid,
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
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
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(300));
            if is_pid_alive(pid) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DaemonShutdown {
    pub had_daemon: bool,
    pub forced: bool,
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
                log_path: log_path(&session)?.display().to_string(),
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

pub(crate) fn ensure_existing_daemon(session: &str) -> OpenPageResult<()> {
    let status = daemon_status(session)?;
    let inventory = daemon_inventory()?;
    if status.ready {
        if !version_matches_current_cli(status.version.as_deref()) {
            let fix = daemon_status_fix(&status, &inventory)
                .unwrap_or_else(|| daemon_session_fix(&status).unwrap_or_default());
            return Err(OpenPageError::BrowserOperation(format!(
                "session `{session}` is backed by daemon version {} but the current CLI expects {}. {}",
                status.version.as_deref().unwrap_or("<missing>"),
                env!("CARGO_PKG_VERSION"),
                fix
            )));
        }
        return Ok(());
    }

    if status.alive {
        let fix = daemon_status_fix(&status, &inventory)
            .unwrap_or_else(|| alive_but_unready_fix(session));
        return Err(OpenPageError::BrowserOperation(format!(
            "session `{session}` exists but its daemon is not ready. {}",
            fix
        )));
    }

    let fix = daemon_status_fix(&status, &inventory).unwrap_or_else(|| inactive_daemon_fix(session));
    Err(OpenPageError::BrowserOperation(format!(
        "session `{session}` is not active. {}",
        fix
    )))
}

pub(crate) fn force_cleanup_daemon(session: &str) -> OpenPageResult<()> {
    kill_stale_daemon(session)
}

pub(crate) fn send_request(session: &str, request: &Request) -> OpenPageResult<Response> {
    send_request_with_retry(
        session,
        request,
        |session| ensure_daemon(session).map(|_| ()),
        send_request_once,
    )
}

pub(crate) fn send_request_existing(session: &str, request: &Request) -> OpenPageResult<Response> {
    send_request_with_retry(
        session,
        request,
        |session| ensure_existing_daemon(session),
        send_request_once,
    )
}

pub(crate) fn shutdown_daemon(session: &str) -> OpenPageResult<DaemonShutdown> {
    let status = daemon_status(session)?;
    let had_daemon = status.alive;

    if status.ready {
        let _ = send_request_once(
            session,
            &Request {
                id: None,
                op: "daemon.shutdown".to_string(),
                target: None,
                params: Value::Null,
            },
        );
    }

    if wait_for_daemon_exit(session, SHUTDOWN_POLL_ATTEMPTS, SHUTDOWN_POLL_DELAY_MS)? {
        cleanup_sidecars(session)?;
        return Ok(DaemonShutdown {
            had_daemon,
            forced: false,
        });
    }

    if had_daemon {
        kill_stale_daemon(session)?;
        return Ok(DaemonShutdown {
            had_daemon: true,
            forced: true,
        });
    }

    cleanup_sidecars(session)?;
    Ok(DaemonShutdown {
        had_daemon: false,
        forced: false,
    })
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

fn wait_for_daemon_exit(session: &str, attempts: u32, delay_ms: u64) -> OpenPageResult<bool> {
    for _ in 0..attempts {
        if !daemon_alive(session)? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    daemon_alive(session).map(|alive| !alive)
}

fn daemon_alive(session: &str) -> OpenPageResult<bool> {
    let ready = daemon_ready(session);
    let pid_alive = read_pid(session)?.map(is_pid_alive).unwrap_or(false);
    Ok(ready || pid_alive)
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
        ExistingDaemonAction, daemon_dir, daemon_inventory_payload_json,
        daemon_inventory_summary_json, ensure_existing_daemon, existing_daemon_action_with_retry,
        incomplete_daemon_reasons, pid_path, port_path, read_pid, send_request_with_retry,
        shutdown_daemon, version_path,
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

    #[test]
    fn ensure_existing_daemon_accepts_ready_session() {
        let home = unique_openpage_home("existing-ready");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener addr").port();
        let session = "existing-ready";

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

        ensure_existing_daemon(session).expect("ready session");

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn ensure_existing_daemon_rejects_missing_session() {
        let home = unique_openpage_home("existing-missing");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let error = ensure_existing_daemon("existing-missing").expect_err("missing session");
        match error {
            OpenPageError::BrowserOperation(message) => {
                assert!(message.contains("is not active"));
                assert!(message.contains("browser start --session existing-missing"));
            }
            other => panic!("expected BrowserOperation, got {other:?}"),
        }

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

    #[test]
    fn shutdown_daemon_cleans_stale_sidecars_when_process_is_gone() {
        let home = unique_openpage_home("shutdown-stale");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let session = "shutdown-stale";
        fs::write(port_path(session).expect("port path"), "9").expect("write port");
        fs::write(pid_path(session).expect("pid path"), "999999").expect("write pid");
        fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        let result = shutdown_daemon(session).expect("shutdown stale session");
        assert!(!result.had_daemon);
        assert!(!result.forced);
        assert!(read_pid(session).expect("read pid").is_none());
        assert!(!port_path(session).expect("port path").exists());
        assert!(!version_path(session).expect("version path").exists());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn incomplete_daemon_reasons_report_missing_version_and_not_ready() {
        let incomplete = super::IncompleteDaemonSession {
            session: "beta".to_string(),
            pid_present: true,
            port_present: true,
            version_present: false,
            pid_valid: true,
            port_valid: true,
            alive: true,
            ready: false,
            log_path: "/tmp/beta.log".to_string(),
        };

        assert_eq!(
            incomplete_daemon_reasons(&incomplete),
            vec!["missing_version", "daemon_not_ready"]
        );
    }

    #[test]
    fn daemon_inventory_payload_json_includes_states_and_summary() {
        let inventory = super::DaemonInventory {
            sessions: vec![super::DaemonSessionInfo {
                session: "alpha".to_string(),
                port: Some(1111),
                pid: Some(2222),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                alive: true,
                ready: true,
                log_path: "/tmp/alpha.log".to_string(),
            }],
            incomplete: vec![super::IncompleteDaemonSession {
                session: "beta".to_string(),
                pid_present: true,
                port_present: true,
                version_present: false,
                pid_valid: true,
                port_valid: true,
                alive: true,
                ready: false,
                log_path: "/tmp/beta.log".to_string(),
            }],
            cleaned: vec![super::CleanedDaemonSession {
                session: "gamma".to_string(),
                reason: "missing version".to_string(),
            }],
        };

        let summary = daemon_inventory_summary_json(&inventory);
        assert_eq!(summary["healthy"], 1);
        assert_eq!(summary["incompatible"], 0);
        assert_eq!(summary["incomplete"], 1);
        assert_eq!(summary["cleaned"], 1);
        assert_eq!(summary["total"], 3);

        let payload = daemon_inventory_payload_json(&inventory);
        assert_eq!(payload["sessions"][0]["state"], "healthy");
        assert_eq!(payload["sessions"][0]["version_matches_current_cli"], true);
        assert_eq!(payload["sessions"][0]["reasons"], json!([]));
        assert!(payload["sessions"][0].get("fix").is_none());
        assert_eq!(payload["incomplete"][0]["state"], "incomplete");
        assert_eq!(
            payload["incomplete"][0]["reasons"],
            json!(["missing_version", "daemon_not_ready"])
        );
        assert_eq!(payload["incomplete"][0]["log_path"], "/tmp/beta.log");
        assert!(payload["incomplete"][0]["fix"]
            .as_str()
            .expect("incomplete fix should be present")
            .contains("doctor --quick --fix"));
        assert_eq!(payload["cleaned"][0]["state"], "cleaned");
    }

    #[test]
    fn daemon_inventory_payload_marks_version_mismatch_as_incompatible() {
        let inventory = super::DaemonInventory {
            sessions: vec![super::DaemonSessionInfo {
                session: "alpha".to_string(),
                port: Some(1111),
                pid: Some(2222),
                version: Some("0.0.1".to_string()),
                alive: true,
                ready: true,
                log_path: "/tmp/alpha.log".to_string(),
            }],
            incomplete: Vec::new(),
            cleaned: Vec::new(),
        };

        let summary = daemon_inventory_summary_json(&inventory);
        assert_eq!(summary["healthy"], 0);
        assert_eq!(summary["incompatible"], 1);

        let payload = daemon_inventory_payload_json(&inventory);
        assert_eq!(payload["sessions"][0]["state"], "incompatible");
        assert_eq!(payload["sessions"][0]["version_matches_current_cli"], false);
        assert_eq!(payload["sessions"][0]["reasons"], json!(["version_mismatch"]));
        assert!(payload["sessions"][0]["fix"]
            .as_str()
            .expect("incompatible fix should be present")
            .contains("browser stop --session alpha"));
    }

    #[test]
    fn daemon_status_payload_json_marks_incomplete_with_reasons() {
        let status = super::DaemonSessionInfo {
            session: "beta".to_string(),
            port: Some(1111),
            pid: Some(2222),
            version: None,
            alive: true,
            ready: false,
            log_path: "/tmp/beta.log".to_string(),
        };
        let inventory = super::DaemonInventory {
            sessions: Vec::new(),
            incomplete: vec![super::IncompleteDaemonSession {
                session: "beta".to_string(),
                pid_present: true,
                port_present: true,
                version_present: false,
                pid_valid: true,
                port_valid: true,
                alive: true,
                ready: false,
                log_path: "/tmp/beta.log".to_string(),
            }],
            cleaned: Vec::new(),
        };

        let payload = super::daemon_status_payload_json(&status, &inventory);
        assert_eq!(payload["state"], "incomplete");
        assert_eq!(
            payload["reasons"],
            json!(["missing_version", "daemon_not_ready"])
        );
        assert_eq!(payload["incomplete"]["session"], "beta");
        assert_eq!(payload["incomplete"]["log_path"], "/tmp/beta.log");
        assert!(payload["fix"]
            .as_str()
            .expect("incomplete fix should be present")
            .contains("doctor --quick --fix"));
    }

    #[test]
    fn daemon_status_payload_json_marks_inactive_when_absent() {
        let status = super::DaemonSessionInfo {
            session: "missing".to_string(),
            port: None,
            pid: None,
            version: None,
            alive: false,
            ready: false,
            log_path: "/tmp/missing.log".to_string(),
        };
        let inventory = super::DaemonInventory::default();

        let payload = super::daemon_status_payload_json(&status, &inventory);
        assert_eq!(payload["state"], "inactive");
        assert!(payload.get("reasons").is_none());
        assert!(payload["fix"]
            .as_str()
            .expect("inactive fix should be present")
            .contains("browser start --session missing"));
    }

    #[test]
    fn daemon_status_payload_json_marks_version_mismatch_as_incompatible() {
        let status = super::DaemonSessionInfo {
            session: "beta".to_string(),
            port: Some(1111),
            pid: Some(2222),
            version: Some("0.0.1".to_string()),
            alive: true,
            ready: true,
            log_path: "/tmp/beta.log".to_string(),
        };
        let inventory = super::DaemonInventory {
            sessions: vec![status.clone()],
            incomplete: Vec::new(),
            cleaned: Vec::new(),
        };

        let payload = super::daemon_status_payload_json(&status, &inventory);
        assert_eq!(payload["state"], "incompatible");
        assert_eq!(payload["version_matches_current_cli"], false);
        assert_eq!(payload["reasons"], json!(["version_mismatch"]));
        assert!(payload["fix"]
            .as_str()
            .expect("incompatible fix should be present")
            .contains("browser stop --session beta"));
    }

    #[test]
    fn ensure_existing_daemon_rejects_version_mismatch() {
        let home = unique_openpage_home("existing-mismatch");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener addr").port();
        let session = "existing-mismatch";

        fs::write(port_path(session).expect("port path"), port.to_string()).expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            std::process::id().to_string(),
        )
        .expect("write pid");
        fs::write(version_path(session).expect("version path"), "0.0.1").expect("write version");

        let error = ensure_existing_daemon(session).expect_err("mismatched session");
        match error {
            OpenPageError::BrowserOperation(message) => {
                assert!(message.contains("current CLI expects"));
                assert!(message.contains("browser stop --session existing-mismatch"));
                assert!(message.contains("doctor --quick --fix"));
            }
            other => panic!("expected BrowserOperation, got {other:?}"),
        }

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }
}
