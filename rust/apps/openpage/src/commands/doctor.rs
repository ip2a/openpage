use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};

use crate::browser::{Browser, OPENPAGE_BROWSER_PATH_ENV};
use crate::cli::args::DoctorArgs;
use crate::config::{ConfigValueSource, load_resolved_config, resolve_browser_executable_path};
use crate::error::{OpenPageError, OpenPageResult};
use openpage::daemon::client::{
    daemon_dir, daemon_inventory, daemon_inventory_payload_json, daemon_inventory_readonly,
    daemon_session_fix, daemon_session_reasons, daemon_session_state, force_cleanup_daemon,
    incomplete_daemon_fix, incomplete_daemon_reasons, openpage_home,
};
use openpage::protocol::print_output_json;

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
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fixable: Option<bool>,
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
    log_exists: Option<bool>,
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
            kind: None,
            fix: None,
            auto_fixable: None,
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
            log_exists: None,
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

    fn with_kind(mut self, kind: &'static str) -> Self {
        self.kind = Some(kind);
        self
    }

    fn with_auto_fixable(mut self) -> Self {
        self.auto_fixable = Some(true);
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
        session: &openpage::daemon::client::DaemonSessionInfo,
    ) -> Self {
        self.alive = Some(session.alive);
        self.ready = Some(session.ready);
        self.pid = session.pid;
        self.port = session.port;
        self.version = session.version.clone();
        self.version_matches_current_cli =
            Some(session.version.as_deref() == Some(env!("CARGO_PKG_VERSION")));
        self.log_path = Some(session.log_path.clone());
        self.log_exists = Some(session.log_exists);
        self
    }

    fn with_incomplete_daemon_info(
        mut self,
        incomplete: &openpage::daemon::client::IncompleteDaemonSession,
    ) -> Self {
        self.alive = Some(incomplete.alive);
        self.ready = Some(incomplete.ready);
        self.log_path = Some(incomplete.log_path.clone());
        self.log_exists = Some(incomplete.log_exists);
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

    fn with_log_path(mut self, log_path: impl Into<String>) -> Self {
        self.log_path = Some(log_path.into());
        self
    }

    fn with_log_exists(mut self, log_exists: bool) -> Self {
        self.log_exists = Some(log_exists);
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct FixedAction {
    check_id: String,
    message: String,
    auto_fixable: bool,
    source: &'static str,
    reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl FixedAction {
    fn new(
        check_id: impl Into<String>,
        message: impl Into<String>,
        auto_fixable: bool,
        source: &'static str,
        reason: &'static str,
    ) -> Self {
        Self {
            check_id: check_id.into(),
            message: message.into(),
            auto_fixable,
            source,
            reason,
            session: None,
            path: None,
        }
    }

    fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

pub fn run(args: DoctorArgs) -> OpenPageResult<i32> {
    let payload = doctor_payload(&args)?;
    let success = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
    print_output_json(&payload);
    Ok(if success { 0 } else { 1 })
}

fn doctor_payload(args: &DoctorArgs) -> OpenPageResult<Value> {
    let fixed = if args.fix { apply_fixes()? } else { Vec::new() };
    let mut checks = Vec::new();
    environment_checks(&mut checks);
    let inventory = daemon_checks(&mut checks);
    browser_checks(&mut checks, args.quick);

    let summary = summarize(&checks);
    let success = summary.fail == 0;

    Ok(json!({
        "ok": success,
        "result": {
            "summary": summary,
            "checks": checks,
            "fixed": fixed,
            "inventory": inventory.as_ref().map(doctor_inventory_payload),
        }
    }))
}

fn apply_fixes() -> OpenPageResult<Vec<FixedAction>> {
    let mut fixed = Vec::new();
    fixed.extend(remove_legacy_session_files()?);
    let inventory = daemon_inventory()?;
    for cleaned in inventory.cleaned {
        fixed.push(
            FixedAction::new(
                format!("daemon.cleaned.{}", cleaned.session),
                format!(
                    "Removed stale daemon sidecars for session {} ({})",
                    cleaned.session, cleaned.reason
                ),
                false,
                "inventory_scan",
                "stale_sidecars",
            )
            .with_session(cleaned.session),
        );
    }
    for incomplete in inventory.incomplete {
        if incomplete.ready {
            continue;
        }
        force_cleanup_daemon(&incomplete.session)?;
        fixed.push(
            FixedAction::new(
                format!("daemon.incomplete.{}", incomplete.session),
                format!(
                    "Stopped and removed incomplete unready daemon session {}",
                    incomplete.session
                ),
                true,
                "direct_fix",
                "incomplete_unready_daemon",
            )
            .with_session(incomplete.session),
        );
    }
    for session in inventory.sessions {
        if session.version.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
            continue;
        }
        let found_version = session.version.as_deref().unwrap_or("<missing>");
        force_cleanup_daemon(&session.session)?;
        fixed.push(
            FixedAction::new(
                format!("daemon.session.{}", session.session),
                format!(
                    "Stopped incompatible daemon session {} (found version {}, current CLI {})",
                    session.session,
                    found_version,
                    env!("CARGO_PKG_VERSION")
                ),
                true,
                "direct_fix",
                "incompatible_daemon",
            )
            .with_session(session.session),
        );
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
        if check.auto_fixable == Some(true) {
            summary.fixable += 1;
            summary.fixable_ids.push(check.id.clone());
        }
    }
    summary
}

fn doctor_inventory_payload(inventory: &openpage::daemon::client::DaemonInventory) -> Value {
    let mut payload = daemon_inventory_payload_json(inventory);
    if let Some(cleaned_entries) = payload.get_mut("cleaned").and_then(Value::as_array_mut) {
        for entry in cleaned_entries {
            if let Some(session) = entry.get("session").and_then(Value::as_str) {
                let log_path = entry
                    .get("log_path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let log_exists = entry
                    .get("log_exists")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                entry["fix"] = json!(doctor_cleaned_daemon_fix(session, log_path, log_exists));
            }
        }
    }
    payload
}

fn doctor_cleaned_daemon_fix(session: &str, log_path: &str, log_exists: bool) -> String {
    let inspect = if log_exists {
        format!(
            " To inspect why the session went stale first, run `openpage browser logs --session {session} --tail 20` and inspect {log_path}."
        )
    } else {
        String::new()
    };
    format!(
        "Run `openpage doctor --quick --fix` to remove these stale sidecars.{inspect} If you still need that session, start it again with `openpage browser start --session {session}`."
    )
}

fn environment_checks(checks: &mut Vec<Check>) {
    let category = "Environment";
    let mut openpage_home_is_directory = None;
    let openpage_home_creation_blocker = openpage_home_creation_blocker();

    match openpage_home() {
        Ok(path) => {
            if path.exists() && path.is_dir() {
                openpage_home_is_directory = Some(true);
                checks.push(Check::new(
                    "env.openpage_home",
                    category,
                    Status::Pass,
                    format!("OPENPAGE_HOME resolved to {}", path.display()),
                )
                .with_kind("openpage_home"));
            } else if path.exists() {
                openpage_home_is_directory = Some(false);
                checks.push(
                    Check::new(
                        "env.openpage_home",
                        category,
                        Status::Fail,
                        format!(
                            "OPENPAGE_HOME resolves to {} but it is not a directory",
                            path.display()
                        ),
                    )
                    .with_kind("openpage_home")
                    .with_fix(format!(
                        "Point OPENPAGE_HOME at a writable directory path instead of {}, then retry.",
                        path.display()
                    )),
                );
            } else if let Some(blocker) = &openpage_home_creation_blocker {
                checks.push(
                    Check::new(
                        "env.openpage_home",
                        category,
                        Status::Fail,
                        blocker.clone(),
                    )
                    .with_kind("openpage_home")
                    .with_fix(format!(
                        "Point OPENPAGE_HOME at a writable directory path, or fix permissions on the existing parent of {} before retrying.",
                        path.display()
                    )),
                );
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
                    .with_kind("openpage_home")
                    .with_fix(format!(
                        "Run `openpage browser start --session default --headless https://example.com` once, or create {} manually before using daemon-backed commands.",
                        path.display()
                    )),
                );
            }
        }
        Err(err) => checks.push(
            Check::new("env.openpage_home", category, Status::Fail, err.to_string())
                .with_kind("openpage_home")
                .with_fix(
                    "Set OPENPAGE_HOME to a writable directory, or make sure HOME is defined before running OpenPage.",
                ),
        ),
    }

    match daemon_dir() {
        Ok(path) => {
            if openpage_home_is_directory == Some(false) {
                checks.push(
                    Check::new(
                        "env.daemon_dir",
                        category,
                        Status::Fail,
                        format!(
                            "Daemon sidecar directory cannot be created because OPENPAGE_HOME parent is not a directory: {}",
                            path.display()
                        ),
                    )
                    .with_kind("daemon_dir")
                    .with_fix(
                        "Point OPENPAGE_HOME at a writable directory path first, then rerun `openpage doctor --quick`.",
                    ),
                );
            } else if let Some(blocker) = &openpage_home_creation_blocker {
                checks.push(
                    Check::new("env.daemon_dir", category, Status::Fail, blocker.clone())
                        .with_kind("daemon_dir")
                        .with_fix(format!(
                            "Fix permissions on the existing parent of {} so OpenPage can create daemon sidecars, then rerun `openpage doctor --quick`.",
                            path.display()
                        )),
                );
            } else if path.exists() {
                checks.push(Check::new(
                    "env.daemon_dir",
                    category,
                    Status::Pass,
                    format!("Daemon sidecars live in {}", path.display()),
                )
                .with_kind("daemon_dir"));
            } else {
                checks.push(
                    Check::new(
                        "env.daemon_dir",
                        category,
                        Status::Info,
                        format!("Daemon directory does not exist yet: {}", path.display()),
                    )
                    .with_kind("daemon_dir")
                    .with_fix(format!(
                        "Start any daemon-backed session once so OpenPage can create {}, or create it manually if your workflow requires it ahead of time.",
                        path.display()
                    )),
                );
            }
        }
        Err(err) => checks.push(
            Check::new("env.daemon_dir", category, Status::Fail, err.to_string())
                .with_kind("daemon_dir")
                .with_fix(
                    "Set OPENPAGE_HOME or HOME first so OpenPage can resolve the daemon sidecar directory.",
                ),
        ),
    }

    match legacy_session_files() {
        _ if openpage_home_is_directory == Some(false) => checks.push(
            Check::new(
                "env.legacy_sessions",
                category,
                Status::Warn,
                "Could not inspect legacy session JSON files because OPENPAGE_HOME is not a directory",
            )
            .with_kind("legacy_sessions")
            .with_fix(
                "Point OPENPAGE_HOME at a real directory first, then rerun `openpage doctor --quick` before inspecting legacy session artifacts.",
            ),
        ),
        _ if openpage_home_creation_blocker.is_some() => checks.push(
            Check::new(
                "env.legacy_sessions",
                category,
                Status::Warn,
                "Could not inspect legacy session JSON files because OPENPAGE_HOME cannot be created yet",
            )
            .with_kind("legacy_sessions")
            .with_fix(
                "Fix OPENPAGE_HOME parent permissions first, then rerun `openpage doctor --quick` before inspecting legacy session artifacts.",
            ),
        ),
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
            )
            .with_kind("legacy_sessions"));
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
                .with_kind("legacy_sessions")
                .with_fix(format!(
                    "If you no longer need the old one-shot session artifacts, back them up and remove {}. Keep the directory only if another non-CLI workflow still reads those JSON files.",
                    sessions_dir.display()
                ))
                .with_auto_fixable(),
            );
        }
        Err(err) => checks.push(
            Check::new(
                "env.legacy_sessions",
                category,
                Status::Warn,
                format!("Could not inspect legacy session JSON files: {err}"),
            )
            .with_kind("legacy_sessions")
            .with_fix(
                "Check permissions for OPENPAGE_HOME and inspect the old sessions directory manually. The active TCP daemon CLI path no longer depends on legacy session JSON files.",
            ),
        ),
    }
}

