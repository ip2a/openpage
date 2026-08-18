#[path = "commands/args.rs"]
pub mod args;
#[path = "commands/doctor.rs"]
mod doctor;
#[path = "commands/mcp.rs"]
mod mcp;
#[path = "commands/oneshot.rs"]
mod oneshot;
#[path = "commands/serve.rs"]
mod serve;

use clap::Parser;
use clap::error::{Error as ClapError, ErrorKind};
use std::ffi::OsString;

use crate::cli::args::{Cli, Command};
use crate::error::{OpenPageError, OpenPageResult};
use openpage::protocol::{known_invalid_input_fix, print_output_json};

pub fn run() -> OpenPageResult<i32> {
    run_from_args(std::env::args_os())
}

pub fn run_from_args<I, T>(args: I) -> OpenPageResult<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();

    if openpage_top_level_help_requested(&args) {
        print_openpage_top_level_help()?;
        return Ok(0);
    }

    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => return print_clap_error(err),
    };

    match cli.command {
        Command::Mcp(args) => match mcp::run(args) {
            Ok(()) => Ok(0),
            Err(err) => {
                print_output_json(&openpage::protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
        Command::Serve(args) => match serve::run(args) {
            Ok(()) => Ok(0),
            Err(err) => {
                print_output_json(&openpage::protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
        Command::Doctor(args) => match doctor::run(args) {
            Ok(code) => Ok(code),
            Err(err) => {
                print_output_json(&openpage::protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
        command => match oneshot::run(command) {
            Ok(code) => Ok(code),
            Err(err) => {
                print_output_json(&openpage::protocol::simple_openpage_error(&err));
                Ok(1)
            }
        },
    }
}

fn openpage_top_level_help_requested(args: &[OsString]) -> bool {
    matches!(
        args.get(1).and_then(|value| value.to_str()),
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
        "  tab/frame   Top-level tab and frame commands (also browser downloads/cookies/storage)",
        "  automation  batch, serve",
        "",
        "Bootstrap Rules:",
        "  `browser start` and `goto` may create a missing session.",
        "  Other `--session` commands require an already active session and fail fast.",
        "",
        "Removed On Purpose:",
        "  `serve --stdio`, `page get`, `page url`, `page title`, `page screenshot`",
        "",
        "Next: `openpage help browser`, `openpage help snapshot`, `openpage help click`, `openpage help wait-for-navigation`",
        "",
    ]
    .join("\n")
}

fn print_openpage_top_level_help() -> OpenPageResult<()> {
    print!("{}", openpage_top_level_help_text());
    Ok(())
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
    Some(openpage::protocol::simple_error_with_fix(
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
    use super::{
        clap_error_payload, openpage_top_level_help_requested, openpage_top_level_help_text,
    };
    use crate::cli::args::Cli;
    use clap::{Parser, error::ErrorKind};
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
        assert!(help.contains("Returns one JSON envelope with a `result.commands` array."));
        assert!(help.contains("`--bail` stops after the first failing command."));
    }
}
