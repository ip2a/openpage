use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;

use crate::browser::{Browser, LaunchOptions};
use crate::cli::args::DoctorArgs;
use crate::cli::connection::{DaemonSessionInfo, daemon_dir, daemon_inventory, openpage_home};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
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
        }
    }

    fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
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

fn daemon_checks(checks: &mut Vec<Check>) {
    let category = "Daemon";
    let path = match daemon_dir() {
        Ok(path) => path,
        Err(err) => {
            checks.push(
                Check::new("daemon.dir", category, Status::Fail, err.to_string()).with_fix(
                    "Resolve the environment error first so OpenPage can locate the daemon sidecar directory.",
                ),
            );
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
            checks.push(
                Check::new("daemon.sessions", category, Status::Fail, err.to_string()).with_fix(
                    format!(
                        "Inspect {} for invalid sidecars or permission issues, then rerun `openpage doctor --quick`.",
                        path.display()
                    ),
                ),
            );
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
            .with_fix(format!(
                "Run `openpage browser status --session {0}` to inspect the session. If it is no longer needed, run `openpage browser stop --session {0}` and rerun `openpage doctor --quick`.",
                incomplete.session
            )),
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

        let check = Check::new(
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
        );

        if let Some(fix) = daemon_session_fix(session) {
            checks.push(check.with_fix(fix));
        } else {
            checks.push(check);
        }
    }
}

fn daemon_session_fix(session: &DaemonSessionInfo) -> Option<String> {
    let version_matches = matches!(
        session.version.as_deref(),
        Some(version) if version == env!("CARGO_PKG_VERSION")
    );

    if !version_matches {
        return Some(format!(
            "Run `openpage browser stop --session {0}` and then restart that session with the current CLI so its daemon sidecars are recreated with version {1}.",
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
        BrowserExecutable::Missing => {
            let fix = browser_executable_fix(&browser_path, browser_hint.as_deref());
            checks.push(
                Check::new(
                    "browser.executable",
                    category,
                    Status::Fail,
                    missing_browser_message(&browser_path, browser_hint.as_deref()),
                )
                .with_fix(fix),
            )
        }
    }

    if matches!(browser_exec, BrowserExecutable::Missing) {
        if let Some(path) = browser_hint.as_ref() {
            checks.push(Check::new(
                "browser.executable.hint",
                category,
                Status::Info,
                format!(
                    "Local browser candidate found at {}. Setting rust/configs.ini browser_path to this absolute path should work on this machine.",
                    path.display()
                ),
            ));
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
            "Configured browser executable `{}` was not found. Update rust/configs.ini browser_path to {}, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path,
            path.display()
        ),
        None => format!(
            "Configured browser executable `{}` was not found. Update rust/configs.ini browser_path, install the browser on PATH, or pass --browser-path explicitly.",
            browser_path
        ),
    }
}

fn browser_executable_fix(browser_path: &str, hint: Option<&Path>) -> String {
    match hint {
        Some(path) => format!(
            "Set rust/configs.ini browser_path to {} on this machine, or rerun the command with --browser-path {}. If you want to keep `{}` as a name, make sure it resolves on PATH.",
            path.display(),
            path.display(),
            browser_path
        ),
        None => format!(
            "Update rust/configs.ini browser_path to a real browser executable, or rerun the command with --browser-path <absolute-browser-path>. If you want to keep `{}`, make sure it resolves on PATH.",
            browser_path
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
        browser_launch_fix, daemon_session_fix, legacy_session_files, missing_browser_message,
        shell_safe_browser_path_arg, suggested_browser_executable_from_known_paths,
    };
    use crate::cli::connection::DaemonSessionInfo;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
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
}
