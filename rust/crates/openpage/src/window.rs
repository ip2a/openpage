#[cfg(target_os = "macos")]
use std::process::Command;

use crate::error::{OpenPageError, OpenPageResult};
#[cfg(not(target_os = "macos"))]
use crate::settings::window_platform_unsupported_message;
#[cfg(target_os = "macos")]
use crate::settings::window_script_operation_failed_message;

pub fn set_app_visibility(pid: u32, visible: bool) -> OpenPageResult<()> {
    #[cfg(target_os = "macos")]
    {
        run_osascript(&format!(
            "tell application \"System Events\" to set visible of first application process whose unix id is {pid} to {}",
            if visible { "true" } else { "false" }
        ))?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        let _ = visible;
        Err(OpenPageError::UnsupportedOperation(
            window_platform_unsupported_message("hide/show", "隐藏/显示"),
        ))
    }
}

pub fn activate_app(pid: u32) -> OpenPageResult<()> {
    #[cfg(target_os = "macos")]
    {
        run_osascript(&format!(
            "tell application \"System Events\" to set frontmost of first application process whose unix id is {pid} to true"
        ))?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        Err(OpenPageError::UnsupportedOperation(
            window_platform_unsupported_message("activation", "激活"),
        ))
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> OpenPageResult<()> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| window_script_error("run osascript", err))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("osascript exited with status {}", output.status)
    };
    Err(window_script_error("run osascript", detail))
}

#[cfg(target_os = "macos")]
fn window_script_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::BrowserOperation(window_script_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use crate::settings::{Settings, scoped_test_settings, window_platform_unsupported_message};

    #[test]
    fn window_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        assert_eq!(
            window_platform_unsupported_message("hide/show", "隐藏/显示"),
            "window hide/show is only supported on macOS in this build"
        );

        Settings::set_language("cn");

        assert_eq!(
            window_platform_unsupported_message("hide/show", "隐藏/显示"),
            "窗口隐藏/显示在此构建中仅支持 macOS"
        );
    }
}