fn daemon_checks(checks: &mut Vec<Check>) -> Option<openpage::daemon::client::DaemonInventory> {
    let category = "Daemon";
    let path = match daemon_dir() {
        Ok(path) => path,
        Err(err) => {
            checks.push(
                Check::new("daemon.dir", category, Status::Fail, err.to_string())
                    .with_kind("daemon_dir")
                    .with_fix(
                        "Resolve the environment error first so OpenPage can locate the daemon sidecar directory.",
                    ),
            );
            return None;
        }
    };

    if !path.exists() {
        checks.push(
            Check::new(
                "daemon.sessions",
                category,
                Status::Info,
                "No daemon directory yet; no sessions to inspect",
            )
            .with_kind("daemon_sessions"),
        );
        return Some(openpage::daemon::client::DaemonInventory::default());
    }

    let inventory = match daemon_inventory_readonly() {
        Ok(inventory) => inventory,
        Err(err) => {
            checks.push(
                Check::new("daemon.sessions", category, Status::Fail, err.to_string())
                    .with_kind("daemon_sessions")
                    .with_fix(format!(
                        "Inspect {} for invalid sidecars or permission issues, then rerun `openpage doctor --quick`.",
                        path.display()
                    )),
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
                    "Detected stale daemon sidecars for session {} ({})",
                    cleaned.session, cleaned.reason
                ),
            )
            .with_session(cleaned.session.clone())
            .with_kind("daemon_session")
            .with_state("cleaned")
            .with_reasons(cleaned.reasons.clone())
            .with_log_path(cleaned.log_path.clone())
            .with_log_exists(cleaned.log_exists)
            .with_fix(doctor_cleaned_daemon_fix(
                &cleaned.session,
                &cleaned.log_path,
                cleaned.log_exists,
            )),
        );
    }

    for incomplete in &inventory.incomplete {
        let reasons = incomplete_daemon_reasons(incomplete);
        let message = if let Some(runtime_issue) = incomplete.runtime_issue {
            format!(
                "Session {} has a runtime health issue: reason={}, alive={}, ready={}, pid_present={}, port_present={}, version_present={}",
                incomplete.session,
                runtime_issue,
                incomplete.alive,
                incomplete.ready,
                incomplete.pid_present,
                incomplete.port_present,
                incomplete.version_present
            )
        } else {
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
            )
        };
        let mut check = Check::new(
            format!("daemon.incomplete.{}", incomplete.session),
            category,
            Status::Warn,
            message,
        )
        .with_session(incomplete.session.clone())
        .with_kind("daemon_session")
        .with_state("incomplete")
        .with_reasons(reasons)
        .with_incomplete_daemon_info(incomplete)
        .with_fix(incomplete_daemon_fix(incomplete));
        if !incomplete.ready {
            check = check.with_auto_fixable();
        }
        checks.push(check);
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
            .with_kind("daemon_sessions")
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
        .with_kind("daemon_session")
        .with_state(state)
        .with_reasons(reasons)
        .with_daemon_session_info(session);

        if let Some(fix) = daemon_session_fix(session) {
            let mut check = check.with_fix(fix);
            if state == "incompatible" {
                check = check.with_auto_fixable();
            }
            checks.push(check);
        } else {
            checks.push(check);
        }
    }

    Some(inventory)
}

