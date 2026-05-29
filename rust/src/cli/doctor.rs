use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;

use crate::browser::{Browser, LaunchOptions};
use crate::cli::args::DoctorArgs;
use crate::cli::connection::{daemon_dir, daemon_status, openpage_home};
use crate::cli::protocol::format_output_json;
use crate::error::{OpenPageError, OpenPageResult};

#[derive(Clone, Copy)]
enum Status {
    Pass,
    Warn,
    Fail,
    Info,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
            Status::Info => "info",
        }
    }
}

#[derive(Serialize)]
struct Check {
    id: String,
    category: &'static str,
    status: &'static str,
    message: String,
}

impl Check {
    fn new(
        id: impl Into<String>,
        category: &'static str,
        status: Status,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            status: status.as_str(),
            message: message.into(),
        }
    }
}

#[derive(Default, Serialize)]
struct Summary {
    pass: usize,
    warn: usize,
    fail: usize,
}

pub fn run(args: DoctorArgs) -> OpenPageResult<i32> {
    let mut checks = Vec::new();
    environment_checks(&mut checks);
    daemon_checks(&mut checks);
    browser_checks(&mut checks, args.quick);

    let summary = summarize(&checks);
    let success = summary.fail == 0;

    println!(
        "{}",
        format_output_json(&json!({
            "ok": success,
            "result": {
                "summary": summary,
                "checks": checks,
            }
        }))
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?
    );

    Ok(if success { 0 } else { 1 })
}

fn summarize(checks: &[Check]) -> Summary {
    let mut summary = Summary::default();
    for check in checks {
        match check.status {
            "pass" => summary.pass += 1,
            "warn" => summary.warn += 1,
            "fail" => summary.fail += 1,
            _ => {}
        }
    }
    summary
}

fn environment_checks(checks: &mut Vec<Check>) {
    let category = "Environment";

    match openpage_home() {
        Ok(path) => {
            if path.exists() {
                checks.push(Check::new(
                    "env.openpage_home",
                    category,
                    Status::Pass,
                    format!("OPENPAGE_HOME resolved to {}", path.display()),
                ));
            } else {
                checks.push(Check::new(
                    "env.openpage_home",
                    category,
                    Status::Info,
                    format!(
                        "OPENPAGE_HOME resolves to {} (directory not created yet)",
                        path.display()
                    ),
                ));
            }
        }
        Err(err) => checks.push(Check::new(
            "env.openpage_home",
            category,
            Status::Fail,
            err.to_string(),
        )),
    }

    match daemon_dir() {
        Ok(path) => {
            if path.exists() {
                checks.push(Check::new(
                    "env.daemon_dir",
                    category,
                    Status::Pass,
                    format!("Daemon sidecars live in {}", path.display()),
                ));
            } else {
                checks.push(Check::new(
                    "env.daemon_dir",
                    category,
                    Status::Info,
                    format!("Daemon directory does not exist yet: {}", path.display()),
                ));
            }
        }
        Err(err) => checks.push(Check::new(
            "env.daemon_dir",
            category,
            Status::Fail,
            err.to_string(),
        )),
    }
}

fn daemon_checks(checks: &mut Vec<Check>) {
    let category = "Daemon";
    let path = match daemon_dir() {
        Ok(path) => path,
        Err(err) => {
            checks.push(Check::new(
                "daemon.dir",
                category,
                Status::Fail,
                err.to_string(),
            ));
            return;
        }
    };

    if !path.exists() {
        checks.push(Check::new(
            "daemon.sessions",
            category,
            Status::Info,
            "No daemon directory yet; no sessions to inspect",
        ));
        return;
    }

    let sessions = match discover_daemon_sessions(&path) {
        Ok(sessions) => sessions,
        Err(err) => {
            checks.push(Check::new(
                "daemon.sessions",
                category,
                Status::Fail,
                err.to_string(),
            ));
            return;
        }
    };

    if sessions.is_empty() {
        checks.push(Check::new(
            "daemon.sessions",
            category,
            Status::Info,
            format!("No daemon sidecars found in {}", path.display()),
        ));
        return;
    }

    for session in sessions {
        match daemon_status(&session) {
            Ok(status) => {
                let version_note = match status.version.as_deref() {
                    Some(version) if version == env!("CARGO_PKG_VERSION") => {
                        format!("version {version}")
                    }
                    Some(version) => format!(
                        "version {version} (CLI is {})",
                        env!("CARGO_PKG_VERSION")
                    ),
                    None => "no version sidecar".to_string(),
                };

                let message = format!(
                    "Session {}: alive={}, ready={}, port={:?}, pid={:?}, {}",
                    status.session, status.alive, status.ready, status.port, status.pid, version_note
                );

                let check_status = if status.ready {
                    Status::Pass
                } else if status.alive {
                    Status::Warn
                } else {
                    Status::Warn
                };

                checks.push(Check::new(
                    format!("daemon.session.{}", status.session),
                    category,
                    check_status,
                    message,
                ));
            }
            Err(err) => checks.push(Check::new(
                format!("daemon.session.{session}"),
                category,
                Status::Warn,
                err.to_string(),
            )),
        }
    }
}

