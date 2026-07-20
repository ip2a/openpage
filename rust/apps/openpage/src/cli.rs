#[path = "commands/args.rs"]
pub mod args;
#[path = "commands/connection.rs"]
pub mod connection;
#[path = "commands/protocol.rs"]
pub mod protocol;

#[path = "commands/doctor.rs"]
mod doctor;
#[path = "commands/oneshot.rs"]
mod oneshot;
#[path = "commands/serve.rs"]
mod serve;

use clap::Parser;
use clap::error::{Error as ClapError, ErrorKind};
use serde_json::json;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::cli::args::{Cli, Command, CompatCli};
use crate::cli::protocol::{known_invalid_input_fix, print_output_json, simple_ok};
use crate::config::{
    ensure_workspace_config_file, load_resolved_config, update_user_browser_paths,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::{Browser, LaunchOptions};

pub fn run() -> OpenPageResult<i32> {
    run_from_args(std::env::args_os())
}

pub fn run_from_args<I, T>(args: I) -> OpenPageResult<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();

    if should_use_dp_compat_mode(&args) {
        return run_dp_compat_from_args(&args);
    }

    if openpage_top_level_help_requested(&args) {
        print_openpage_top_level_help()?;
        return Ok(0);
    }

    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => return print_clap_error(err),
    };

    match cli.command {
        Command::Serve(args) => match serve::run(args) {
            Ok(()) => Ok(0),
            Err(err) => {
                print_output_json(&protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
        Command::Doctor(args) => match doctor::run(args) {
            Ok(code) => Ok(code),
            Err(err) => {
                print_output_json(&protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
        command => match oneshot::run(command) {
            Ok(code) => Ok(code),
            Err(err) => {
                print_output_json(&protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
    }
}

fn should_use_dp_compat_mode(args: &[OsString]) -> bool {
    if executable_stem(args) != Some("dp") {
        return false;
    }

    if dp_help_requested(args) {
        return true;
    }

    match first_cli_arg(args).and_then(OsStr::to_str) {
        Some(
            "-p" | "--set-browser-path" | "-u" | "--set-user-path" | "-c" | "--configs-to-here"
            | "-l" | "--launch-browser",
        ) => true,
        _ => false,
    }
}

fn dp_help_requested(args: &[OsString]) -> bool {
    if executable_stem(args) != Some("dp") {
        return false;
    }
    matches!(
        first_cli_arg(args).and_then(OsStr::to_str),
        None | Some("-h" | "--help")
    )
}

fn executable_stem(args: &[OsString]) -> Option<&str> {
    let arg0 = args.first()?;
    Path::new(arg0)
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|stem| !stem.is_empty())
}

fn first_cli_arg(args: &[OsString]) -> Option<&OsStr> {
    args.get(1).map(|value| value.as_os_str())
}

fn openpage_top_level_help_requested(args: &[OsString]) -> bool {
    if should_use_dp_compat_mode(args) {
        return false;
    }

    matches!(
        first_cli_arg(args).and_then(OsStr::to_str),
        None | Some("-h" | "--help")
    )
}

fn openpage_top_level_help_text() -> String {
    [
        "OpenPage — Agent-friendly browser automation CLI",
        "",
        "TCP daemon only. Use `openpage help <command>` for full subcommand help.",
        "",
        "Usage:",
        "  openpage browser start --session default --headless https://example.com",
        "  openpage title --session default",
        "  openpage snapshot --session default",
        "  openpage click --session default @e1",
        "  openpage wait-for-navigation --session default --token nav-1",
        "  openpage browser stop --session default",
        "",
        "Start Here:",
        "  doctor      Inspect local config, browser resolution, and daemon sidecars",
        "  browser     Start, stop, list, inspect logs, and check session state",
        "  goto        Bootstrap a missing session and navigate to a URL",
        "  snapshot    Read page state in an agent-friendly text shape",
        "",
        "Common Commands:",
        "  page        title, url, html, snapshot, screenshot, js, reload, back, forward",
        "  actions     click, fill, press, scroll, select, upload, download",
        "  waits       wait, wait-for-navigation, wait-for-ready, wait-for-text",
        "  browser     tab, frame, window, downloads, cookies, storage, permissions",
        "  automation  batch, serve",
        "",
        "Bootstrap Rules:",
        "  `browser start` and `goto` may create a missing session.",
        "  Other `--session` commands require an already active session and fail fast.",
        "",
        "Removed On Purpose:",
        "  `serve --stdio`, `page get`, `page url`, `page title`, `page screenshot`",
        "",
        "Compatibility: `dp` is compatibility glue only. It does not define a second protocol surface.",
        "Next: `openpage help browser`, `openpage help snapshot`, `openpage help click`, `openpage help wait-for-navigation`",
        "",
    ]
    .join("\n")
}

fn print_openpage_top_level_help() -> OpenPageResult<()> {
    print!("{}", openpage_top_level_help_text());
    Ok(())
}

fn run_dp_compat_from_args(args: &[OsString]) -> OpenPageResult<i32> {
    let cli = match CompatCli::try_parse_from(args.iter().cloned()) {
        Ok(cli) => cli,
        Err(err) => return print_clap_error(err),
    };

    run_dp_compat(cli)?;
    Ok(0)
}

fn run_dp_compat(cli: CompatCli) -> OpenPageResult<()> {
    let mut result = serde_json::Map::new();

    if cli.set_browser_path.is_some() || cli.set_user_path.is_some() {
        let saved = update_dp_compat_launch_paths(
            cli.set_browser_path.as_deref(),
            cli.set_user_path.as_deref(),
        )?;
        result.insert(
            "config".to_string(),
            json!({
                "updated": true,
                "saved_to": saved.to_string_lossy(),
                "browser_path": cli.set_browser_path.as_ref().map(|path| path.to_string_lossy().to_string()),
                "user_data_path": cli.set_user_path.as_ref().map(|path| path.to_string_lossy().to_string()),
            }),
        );
    }

    if cli.configs_to_here {
        let copied = ensure_workspace_config_file()?;
        result.insert(
            "configs_to_here".to_string(),
            json!({
                "saved_to": copied.to_string_lossy(),
            }),
        );
    }

    if let Some(port) = cli.launch_browser {
        result.insert(
            "launch_browser".to_string(),
            launch_dp_compat_browser(port)?,
        );
    }

    print_json_value(simple_ok(serde_json::Value::Object(result)))
}

fn update_dp_compat_launch_paths(
    browser_path: Option<&Path>,
    user_data_path: Option<&Path>,
) -> OpenPageResult<PathBuf> {
    update_user_browser_paths(browser_path, user_data_path)
}

fn load_dp_compat_launch_options(
    launch_browser_port: Option<u16>,
) -> OpenPageResult<LaunchOptions> {
    let mut options = load_resolved_config()?.launch;
    if let Some(port) = launch_browser_port.filter(|port| *port > 0) {
        options.set_local_port(port);
    }
    Ok(options)
}

fn launch_dp_compat_browser(launch_browser_port: u16) -> OpenPageResult<serde_json::Value> {
    let options = load_dp_compat_launch_options(Some(launch_browser_port))?;
    let address = options.address();
    let browser = Browser::launch(options)?;
    let result = json!({
        "launched": true,
        "address": address,
        "browser_pid": browser.browser_pid(),
        "headless": browser.is_headless(),
    });
    std::mem::forget(browser);
    Ok(result)
}

fn print_json_value(value: serde_json::Value) -> OpenPageResult<()> {
    print_output_json(&value);
    Ok(())
}

fn clap_error_payload(err: &ClapError) -> Option<serde_json::Value> {
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        return None;
    }

    let detail = err.to_string();
    Some(protocol::simple_error_with_fix(
        "invalid_input",
        detail.clone(),
        known_invalid_input_fix(&detail).map(str::to_string),
    ))
}

fn print_clap_error(err: ClapError) -> OpenPageResult<i32> {
    let exit_code = err.exit_code();
    if let Some(payload) = clap_error_payload(&err) {
        print_json_value(payload)?;
        return Ok(exit_code);
    }

    err.print()
        .map_err(|err| OpenPageError::Io(err.to_string()))?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use clap::{Parser, error::ErrorKind};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        CompatCli, clap_error_payload, dp_help_requested, load_dp_compat_launch_options,
        openpage_top_level_help_requested, openpage_top_level_help_text, should_use_dp_compat_mode,
        update_dp_compat_launch_paths,
    };
    use crate::cli::args::Cli;
    use crate::config::OPENPAGE_CONFIG_ENV;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_ref() {
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("openpage-cli-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn detects_dp_compat_mode_only_for_dp_binary() {
        assert!(should_use_dp_compat_mode(&[
            "dp".into(),
            "--set-browser-path".into(),
            "/tmp/chrome".into(),
        ]));
        assert!(!should_use_dp_compat_mode(&[
            "openpage".into(),
            "--set-browser-path".into(),
            "/tmp/chrome".into(),
        ]));
        assert!(!should_use_dp_compat_mode(&[
            "openpage".into(),
            "browser".into(),
            "start".into(),
        ]));
    }

    #[test]
    fn detects_dp_help_only_for_dp_binary() {
        assert!(dp_help_requested(&["dp".into(), "--help".into()]));
        assert!(dp_help_requested(&["dp".into()]));
        assert!(!dp_help_requested(&["openpage".into(), "--help".into()]));
    }

    #[test]
    fn parses_dp_compat_flags() {
        let cli = CompatCli::try_parse_from([
            "dp",
            "-p",
            "/tmp/chrome",
            "-u",
            "/tmp/user",
            "-c",
            "-l",
            "9333",
        ])
        .expect("parse dp compat cli");

        assert_eq!(
            cli.set_browser_path.as_deref(),
            Some(Path::new("/tmp/chrome"))
        );
        assert_eq!(cli.set_user_path.as_deref(), Some(Path::new("/tmp/user")));
        assert!(cli.configs_to_here);
        assert_eq!(cli.launch_browser, Some(9333));
    }

    #[test]
    fn dp_compat_help_marks_surface_as_compat_only() {
        let err = CompatCli::try_parse_from(["dp", "--help"]).expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        let help = err.to_string();
        assert!(help.contains("Compatibility only."));
        assert!(help.contains("active TCP daemon workflow"));
    }

    #[test]
    fn parse_errors_render_machine_friendly_json_shell() {
        let err = Cli::try_parse_from(["openpage", "page", "url"]).expect_err("parse fails");
        let payload = clap_error_payload(&err).expect("json payload for parse error");
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("string message")
                .contains("unrecognized subcommand 'page'")
        );
        assert!(
            payload["error"]["fix"]
                .as_str()
                .expect("string fix")
                .contains("old `page ...` surface was removed")
        );
    }

    #[test]
    fn removed_stdio_parse_errors_expose_migration_fix() {
        let err = Cli::try_parse_from(["openpage", "serve", "--stdio"]).expect_err("parse fails");
        let payload = clap_error_payload(&err).expect("json payload for parse error");

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("string message")
                .contains("--stdio")
        );
        assert_eq!(
            payload["error"]["fix"],
            "Use `openpage serve --session <name>` for the TCP daemon workflow. The removed `serve --stdio` surface is intentionally rejected."
        );
    }

    #[test]
    fn help_output_keeps_text_shell() {
        let err = Cli::try_parse_from(["openpage", "--help"]).expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        assert!(clap_error_payload(&err).is_none());
    }

    #[test]
    fn detects_openpage_top_level_help_request() {
        assert!(openpage_top_level_help_requested(&["openpage".into()]));
        assert!(openpage_top_level_help_requested(&[
            "openpage".into(),
            "--help".into()
        ]));
        assert!(openpage_top_level_help_requested(&[
            "openpage".into(),
            "-h".into()
        ]));
        assert!(!openpage_top_level_help_requested(&[
            "openpage".into(),
            "browser".into(),
            "--help".into()
        ]));
        assert!(!openpage_top_level_help_requested(&[
            "dp".into(),
            "--help".into()
        ]));
    }

    #[test]
    fn custom_top_level_help_is_shorter_and_points_to_next_steps() {
        let help = openpage_top_level_help_text();
        assert!(help.contains("TCP daemon only."));
        assert!(help.contains("Start Here:"));
        assert!(help.contains("Common Commands:"));
        assert!(help.contains("Bootstrap Rules:"));
        assert!(help.contains("Removed On Purpose:"));
        assert!(help.contains("openpage help browser"));
        assert!(help.lines().count() < 40, "help should stay concise");
    }

    #[test]
    fn openpage_help_marks_tcp_daemon_as_only_active_protocol() {
        let help = openpage_top_level_help_text();
        assert!(help.contains("TCP daemon only."));
        assert!(help.contains("Bootstrap Rules:"));
        assert!(help.contains("already active session"));
        assert!(help.contains("Removed On Purpose:"));
        assert!(help.contains("serve --stdio"));
        assert!(help.contains("page title"));
        assert!(help.contains("`dp` is compatibility glue only."));
    }

    #[test]
    fn serve_help_marks_stdio_surface_as_removed() {
        let err =
            Cli::try_parse_from(["openpage", "serve", "--help"]).expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        let help = err.to_string();
        assert!(help.contains("TCP-only daemon mode."));
        assert!(help.contains("serve --stdio"));
        assert!(help.contains("active OpenPage daemon protocol remains unique"));
    }

    #[test]
    fn browser_start_help_points_to_examples_and_follow_up() {
        let err = Cli::try_parse_from(["openpage", "browser", "start", "--help"])
            .expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        let help = err.to_string();
        assert!(help.contains("Examples:"));
        assert!(help.contains("openpage browser start --session review --headless"));
        assert!(help.contains("openpage wait-for-ready --session review"));
        assert!(help.contains("openpage goto --session review https://example.com"));
        assert!(help.contains("Bootstrap rule:"));
    }

    #[test]
    fn goto_help_points_to_bootstrap_and_wait_follow_up() {
        let err =
            Cli::try_parse_from(["openpage", "goto", "--help"]).expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        let help = err.to_string();
        assert!(help.contains("Bootstrap behavior:"));
        assert!(help.contains("openpage goto --session review --wait https://example.com"));
        assert!(help.contains("wait_for_navigation.command"));
        assert!(help.contains("openpage wait-for-ready --session review"));
    }

    #[test]
    fn batch_help_points_to_examples_stdin_shape_and_output_contract() {
        let err =
            Cli::try_parse_from(["openpage", "batch", "--help"]).expect_err("help exits early");
        assert!(matches!(err.kind(), ErrorKind::DisplayHelp));
        let help = err.to_string();
        assert!(help.contains("Examples:"));
        assert!(help.contains("openpage batch --bail"));
        assert!(help.contains("[ [\"browser\",\"start\""));
        assert!(help.contains("Each command writes its own JSON result as a separate line."));
        assert!(help.contains("`--bail` stops after the first failing command."));
    }

    #[test]
    fn dp_compat_path_update_persists_browser_and_user_data_paths() {
        let dir = temp_dir("compat-save");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "[browser]\n").expect("seed config");
        let _config_guard =
            EnvVarGuard::set(OPENPAGE_CONFIG_ENV, config_path.to_string_lossy().as_ref());

        let saved = update_dp_compat_launch_paths(
            Some(Path::new("/tmp/compat-browser")),
            Some(Path::new("/tmp/compat-user")),
        )
        .expect("update compat config");

        let loaded = fs::read_to_string(&config_path).expect("read config");
        assert_eq!(saved, config_path);
        assert!(loaded.contains("executable_path = \"/tmp/compat-browser\""));
        assert!(loaded.contains("user_data_dir = \"/tmp/compat-user\""));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dp_compat_launch_options_keep_configured_port_for_zero_and_override_nonzero() {
        let dir = temp_dir("compat-port");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "[browser]\nexecutable_path = \"/tmp/compat-browser\"\n",
        )
        .expect("seed config");
        let _config_guard =
            EnvVarGuard::set(OPENPAGE_CONFIG_ENV, config_path.to_string_lossy().as_ref());

        let keep = load_dp_compat_launch_options(Some(0)).expect("load config port");
        let override_port = load_dp_compat_launch_options(Some(9333)).expect("load override port");

        assert_eq!(keep.remote_debugging_port, Some(9222));
        assert_eq!(override_port.address(), "127.0.0.1:9333");
        assert_eq!(override_port.remote_debugging_port, Some(9333));

        let _ = fs::remove_dir_all(dir);
    }
}