fn browser_checks(checks: &mut Vec<Check>, quick: bool) {
    let category = "Browser";
    let openpage_home_creation_blocker = openpage_home_creation_blocker();
    let resolved = match load_resolved_config() {
        Ok(value) => value,
        Err(err) => {
            checks.push(
                Check::new(
                    "browser.config",
                    category,
                    Status::Fail,
                    format!("Could not load config.toml: {err}"),
                )
                .with_kind("browser_config")
                .with_fix(
                    "Check OPENPAGE_CONFIG, ~/.openpage/config.toml, and ./.openpage/config.toml for invalid TOML formatting.",
                ),
            );
            return;
        }
    };
    let options = resolved.launch;
    let browser_path = options.browser_path();
    let browser_path = if browser_path.is_empty() {
        "<default>".to_string()
    } else {
        browser_path
    };
    let source = match resolved.browser_path_source {
        ConfigValueSource::BuiltInDefault => "default",
        ConfigValueSource::UserConfig => "user config.toml",
        ConfigValueSource::WorkspaceConfig => "workspace config.toml",
        ConfigValueSource::Environment => OPENPAGE_BROWSER_PATH_ENV,
    };
    let config_message = format!(
        "Resolved browser config (source={source}, browser_path={}, headless={}, auto_port={}, user_config={}, workspace_config={})",
        browser_path,
        options.is_headless(),
        options.is_auto_port(),
        resolved.user_config_path.display(),
        resolved.workspace_config_path.display(),
    );
    let config_check = if let Some(path) = invalid_openpage_home_path() {
        Check::new(
            "browser.config",
            category,
            Status::Fail,
            format!(
                "{config_message}, but OPENPAGE_HOME {} is not a directory so browser sessions cannot create daemon sidecars",
                path.display()
            ),
        )
        .with_kind("browser_config")
        .with_fix(format!(
            "Point OPENPAGE_HOME at a writable directory path instead of {}, then rerun `openpage doctor --quick`.",
            path.display()
        ))
        .with_browser_path(browser_path.clone())
    } else if let Some(blocker) = &openpage_home_creation_blocker {
        Check::new("browser.config", category, Status::Fail, blocker.clone())
            .with_kind("browser_config")
            .with_fix(
                "Fix OPENPAGE_HOME parent permissions first, then rerun `openpage doctor --quick` before trusting browser launch checks.",
            )
            .with_browser_path(browser_path.clone())
    } else {
        Check::new("browser.config", category, Status::Pass, config_message)
            .with_kind("browser_config")
            .with_browser_path(browser_path.clone())
    };
    checks.push(config_check);

    let browser_exec = resolve_browser_executable(&browser_path);
    let browser_hint = suggested_browser_executable(&browser_path);
    let launch_blocker = openpage_home_creation_blocker.as_deref();
    match &browser_exec {
        BrowserExecutable::Default => checks.push(
            Check::new(
                "browser.executable",
                category,
                Status::Info,
                if let Some(blocker) = launch_blocker {
                    format!(
                        "No explicit browser_path configured; live launch would rely on built-in browser resolution, but {blocker}"
                    )
                } else {
                    "No explicit browser_path configured; live launch will rely on built-in browser resolution".to_string()
                },
            )
            .with_kind("browser_executable")
            .with_browser_path(browser_path.clone()),
        ),
        BrowserExecutable::Found(path) => checks.push({
            let status = if launch_blocker.is_some() || quick {
                Status::Info
            } else {
                Status::Pass
            };
            let message = if let Some(blocker) = launch_blocker {
                format!(
                    "Configured browser executable `{}` resolves to {}, but {blocker}",
                    browser_path,
                    path.display()
                )
            } else if quick {
                format!(
                    "Configured browser executable `{}` resolves to {}, but `doctor --quick` only verified path resolution",
                    browser_path,
                    path.display()
                )
            } else {
                format!(
                    "Configured browser executable `{}` resolves to {}",
                    browser_path,
                    path.display()
                )
            };
            let check = Check::new("browser.executable", category, status, message)
                .with_kind("browser_executable")
                .with_browser_path(browser_path.clone())
                .with_resolved_path(path.display().to_string());
            if launch_blocker.is_some() {
                check.with_fix(
                    "Fix OPENPAGE_HOME parent permissions first, then rerun `openpage doctor --quick` before relying on browser executable resolution.",
                )
            } else if quick {
                check.with_fix(
                    "Rerun `openpage doctor` without --quick when you want a real headless launch smoke test.",
                )
            } else {
                check
            }
        }),
        BrowserExecutable::Missing => {
            let fix = browser_executable_fix(&browser_path, browser_hint.as_deref());
            let mut check = Check::new(
                "browser.executable",
                category,
                Status::Fail,
                missing_browser_message(&browser_path, browser_hint.as_deref()),
            )
            .with_kind("browser_executable")
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
                        "Local browser candidate found at {}. Set browser.executable_path in config.toml to this absolute path.",
                        path.display()
                    ),
                )
                .with_kind("browser_executable")
                .with_browser_path(browser_path.clone())
                .with_suggested_path(path.display().to_string()),
            );
        }
    }

    if quick {
        let message = match launch_blocker {
            Some(blocker) => format!(
                "Skipped live browser launch check because --quick was set, and {blocker}"
            ),
            None => match &browser_exec {
                BrowserExecutable::Found(path) => format!(
                    "Skipped live browser launch check because --quick was set. Configured browser executable `{}` resolves to {}, but it has not been validated as a Chromium browser yet.",
                    browser_path,
                    path.display()
                ),
                BrowserExecutable::Missing => match browser_hint.as_ref() {
                    Some(path) => format!(
                        "Skipped live browser launch check because --quick was set, and the configured browser executable was not found. Local candidate: {}",
                        path.display()
                    ),
                    None => "Skipped live browser launch check because --quick was set, and the configured browser executable was not found".to_string(),
                },
                BrowserExecutable::Default => {
                    "Skipped live browser launch check because --quick was set. Built-in browser resolution has not been validated yet."
                        .to_string()
                }
            },
        };
        checks.push(
            Check::new("browser.launch", category, Status::Info, message)
            .with_kind("browser_launch")
            .with_fix(match launch_blocker {
                Some(_) => "Fix OPENPAGE_HOME parent permissions first, then rerun `openpage doctor` without --quick when you want a real headless launch smoke test.".to_string(),
                None => "Rerun `openpage doctor` without --quick when you want a real headless launch smoke test.".to_string(),
            }),
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
            .with_kind("browser_launch")
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
                )
                .with_kind("browser_launch")),
                Err(err) => checks.push(Check::new(
                    "browser.launch",
                    category,
                    Status::Warn,
                    format!(
                        "Headless browser launch succeeded (version={}, tabs={}, pid={pid:?}) but close failed: {}",
                        version, tabs, err
                    ),
                )
                .with_kind("browser_launch")
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
                .with_kind("browser_launch")
                .with_fix(browser_launch_fix(&browser_path)),
            );
        }
    }
}

