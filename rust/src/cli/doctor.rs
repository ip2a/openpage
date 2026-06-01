use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};

use crate::browser::{
    Browser, LaunchOptions, OPENPAGE_BROWSER_PATH_ENV, browser_path_env_override,
};
use crate::cli::args::DoctorArgs;
use crate::cli::connection::{
    daemon_dir, daemon_inventory, daemon_inventory_payload_json, daemon_session_fix,
    daemon_session_reasons, daemon_session_state, force_cleanup_daemon, incomplete_daemon_fix,
    incomplete_daemon_reasons, openpage_home,
};
use crate::cli::protocol::print_output_json;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasons: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_matches_current_cli: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_path: Option<String>,
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
            fix: None,
            session: None,
            state: None,
            reasons: None,
            alive: None,
            ready: None,
            pid: None,
            port: None,
            version: None,
            version_matches_current_cli: None,
            log_path: None,
            pid_present: None,
            port_present: None,
            version_present: None,
            pid_valid: None,
            port_valid: None,
            browser_path: None,
            resolved_path: None,
            suggested_path: None,
        }
    }

    fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    fn with_state(mut self, state: &'static str) -> Self {
        self.state = Some(state);
        self
    }

    fn with_reasons(mut self, reasons: Vec<&'static str>) -> Self {
        if !reasons.is_empty() {
            self.reasons = Some(reasons);
        }
        self
    }

    fn with_daemon_session_info(
        mut self,
        session: &crate::cli::connection::DaemonSessionInfo,
    ) -> Self {
        self.alive = Some(session.alive);
        self.ready = Some(session.ready);
        self.pid = session.pid;
        self.port = session.port;
        self.version = session.version.clone();
        self.version_matches_current_cli = Some(
            session.version.as_deref() == Some(env!("CARGO_PKG_VERSION")),
        );
        self.log_path = Some(session.log_path.clone());
        self
    }

    fn with_incomplete_daemon_info(
        mut self,
        incomplete: &crate::cli::connection::IncompleteDaemonSession,
    ) -> Self {
        self.alive = Some(incomplete.alive);
        self.ready = Some(incomplete.ready);
        self.log_path = Some(incomplete.log_path.clone());
        self.pid_present = Some(incomplete.pid_present);
        self.port_present = Some(incomplete.port_present);
        self.version_present = Some(incomplete.version_present);
        self.pid_valid = Some(incomplete.pid_valid);
        self.port_valid = Some(incomplete.port_valid);
        self
    }

    fn with_browser_path(mut self, browser_path: impl Into<String>) -> Self {
        self.browser_path = Some(browser_path.into());
        self
    }

    fn with_resolved_path(mut self, resolved_path: impl Into<String>) -> Self {
        self.resolved_path = Some(resolved_path.into());
        self
    }

    fn with_suggested_path(mut self, suggested_path: impl Into<String>) -> Self {
        self.suggested_path = Some(suggested_path.into());
        self
    }
}

#[derive(Default, Serialize)]
struct Summary {
    pass: usize,
    warn: usize,
    fail: usize,
    info: usize,
    fixable: usize,
    total: usize,
    warn_ids: Vec<String>,
    fail_ids: Vec<String>,
    info_ids: Vec<String>,
    fixable_ids: Vec<String>,
}

pub fn run(args: DoctorArgs) -> OpenPageResult<i32> {
    let fixed = if args.fix { apply_fixes()? } else { Vec::new() };
    let mut checks = Vec::new();
    environment_checks(&mut checks);
    let inventory = daemon_checks(&mut checks);
    browser_checks(&mut checks, args.quick);

    let summary = summarize(&checks);
    let success = summary.fail == 0;

    print_output_json(&json!({
        "ok": success,
        "result": {
            "summary": summary,
            "checks": checks,
            "fixed": fixed,
            "inventory": inventory.as_ref().map(doctor_inventory_payload),
        }
    }));

    Ok(if success { 0 } else { 1 })
}

fn apply_fixes() -> OpenPageResult<Vec<String>> {
    let mut fixed = Vec::new();
    fixed.extend(remove_legacy_session_files()?);
    let inventory = daemon_inventory()?;
    for cleaned in inventory.cleaned {
        fixed.push(format!(
            "Removed stale daemon sidecars for session {} ({})",
            cleaned.session, cleaned.reason
        ));
    }
    for incomplete in inventory.incomplete {
        if incomplete.ready {
            continue;
        }
        force_cleanup_daemon(&incomplete.session)?;
        fixed.push(format!(
            "Stopped and removed incomplete unready daemon session {}",
            incomplete.session
        ));
    }
    for session in inventory.sessions {
        if session.version.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
            continue;
        }
        let found_version = session.version.as_deref().unwrap_or("<missing>");
        force_cleanup_daemon(&session.session)?;
        fixed.push(format!(
            "Stopped incompatible daemon session {} (found version {}, current CLI {})",
            session.session,
            found_version,
            env!("CARGO_PKG_VERSION")
        ));
    }
    Ok(fixed)
}

