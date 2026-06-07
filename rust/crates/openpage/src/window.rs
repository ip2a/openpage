use std::process::Command;

use crate::error::{OpenPageError, OpenPageResult};

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
            "window hide/show is only supported on macOS in this build".to_string(),
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
            "window activation is only supported on macOS in this build".to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> OpenPageResult<()> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
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
    Err(OpenPageError::BrowserOperation(detail))
}