fn browser_checks(checks: &mut Vec<Check>, quick: bool) {
    let category = "Browser";
    let options = match LaunchOptions::from_ini(None) {
        Ok(options) => {
            let source = options
                .source_ini_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let browser_path = options.browser_path();
            let browser_path = if browser_path.is_empty() {
                "<default>".to_string()
            } else {
                browser_path
            };
            checks.push(Check::new(
                "browser.config",
                category,
                Status::Pass,
                format!(
                    "Loaded launch options from {} (browser_path={}, headless={}, auto_port={})",
                    source,
                    browser_path,
                    options.is_headless(),
                    options.is_auto_port()
                ),
            ));
            options
        }
        Err(err) => {
            checks.push(Check::new(
                "browser.config",
                category,
                Status::Fail,
                format!("Could not load launch options: {err}"),
            ));
            return;
        }
    };

    let browser_path = {
        let value = options.browser_path();
        if value.is_empty() {
            "<default>".to_string()
        } else {
            value
        }
    };
    let browser_exec = resolve_browser_executable(&browser_path);
    match &browser_exec {
        BrowserExecutable::Default => checks.push(Check::new(
            "browser.executable",
            category,
            Status::Info,
            "No explicit browser_path configured; live launch will rely on built-in browser resolution",
        )),
        BrowserExecutable::Found(path) => checks.push(Check::new(
            "browser.executable",
            category,
            Status::Pass,
            format!(
                "Configured browser executable `{}` resolves to {}",
                browser_path,
                path.display()
            ),
        )),
        BrowserExecutable::Missing => checks.push(Check::new(
            "browser.executable",
            category,
            Status::Fail,
            format!(
                "Configured browser executable `{}` was not found. Update rust/configs.ini browser_path, install the browser on PATH, or pass --browser-path explicitly.",
                browser_path
            ),
        )),
    }

    if quick {
        checks.push(Check::new(
            "browser.launch",
            category,
            Status::Info,
            "Skipped live browser launch check because --quick was set",
        ));
        return;
    }

    if matches!(browser_exec, BrowserExecutable::Missing) {
        checks.push(Check::new(
            "browser.launch",
            category,
            Status::Info,
            "Skipped live browser launch because the configured browser executable was not found",
        ));
        return;
    }

    let temp_dir = doctor_temp_dir("launch");
    let mut launch = options;
    launch.headless(true);
    launch.auto_port(true);
    launch.new_env(true);
    launch.set_tmp_path(&temp_dir);
    launch.set_timeouts(Some(1.0), Some(5.0), Some(1.0));

    match Browser::launch(launch) {
        Ok(browser) => {
            let version = browser.version().unwrap_or_else(|_| "<unknown>".to_string());
            let tabs = browser.tabs_count().unwrap_or(0);
            let pid = browser.browser_pid();
            let close_result = browser.close();
            let _ = fs::remove_dir_all(&temp_dir);

            match close_result {
                Ok(()) => checks.push(Check::new(
                    "browser.launch",
                    category,
                    Status::Pass,
                    format!(
                        "Headless browser launch succeeded (version={}, tabs={}, pid={pid:?})",
                        version, tabs
                    ),
                )),
                Err(err) => checks.push(Check::new(
                    "browser.launch",
                    category,
                    Status::Warn,
                    format!(
                        "Headless browser launch succeeded (version={}, tabs={}, pid={pid:?}) but close failed: {}",
                        version, tabs, err
                    ),
                )),
            }
        }
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_dir);
            checks.push(Check::new(
                "browser.launch",
                category,
                Status::Fail,
                format!(
                    "Headless browser launch failed with browser_path={browser_path}: {err}"
                ),
            ));
        }
    }
}

fn discover_daemon_sessions(path: &Path) -> OpenPageResult<Vec<String>> {
    let mut sessions = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        for suffix in [".port", ".pid", ".version"] {
            if let Some(session) = name.strip_suffix(suffix) {
                if !session.is_empty() {
                    sessions.insert(session.to_string());
                }
            }
        }
    }
    Ok(sessions.into_iter().collect())
}

fn doctor_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openpage-doctor-{label}-{}-{unique}",
        std::process::id()
    ))
}

enum BrowserExecutable {
    Default,
    Found(PathBuf),
    Missing,
}

fn resolve_browser_executable(browser_path: &str) -> BrowserExecutable {
    if browser_path.is_empty() || browser_path == "<default>" {
        return BrowserExecutable::Default;
    }

    let path = Path::new(browser_path);
    if path.is_absolute() || path.components().count() > 1 {
        return if path.exists() {
            BrowserExecutable::Found(path.to_path_buf())
        } else {
            BrowserExecutable::Missing
        };
    }

    if let Some(found) = find_in_path(browser_path) {
        return BrowserExecutable::Found(found);
    }

    BrowserExecutable::Missing
}

fn find_in_path(executable: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(executable);
        if candidate.exists() {
            return Some(candidate);
        }

        #[cfg(windows)]
        {
            for suffix in [".exe", ".cmd", ".bat"] {
                let candidate = dir.join(format!("{executable}{suffix}"));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}