fn summarize(checks: &[Check]) -> Summary {
    let mut summary = Summary::default();
    for check in checks {
        summary.total += 1;
        match check.status {
            "pass" => summary.pass += 1,
            "warn" => {
                summary.warn += 1;
                summary.warn_ids.push(check.id.clone());
            }
            "fail" => {
                summary.fail += 1;
                summary.fail_ids.push(check.id.clone());
            }
            "info" => {
                summary.info += 1;
                summary.info_ids.push(check.id.clone());
            }
            _ => {}
        }
        if check.fix.is_some() {
            summary.fixable += 1;
            summary.fixable_ids.push(check.id.clone());
        }
    }
    summary
}

fn doctor_inventory_payload(inventory: &crate::cli::connection::DaemonInventory) -> Value {
    daemon_inventory_payload_json(inventory)
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
                checks.push(
                    Check::new(
                        "env.openpage_home",
                        category,
                        Status::Info,
                        format!(
                            "OPENPAGE_HOME resolves to {} (directory not created yet)",
                            path.display()
                        ),
                    )
                    .with_fix(format!(
                        "Run `openpage browser start --session default --headless https://example.com` once, or create {} manually before using daemon-backed commands.",
                        path.display()
                    )),
                );
            }
        }
        Err(err) => checks.push(
            Check::new("env.openpage_home", category, Status::Fail, err.to_string()).with_fix(
                "Set OPENPAGE_HOME to a writable directory, or make sure HOME is defined before running OpenPage.",
            ),
        ),
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
                checks.push(
                    Check::new(
                        "env.daemon_dir",
                        category,
                        Status::Info,
                        format!("Daemon directory does not exist yet: {}", path.display()),
                    )
                    .with_fix(format!(
                        "Start any daemon-backed session once so OpenPage can create {}, or create it manually if your workflow requires it ahead of time.",
                        path.display()
                    )),
                );
            }
        }
        Err(err) => checks.push(
            Check::new("env.daemon_dir", category, Status::Fail, err.to_string()).with_fix(
                "Set OPENPAGE_HOME or HOME first so OpenPage can resolve the daemon sidecar directory.",
            ),
        ),
    }

    match legacy_session_files() {
        Ok(files) if files.is_empty() => {
            let sessions_dir = legacy_sessions_dir().unwrap_or_else(|_| PathBuf::from("<unknown>"));
            checks.push(Check::new(
                "env.legacy_sessions",
                category,
                Status::Pass,
                format!(
                    "No legacy session JSON files found in {}",
                    sessions_dir.display()
                ),
            ));
        }
        Ok(files) => {
            let sessions_dir = legacy_sessions_dir().unwrap_or_else(|_| PathBuf::from("<unknown>"));
            let names = files
                .iter()
                .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
                .take(3)
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let suffix = if files.len() > names.len() {
                format!(", plus {} more", files.len() - names.len())
            } else {
                String::new()
            };
            let examples = if names.is_empty() {
                String::new()
            } else {
                format!(" Example files: {}{}", names.join(", "), suffix)
            };
            checks.push(
                Check::new(
                    "env.legacy_sessions",
                    category,
                    Status::Warn,
                    format!(
                        "Found {} legacy session JSON file(s) in {}. The current TCP daemon CLI path no longer uses them.{}",
                        files.len(),
                        sessions_dir.display(),
                        examples
                    ),
                )
                .with_fix(format!(
                    "If you no longer need the old one-shot session artifacts, back them up and remove {}. Keep the directory only if another non-CLI workflow still reads those JSON files.",
                    sessions_dir.display()
                )),
            );
        }
        Err(err) => checks.push(
            Check::new(
                "env.legacy_sessions",
                category,
                Status::Warn,
                format!("Could not inspect legacy session JSON files: {err}"),
            )
            .with_fix(
                "Check permissions for OPENPAGE_HOME and inspect the old sessions directory manually. The active TCP daemon CLI path no longer depends on legacy session JSON files.",
            ),
        ),
    }
}

