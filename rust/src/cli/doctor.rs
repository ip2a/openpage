use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;

use crate::browser::{Browser, LaunchOptions};
use crate::cli::args::DoctorArgs;
use crate::cli::connection::{daemon_dir, daemon_inventory, openpage_home};
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

    let inventory = match daemon_inventory() {
        Ok(inventory) => inventory,
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

    for cleaned in &inventory.cleaned {
        checks.push(Check::new(
            format!("daemon.cleaned.{}", cleaned.session),
            category,
            Status::Warn,
            format!(
                "Cleaned stale daemon sidecars for session {} ({})",
                cleaned.session, cleaned.reason
            ),
        ));
    }

    for incomplete in &inventory.incomplete {
        checks.push(Check::new(
            format!("daemon.incomplete.{}", incomplete.session),
            category,
            Status::Warn,
            format!(
                "Session {} has incomplete sidecars: pid_present={}, port_present={}, version_present={}, pid_valid={}, port_valid={}, alive={}, ready={}",
                incomplete.session,
                incomplete.pid_present,
                incomplete.port_present,
                incomplete.version_present,
                incomplete.pid_valid,
                incomplete.port_valid,
                incomplete.alive,
                incomplete.ready
            ),
        ));
    }

    if inventory.sessions.is_empty() {
        let status = if inventory.cleaned.is_empty() && inventory.incomplete.is_empty() {
            Status::Info
        } else {
            Status::Pass
        };
        checks.push(Check::new(
            "daemon.sessions",
            category,
            status,
            format!("No active healthy daemon sessions in {}", path.display()),
        ));
        return;
    }

    for session in &inventory.sessions {
        let version_note = match session.version.as_deref() {
            Some(version) if version == env!("CARGO_PKG_VERSION") => {
                format!("version {version}")
            }
            Some(version) => format!("version {version} (CLI is {})", env!("CARGO_PKG_VERSION")),
            None => "no version sidecar".to_string(),
        };

        let check_status = if session.ready
            && matches!(session.version.as_deref(), Some(version) if version == env!("CARGO_PKG_VERSION"))
        {
            Status::Pass
        } else {
            Status::Warn
        };

        checks.push(Check::new(
            format!("daemon.session.{}", session.session),
            category,
            check_status,
            format!(
                "Session {}: alive={}, ready={}, port={:?}, pid={:?}, {}",
                session.session,
                session.alive,
                session.ready,
                session.port,
                session.pid,
                version_note
            ),
        ));
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