fn invalid_openpage_home_path() -> Option<PathBuf> {
    let path = openpage_home().ok()?;
    if path.exists() && !path.is_dir() {
        Some(path)
    } else {
        None
    }
}

fn openpage_home_creation_blocker() -> Option<String> {
    let path = openpage_home().ok()?;
    if path.exists() {
        return None;
    }

    let parent = path
        .ancestors()
        .skip(1)
        .find(|ancestor| ancestor.exists())?;
    let metadata = fs::metadata(parent).ok()?;
    if metadata.permissions().readonly() {
        Some(format!(
            "OPENPAGE_HOME {} cannot be created because its existing parent {} is not writable",
            path.display(),
            parent.display()
        ))
    } else {
        None
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

fn remove_legacy_session_files() -> OpenPageResult<Vec<FixedAction>> {
    let files = legacy_session_files()?;
    let mut removed = Vec::new();
    for path in files {
        match fs::remove_file(&path) {
            Ok(()) => removed.push(
                FixedAction::new(
                    "env.legacy_sessions",
                    format!("Removed legacy session JSON {}", path.display()),
                    true,
                    "direct_fix",
                    "legacy_session_json",
                )
                .with_path(path.display().to_string()),
            ),
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
            "Configured browser executable `{}` was not found. Set browser.executable_path in config.toml to {}, set {}={} for this process, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path,
            path.display(),
            OPENPAGE_BROWSER_PATH_ENV,
            path.display()
        ),
        None => format!(
            "Configured browser executable `{}` was not found. Set browser.executable_path in config.toml, set {}=<absolute-browser-path> for this process, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path, OPENPAGE_BROWSER_PATH_ENV
        ),
    }
}

fn browser_executable_fix(browser_path: &str, hint: Option<&Path>) -> String {
    match hint {
        Some(path) => format!(
            "Set browser.executable_path in config.toml to {} on this machine, set {}={} for a process-local override, or rerun the command with --browser-path {}. If you want to keep `{}` as a name, make sure it resolves on PATH.",
            path.display(),
            OPENPAGE_BROWSER_PATH_ENV,
            path.display(),
            path.display(),
            browser_path
        ),
        None => format!(
            "Set browser.executable_path in config.toml to a real browser executable, set {}=<absolute-browser-path> for a process-local override, or rerun the command with --browser-path <absolute-browser-path>. If you want to keep `{}`, make sure it resolves on PATH.",
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
    if browser_path.is_empty() || browser_path == "<default>" {
        return resolve_browser_executable_path(None);
    }
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
    let mut candidates = crate::config::browser_exec_candidates();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        candidates.push(
            home.join("Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary"),
        );
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
        BrowserLaunchGuard, Check, FixedAction, Status, TempDirGuard, browser_checks,
        browser_executable_fix, browser_launch_fix, daemon_checks, doctor_payload,
        environment_checks, legacy_session_files, missing_browser_message,
        shell_safe_browser_path_arg, suggested_browser_executable_from_known_paths,
    };
    use crate::cli::args::DoctorArgs;
    use openpage::daemon::client::{
        DaemonSessionInfo, daemon_dir, daemon_session_fix, pid_path, port_path, version_path,
    };
    use serde_json::json;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
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

    fn spawn_one_response_daemon() -> (u16, thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test daemon listener");
        let port = listener.local_addr().expect("listener addr").port();
        let handle = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().expect("accept daemon probe");
                let mut request = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    reader
                        .read_line(&mut request)
                        .expect("read daemon probe request");
                }
                if request.trim().is_empty() {
                    continue;
                }
                let response = json!({
                    "ok": true,
                    "result": "about:blank",
                });
                writeln!(stream, "{response}").expect("write daemon probe response");
                return;
            }
            panic!("daemon probe request was not received");
        });
        (port, handle)
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
        assert!(fix.contains("config.toml"));
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
        assert!(with_fix.get("kind").is_none());
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
        assert!(with_fix.get("log_exists").is_none());
        assert!(with_fix.get("auto_fixable").is_none());
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
            Check::new(
                "daemon.session.review",
                "Daemon",
                Status::Warn,
                "version mismatch",
            )
            .with_session("review")
            .with_kind("daemon_session")
            .with_state("incompatible")
            .with_reasons(vec!["version_mismatch"])
            .with_fix("restart session"),
        )
        .expect("serialize check with state and reasons");

        assert_eq!(value["session"], "review");
        assert_eq!(value["kind"], "daemon_session");
        assert_eq!(value["state"], "incompatible");
        assert_eq!(value["reasons"], json!(["version_mismatch"]));
        assert_eq!(value["fix"], "restart session");
    }

    #[test]
    fn check_serializes_auto_fixable_only_when_present() {
        let value = serde_json::to_value(
            Check::new(
                "env.legacy_sessions",
                "Environment",
                Status::Warn,
                "legacy residue",
            )
            .with_kind("legacy_sessions")
            .with_fix("remove files")
            .with_auto_fixable(),
        )
        .expect("serialize auto-fixable check");

        assert_eq!(value["kind"], "legacy_sessions");
        assert_eq!(value["fix"], "remove files");
        assert_eq!(value["auto_fixable"], true);
    }

    #[test]
    fn environment_checks_include_legacy_sessions_kind() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("legacy-kind");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let sessions_dir = home.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("create sessions dir");
        fs::write(sessions_dir.join("legacy-a.json"), "{}").expect("write legacy json");

        let mut checks = Vec::new();
        environment_checks(&mut checks);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let legacy = checks
            .iter()
            .find(|check| check["id"] == "env.legacy_sessions")
            .expect("legacy sessions check should exist");
        let openpage_home = checks
            .iter()
            .find(|check| check["id"] == "env.openpage_home")
            .expect("openpage home check should exist");
        let daemon_dir = checks
            .iter()
            .find(|check| check["id"] == "env.daemon_dir")
            .expect("daemon dir check should exist");

        assert_eq!(openpage_home["kind"], "openpage_home");
        assert_eq!(daemon_dir["kind"], "daemon_dir");
        assert_eq!(legacy["kind"], "legacy_sessions");
        assert_eq!(legacy["auto_fixable"], true);

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn environment_checks_fail_when_openpage_home_is_a_file() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("home-file");
        let parent = home.parent().expect("temp parent").to_path_buf();
        fs::create_dir_all(&parent).expect("create temp parent");
        fs::write(&home, "not a directory").expect("write fake home file");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        environment_checks(&mut checks);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");

        let openpage_home = checks
            .iter()
            .find(|check| check["id"] == "env.openpage_home")
            .expect("openpage home check should exist");
        let daemon_dir = checks
            .iter()
            .find(|check| check["id"] == "env.daemon_dir")
            .expect("daemon dir check should exist");
        let legacy = checks
            .iter()
            .find(|check| check["id"] == "env.legacy_sessions")
            .expect("legacy sessions check should exist");

        assert_eq!(openpage_home["status"], "fail");
        assert!(
            openpage_home["message"]
                .as_str()
                .expect("message string")
                .contains("is not a directory")
        );
        assert_eq!(daemon_dir["status"], "fail");
        assert!(
            daemon_dir["message"]
                .as_str()
                .expect("message string")
                .contains("cannot be created because OPENPAGE_HOME parent is not a directory")
        );
        assert_eq!(legacy["status"], "warn");
        assert!(
            legacy["message"]
                .as_str()
                .expect("message string")
                .contains("OPENPAGE_HOME is not a directory")
        );

        let _ = fs::remove_file(home);
    }

    #[test]
    fn browser_checks_include_stable_kinds_for_core_browser_checks() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let browser_path = std::env::current_exe().expect("current exe path");
        let _browser_guard = EnvVarGuard::set("OPENPAGE_BROWSER_PATH", &browser_path);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");

        let config = checks
            .iter()
            .find(|check| check["id"] == "browser.config")
            .expect("browser config check should exist");
        assert_eq!(config["kind"], "browser_config");

        let executable = checks
            .iter()
            .find(|check| check["id"] == "browser.executable")
            .expect("browser executable check should exist");
        assert_eq!(executable["kind"], "browser_executable");

        let launch = checks
            .iter()
            .find(|check| check["id"] == "browser.launch")
            .expect("browser launch check should exist");
        assert_eq!(launch["kind"], "browser_launch");
    }

    #[test]
    fn browser_checks_fail_config_when_openpage_home_is_a_file() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("browser-home-file");
        let parent = home.parent().expect("temp parent").to_path_buf();
        fs::create_dir_all(&parent).expect("create temp parent");
        fs::write(&home, "not a directory").expect("write fake home file");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let config = checks
            .iter()
            .find(|check| check["id"] == "browser.config")
            .expect("browser config check should exist");

        assert_eq!(config["status"], "fail");
        assert!(
            config["message"]
                .as_str()
                .expect("message string")
                .contains("is not a directory so browser sessions cannot create daemon sidecars")
        );
        assert!(
            config["fix"]
                .as_str()
                .expect("fix string")
                .contains("Point OPENPAGE_HOME at a writable directory path")
        );

        let _ = fs::remove_file(home);
    }

    #[test]
    fn browser_checks_fail_config_when_openpage_home_parent_is_readonly() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let root = unique_openpage_home("browser-readonly-parent-root");
        fs::create_dir_all(&root).expect("create root");
        let parent = root.join("readonly-parent");
        fs::create_dir_all(&parent).expect("create readonly parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o500);
            fs::set_permissions(&parent, permissions).expect("set readonly perms");
        }
        let home = parent.join("home");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let config = checks
            .iter()
            .find(|check| check["id"] == "browser.config")
            .expect("browser config check should exist");

        assert_eq!(config["status"], "fail");
        assert!(
            config["message"]
                .as_str()
                .expect("message string")
                .contains("cannot be created because its existing parent")
        );
        assert!(
            config["fix"]
                .as_str()
                .expect("fix string")
                .contains("Fix OPENPAGE_HOME parent permissions first")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&parent, permissions).expect("restore perms");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_executable_check_does_not_pass_when_openpage_home_parent_is_readonly() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let root = unique_openpage_home("browser-exec-readonly-parent-root");
        fs::create_dir_all(&root).expect("create root");
        let parent = root.join("readonly-parent");
        fs::create_dir_all(&parent).expect("create readonly parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o500);
            fs::set_permissions(&parent, permissions).expect("set readonly perms");
        }
        let home = parent.join("home");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let browser_path = std::env::current_exe().expect("current exe path");
        let _browser_guard = EnvVarGuard::set("OPENPAGE_BROWSER_PATH", &browser_path);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let executable = checks
            .iter()
            .find(|check| check["id"] == "browser.executable")
            .expect("browser executable check should exist");

        assert_eq!(executable["status"], "info");
        assert!(
            executable["message"]
                .as_str()
                .expect("message string")
                .contains("but OPENPAGE_HOME")
        );
        assert!(
            executable["fix"]
                .as_str()
                .expect("fix string")
                .contains("Fix OPENPAGE_HOME parent permissions first")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&parent, permissions).expect("restore perms");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_launch_check_mentions_blocker_when_quick_and_openpage_home_parent_is_readonly() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let root = unique_openpage_home("browser-launch-readonly-parent-root");
        fs::create_dir_all(&root).expect("create root");
        let parent = root.join("readonly-parent");
        fs::create_dir_all(&parent).expect("create readonly parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o500);
            fs::set_permissions(&parent, permissions).expect("set readonly perms");
        }
        let home = parent.join("home");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let launch = checks
            .iter()
            .find(|check| check["id"] == "browser.launch")
            .expect("browser launch check should exist");

        assert_eq!(launch["status"], "info");
        assert!(
            launch["message"]
                .as_str()
                .expect("message string")
                .contains("because --quick was set, and OPENPAGE_HOME")
        );
        assert!(
            launch["fix"]
                .as_str()
                .expect("fix string")
                .contains("Fix OPENPAGE_HOME parent permissions first")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&parent, permissions).expect("restore perms");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_launch_check_mentions_unvalidated_executable_when_quick() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let browser_path = std::env::current_exe().expect("current exe path");
        let _browser_guard = EnvVarGuard::set("OPENPAGE_BROWSER_PATH", &browser_path);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let launch = checks
            .iter()
            .find(|check| check["id"] == "browser.launch")
            .expect("browser launch check should exist");

        assert_eq!(launch["status"], "info");
        assert!(
            launch["message"]
                .as_str()
                .expect("message string")
                .contains("has not been validated as a Chromium browser yet")
        );
    }

    #[test]
    fn browser_executable_check_is_info_when_quick_only_verified_path_resolution() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let browser_path = std::env::current_exe().expect("current exe path");
        let _browser_guard = EnvVarGuard::set("OPENPAGE_BROWSER_PATH", &browser_path);

        let mut checks = Vec::new();
        browser_checks(&mut checks, true);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let executable = checks
            .iter()
            .find(|check| check["id"] == "browser.executable")
            .expect("browser executable check should exist");

        assert_eq!(executable["status"], "info");
        assert!(
            executable["message"]
                .as_str()
                .expect("message string")
                .contains("only verified path resolution")
        );
        assert!(
            executable["fix"]
                .as_str()
                .expect("fix string")
                .contains("Rerun `openpage doctor` without --quick")
        );
    }

    #[test]
    fn environment_checks_fail_when_openpage_home_parent_is_readonly() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let root = unique_openpage_home("readonly-parent-root");
        fs::create_dir_all(&root).expect("create root");
        let parent = root.join("readonly-parent");
        fs::create_dir_all(&parent).expect("create readonly parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o500);
            fs::set_permissions(&parent, permissions).expect("set readonly perms");
        }
        let home = parent.join("home");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        let mut checks = Vec::new();
        environment_checks(&mut checks);
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let openpage_home = checks
            .iter()
            .find(|check| check["id"] == "env.openpage_home")
            .expect("openpage home check should exist");
        let daemon_dir = checks
            .iter()
            .find(|check| check["id"] == "env.daemon_dir")
            .expect("daemon dir check should exist");
        let legacy = checks
            .iter()
            .find(|check| check["id"] == "env.legacy_sessions")
            .expect("legacy sessions check should exist");

        assert_eq!(openpage_home["status"], "fail");
        assert!(
            openpage_home["message"]
                .as_str()
                .expect("message string")
                .contains("cannot be created because its existing parent")
        );
        assert_eq!(daemon_dir["status"], "fail");
        assert!(
            daemon_dir["message"]
                .as_str()
                .expect("message string")
                .contains("cannot be created because its existing parent")
        );
        assert_eq!(legacy["status"], "warn");
        assert!(
            legacy["message"]
                .as_str()
                .expect("message string")
                .contains("OPENPAGE_HOME cannot be created yet")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&parent).expect("metadata").permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&parent, permissions).expect("restore perms");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_check_builders_all_include_kind() {
        let source = include_str!("doctor.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production segment should exist");
        let lines: Vec<&str> = production.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if !line.contains("Check::new(") {
                continue;
            }
            let end = (idx + 24).min(lines.len());
            let block = lines[idx..end].join("\n");
            assert!(
                block.contains(".with_kind("),
                "production Check::new at line {} is missing .with_kind(...)\n{}",
                idx + 1,
                block
            );
        }
    }

    #[test]
    fn production_check_kinds_match_documented_stable_set() {
        let source = include_str!("doctor.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production segment should exist");

        let mut found = std::collections::BTreeSet::new();
        for line in production.lines() {
            let needle = ".with_kind(\"";
            let Some(start) = line.find(needle) else {
                continue;
            };
            let rest = &line[start + needle.len()..];
            let Some(end) = rest.find('"') else {
                continue;
            };
            found.insert(rest[..end].to_string());
        }

        let expected = std::collections::BTreeSet::from([
            "openpage_home".to_string(),
            "daemon_dir".to_string(),
            "legacy_sessions".to_string(),
            "daemon_sessions".to_string(),
            "daemon_session".to_string(),
            "browser_config".to_string(),
            "browser_executable".to_string(),
            "browser_launch".to_string(),
        ]);

        assert_eq!(found, expected);
    }

    #[test]
    fn fixed_action_serializes_machine_fields() {
        let value = serde_json::to_value(
            FixedAction::new(
                "daemon.incomplete.review",
                "Stopped and removed incomplete unready daemon session review",
                true,
                "direct_fix",
                "incomplete_unready_daemon",
            )
            .with_session("review"),
        )
        .expect("serialize fixed action");

        assert_eq!(value["check_id"], "daemon.incomplete.review");
        assert_eq!(
            value["message"],
            "Stopped and removed incomplete unready daemon session review"
        );
        assert_eq!(value["auto_fixable"], true);
        assert_eq!(value["source"], "direct_fix");
        assert_eq!(value["reason"], "incomplete_unready_daemon");
        assert_eq!(value["session"], "review");
        assert!(value.get("path").is_none());
    }

    #[test]
    fn check_serializes_daemon_runtime_fields_when_present() {
        let value = serde_json::to_value(
            Check::new(
                "daemon.session.review",
                "Daemon",
                Status::Warn,
                "version mismatch",
            )
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
                log_exists: true,
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
        assert_eq!(value["log_exists"], true);
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
            log_exists: false,
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
            log_exists: true,
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
            vec![
                FixedAction::new(
                    "env.legacy_sessions",
                    format!("Removed legacy session JSON {}", keep_json.display()),
                    true,
                    "direct_fix",
                    "legacy_session_json",
                )
                .with_path(keep_json.display().to_string()),
            ]
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
        assert!(fixed.iter().any(|entry| {
            entry.check_id == "daemon.cleaned.stale-daemon"
                && entry.session.as_deref() == Some("stale-daemon")
                && !entry.auto_fixable
                && entry.source == "inventory_scan"
                && entry.reason == "stale_sidecars"
                && entry
                    .message
                    .contains("Removed stale daemon sidecars for session stale-daemon")
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
        fs::write(pid_path(session).expect("pid path"), child.id().to_string()).expect("write pid");

        let fixed = super::apply_fixes().expect("apply fixes");
        assert!(fixed.iter().any(|entry| {
            entry.check_id == "daemon.incomplete.incomplete-daemon"
                && entry.session.as_deref() == Some("incomplete-daemon")
                && entry.auto_fixable
                && entry.source == "direct_fix"
                && entry.reason == "incomplete_unready_daemon"
                && entry.message.contains(
                    "Stopped and removed incomplete unready daemon session incomplete-daemon",
                )
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
        fs::write(pid_path(session).expect("pid path"), child.id().to_string()).expect("write pid");
        fs::write(version_path(session).expect("version path"), "0.0.1").expect("write version");

        let fixed = super::apply_fixes().expect("apply fixes");
        assert!(fixed.iter().any(|entry| {
            entry.check_id == "daemon.session.incompatible-daemon"
                && entry.session.as_deref() == Some("incompatible-daemon")
                && entry.auto_fixable
                && entry.source == "direct_fix"
                && entry.reason == "incompatible_daemon"
                && entry
                    .message
                    .contains("Stopped incompatible daemon session incompatible-daemon")
        }));
        let status = child.wait().expect("wait for child to exit");
        assert!(!status.success());
        assert!(!port_path(session).expect("port path").exists());
        assert!(!pid_path(session).expect("pid path").exists());
        assert!(!version_path(session).expect("version path").exists());

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[cfg(unix)]
    #[test]
    fn doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("doctor-fix-payload");
        let _home_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let browser_path = std::env::current_exe().expect("current exe path");
        let _browser_guard = EnvVarGuard::set("OPENPAGE_BROWSER_PATH", &browser_path);

        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");
        fs::create_dir_all(home.join("sessions")).expect("create legacy sessions dir");
        fs::write(home.join("sessions").join("legacy-a.json"), "{}").expect("write legacy json");

        fs::write(port_path("stale").expect("stale port path"), "9").expect("write stale port");
        fs::write(pid_path("stale").expect("stale pid path"), "999999").expect("write stale pid");
        fs::write(
            version_path("stale").expect("stale version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write stale version");

        let mut incomplete_child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn incomplete sleep child");
        fs::write(port_path("incomplete").expect("incomplete port path"), "9")
            .expect("write incomplete port");
        fs::write(
            pid_path("incomplete").expect("incomplete pid path"),
            incomplete_child.id().to_string(),
        )
        .expect("write incomplete pid");

        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind incompatible listener");
        let incompatible_port = listener.local_addr().expect("listener addr").port();
        let mut incompatible_child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn incompatible sleep child");
        fs::write(
            port_path("mismatch").expect("mismatch port path"),
            incompatible_port.to_string(),
        )
        .expect("write mismatch port");
        fs::write(
            pid_path("mismatch").expect("mismatch pid path"),
            incompatible_child.id().to_string(),
        )
        .expect("write mismatch pid");
        fs::write(
            version_path("mismatch").expect("mismatch version path"),
            "0.0.1",
        )
        .expect("write mismatch version");

        let payload = doctor_payload(&DoctorArgs {
            quick: true,
            fix: true,
        })
        .expect("doctor payload with fix");

        assert_eq!(payload["ok"], true);
        assert_eq!(payload["result"]["summary"]["fixable"], 0);
        assert_eq!(payload["result"]["summary"]["fixable_ids"], json!([]));
        assert_eq!(payload["result"]["inventory"]["summary"]["healthy"], 0);
        assert_eq!(payload["result"]["inventory"]["summary"]["incompatible"], 0);
        assert_eq!(payload["result"]["inventory"]["summary"]["incomplete"], 0);
        assert_eq!(payload["result"]["inventory"]["summary"]["cleaned"], 0);
        assert_eq!(payload["result"]["inventory"]["summary"]["total"], 0);

        let fixed = payload["result"]["fixed"]
            .as_array()
            .expect("fixed should be an array");
        assert_eq!(fixed.len(), 4);
        assert!(fixed.iter().any(|entry| {
            entry["check_id"] == "env.legacy_sessions"
                && entry["auto_fixable"] == true
                && entry["source"] == "direct_fix"
                && entry["reason"] == "legacy_session_json"
                && entry["path"]
                    .as_str()
                    .expect("legacy path")
                    .ends_with("legacy-a.json")
        }));
        assert!(fixed.iter().any(|entry| {
            entry["check_id"] == "daemon.cleaned.stale"
                && entry["auto_fixable"] == false
                && entry["source"] == "inventory_scan"
                && entry["reason"] == "stale_sidecars"
                && entry["session"] == "stale"
        }));
        assert!(fixed.iter().any(|entry| {
            entry["check_id"] == "daemon.incomplete.incomplete"
                && entry["auto_fixable"] == true
                && entry["source"] == "direct_fix"
                && entry["reason"] == "incomplete_unready_daemon"
                && entry["session"] == "incomplete"
        }));
        assert!(fixed.iter().any(|entry| {
            entry["check_id"] == "daemon.session.mismatch"
                && entry["auto_fixable"] == true
                && entry["source"] == "direct_fix"
                && entry["reason"] == "incompatible_daemon"
                && entry["session"] == "mismatch"
        }));

        let incomplete_status = incomplete_child.wait().expect("wait incomplete child");
        let incompatible_status = incompatible_child.wait().expect("wait incompatible child");
        assert!(!incomplete_status.success());
        assert!(!incompatible_status.success());

        drop(listener);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn doctor_payload_without_fix_preserves_stale_sidecars() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("doctor-readonly-payload");
        let _home_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");
        fs::write(port_path("stale").expect("stale port path"), "9").expect("write stale port");
        fs::write(pid_path("stale").expect("stale pid path"), "999999").expect("write stale pid");
        fs::write(
            version_path("stale").expect("stale version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write stale version");

        let payload = doctor_payload(&DoctorArgs {
            quick: true,
            fix: false,
        })
        .expect("doctor payload without fix");

        assert_eq!(payload["ok"], true);
        assert_eq!(payload["result"]["inventory"]["summary"]["cleaned"], 1);
        assert!(port_path("stale").expect("stale port path").exists());
        assert!(pid_path("stale").expect("stale pid path").exists());
        assert!(version_path("stale").expect("stale version path").exists());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn summarize_counts_info_fixable_and_total() {
        let checks = vec![
            Check::new("a", "Env", Status::Pass, "ok"),
            Check::new("b", "Env", Status::Warn, "warn")
                .with_fix("do something")
                .with_auto_fixable(),
            Check::new("c", "Env", Status::Fail, "fail").with_fix("do something else"),
            Check::new("d", "Env", Status::Info, "info"),
        ];

        let summary = super::summarize(&checks);
        assert_eq!(summary.pass, 1);
        assert_eq!(summary.warn, 1);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.info, 1);
        assert_eq!(summary.fixable, 1);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.warn_ids, vec![String::from("b")]);
        assert_eq!(summary.fail_ids, vec![String::from("c")]);
        assert_eq!(summary.info_ids, vec![String::from("d")]);
        assert_eq!(summary.fixable_ids, vec![String::from("b")]);
    }

    #[test]
    fn doctor_inventory_payload_includes_state_and_reasons() {
        let inventory = openpage::daemon::client::DaemonInventory {
            sessions: vec![openpage::daemon::client::DaemonSessionInfo {
                session: "alpha".to_string(),
                port: Some(1111),
                pid: Some(2222),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                alive: true,
                ready: true,
                log_path: "/tmp/alpha.log".to_string(),
                log_exists: true,
            }],
            incomplete: vec![openpage::daemon::client::IncompleteDaemonSession {
                session: "beta".to_string(),
                pid_present: true,
                port_present: true,
                version_present: false,
                pid_valid: true,
                port_valid: true,
                alive: true,
                ready: false,
                log_path: "/tmp/beta.log".to_string(),
                log_exists: false,
                runtime_issue: None,
            }],
            cleaned: vec![openpage::daemon::client::CleanedDaemonSession {
                session: "gamma".to_string(),
                reason: "missing version".to_string(),
                reasons: vec!["missing_version"],
                log_path: "/tmp/gamma.log".to_string(),
                log_exists: true,
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
        assert_eq!(payload["sessions"][0]["log_exists"], true);
        assert_eq!(payload["incomplete"][0]["state"], "incomplete");
        assert_eq!(
            payload["incomplete"][0]["reasons"],
            json!(["missing_version", "daemon_not_ready"])
        );
        assert_eq!(payload["incomplete"][0]["log_path"], "/tmp/beta.log");
        assert_eq!(payload["incomplete"][0]["log_exists"], false);
        assert!(
            payload["incomplete"][0]["fix"]
                .as_str()
                .expect("incomplete fix should be present")
                .contains("doctor --quick --fix")
        );
        assert_eq!(payload["cleaned"][0]["state"], "cleaned");
        assert_eq!(payload["cleaned"][0]["reason"], "missing version");
        assert_eq!(payload["cleaned"][0]["reasons"], json!(["missing_version"]));
        assert_eq!(payload["cleaned"][0]["log_path"], "/tmp/gamma.log");
        assert_eq!(payload["cleaned"][0]["log_exists"], true);
        assert!(
            payload["cleaned"][0]["fix"]
                .as_str()
                .expect("cleaned fix should be present")
                .contains("doctor --quick --fix")
        );
        assert!(
            payload["cleaned"][0]["fix"]
                .as_str()
                .expect("cleaned fix should be present")
                .contains("browser logs --session gamma --tail 20")
        );
    }

    #[test]
    fn daemon_checks_include_machine_readable_state_and_reasons() {
        let _guard = test_env_lock().lock().expect("lock test env");
        let home = unique_openpage_home("daemon-check-shapes");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let (healthy_port, healthy_handle) = spawn_one_response_daemon();
        fs::write(
            port_path("healthy").expect("healthy port path"),
            healthy_port.to_string(),
        )
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

        fs::write(port_path("incomplete").expect("incomplete port path"), "9")
            .expect("write incomplete port");
        fs::write(
            pid_path("incomplete").expect("incomplete pid path"),
            std::process::id().to_string(),
        )
        .expect("write incomplete pid");
        fs::write(pid_path("cleaned").expect("cleaned pid path"), "999999")
            .expect("write cleaned pid");
        fs::write(port_path("cleaned").expect("cleaned port path"), "12345")
            .expect("write cleaned port");
        fs::write(
            daemon_dir().expect("daemon dir").join("cleaned.log"),
            "cleaned stale daemon log",
        )
        .expect("write cleaned log");

        let mut checks = Vec::new();
        let inventory = daemon_checks(&mut checks).expect("inventory should be present");
        assert_eq!(inventory.sessions.len(), 2);
        assert_eq!(inventory.incomplete.len(), 1);
        assert_eq!(inventory.cleaned.len(), 1);

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
        assert_eq!(
            healthy["log_path"],
            daemon_dir()
                .expect("daemon dir")
                .join("healthy.log")
                .display()
                .to_string()
        );
        assert_eq!(healthy["kind"], "daemon_session");
        assert_eq!(healthy["log_exists"], false);
        assert!(healthy.get("auto_fixable").is_none());
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
        assert_eq!(
            mismatch["log_path"],
            daemon_dir()
                .expect("daemon dir")
                .join("mismatch.log")
                .display()
                .to_string()
        );
        assert_eq!(mismatch["kind"], "daemon_session");
        assert_eq!(mismatch["log_exists"], false);
        assert_eq!(mismatch["auto_fixable"], true);
        assert!(
            mismatch["fix"]
                .as_str()
                .expect("mismatch fix should exist")
                .contains("browser stop --session mismatch")
        );

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
        assert_eq!(
            incomplete["log_path"],
            daemon_dir()
                .expect("daemon dir")
                .join("incomplete.log")
                .display()
                .to_string()
        );
        assert_eq!(incomplete["kind"], "daemon_session");
        assert_eq!(incomplete["log_exists"], false);
        assert_eq!(incomplete["auto_fixable"], true);
        assert!(
            incomplete["fix"]
                .as_str()
                .expect("incomplete fix should exist")
                .contains("doctor --quick --fix")
        );

        let cleaned = checks
            .iter()
            .find(|check| check["id"] == "daemon.cleaned.cleaned")
            .expect("cleaned check should exist");
        assert!(
            cleaned["message"]
                .as_str()
                .expect("cleaned message should exist")
                .contains("Detected stale daemon sidecars")
        );
        assert_eq!(cleaned["session"], "cleaned");
        assert_eq!(cleaned["kind"], "daemon_session");
        assert_eq!(cleaned["state"], "cleaned");
        assert_eq!(cleaned["reasons"], json!(["missing_version"]));
        assert_eq!(cleaned["log_exists"], true);
        assert!(cleaned.get("auto_fixable").is_none());
        assert_eq!(
            cleaned["log_path"],
            daemon_dir()
                .expect("daemon dir")
                .join("cleaned.log")
                .display()
                .to_string()
        );
        assert!(
            cleaned["fix"]
                .as_str()
                .expect("cleaned fix should exist")
                .contains("doctor --quick --fix")
        );
        assert!(
            cleaned["fix"]
                .as_str()
                .expect("cleaned fix should exist")
                .contains("browser logs --session cleaned --tail 20")
        );
        assert!(pid_path("cleaned").expect("cleaned pid path").exists());
        assert!(port_path("cleaned").expect("cleaned port path").exists());

        healthy_handle.join().expect("healthy probe daemon thread");
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
        let serialized = serde_json::to_value(&checks).expect("serialize checks");
        let checks = serialized.as_array().expect("checks array");
        let daemon_sessions = checks
            .iter()
            .find(|check| check["id"] == "daemon.sessions")
            .expect("daemon sessions check should exist");

        assert_eq!(daemon_sessions["status"], "info");
        assert_eq!(daemon_sessions["kind"], "daemon_sessions");
    }
}