fn daemon_checks(checks: &mut Vec<Check>) -> Option<crate::cli::connection::DaemonInventory> {
    let category = "Daemon";
    let path = match daemon_dir() {
        Ok(path) => path,
        Err(err) => {
            checks.push(
                Check::new("daemon.dir", category, Status::Fail, err.to_string()).with_fix(
                    "Resolve the environment error first so OpenPage can locate the daemon sidecar directory.",
                ),
            );
            return None;
        }
    };

    if !path.exists() {
        checks.push(Check::new(
            "daemon.sessions",
            category,
            Status::Info,
            "No daemon directory yet; no sessions to inspect",
        ));
        return Some(crate::cli::connection::DaemonInventory::default());
    }

    let inventory = match daemon_inventory() {
        Ok(inventory) => inventory,
        Err(err) => {
            checks.push(
                Check::new("daemon.sessions", category, Status::Fail, err.to_string()).with_fix(
                    format!(
                        "Inspect {} for invalid sidecars or permission issues, then rerun `openpage doctor --quick`.",
                        path.display()
                    ),
                ),
            );
            return None;
        }
    };

    for cleaned in &inventory.cleaned {
        checks.push(
            Check::new(
                format!("daemon.cleaned.{}", cleaned.session),
                category,
                Status::Warn,
                format!(
                    "Cleaned stale daemon sidecars for session {} ({})",
                    cleaned.session, cleaned.reason
                ),
            )
            .with_session(cleaned.session.clone())
            .with_state("cleaned"),
        );
    }

    for incomplete in &inventory.incomplete {
        let reasons = incomplete_daemon_reasons(incomplete);
        checks.push(
            Check::new(
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
            )
            .with_session(incomplete.session.clone())
            .with_state("incomplete")
            .with_reasons(reasons)
            .with_incomplete_daemon_info(incomplete)
            .with_fix(incomplete_daemon_fix(incomplete)),
        );
    }

    if inventory.sessions.is_empty() {
        let status = if inventory.cleaned.is_empty() && inventory.incomplete.is_empty() {
            Status::Info
        } else {
            Status::Pass
        };
        checks.push(
            Check::new(
                "daemon.sessions",
                category,
                status,
                format!("No active healthy daemon sessions in {}", path.display()),
            )
            .with_fix(
                "If you expected a session to be running, start one with `openpage browser start --session <name> --headless <url>` and rerun the audit.",
            ),
        );
        return Some(inventory);
    }

    for session in &inventory.sessions {
        let version_note = match session.version.as_deref() {
            Some(version) if version == env!("CARGO_PKG_VERSION") => {
                format!("version {version}")
            }
            Some(version) => format!("version {version} (CLI is {})", env!("CARGO_PKG_VERSION")),
            None => "no version sidecar".to_string(),
        };
        let state = daemon_session_state(session);
        let reasons = daemon_session_reasons(session);

        let check_status = if session.ready
            && matches!(session.version.as_deref(), Some(version) if version == env!("CARGO_PKG_VERSION"))
        {
            Status::Pass
        } else {
            Status::Warn
        };

        let check = Check::new(
            format!("daemon.session.{}", session.session),
            category,
            check_status,
            format!(
                "Session {}: state={}, alive={}, ready={}, port={:?}, pid={:?}, {}",
                session.session,
                state,
                session.alive,
                session.ready,
                session.port,
                session.pid,
                version_note
            ),
        )
        .with_session(session.session.clone())
        .with_state(state)
        .with_reasons(reasons)
        .with_daemon_session_info(session);

        if let Some(fix) = daemon_session_fix(session) {
            checks.push(check.with_fix(fix));
        } else {
            checks.push(check);
        }
    }

    Some(inventory)
}

fn browser_checks(checks: &mut Vec<Check>, quick: bool) {
    let category = "Browser";
    let browser_path_override = browser_path_env_override();
    let options = match LaunchOptions::from_ini(None) {
        Ok(mut options) => {
            let configured_browser_path = options.browser_path();
            if let Some(path) = browser_path_override.as_ref() {
                options.set_browser_path(path);
            }
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
            let browser_path_display = match browser_path_override.as_ref() {
                Some(path) => {
                    let configured = if configured_browser_path.is_empty() {
                        "<default>".to_string()
                    } else {
                        configured_browser_path
                    };
                    format!(
                        "{} (overrides configured {} via {})",
                        path.display(),
                        configured,
                        OPENPAGE_BROWSER_PATH_ENV
                    )
                }
                None => browser_path.clone(),
            };
            checks.push(
                Check::new(
                    "browser.config",
                    category,
                    Status::Pass,
                    format!(
                        "Loaded launch options from {} (browser_path={}, headless={}, auto_port={})",
                        source,
                        browser_path_display,
                        options.is_headless(),
                        options.is_auto_port()
                    ),
                )
                .with_browser_path(browser_path.clone()),
            );
            options
        }
        Err(err) => {
            checks.push(
                Check::new(
                    "browser.config",
                    category,
                    Status::Fail,
                    format!("Could not load launch options: {err}"),
                )
                .with_fix(
                    "Check rust/configs.ini for invalid values or formatting. If needed, temporarily bypass it by passing explicit CLI flags such as --browser-path.",
                ),
            );
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
    let browser_hint = suggested_browser_executable(&browser_path);
    match &browser_exec {
        BrowserExecutable::Default => checks.push(
            Check::new(
                "browser.executable",
                category,
                Status::Info,
                "No explicit browser_path configured; live launch will rely on built-in browser resolution",
            )
            .with_browser_path(browser_path.clone()),
        ),
        BrowserExecutable::Found(path) => checks.push(
            Check::new(
                "browser.executable",
                category,
                Status::Pass,
                format!(
                    "Configured browser executable `{}` resolves to {}",
                    browser_path,
                    path.display()
                ),
            )
            .with_browser_path(browser_path.clone())
            .with_resolved_path(path.display().to_string()),
        ),
        BrowserExecutable::Missing => {
            let fix = browser_executable_fix(&browser_path, browser_hint.as_deref());
            let mut check = Check::new(
                "browser.executable",
                category,
                Status::Fail,
                missing_browser_message(&browser_path, browser_hint.as_deref()),
            )
            .with_fix(fix)
            .with_browser_path(browser_path.clone());
            if let Some(path) = browser_hint.as_ref() {
                check = check.with_suggested_path(path.display().to_string());
            }
            checks.push(check)
        }
    }

    if matches!(browser_exec, BrowserExecutable::Missing) {
        if let Some(path) = browser_hint.as_ref() {
            checks.push(
                Check::new(
                    "browser.executable.hint",
                    category,
                    Status::Info,
                    format!(
                        "Local browser candidate found at {}. Setting rust/configs.ini browser_path to this absolute path should work on this machine.",
                        path.display()
                    ),
                )
                .with_browser_path(browser_path.clone())
                .with_suggested_path(path.display().to_string()),
            );
        }
    }

    if quick {
        checks.push(
            Check::new(
                "browser.launch",
                category,
                Status::Info,
                "Skipped live browser launch check because --quick was set",
            )
            .with_fix("Rerun `openpage doctor` without --quick when you want a real headless launch smoke test."),
        );
        return;
    }

    if matches!(browser_exec, BrowserExecutable::Missing) {
        checks.push(
            Check::new(
                "browser.launch",
                category,
                Status::Info,
                match browser_hint.as_ref() {
                    Some(path) => format!(
                        "Skipped live browser launch because the configured browser executable was not found. Local candidate: {}",
                        path.display()
                    ),
                    None => {
                        "Skipped live browser launch because the configured browser executable was not found"
                            .to_string()
                    }
                },
            )
            .with_fix(browser_executable_fix(
                &browser_path,
                browser_hint.as_deref(),
            )),
        );
        return;
    }

    let temp_dir = doctor_temp_dir("launch");
    let temp_dir_guard = TempDirGuard::new(temp_dir.clone());
    let mut launch = options;
    launch.headless(true);
    launch.auto_port(true);
    launch.new_env(true);
    launch.set_tmp_path(temp_dir_guard.path());
    launch.set_timeouts(Some(1.0), Some(5.0), Some(1.0));

    match Browser::launch(launch) {
        Ok(browser) => {
            let mut launch_guard = BrowserLaunchGuard::new(browser, temp_dir_guard);
            let version = launch_guard
                .browser()
                .version()
                .unwrap_or_else(|_| "<unknown>".to_string());
            let tabs = launch_guard.browser().tabs_count().unwrap_or(0);
            let pid = launch_guard.browser().browser_pid();
            let close_result = launch_guard.close();

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
                )
                .with_fix(
                    "Rerun `openpage doctor` or try `openpage browser start --headless --session doctor-smoke https://example.com` to confirm whether teardown is consistently failing.",
                )),
            }
        }
        Err(err) => {
            checks.push(
                Check::new(
                    "browser.launch",
                    category,
                    Status::Fail,
                    format!(
                        "Headless browser launch failed with browser_path={browser_path}: {err}"
                    ),
                )
                .with_fix(browser_launch_fix(&browser_path)),
            );
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

fn legacy_sessions_dir() -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?.join("sessions"))
}

