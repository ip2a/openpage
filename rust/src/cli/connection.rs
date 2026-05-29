#![allow(dead_code)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

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
    for path in [port_path(session)?, pid_path(session)?, version_path(session)?] {
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

pub(crate) fn daemon_ready(session: &str) -> bool {
    let Ok(Some(port)) = read_port(session) else {
        return false;
    };
    let socket = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&socket, Duration::from_millis(CONNECT_TIMEOUT_MS)).is_ok()
}

pub(crate) fn ensure_daemon(session: &str) -> OpenPageResult<DaemonStatus> {
    if daemon_ready(session) {
        if daemon_version_matches(session) {
            return Ok(DaemonStatus {
                already_running: true,
            });
        }
        kill_stale_daemon(session)?;
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
    let _ = ensure_daemon(session)?;
    let mut last_error: Option<OpenPageError> = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100 * attempt as u64));
        }

        match send_request_once(session, request) {
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

fn send_request_once(session: &str, request: &Request) -> OpenPageResult<Response> {
    let port = read_port(session)?
        .ok_or_else(|| OpenPageError::Io(format!("daemon port not found for session '{session}'")))?;
    let socket = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream =
        TcpStream::connect_timeout(&socket, Duration::from_millis(CONNECT_TIMEOUT_MS))?;
    stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECS)))?;

    let mut payload =
        serde_json::to_string(request).map_err(|err| OpenPageError::Serialization(err.to_string()))?;
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