fn legacy_session_files() -> OpenPageResult<Vec<PathBuf>> {
    let dir = legacy_sessions_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn remove_legacy_session_files() -> OpenPageResult<Vec<String>> {
    let files = legacy_session_files()?;
    let mut removed = Vec::new();
    for path in files {
        match fs::remove_file(&path) {
            Ok(()) => removed.push(format!("Removed legacy session JSON {}", path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(OpenPageError::Io(format!(
                    "failed to remove legacy session JSON {}: {err}",
                    path.display()
                )));
            }
        }
    }
    Ok(removed)
}

enum BrowserExecutable {
    Default,
    Found(PathBuf),
    Missing,
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct BrowserLaunchGuard {
    browser: Option<Browser>,
    _temp_dir: TempDirGuard,
}

impl BrowserLaunchGuard {
    fn new(browser: Browser, temp_dir: TempDirGuard) -> Self {
        Self {
            browser: Some(browser),
            _temp_dir: temp_dir,
        }
    }

    fn browser(&self) -> &Browser {
        self.browser
            .as_ref()
            .expect("browser launch guard should hold browser until closed")
    }

    fn close(&mut self) -> OpenPageResult<()> {
        if let Some(browser) = self.browser.take() {
            browser.close()
        } else {
            Ok(())
        }
    }
}

impl Drop for BrowserLaunchGuard {
    fn drop(&mut self) {
        if let Some(browser) = self.browser.take() {
            let _ = browser.close();
        }
    }
}

fn missing_browser_message(browser_path: &str, hint: Option<&Path>) -> String {
    match hint {
        Some(path) => format!(
            "Configured browser executable `{}` was not found. Update rust/configs.ini browser_path to {}, set {}={} for this process, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path,
            path.display(),
            OPENPAGE_BROWSER_PATH_ENV,
            path.display()
        ),
        None => format!(
            "Configured browser executable `{}` was not found. Update rust/configs.ini browser_path, set {}=<absolute-browser-path> for this process, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path, OPENPAGE_BROWSER_PATH_ENV
        ),
    }
}

fn browser_executable_fix(browser_path: &str, hint: Option<&Path>) -> String {
    match hint {
        Some(path) => format!(
            "Set rust/configs.ini browser_path to {} on this machine, set {}={} for a process-local override, or rerun the command with --browser-path {}. If you want to keep `{}` as a name, make sure it resolves on PATH.",
            path.display(),
            OPENPAGE_BROWSER_PATH_ENV,
            path.display(),
            path.display(),
            browser_path
        ),
        None => format!(
            "Update rust/configs.ini browser_path to a real browser executable, set {}=<absolute-browser-path> for a process-local override, or rerun the command with --browser-path <absolute-browser-path>. If you want to keep `{}`, make sure it resolves on PATH.",
            OPENPAGE_BROWSER_PATH_ENV, browser_path
        ),
    }
}

fn browser_launch_fix(browser_path: &str) -> String {
    format!(
        "First rerun `openpage doctor --quick` to confirm browser-path resolution. If that passes, try `openpage browser start --headless --session doctor-smoke --browser-path {}` to reproduce the launch failure outside doctor.",
        shell_safe_browser_path_arg(browser_path)
    )
}

fn shell_safe_browser_path_arg(browser_path: &str) -> String {
    if browser_path.contains(char::is_whitespace) {
        format!("{browser_path:?}")
    } else {
        browser_path.to_string()
    }
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

fn suggested_browser_executable(browser_path: &str) -> Option<PathBuf> {
    suggested_browser_executable_from_known_paths(browser_path, common_browser_candidates())
}

fn suggested_browser_executable_from_known_paths(
    browser_path: &str,
    candidates: Vec<PathBuf>,
) -> Option<PathBuf> {
    if browser_path.is_empty() || browser_path == "<default>" {
        return None;
    }

    if browser_path.contains(std::path::MAIN_SEPARATOR) {
        return None;
    }

    let normalized = browser_path.trim().to_ascii_lowercase();
    if !matches!(
        normalized.as_str(),
        "chrome" | "google-chrome" | "google chrome" | "chromium" | "chromium-browser"
    ) {
        return None;
    }

    candidates.into_iter().find(|path| path.exists())
}

fn common_browser_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        ));
        candidates.push(PathBuf::from(
            "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        ));

        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates
                .push(home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
            candidates.push(
                home.join(
                    "Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
                ),
            );
        }
    }

    candidates
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

#[cfg(test)]
mod tests {
    use super::{
        BrowserLaunchGuard, Check, Status, TempDirGuard, browser_executable_fix,
        browser_launch_fix, daemon_checks, legacy_session_files, missing_browser_message,
        shell_safe_browser_path_arg, suggested_browser_executable_from_known_paths,
    };
    use crate::cli::connection::{
        DaemonSessionInfo, daemon_dir, daemon_session_fix, pid_path, port_path, version_path,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};
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
            "openpage-doctor-test-home-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    fn test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn missing_browser_message_includes_hint_when_present() {
        let hint = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        let message = missing_browser_message("chrome", Some(hint.as_path()));
        assert!(message.contains("chrome"));
        assert!(message.contains("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        assert!(message.contains("OPENPAGE_BROWSER_PATH"));
    }

    #[test]
    fn suggested_browser_executable_only_applies_to_known_aliases() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "openpage-doctor-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        let candidate = dir.join("Chrome");
        fs::write(&candidate, "").expect("candidate file");

        let found =
            suggested_browser_executable_from_known_paths("chrome", vec![candidate.clone()]);
        assert_eq!(found, Some(candidate.clone()));

        let ignored =
            suggested_browser_executable_from_known_paths("custom-browser", vec![candidate]);
        assert!(ignored.is_none());

        let _ = fs::remove_file(dir.join("Chrome"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn browser_executable_fix_uses_hint_when_present() {
        let hint = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        let fix = browser_executable_fix("chrome", Some(hint.as_path()));
        assert!(fix.contains("rust/configs.ini"));
        assert!(fix.contains("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        assert!(fix.contains("--browser-path"));
        assert!(fix.contains("OPENPAGE_BROWSER_PATH"));
    }

    #[test]
    fn check_omits_fix_when_absent_and_serializes_when_present() {
        let without_fix = serde_json::to_value(Check::new(
            "browser.executable",
            "Browser",
            Status::Fail,
            "bad browser path",
        ))
        .expect("serialize check without fix");
        assert_eq!(
            without_fix,
            json!({
                "id": "browser.executable",
                "category": "Browser",
                "status": "fail",
                "message": "bad browser path"
            })
        );

        let with_fix = serde_json::to_value(
            Check::new(
                "browser.executable",
                "Browser",
                Status::Fail,
                "bad browser path",
            )
            .with_fix("set browser_path"),
        )
        .expect("serialize check with fix");
        assert_eq!(with_fix["fix"], "set browser_path");
        assert!(with_fix.get("session").is_none());
        assert!(with_fix.get("state").is_none());
        assert!(with_fix.get("reasons").is_none());
        assert!(with_fix.get("alive").is_none());
        assert!(with_fix.get("ready").is_none());
        assert!(with_fix.get("pid").is_none());
        assert!(with_fix.get("port").is_none());
        assert!(with_fix.get("version").is_none());
        assert!(with_fix.get("version_matches_current_cli").is_none());
        assert!(with_fix.get("log_path").is_none());
        assert!(with_fix.get("pid_present").is_none());
        assert!(with_fix.get("port_present").is_none());
        assert!(with_fix.get("version_present").is_none());
        assert!(with_fix.get("pid_valid").is_none());
        assert!(with_fix.get("port_valid").is_none());
        assert!(with_fix.get("browser_path").is_none());
        assert!(with_fix.get("resolved_path").is_none());
        assert!(with_fix.get("suggested_path").is_none());
    }

    #[test]
    fn check_serializes_state_and_reasons_when_present() {
        let value = serde_json::to_value(
            Check::new("daemon.session.review", "Daemon", Status::Warn, "version mismatch")
                .with_session("review")
                .with_state("incompatible")
                .with_reasons(vec!["version_mismatch"])
                .with_fix("restart session"),
        )
        .expect("serialize check with state and reasons");

        assert_eq!(value["session"], "review");
        assert_eq!(value["state"], "incompatible");
        assert_eq!(value["reasons"], json!(["version_mismatch"]));
        assert_eq!(value["fix"], "restart session");
    }

    #[test]
    fn check_serializes_daemon_runtime_fields_when_present() {
        let value = serde_json::to_value(
            Check::new("daemon.session.review", "Daemon", Status::Warn, "version mismatch")
                .with_session("review")
                .with_state("incompatible")
                .with_reasons(vec!["version_mismatch"])
                .with_daemon_session_info(&DaemonSessionInfo {
                    session: "review".to_string(),
                    port: Some(1234),
                    pid: Some(5678),
                    version: Some("0.0.1".to_string()),
                    alive: true,
                    ready: true,
                    log_path: "/tmp/review.log".to_string(),
                })
                .with_fix("restart session"),
        )
        .expect("serialize check with daemon runtime fields");

        assert_eq!(value["alive"], true);
        assert_eq!(value["ready"], true);
        assert_eq!(value["pid"], 5678);
        assert_eq!(value["port"], 1234);
        assert_eq!(value["version"], "0.0.1");
        assert_eq!(value["version_matches_current_cli"], false);
        assert_eq!(value["log_path"], "/tmp/review.log");
    }

    #[test]
    fn check_serializes_browser_path_fields_when_present() {
        let value = serde_json::to_value(
            Check::new(
                "browser.executable",
                "Browser",
                Status::Fail,
                "bad browser path",
            )
            .with_browser_path("/tmp/dp-browser")
            .with_suggested_path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        )
        .expect("serialize check with browser path fields");

        assert_eq!(value["browser_path"], "/tmp/dp-browser");
        assert_eq!(
            value["suggested_path"],
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        );
        assert!(value.get("resolved_path").is_none());

        let resolved = serde_json::to_value(
            Check::new(
                "browser.executable",
                "Browser",
                Status::Pass,
                "browser found",
            )
            .with_browser_path("chrome")
            .with_resolved_path("/usr/bin/chrome"),
        )
        .expect("serialize resolved browser path");

        assert_eq!(resolved["browser_path"], "chrome");
        assert_eq!(resolved["resolved_path"], "/usr/bin/chrome");
        assert!(resolved.get("suggested_path").is_none());
    }

    #[test]
    fn shell_safe_browser_path_quotes_paths_with_spaces() {
        assert_eq!(
            shell_safe_browser_path_arg(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            ),
            "\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\""
        );
        assert_eq!(shell_safe_browser_path_arg("chrome"), "chrome");
    }

    #[test]
    fn browser_launch_fix_points_to_quick_and_manual_repro() {
        let fix =
            browser_launch_fix("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        assert!(fix.contains("doctor --quick"));
        assert!(fix.contains("browser start"));
        assert!(fix.contains("--browser-path"));
    }

    #[test]
    fn temp_dir_guard_removes_directory_on_drop() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "openpage-doctor-guard-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create guard temp dir");
        fs::write(dir.join("probe.txt"), "x").expect("write temp file");

        {
            let guard = TempDirGuard::new(dir.clone());
            assert!(guard.path().exists());
        }

        assert!(!dir.exists());
    }

    #[test]
    fn browser_launch_guard_without_browser_still_cleans_temp_dir() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "openpage-doctor-browser-guard-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create guard temp dir");
        fs::write(dir.join("probe.txt"), "x").expect("write temp file");

        {
            let _guard = BrowserLaunchGuard {
                browser: None,
                _temp_dir: TempDirGuard::new(dir.clone()),
            };
        }

        assert!(!dir.exists());
    }

    #[test]
    fn daemon_session_fix_prefers_version_restart_guidance() {
        let session = DaemonSessionInfo {
            session: "review".to_string(),
            port: Some(1234),
            pid: Some(5678),
            version: Some("0.0.1".to_string()),
            alive: true,
            ready: true,
            log_path: "/tmp/review.log".to_string(),
        };

        let fix = daemon_session_fix(&session).expect("version-mismatched session should have fix");
        assert!(fix.contains("browser stop --session review"));
        assert!(fix.contains(env!("CARGO_PKG_VERSION")));
        assert!(fix.contains("doctor --quick --fix"));
    }

    #[test]
    fn daemon_session_fix_points_to_status_and_log_when_not_ready() {
        let session = DaemonSessionInfo {
            session: "review".to_string(),
            port: Some(1234),
            pid: Some(5678),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            alive: true,
            ready: false,
            log_path: "/tmp/review.log".to_string(),
        };

        let fix = daemon_session_fix(&session).expect("not-ready session should have fix");
        assert!(fix.contains("browser status --session review"));
        assert!(fix.contains("/tmp/review.log"));
    }

    #[test]
    fn legacy_session_files_returns_only_json_entries() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("legacy-json");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let sessions_dir = home.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");
        fs::write(sessions_dir.join("keep.json"), "{}").expect("write keep.json");
        fs::write(sessions_dir.join("other.txt"), "x").expect("write other.txt");
        fs::create_dir_all(sessions_dir.join("nested")).expect("create nested dir");

        let files = legacy_session_files().expect("list legacy session files");
        assert_eq!(files, vec![sessions_dir.join("keep.json")]);

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn legacy_session_files_returns_empty_when_directory_missing() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("legacy-missing");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let files = legacy_session_files().expect("list legacy session files");
        assert!(files.is_empty());
    }

    #[test]
    fn remove_legacy_session_files_deletes_only_json_entries() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("legacy-remove");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let sessions_dir = home.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");
        let keep_json = sessions_dir.join("keep.json");
        let keep_txt = sessions_dir.join("other.txt");
        fs::write(&keep_json, "{}").expect("write keep.json");
        fs::write(&keep_txt, "x").expect("write other.txt");

        let removed = super::remove_legacy_session_files().expect("remove legacy session files");
        assert_eq!(
            removed,
            vec![format!(
                "Removed legacy session JSON {}",
                keep_json.display()
            )]
        );
        assert!(!keep_json.exists());
        assert!(keep_txt.exists());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn apply_fixes_reports_stale_daemon_sidecar_cleanup() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("fix-stale-daemon");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let session = "stale-daemon";
        fs::write(port_path(session).expect("port path"), "9").expect("write port");
        fs::write(pid_path(session).expect("pid path"), "999999").expect("write pid");
        fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        let fixed = super::apply_fixes().expect("apply fixes");
        assert!(fixed.iter().any(|line| {
            line.contains("Removed stale daemon sidecars for session stale-daemon")
        }));
        assert!(!port_path(session).expect("port path").exists());
        assert!(!pid_path(session).expect("pid path").exists());
        assert!(!version_path(session).expect("version path").exists());

        let _ = fs::remove_dir_all(home);
    }

    #[cfg(unix)]
    #[test]
    fn apply_fixes_stops_incomplete_unready_daemon_session() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("fix-incomplete-daemon");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep child");

        let session = "incomplete-daemon";
        fs::write(port_path(session).expect("port path"), "9").expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            child.id().to_string(),
        )
        .expect("write pid");

        let fixed = super::apply_fixes().expect("apply fixes");
        assert!(fixed.iter().any(|line| {
            line.contains("Stopped and removed incomplete unready daemon session incomplete-daemon")
        }));
        let status = child.wait().expect("wait for child to exit");
        assert!(!status.success());
        assert!(!port_path(session).expect("port path").exists());
        assert!(!pid_path(session).expect("pid path").exists());
        assert!(!version_path(session).expect("version path").exists());

        let _ = fs::remove_dir_all(home);
    }

    #[cfg(unix)]
    #[test]
    fn apply_fixes_stops_incompatible_daemon_session() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("fix-incompatible-daemon");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener addr").port();
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep child");

        let session = "incompatible-daemon";
        fs::write(port_path(session).expect("port path"), port.to_string()).expect("write port");
        fs::write(
            pid_path(session).expect("pid path"),
            child.id().to_string(),
        )
        .expect("write pid");
        fs::write(version_path(session).expect("version path"), "0.0.1").expect("write version");

        let fixed = super::apply_fixes().expect("apply fixes");
        assert!(fixed.iter().any(|line| {
            line.contains("Stopped incompatible daemon session incompatible-daemon")
        }));
        let status = child.wait().expect("wait for child to exit");
        assert!(!status.success());
        assert!(!port_path(session).expect("port path").exists());
        assert!(!pid_path(session).expect("pid path").exists());
        assert!(!version_path(session).expect("version path").exists());

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn summarize_counts_info_fixable_and_total() {
        let checks = vec![
            Check::new("a", "Env", Status::Pass, "ok"),
            Check::new("b", "Env", Status::Warn, "warn").with_fix("do something"),
            Check::new("c", "Env", Status::Fail, "fail").with_fix("do something else"),
            Check::new("d", "Env", Status::Info, "info"),
        ];

        let summary = super::summarize(&checks);
        assert_eq!(summary.pass, 1);
        assert_eq!(summary.warn, 1);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.info, 1);
        assert_eq!(summary.fixable, 2);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.warn_ids, vec![String::from("b")]);
        assert_eq!(summary.fail_ids, vec![String::from("c")]);
        assert_eq!(summary.info_ids, vec![String::from("d")]);
        assert_eq!(
            summary.fixable_ids,
            vec![String::from("b"), String::from("c")]
        );
    }

    #[test]
    fn doctor_inventory_payload_includes_state_and_reasons() {
        let inventory = crate::cli::connection::DaemonInventory {
            sessions: vec![crate::cli::connection::DaemonSessionInfo {
                session: "alpha".to_string(),
                port: Some(1111),
                pid: Some(2222),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                alive: true,
                ready: true,
                log_path: "/tmp/alpha.log".to_string(),
            }],
            incomplete: vec![crate::cli::connection::IncompleteDaemonSession {
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
            cleaned: vec![crate::cli::connection::CleanedDaemonSession {
                session: "gamma".to_string(),
                reason: "missing version".to_string(),
            }],
        };

        let payload = super::doctor_inventory_payload(&inventory);
        assert_eq!(payload["summary"]["healthy"], 1);
        assert_eq!(payload["summary"]["incompatible"], 0);
        assert_eq!(payload["summary"]["incomplete"], 1);
        assert_eq!(payload["summary"]["cleaned"], 1);
        assert_eq!(payload["summary"]["total"], 3);
        assert_eq!(payload["sessions"][0]["state"], "healthy");
        assert_eq!(payload["sessions"][0]["version_matches_current_cli"], true);
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
    fn daemon_checks_include_machine_readable_state_and_reasons() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("daemon-check-shapes");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let healthy_listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind healthy listener");
        let healthy_port = healthy_listener.local_addr().expect("healthy addr").port();
        fs::write(port_path("healthy").expect("healthy port path"), healthy_port.to_string())
            .expect("write healthy port");
        fs::write(
            pid_path("healthy").expect("healthy pid path"),
            std::process::id().to_string(),
        )
        .expect("write healthy pid");
        fs::write(
            version_path("healthy").expect("healthy version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write healthy version");

        let incompatible_listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind incompatible listener");
        let incompatible_port = incompatible_listener
            .local_addr()
            .expect("incompatible addr")
            .port();
        fs::write(
            port_path("mismatch").expect("mismatch port path"),
            incompatible_port.to_string(),
        )
        .expect("write mismatch port");
        fs::write(
            pid_path("mismatch").expect("mismatch pid path"),
            std::process::id().to_string(),
        )
        .expect("write mismatch pid");
        fs::write(
            version_path("mismatch").expect("mismatch version path"),
            "0.0.1",
        )
        .expect("write mismatch version");

        fs::write(
            port_path("incomplete").expect("incomplete port path"),
            "9",
        )
        .expect("write incomplete port");
        fs::write(
            pid_path("incomplete").expect("incomplete pid path"),
            std::process::id().to_string(),
        )
        .expect("write incomplete pid");

        let mut checks = Vec::new();
        let inventory = daemon_checks(&mut checks).expect("inventory should be present");
        assert_eq!(inventory.sessions.len(), 2);
        assert_eq!(inventory.incomplete.len(), 1);

        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");

        let healthy = checks
            .iter()
            .find(|check| check["id"] == "daemon.session.healthy")
            .expect("healthy check should exist");
        assert_eq!(healthy["session"], "healthy");
        assert_eq!(healthy["state"], "healthy");
        assert_eq!(healthy["alive"], true);
        assert_eq!(healthy["ready"], true);
        assert_eq!(healthy["port"], healthy_port);
        assert_eq!(healthy["pid"], std::process::id());
        assert_eq!(healthy["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(healthy["version_matches_current_cli"], true);
        assert_eq!(healthy["log_path"], daemon_dir().expect("daemon dir").join("healthy.log").display().to_string());
        assert!(healthy.get("reasons").is_none());

        let mismatch = checks
            .iter()
            .find(|check| check["id"] == "daemon.session.mismatch")
            .expect("mismatch check should exist");
        assert_eq!(mismatch["session"], "mismatch");
        assert_eq!(mismatch["state"], "incompatible");
        assert_eq!(mismatch["reasons"], json!(["version_mismatch"]));
        assert_eq!(mismatch["alive"], true);
        assert_eq!(mismatch["ready"], true);
        assert_eq!(mismatch["port"], incompatible_port);
        assert_eq!(mismatch["pid"], std::process::id());
        assert_eq!(mismatch["version"], "0.0.1");
        assert_eq!(mismatch["version_matches_current_cli"], false);
        assert_eq!(mismatch["log_path"], daemon_dir().expect("daemon dir").join("mismatch.log").display().to_string());
        assert!(mismatch["fix"]
            .as_str()
            .expect("mismatch fix should exist")
            .contains("browser stop --session mismatch"));

        let incomplete = checks
            .iter()
            .find(|check| check["id"] == "daemon.incomplete.incomplete")
            .expect("incomplete check should exist");
        assert_eq!(incomplete["session"], "incomplete");
        assert_eq!(incomplete["state"], "incomplete");
        assert_eq!(
            incomplete["reasons"],
            json!(["missing_version", "daemon_not_ready"])
        );
        assert_eq!(incomplete["alive"], true);
        assert_eq!(incomplete["ready"], false);
        assert_eq!(incomplete["pid_present"], true);
        assert_eq!(incomplete["port_present"], true);
        assert_eq!(incomplete["version_present"], false);
        assert_eq!(incomplete["pid_valid"], true);
        assert_eq!(incomplete["port_valid"], true);
        assert_eq!(incomplete["log_path"], daemon_dir().expect("daemon dir").join("incomplete.log").display().to_string());
        assert!(incomplete["fix"]
            .as_str()
            .expect("incomplete fix should exist")
            .contains("doctor --quick --fix"));

        drop(healthy_listener);
        drop(incompatible_listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn daemon_checks_return_empty_inventory_when_daemon_dir_is_missing() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("daemon-no-dir");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        let inventory = daemon_checks(&mut checks).expect("inventory should still be present");

        assert!(inventory.sessions.is_empty());
        assert!(inventory.incomplete.is_empty());
        assert!(inventory.cleaned.is_empty());
        assert!(checks
            .iter()
            .any(|check| check.id == "daemon.sessions" && check.status == "info"));
    }
}
