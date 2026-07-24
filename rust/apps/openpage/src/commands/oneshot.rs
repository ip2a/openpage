use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::cli::args::{
    AlertCommand, AttrArgs, BatchArgs, BrowserCommand, BrowserLogsArgs, BrowserStartArgs,
    BrowserStopArgs, ClearCacheArgs, Cli, ClickAtArgs, ClickForNewTabArgs, ClickToDownloadArgs,
    ClickToUploadArgs, ClipboardCommand, Command, CookiesCommand, DiffCommand, DownloadArgs,
    DownloadsCancelArgs, DownloadsCommand, DownloadsModeArgs, DownloadsOpenArgs, DownloadsPathArgs,
    DragArgs, DragInArgs, DragToArgs, DragToPointArgs, ElementArgs, ElementScrollArgs, FillArgs,
    FindInPageArgs, FrameCommand, GotoArgs, HistoryCommand, HoverAtArgs, InterceptCommand, JsArgs,
    KeyArgs, LocateArgs, OpenLinkArgs, PageTextArgs, PdfArgs, PermissionSetArgs,
    PermissionsCommand, PressArgs, RecorderCommand, ReloadArgs, SaveArgs, ScreenshotArgs,
    ScreenshotElementArgs, ScrollArgs, ScrollIntoViewArgs, SelectArgs, SelectRangeArgs,
    SelectTextArgs, SessionArgs, ShortcutArgs, SnapshotArgs, StorageCommand, StorageScope,
    TabCommand, TabDuplicateArgs, TabReopenArgs, TypeWithIntervalArgs, UploadArgs, WaitArgs,
    WaitElementArgs, WaitElementsLoadedArgs, WaitForDownloadArgs, WaitForFunctionArgs,
    WaitForNavigationArgs, WaitForTextArgs, WaitForTitleArgs, WaitForUrlArgs, WaitTimeoutArgs,
    WindowCloseArgs, WindowCommand, WindowMoveArgs, WindowSwitchArgs, ZoomCommand, ZoomSetArgs,
    ZoomStepArgs,
};
use crate::error::{OpenPageError, OpenPageResult};
use openpage::daemon::client::{
    daemon_dir, daemon_inventory, daemon_inventory_payload_json, daemon_status_payload_for_session,
    read_port, send_request, send_request_existing, shutdown_daemon,
};
#[cfg(test)]
use openpage::daemon::client::{daemon_inventory_summary_json, incomplete_daemon_reasons};
use openpage::protocol::{Request, Response, print_output_json, simple_ok};

pub fn run(command: Command) -> OpenPageResult<i32> {
    match command {
        Command::Batch(args) => run_batch(args),
        command => {
            run_single(command)?;
            Ok(0)
        }
    }
}

fn run_single(command: Command) -> OpenPageResult<()> {
    match command {
        Command::Browser(command) => run_browser(command),
        Command::Record(command) => run_recorder(command),
        Command::Goto(args) => run_goto(args),
        Command::Back(args) => run_back(args),
        Command::Forward(args) => run_forward(args),
        Command::Reload(args) => run_reload(args),
        Command::StopLoading(args) => run_stop_loading(args),
        Command::Url(args) => run_url(args),
        Command::Title(args) => run_title(args),
        Command::UserAgent(args) => run_user_agent(args),
        Command::StatusCode(args) => run_status_code(args),
        Command::ReadyState(args) => run_ready_state(args),
        Command::IsLoading(args) => run_is_loading(args),
        Command::IsHeadless(args) => run_is_headless(args),
        Command::Html(args) => run_html(args),
        Command::Snapshot(args) => run_snapshot(args),
        Command::Diff(command) => run_diff(command),
        Command::Screenshot(args) => run_screenshot(args),
        Command::ScreenshotElement(args) => run_screenshot_element(args),
        Command::Click(args) => run_click(args),
        Command::Fill(args) => run_fill(args),
        Command::Focus(args) => run_focus(args),
        Command::Clear(args) => run_clear(args),
        Command::Submit(args) => run_submit(args),
        Command::Check(args) => run_check(args),
        Command::Uncheck(args) => run_uncheck(args),
        Command::RightClick(args) => run_right_click(args),
        Command::MiddleClick(args) => run_middle_click(args),
        Command::DoubleClick(args) => run_double_click(args),
        Command::ClickAt(args) => run_click_at(args),
        Command::KeyDown(args) => run_key_down(args),
        Command::KeyUp(args) => run_key_up(args),
        Command::Shortcut(args) => run_shortcut(args),
        Command::SelectAll(args) => run_shortcut_action(args, "a", "selected_all"),
        Command::Copy(args) => run_shortcut_action(args, "c", "copied"),
        Command::Cut(args) => run_shortcut_action(args, "x", "cut"),
        Command::Paste(args) => run_shortcut_action(args, "v", "pasted"),
        Command::Clipboard(command) => run_clipboard(command),
        Command::Undo(args) => run_shortcut_action(args, "z", "undone"),
        Command::Redo(args) => run_shortcut_action(args, "y", "redone"),
        Command::Input(args) => run_input(args),
        Command::Type(args) => run_type(args),
        Command::TypeWithInterval(args) => run_type_with_interval(args),
        Command::Drag(args) => run_drag(args),
        Command::DragTo(args) => run_drag_to(args),
        Command::DragToPoint(args) => run_drag_to_point(args),
        Command::DragIn(args) => run_drag_in(args),
        Command::Text(args) => run_text(args),
        Command::Value(args) => run_value(args),
        Command::RawText(args) => run_raw_text(args),
        Command::Link(args) => run_link(args),
        Command::OpenLink(args) => run_open_link(args),
        Command::ChildCount(args) => run_child_count(args),
        Command::CssPath(args) => run_css_path(args),
        Command::Xpath(args) => run_xpath(args),
        Command::ElementHtml(args) => run_element_html(args),
        Command::SelectedText(args) => run_selected_text(args),
        Command::Attr(args) => run_attr(args),
        Command::Wait(args) => run_wait(args),
        Command::Intercept(command) => run_intercept(command),
        Command::Js(args) => run_js(args),
        Command::Download(args) => run_download(args),
        Command::Downloads(command) => run_downloads(command),
        Command::Zoom(command) => run_zoom(command),
        Command::Window(command) => run_window(command),
        Command::Alert(command) => run_alert(command),
        Command::Scroll(args) => run_scroll(args),
        Command::ScrollPosition(args) => run_scroll_position(args),
        Command::ScrollElement(args) => run_scroll_element(args),
        Command::ScrollElementPosition(args) => run_scroll_element_position(args),
        Command::ScrollIntoView(args) => run_scroll_into_view(args),
        Command::Hover(args) => run_hover(args),
        Command::HoverAt(args) => run_hover_at(args),
        Command::Press(args) => run_press(args),
        Command::Select(args) => run_select(args),
        Command::OptionTexts(args) => run_option_texts(args),
        Command::SelectedOption(args) => run_selected_option(args),
        Command::SelectedOptions(args) => run_selected_options(args),
        Command::SelectAllOptions(args) => run_select_all_options(args),
        Command::ClearSelectedOptions(args) => run_clear_selected_options(args),
        Command::InvertSelectedOptions(args) => run_invert_selected_options(args),
        Command::SelectText(args) => run_select_text(args),
        Command::SelectRange(args) => run_select_range(args),
        Command::Upload(args) => run_upload(args),
        Command::ClickToDownload(args) => run_click_to_download(args),
        Command::ClickToUpload(args) => run_click_to_upload(args),
        Command::ClickForNewTab(args) => run_click_for_new_tab(args),
        Command::IsVisible(args) => run_is_visible(args),
        Command::IsEnabled(args) => run_is_enabled(args),
        Command::IsChecked(args) => run_is_checked(args),
        Command::IsSelected(args) => run_is_selected(args),
        Command::IsAlive(args) => run_is_alive(args),
        Command::IsInViewport(args) => run_is_in_viewport(args),
        Command::IsWholeInViewport(args) => run_is_whole_in_viewport(args),
        Command::IsCovered(args) => run_is_covered(args),
        Command::IsClickable(args) => run_is_clickable(args),
        Command::HasRect(args) => run_has_rect(args),
        Command::Find(args) => run_find(args),
        Command::FindInPage(args) => run_find_in_page(args),
        Command::FindAll(args) => run_find_all(args),
        Command::Locate(args) => run_locate(args),
        Command::Count(args) => run_count(args),
        Command::WaitVisible(args) => run_wait_visible(args),
        Command::WaitHidden(args) => run_wait_hidden(args),
        Command::WaitEnabled(args) => run_wait_enabled(args),
        Command::WaitDisabled(args) => run_wait_disabled(args),
        Command::WaitDeleted(args) => run_wait_deleted(args),
        Command::WaitClickable(args) => run_wait_clickable(args),
        Command::WaitHasRect(args) => run_wait_has_rect(args),
        Command::WaitCovered(args) => run_wait_covered(args),
        Command::WaitNotCovered(args) => run_wait_not_covered(args),
        Command::WaitStopMoving(args) => run_wait_stop_moving(args),
        Command::ActiveElement(args) => run_active_element(args),
        Command::WaitForNewTab(args) => run_wait_for_new_tab(args),
        Command::WaitForDownloadBegin(args) => run_wait_for_download_begin(args),
        Command::WaitForDownloadsDone(args) => run_wait_for_downloads_done(args),
        Command::WaitForAlertClosed(args) => run_wait_for_alert_closed(args),
        Command::WaitForLoadStart(args) => run_wait_for_load_start(args),
        Command::WaitForDocLoaded(args) => run_wait_for_doc_loaded(args),
        Command::WaitForReady(args) => run_wait_for_ready(args),
        Command::WaitForNavigation(args) => run_wait_for_navigation(args),
        Command::WaitForUrl(args) => run_wait_for_url(args),
        Command::WaitForTitle(args) => run_wait_for_title(args),
        Command::WaitForElementsLoaded(args) => run_wait_for_elements_loaded(args),
        Command::WaitForFunction(args) => run_wait_for_function(args),
        Command::WaitForText(args) => run_wait_for_text(args),
        Command::WaitDisabledOrDeleted(args) => run_wait_disabled_or_deleted(args),
        Command::WaitUploadPathsInputted(args) => run_wait_upload_paths_inputted(args),
        Command::Save(args) => run_save(args),
        Command::Pdf(args) => run_pdf(args),
        Command::History(command) => run_history(command),
        Command::Storage(command) => run_storage(command),
        Command::Permissions(command) => run_permissions(command),
        Command::ClearCache(args) => run_clear_cache(args),
        Command::Cookies(command) => run_cookies(command),
        Command::Tab(command) => run_tab(command),
        Command::Frame(command) => run_frame(command),
        Command::Doctor(_) => unreachable!("doctor is handled by cli::doctor"),
        Command::Batch(_) => unreachable!("batch is handled by oneshot::run"),
        Command::Serve(_) | Command::Mcp(_) => unreachable!("handled by cli entrypoint"),
    }
}

fn run_scroll(args: ScrollArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.scroll",
        json!({
            "direction": args.direction.clone(),
            "pixels": args.pixels,
            "x": args.x,
            "y": args.y,
        }),
    )?;
    print_json(simple_ok(
        json!({"scrolled": true, "direction": args.direction}),
    ))
}

fn run_scroll_position(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.scroll_position",
        Value::Null,
    )?))
}

fn run_scroll_element(args: ElementScrollArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.scroll",
        json!({
            "locator": args.locator,
            "direction": args.direction.clone(),
            "pixels": args.pixels,
            "x": args.x,
            "y": args.y,
        }),
    )?;
    print_json(simple_ok(json!({
        "scrolled": true,
        "direction": args.direction,
    })))
}

fn run_scroll_element_position(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.scroll_position",
        json!({"locator": args.locator}),
    )?))
}

fn run_recorder(command: RecorderCommand) -> OpenPageResult<()> {
    match command {
        RecorderCommand::Start(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "recorder.start",
            Value::Null,
        )?)),
        RecorderCommand::Replay(args) => {
            let flow = serde_json::from_slice::<Value>(&fs::read(&args.flow)?)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            print_json(simple_ok(rpc_page(&args.session, "recorder.replay", flow)?))
        }
        RecorderCommand::Stop(args) => {
            let flow = rpc_page(&args.session, "recorder.stop", Value::Null)?;
            if let Some(output) = args.output {
                let bytes = serde_json::to_vec_pretty(&flow)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
                std::fs::write(&output, bytes)?;
                print_json(simple_ok(json!({"output": output, "flow": flow})))
            } else {
                print_json(simple_ok(flow))
            }
        }
        RecorderCommand::Steps(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "recorder.steps",
            Value::Null,
        )?)),
        RecorderCommand::Status(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "recorder.status",
            Value::Null,
        )?)),
        RecorderCommand::Clear(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "recorder.clear",
            Value::Null,
        )?)),
    }
}

fn run_browser(command: BrowserCommand) -> OpenPageResult<()> {
    match command {
        BrowserCommand::Start(args) => start_browser(args),
        BrowserCommand::Stop(args) => stop_browser(args, false),
        BrowserCommand::Activate(args) => {
            let _ = rpc_page(&args.session, "page.activate", Value::Null)?;
            print_json(simple_ok(json!({"activated": true})))
        }
        BrowserCommand::IsIncognito(args) => {
            let is_incognito = rpc_page(&args.session, "page.is_incognito", Value::Null)?
                .get("is_incognito")
                .cloned();
            print_json(simple_ok(json!({"is_incognito": is_incognito})))
        }
        BrowserCommand::Logs(args) => run_browser_logs(args),
        BrowserCommand::List => {
            let inventory = daemon_inventory()?;
            print_json(simple_ok(browser_inventory_payload(&inventory)))
        }
        BrowserCommand::Status(args) => {
            let status = daemon_status_payload_for_session(&args.session)?;
            print_json(simple_ok(status))
        }
    }
}

fn browser_stop_all_sessions(inventory: &openpage::daemon::client::DaemonInventory) -> Vec<String> {
    let mut sessions = std::collections::BTreeSet::new();
    for session in &inventory.sessions {
        sessions.insert(session.session.clone());
    }
    for session in &inventory.incomplete {
        if session.alive {
            sessions.insert(session.session.clone());
        }
    }
    sessions.into_iter().collect()
}

#[cfg(test)]
fn browser_inventory_summary(
    inventory: &openpage::daemon::client::DaemonInventory,
) -> serde_json::Value {
    daemon_inventory_summary_json(inventory)
}

fn browser_inventory_payload(
    inventory: &openpage::daemon::client::DaemonInventory,
) -> serde_json::Value {
    daemon_inventory_payload_json(inventory)
}

#[cfg(test)]
fn incomplete_session_reasons(
    incomplete: &openpage::daemon::client::IncompleteDaemonSession,
) -> Vec<&'static str> {
    incomplete_daemon_reasons(incomplete)
}

fn run_browser_logs(args: BrowserLogsArgs) -> OpenPageResult<()> {
    let status = daemon_status_payload_for_session(&args.session)?;
    let path = PathBuf::from(
        status
            .get("log_path")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let log_exists = status
        .get("log_exists")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| path.exists());
    let content = if log_exists {
        Some(read_browser_log(&path, args.tail)?)
    } else {
        None
    };
    print_json(simple_ok(browser_logs_payload(
        status, log_exists, args.tail, content,
    )))
}

fn read_browser_log(path: &Path, tail: Option<usize>) -> OpenPageResult<String> {
    let content = fs::read_to_string(path)?;
    Ok(match tail {
        Some(limit) => tail_log_lines(&content, limit),
        None => content,
    })
}

fn browser_logs_payload(
    mut status: Value,
    log_exists: bool,
    tail: Option<usize>,
    content: Option<String>,
) -> Value {
    if status.get("kind").is_none() {
        status["kind"] = Value::from("daemon_session");
    }
    let log_empty = log_exists
        && content
            .as_deref()
            .map(|value| value.is_empty())
            .unwrap_or(false);
    let path = status
        .get("log_path")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    status["path"] = path;
    status["log_exists"] = Value::Bool(log_exists);
    status["exists"] = Value::Bool(log_exists);
    status["log_empty"] = Value::Bool(log_empty);
    status["tail"] = json!(tail);
    status["content"] = json!(content);
    if log_empty {
        let hint = if status.get("state").and_then(Value::as_str) == Some("inactive") {
            "Log file exists but is empty. The previous startup may have failed before anything was written to stderr; rely on the original browser_launch error or rerun `openpage doctor` for a live launch smoke test."
        } else {
            "Log file exists but is empty. The process may have exited before writing anything to stderr."
        };
        status["log_hint"] = Value::from(hint);
    }
    status
}

fn tail_log_lines(content: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(limit);
    lines[start..].join("\n")
}

fn run_goto(args: GotoArgs) -> OpenPageResult<()> {
    ensure_page_session(&args.session)?;
    let result = rpc_page(
        &args.session,
        "page.goto",
        json!({
            "url": args.url,
            "wait": args.wait,
            "timeout_ms": 10_000,
        }),
    )?;
    print_page_json(
        &args.session,
        json!({
            "loaded": true,
            "url": result.get("url").cloned(),
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_back(args: SessionArgs) -> OpenPageResult<()> {
    let result = rpc_page(&args.session, "page.back", Value::Null)?;
    let navigated = result.get("back").and_then(Value::as_bool).unwrap_or(false);
    if navigated {
        let _ = rpc_page(
            &args.session,
            "wait.doc_loaded",
            json!({"timeout_ms": 10_000}),
        )?;
    }
    let url = rpc_page(&args.session, "page.url", Value::Null)?;
    print_page_json(
        &args.session,
        json!({
            "back": navigated,
            "url": url.get("url").cloned(),
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_forward(args: SessionArgs) -> OpenPageResult<()> {
    let result = rpc_page(&args.session, "page.forward", Value::Null)?;
    let navigated = result
        .get("forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if navigated {
        let _ = rpc_page(
            &args.session,
            "wait.doc_loaded",
            json!({"timeout_ms": 10_000}),
        )?;
    }
    let url = rpc_page(&args.session, "page.url", Value::Null)?;
    print_page_json(
        &args.session,
        json!({
            "forward": navigated,
            "url": url.get("url").cloned(),
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_reload(args: ReloadArgs) -> OpenPageResult<()> {
    let result = rpc_page(
        &args.session,
        "page.reload",
        json!({
            "timeout_ms": 10_000,
            "ignore_cache": args.ignore_cache,
        }),
    )?;
    let url = rpc_page(&args.session, "page.url", Value::Null)?;
    print_page_json(
        &args.session,
        json!({
            "reloaded": true,
            "ignore_cache": args.ignore_cache,
            "url": url.get("url").cloned(),
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_stop_loading(args: SessionArgs) -> OpenPageResult<()> {
    let _ = rpc_page(&args.session, "page.stop_loading", Value::Null)?;
    print_json(simple_ok(json!({"stopped_loading": true})))
}

fn run_url(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(&args.session, "page.url", Value::Null)?))
}

fn run_title(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.title",
        Value::Null,
    )?))
}

fn run_user_agent(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.user_agent",
        Value::Null,
    )?))
}

fn run_status_code(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.status_code",
        Value::Null,
    )?))
}

fn run_ready_state(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.ready_state",
        Value::Null,
    )?))
}

fn run_is_loading(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.is_loading",
        Value::Null,
    )?))
}

fn run_is_headless(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.is_headless",
        Value::Null,
    )?))
}

fn run_html(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.html",
        Value::Null,
    )?))
}

fn run_snapshot(args: SnapshotArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.snapshot",
        json!({
            "mode": args.mode.as_str(),
            "format": args.format.as_str(),
            "raw": args.raw,
            "depth": args.depth,
            "selector": args.selector,
        }),
    )?))
}

fn run_screenshot(args: ScreenshotArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.screenshot",
        json!({
            "path": args.output,
            "full_page": args.full_page,
        }),
    )?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_screenshot_element(args: ScreenshotElementArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.screenshot",
        json!({
            "locator": args.locator,
            "path": args.output,
        }),
    )?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_click(args: ElementArgs) -> OpenPageResult<()> {
    print_page_result(
        &args.session,
        rpc_page(
            &args.session,
            "element.click",
            json!({"locator": args.locator}),
        )?,
    )
}

fn run_fill(args: FillArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.input",
        json!({"locator": args.locator, "text": args.text}),
    )?))
}

fn run_focus(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.focus",
        json!({"locator": args.locator}),
    )?))
}

fn run_clear(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.clear",
        json!({"locator": args.locator}),
    )?))
}

fn run_submit(args: ElementArgs) -> OpenPageResult<()> {
    print_page_result(
        &args.session,
        rpc_page(
            &args.session,
            "element.submit",
            json!({"locator": args.locator}),
        )?,
    )
}

fn run_check(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.check",
        json!({"locator": args.locator}),
    )?))
}

fn run_uncheck(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.uncheck",
        json!({"locator": args.locator}),
    )?))
}

fn run_right_click(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.click_right",
        json!({"locator": args.locator}),
    )?))
}

fn run_middle_click(args: ElementArgs) -> OpenPageResult<()> {
    print_page_result(
        &args.session,
        rpc_page(
            &args.session,
            "element.click_middle",
            json!({"locator": args.locator}),
        )?,
    )
}

fn run_double_click(args: ElementArgs) -> OpenPageResult<()> {
    let result = rpc_page(
        &args.session,
        "element.click_multi",
        json!({"locator": args.locator, "count": 2}),
    )?;
    print_page_json(
        &args.session,
        json!({
            "clicked": true,
            "count": 2,
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_click_at(args: ClickAtArgs) -> OpenPageResult<()> {
    let ClickAtArgs {
        locator,
        x,
        y,
        button,
        count,
        session,
    } = args;
    let result = rpc_page(
        &session,
        "element.click_at",
        json!({
            "locator": locator,
            "x": x,
            "y": y,
            "button": button.clone(),
            "count": count,
        }),
    )?;
    print_page_json(
        &session,
        json!({
            "clicked": true,
            "button": button,
            "count": count,
            "navigation_token": result.get("navigation_token").cloned(),
        }),
    )
}

fn run_key_down(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.key_down",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(
        json!({"dispatched": true, "event": "keydown", "key": args.key}),
    ))
}

fn run_key_up(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.key_up",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(
        json!({"dispatched": true, "event": "keyup", "key": args.key}),
    ))
}

fn run_shortcut(args: ShortcutArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.type_keys",
        json!({"text": args.keys.clone()}),
    )?;
    print_json(simple_ok(json!({"pressed": true, "keys": args.keys})))
}

fn run_shortcut_action(args: SessionArgs, key: &str, result_key: &str) -> OpenPageResult<()> {
    let modifier = if cfg!(target_os = "macos") {
        "Meta"
    } else {
        "Control"
    };
    let _ = rpc_page(
        &args.session,
        "page.type_keys",
        json!({"text": [modifier, key]}),
    )?;
    print_json(simple_ok(json!({result_key: true, "key": key})))
}

fn run_clipboard(command: ClipboardCommand) -> OpenPageResult<()> {
    match command {
        ClipboardCommand::Read(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "clipboard.read",
            Value::Null,
        )?)),
        ClipboardCommand::Write(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "clipboard.write",
            json!({"text": args.text}),
        )?)),
    }
}

fn run_input(args: PageTextArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.input",
        json!({"text": args.text.clone()}),
    )?;
    print_json(simple_ok(json!({"input": true, "text": args.text})))
}

fn run_type(args: PageTextArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.type",
        json!({"text": args.text.clone()}),
    )?;
    print_json(simple_ok(json!({"typed": true, "text": args.text})))
}

fn run_type_with_interval(args: TypeWithIntervalArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.type_with_interval",
        json!({"text": args.text.clone(), "interval": args.interval}),
    )?;
    print_json(simple_ok(json!({
        "typed": true,
        "text": args.text,
        "interval": args.interval,
    })))
}

fn run_drag(args: DragArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.drag",
        json!({
            "locator": args.locator,
            "dx": args.dx,
            "dy": args.dy,
            "duration": args.duration,
        }),
    )?;
    print_json(simple_ok(json!({
        "dragged": true,
        "dx": args.dx,
        "dy": args.dy,
    })))
}

fn run_drag_to(args: DragToArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.drag_to",
        json!({
            "locator": args.source,
            "target": args.target,
            "duration": args.duration,
        }),
    )?;
    print_json(simple_ok(json!({"dragged": true, "target": args.target})))
}

fn run_drag_to_point(args: DragToPointArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.drag_to_point",
        json!({
            "locator": args.locator,
            "x": args.x,
            "y": args.y,
            "duration": args.duration,
        }),
    )?;
    print_json(simple_ok(json!({
        "dragged": true,
        "x": args.x,
        "y": args.y,
    })))
}

fn run_drag_in(args: DragInArgs) -> OpenPageResult<()> {
    if let Some(text) = args.text {
        let _ = rpc_page(
            &args.session,
            "page.drag_in",
            json!({"target": args.target, "text": text}),
        )?;
        print_json(simple_ok(
            json!({"dragged": true, "target": args.target, "kind": "text"}),
        ))
    } else if !args.files.is_empty() {
        let _ = rpc_page(
            &args.session,
            "page.drag_in",
            json!({"target": args.target, "files": args.files}),
        )?;
        print_json(simple_ok(
            json!({"dragged": true, "target": args.target, "kind": "files"}),
        ))
    } else {
        Err(OpenPageError::UnsupportedOperation(
            "drag-in requires --text or --files".to_string(),
        ))
    }
}

fn run_text(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.text",
        json!({"locator": args.locator}),
    )?))
}

fn run_value(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.value",
        json!({"locator": args.locator}),
    )?))
}

fn run_raw_text(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.raw_text",
        json!({"locator": args.locator}),
    )?))
}

fn run_link(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.link",
        json!({"locator": args.locator}),
    )?))
}

fn run_open_link(args: OpenLinkArgs) -> OpenPageResult<()> {
    let link = rpc_page(
        &args.session,
        "element.link",
        json!({"locator": args.locator}),
    )?
    .get("link")
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| OpenPageError::ElementNotFound("element link is unavailable".to_string()))?
    .to_string();
    print_json(simple_ok(rpc_page(
        &args.session,
        "tab.new",
        json!({
            "url": link,
            "window": args.window,
            "background": args.background,
        }),
    )?))
}

fn run_child_count(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.child_count",
        json!({"locator": args.locator}),
    )?))
}

fn run_css_path(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.css_path",
        json!({"locator": args.locator}),
    )?))
}

fn run_xpath(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.xpath",
        json!({"locator": args.locator}),
    )?))
}

fn run_element_html(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.html",
        json!({"locator": args.locator}),
    )?))
}

fn run_attr(args: AttrArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.attr",
        json!({"locator": args.locator, "name": args.name}),
    )?))
}

fn wait_condition_op(condition: &str) -> Option<&'static str> {
    let normalized = condition.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "load-start" | "load_start" | "start-loading" => Some("wait.load_start"),
        "doc-loaded" | "doc_loaded" | "loaded" => Some("wait.doc_loaded"),
        "ready" => Some("wait.ready"),
        "navigation" => Some("wait.navigation"),
        _ => None,
    }
}

fn run_wait(args: WaitArgs) -> OpenPageResult<()> {
    let condition = args.condition.trim();
    if let Some(op) = wait_condition_op(condition) {
        let params = if op == "wait.navigation" {
            json!({"timeout_ms": args.timeout, "token": args.token})
        } else {
            json!({"timeout_ms": args.timeout})
        };
        let _ = rpc_page(&args.session, op, params)?;
    } else {
        let locator = condition
            .strip_prefix("element ")
            .map(str::trim)
            .unwrap_or(condition);
        let _ = rpc_page(
            &args.session,
            "wait.locator",
            json!({"locator": locator, "timeout_ms": args.timeout}),
        )?;
    }
    print_json(simple_ok(json!({"waited": true, "condition": condition})))
}

fn run_intercept(command: InterceptCommand) -> OpenPageResult<()> {
    let session = match &command {
        InterceptCommand::Start(args)
        | InterceptCommand::Stop(args)
        | InterceptCommand::Status(args) => args.session.clone(),
    };
    match command {
        InterceptCommand::Start(_) => {
            let _ = rpc_page(&session, "intercept.start", Value::Null)?;
            print_json(simple_ok(json!({"intercept": "started"})))
        }
        InterceptCommand::Stop(_) => {
            let _ = rpc_page(&session, "intercept.stop", Value::Null)?;
            print_json(simple_ok(json!({"intercept": "stopped"})))
        }
        InterceptCommand::Status(_) => {
            let status = rpc_page(&session, "intercept.status", Value::Null)?;
            print_json(simple_ok(json!({
                "listening": status.get("listening").cloned(),
                "paused": status.get("paused").cloned(),
            })))
        }
    }
}

fn run_js(args: JsArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.run_js",
        json!({"script": args.script}),
    )?))
}

fn run_download(args: DownloadArgs) -> OpenPageResult<()> {
    let path = rpc_page(
        &args.session,
        "page.download_url",
        json!({
            "url": args.url,
            "path": args.output,
        }),
    )?
    .get("path")
    .cloned();
    print_json(simple_ok(json!({"downloaded": true, "path": path})))
}

fn run_downloads(command: DownloadsCommand) -> OpenPageResult<()> {
    match command {
        DownloadsCommand::List(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "page.download_missions",
            Value::Null,
        )?)),
        DownloadsCommand::Last(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "page.last_download",
            Value::Null,
        )?)),
        DownloadsCommand::Clear(args) => run_downloads_clear(args),
        DownloadsCommand::Cancel(args) => run_downloads_cancel(args),
        DownloadsCommand::Open(args) => run_downloads_open(args),
        DownloadsCommand::Reveal(args) => run_downloads_reveal(args),
        DownloadsCommand::Path(args) => run_downloads_path(args),
        DownloadsCommand::SetPath(args) => run_downloads_set_path(args),
        DownloadsCommand::Mode(args) => run_downloads_mode(args),
        DownloadsCommand::SetMode(args) => run_downloads_set_mode(args),
        DownloadsCommand::Wait(args) => run_downloads_wait(args),
    }
}

fn run_downloads_clear(args: SessionArgs) -> OpenPageResult<()> {
    let removed = rpc_page(&args.session, "page.clear_finished_downloads", Value::Null)?
        .get("removed")
        .cloned();
    print_json(simple_ok(json!({"cleared": true, "removed": removed})))
}

fn run_downloads_cancel(args: DownloadsCancelArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "page.cancel_download",
        json!({"guid": args.guid}),
    )?;
    print_json(simple_ok(json!({"cancelled": true, "guid": args.guid})))
}

fn run_downloads_open(args: DownloadsOpenArgs) -> OpenPageResult<()> {
    let mission = resolve_download_mission(&args.session, args.guid.as_deref())?;
    let path = download_final_path(&mission)?;
    open_path_with_system(Path::new(&path))?;
    print_json(simple_ok(json!({
        "opened": true,
        "guid": mission.get("guid").and_then(Value::as_str),
        "path": path,
    })))
}

fn run_downloads_reveal(args: DownloadsOpenArgs) -> OpenPageResult<()> {
    let mission = resolve_download_mission(&args.session, args.guid.as_deref())?;
    let path = download_final_path(&mission)?;
    reveal_path_with_system(Path::new(&path))?;
    print_json(simple_ok(json!({
        "revealed": true,
        "guid": mission.get("guid").and_then(Value::as_str),
        "path": path,
    })))
}

fn run_downloads_path(args: SessionArgs) -> OpenPageResult<()> {
    let path = rpc_page(&args.session, "page.current_tab_download_path", Value::Null)?
        .get("download_path")
        .cloned();
    print_json(simple_ok(json!({"download_path": path})))
}

fn run_downloads_set_path(args: DownloadsPathArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "set.current_tab_download_path",
        json!({"path": args.path}),
    )?;
    print_json(simple_ok(json!({"set": true, "download_path": args.path})))
}

fn run_downloads_mode(args: SessionArgs) -> OpenPageResult<()> {
    let mode = rpc_page(&args.session, "page.download_file_exists_mode", Value::Null)?
        .get("mode")
        .cloned();
    print_json(simple_ok(json!({"mode": mode})))
}

fn run_downloads_set_mode(args: DownloadsModeArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "set.current_tab_download_file_exists_mode",
        json!({"mode": args.mode}),
    )?;
    print_json(simple_ok(json!({"set": true, "mode": args.mode})))
}

fn run_downloads_wait(args: WaitForDownloadArgs) -> OpenPageResult<()> {
    let baseline = rpc_page(&args.session, "page.download_missions", Value::Null)?
        .get("missions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let baseline_guids = baseline
        .iter()
        .filter_map(|mission| mission.get("guid").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();

    let deadline = Instant::now() + Duration::from_millis(args.timeout);
    let path = loop {
        let missions = rpc_page(&args.session, "page.download_missions", Value::Null)?
            .get("missions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(path) = find_download_path(&missions, args.filename.as_deref(), &baseline_guids)
        {
            break path;
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(match &args.filename {
                Some(filename) => format!(
                    "downloads wait timed out after {}ms: filename={filename}",
                    args.timeout
                ),
                None => format!("downloads wait timed out after {}ms", args.timeout),
            }));
        }
        sleep(Duration::from_millis(200));
    };
    print_json(simple_ok(json!({
        "waited": true,
        "filename": args.filename,
        "path": path,
    })))
}

fn find_download_path(
    missions: &[Value],
    filename: Option<&str>,
    baseline_guids: &[String],
) -> Option<Value> {
    missions.iter().find_map(|mission| {
        let guid = mission.get("guid").and_then(Value::as_str)?;
        let state = mission.get("state").and_then(Value::as_str)?;
        let final_path = mission.get("final_path")?.clone();
        let final_path_str = final_path.as_str()?;
        if final_path_str.is_empty() || state != "done" {
            return None;
        }
        if let Some(filename) = filename {
            let suggested = mission.get("suggested_filename").and_then(Value::as_str)?;
            if suggested == filename {
                return Some(final_path);
            }
            return None;
        }
        if baseline_guids.iter().any(|item| item == guid) {
            return None;
        }
        Some(final_path)
    })
}

fn resolve_download_mission(session: &str, guid: Option<&str>) -> OpenPageResult<Value> {
    if let Some(guid) = guid {
        let missions = rpc_page(session, "page.download_missions", Value::Null)?
            .get("missions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return missions
            .into_iter()
            .find(|mission| mission.get("guid").and_then(Value::as_str) == Some(guid))
            .ok_or_else(|| OpenPageError::ElementNotFound(format!("download not found: {guid}")));
    }
    rpc_page(session, "page.last_download", Value::Null)?
        .get("mission")
        .cloned()
        .filter(|mission| !mission.is_null())
        .ok_or_else(|| OpenPageError::ElementNotFound("no tracked download found".to_string()))
}

fn download_final_path(mission: &Value) -> OpenPageResult<String> {
    let guid = mission
        .get("guid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = mission
        .get("final_path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenPageError::BrowserOperation(format!(
                "download has no finalized file path yet: {guid}"
            ))
        })?;
    Ok(path.to_string())
}

fn open_path_with_system(path: &Path) -> OpenPageResult<()> {
    if !path.exists() {
        return Err(OpenPageError::Io(format!(
            "download path does not exist: {}",
            path.display()
        )));
    }
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = ProcessCommand::new("open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let command = {
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = ProcessCommand::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        return Err(OpenPageError::UnsupportedOperation(
            "downloads open is unsupported on this platform".to_string(),
        ));
    }
    run_gui_command(command, "open download")
}

fn reveal_path_with_system(path: &Path) -> OpenPageResult<()> {
    if !path.exists() {
        return Err(OpenPageError::Io(format!(
            "download path does not exist: {}",
            path.display()
        )));
    }
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = ProcessCommand::new("open");
        command.arg("-R").arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let command = {
        let parent = path.parent().ok_or_else(|| {
            OpenPageError::Io(format!(
                "download path has no parent directory: {}",
                path.display()
            ))
        })?;
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(parent);
        command
    };
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = ProcessCommand::new("explorer");
        command.arg(format!("/select,{}", path.display()));
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        return Err(OpenPageError::UnsupportedOperation(
            "downloads reveal is unsupported on this platform".to_string(),
        ));
    }
    run_gui_command(command, "reveal download")
}

fn run_gui_command(mut command: ProcessCommand, action: &str) -> OpenPageResult<()> {
    let output = command.output()?;
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
        format!("{action} exited with status {}", output.status)
    };
    Err(OpenPageError::BrowserOperation(detail))
}

fn run_window(command: WindowCommand) -> OpenPageResult<()> {
    match command {
        WindowCommand::List(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "window.list",
            Value::Null,
        )?)),
        WindowCommand::Switch(args) => run_window_switch(args),
        WindowCommand::Close(args) => run_window_close(args),
        WindowCommand::State(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "window.state",
            Value::Null,
        )?)),
        WindowCommand::Location(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "window.location",
            Value::Null,
        )?)),
        WindowCommand::Max(args) => {
            let _ = rpc_page(&args.session, "window.max", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "maximized"})))
        }
        WindowCommand::Min(args) => {
            let _ = rpc_page(&args.session, "window.min", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "minimized"})))
        }
        WindowCommand::Fullscreen(args) => {
            let _ = rpc_page(&args.session, "window.full", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "fullscreen"})))
        }
        WindowCommand::Normal(args) => {
            let _ = rpc_page(&args.session, "window.normal", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "normal"})))
        }
        WindowCommand::Hide(args) => {
            let _ = rpc_page(&args.session, "window.hide", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "visible": false})))
        }
        WindowCommand::Show(args) => {
            let _ = rpc_page(&args.session, "window.show", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "visible": true})))
        }
        WindowCommand::Size(args) => {
            let _ = rpc_page(
                &args.session,
                "window.size_set",
                json!({"width": args.width, "height": args.height}),
            )?;
            print_json(simple_ok(
                json!({"window": true, "width": args.width, "height": args.height}),
            ))
        }
        WindowCommand::Move(args) => run_window_move(args),
    }
}

fn run_window_switch(args: WindowSwitchArgs) -> OpenPageResult<()> {
    let target_id = window_target_id_from_selector(&args.session, &args.target)?;
    let result = rpc_page(
        &args.session,
        "window.switch",
        json!({"target_id": target_id}),
    )?;
    print_json(simple_ok(result))
}

fn run_window_close(args: WindowCloseArgs) -> OpenPageResult<()> {
    let target_id = match (args.target.as_deref(), args.index) {
        (Some(target), _) => Some(window_target_id_from_selector(&args.session, target)?),
        (None, Some(index)) => Some(window_target_id_from_selector(
            &args.session,
            &index.to_string(),
        )?),
        (None, None) => None,
    };
    let result = rpc_page(
        &args.session,
        "window.close",
        json!({"target_id": target_id}),
    )?;
    print_json(simple_ok(result))
}

fn run_zoom(command: ZoomCommand) -> OpenPageResult<()> {
    match command {
        ZoomCommand::Get(args) => {
            print_json(simple_ok(rpc_page(&args.session, "zoom.get", Value::Null)?))
        }
        ZoomCommand::In(args) => run_zoom_step(args, 1.0),
        ZoomCommand::Out(args) => run_zoom_step(args, -1.0),
        ZoomCommand::Set(args) => run_zoom_set(args),
        ZoomCommand::Reset(args) => {
            let result = rpc_page(&args.session, "zoom.reset", Value::Null)?;
            print_json(simple_ok(result))
        }
    }
}

fn run_zoom_set(args: ZoomSetArgs) -> OpenPageResult<()> {
    let result = rpc_page(&args.session, "zoom.set", json!({"factor": args.factor}))?;
    print_json(simple_ok(result))
}

fn run_zoom_step(args: ZoomStepArgs, direction: f64) -> OpenPageResult<()> {
    if !(args.step.is_finite() && args.step > 0.0) {
        return Err(OpenPageError::UnsupportedOperation(
            "zoom step must be a positive finite number".to_string(),
        ));
    }
    let current = rpc_page(&args.session, "zoom.get", Value::Null)?
        .get("factor")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            OpenPageError::BrowserOperation("zoom.get returned no numeric factor".to_string())
        })?;
    let factor = current + direction * args.step;
    if !(factor.is_finite() && factor > 0.0) {
        return Err(OpenPageError::UnsupportedOperation(format!(
            "zoom factor must stay positive, got {factor}"
        )));
    }
    let result = rpc_page(&args.session, "zoom.set", json!({"factor": factor}))?;
    print_json(simple_ok(result))
}

fn run_window_move(args: WindowMoveArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "window.location_set",
        json!({"left": args.left, "top": args.top}),
    )?;
    print_json(simple_ok(
        json!({"moved": true, "left": args.left, "top": args.top}),
    ))
}

fn run_alert(command: AlertCommand) -> OpenPageResult<()> {
    match command {
        AlertCommand::Accept(args) => {
            let text = rpc_page(
                &args.session,
                "alert.handle",
                json!({
                    "accept": true,
                    "prompt_text": args.prompt_text,
                    "timeout_ms": 10_000
                }),
            )?
            .get("text")
            .cloned();
            print_json(simple_ok(json!({"accepted": true, "text": text})))
        }
        AlertCommand::Dismiss(args) => {
            let text = rpc_page(
                &args.session,
                "alert.handle",
                json!({
                    "accept": false,
                    "prompt_text": args.prompt_text,
                    "timeout_ms": 10_000
                }),
            )?
            .get("text")
            .cloned();
            print_json(simple_ok(json!({"dismissed": true, "text": text})))
        }
        AlertCommand::Has(args) => {
            let has_alert = rpc_page(&args.session, "alert.has", Value::Null)?
                .get("has_alert")
                .cloned();
            print_json(simple_ok(json!({"has_alert": has_alert})))
        }
        AlertCommand::Text(args) => {
            let text = rpc_page(&args.session, "alert.text", Value::Null)?
                .get("text")
                .cloned();
            print_json(simple_ok(json!({"text": text})))
        }
    }
}

fn start_browser(args: BrowserStartArgs) -> OpenPageResult<()> {
    if args.replace {
        stop_browser_session(&args.session, true)?;
    }

    let headless = if args.head {
        Some(false)
    } else if args.headless {
        Some(true)
    } else {
        None
    };
    let create = match rpc_request(
        &args.session,
        Some(args.session.clone()),
        "page.create",
        json!({
            "session": args.session,
            "headless": headless,
            "browser_path": args.browser_path,
            "user_data_dir": args.user_data_dir,
            "port": args.port,
            "width": args.width,
            "height": args.height,
            "no_sandbox": args.no_sandbox,
            "incognito": args.incognito,
            "mute": args.mute,
        }),
    ) {
        Ok(create) => create,
        Err(err) => {
            cleanup_after_browser_launch_failure(&args.session, &err);
            return Err(err);
        }
    };

    if let Some(url) = &args.url {
        let _ = rpc_page(
            &args.session,
            "page.goto",
            json!({
                "url": url,
            }),
        )?;
    }

    let existing = create
        .get("existing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let port = read_port(&args.session)?;

    let payload = if !existing {
        json!({
            "session": args.session,
            "target": create.get("target").cloned(),
            "port": port,
            "headless": headless,
            "incognito": args.incognito,
            "mute": args.mute,
            "url": args.url,
        })
    } else {
        json!({
            "session": args.session,
            "already_running": true,
            "target": create.get("target").cloned(),
            "port": port,
            "headless": headless,
            "incognito": args.incognito,
            "mute": args.mute,
            "url": args.url,
        })
    };

    print_json(simple_ok(with_browser_start_followup(
        &args.session,
        args.url.as_deref(),
        payload,
    )))
}

fn stop_browser_session(session: &str, quiet: bool) -> OpenPageResult<()> {
    let shutdown = shutdown_daemon(session)?;
    let _ = write_recently_closed_tabs(session, &[]);
    if quiet {
        Ok(())
    } else {
        print_json(simple_ok(json!({
            "stopped": true,
            "session": session,
            "had_daemon": shutdown.had_daemon,
            "forced": shutdown.forced,
        })))
    }
}

fn stop_all_browsers(quiet: bool) -> OpenPageResult<()> {
    let inventory = daemon_inventory()?;
    let sessions = browser_stop_all_sessions(&inventory);
    let mut stopped = Vec::new();
    let mut failed = Vec::new();

    for session in sessions {
        match stop_browser_session(&session, true) {
            Ok(()) => stopped.push(session),
            Err(err) => failed.push(json!({
                "session": session,
                "kind": openpage::protocol::openpage_error_kind(&err),
                "message": err.to_string(),
            })),
        }
    }

    if quiet {
        Ok(())
    } else {
        print_json(simple_ok(json!({
            "stopped": stopped.len(),
            "sessions": stopped,
            "failed": failed,
            "all_stopped": failed.is_empty(),
        })))
    }
}

fn stop_browser(args: BrowserStopArgs, quiet: bool) -> OpenPageResult<()> {
    if args.all {
        return stop_all_browsers(quiet);
    }

    let session = args.session.unwrap_or_else(|| "default".to_string());
    stop_browser_session(&session, quiet)
}

fn run_batch(args: BatchArgs) -> OpenPageResult<i32> {
    let commands = match batch_commands(&args) {
        Ok(commands) => commands,
        Err(err) => {
            print_json(batch_error_payload(&err))?;
            return Ok(1);
        }
    };
    let mut had_error = false;

    for command_args in commands {
        if command_args.is_empty() {
            continue;
        }

        let command = match parse_batch_command(&command_args) {
            Ok(command) => command,
            Err(err) => {
                print_json(batch_error_payload(&err))?;
                had_error = true;
                if args.bail {
                    break;
                }
                continue;
            }
        };

        if let Err(err) = run_single(command) {
            print_json(openpage::protocol::simple_openpage_error(&err))?;
            had_error = true;
            if args.bail {
                break;
            }
        }
    }

    Ok(if had_error { 1 } else { 0 })
}

fn batch_error_payload(error: &OpenPageError) -> Value {
    match error {
        OpenPageError::UnsupportedOperation(detail)
            if detail.starts_with("invalid batch command `")
                || detail.starts_with("invalid batch command quoting:") =>
        {
            openpage::protocol::simple_error_with_fix(
                "invalid_input",
                detail,
                openpage::protocol::known_invalid_input_fix(detail).map(str::to_string),
            )
        }
        OpenPageError::Serialization(detail) if detail.starts_with("invalid batch stdin JSON:") => {
            openpage::protocol::simple_error("invalid_input", detail)
        }
        _ => openpage::protocol::simple_openpage_error(error),
    }
}

fn batch_commands(args: &BatchArgs) -> OpenPageResult<Vec<Vec<String>>> {
    if !args.commands.is_empty() {
        return args
            .commands
            .iter()
            .map(|command| {
                shlex::split(command).ok_or_else(|| {
                    OpenPageError::UnsupportedOperation(format!(
                        "invalid batch command quoting: {command}"
                    ))
                })
            })
            .collect();
    }

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    serde_json::from_str::<Vec<Vec<String>>>(&input).map_err(|err| {
        OpenPageError::Serialization(format!(
            "invalid batch stdin JSON: {err}; expected an array of argv arrays"
        ))
    })
}

fn parse_batch_command(command_args: &[String]) -> OpenPageResult<Command> {
    let mut argv = Vec::with_capacity(command_args.len() + 1);
    argv.push("openpage".to_string());
    argv.extend(command_args.iter().cloned());

    let cli = Cli::try_parse_from(argv).map_err(|err| {
        OpenPageError::UnsupportedOperation(format!(
            "invalid batch command `{}`: {}",
            command_args.join(" "),
            err.to_string().trim()
        ))
    })?;

    match cli.command {
        Command::Doctor(_) => Err(OpenPageError::UnsupportedOperation(
            "batch cannot execute `doctor`; run `openpage doctor` separately".to_string(),
        )),
        Command::Batch(_) => Err(OpenPageError::UnsupportedOperation(
            "batch cannot execute nested batch commands".to_string(),
        )),
        Command::Serve(_) => Err(OpenPageError::UnsupportedOperation(
            "batch cannot execute `serve`; use top-level `serve` separately".to_string(),
        )),
        command => Ok(command),
    }
}

fn rpc_request(
    daemon_session: &str,
    target: Option<String>,
    op: &str,
    params: Value,
) -> OpenPageResult<Value> {
    let response = send_request(
        daemon_session,
        &Request {
            id: Some(json!("cli")),
            op: op.to_string(),
            target,
            params,
        },
    )?;
    response_result(response)
}

fn rpc_request_existing(
    daemon_session: &str,
    target: Option<String>,
    op: &str,
    params: Value,
) -> OpenPageResult<Value> {
    let response = send_request_existing(
        daemon_session,
        &Request {
            id: Some(json!("cli")),
            op: op.to_string(),
            target,
            params,
        },
    )?;
    response_result(response)
}

fn ensure_page_session(session: &str) -> OpenPageResult<()> {
    let _ = match rpc_request(
        session,
        Some(session.to_string()),
        "page.create",
        json!({
            "session": session,
            "port": 0,
        }),
    ) {
        Ok(result) => result,
        Err(err) => {
            cleanup_after_browser_launch_failure(session, &err);
            return Err(err);
        }
    };
    Ok(())
}

fn cleanup_after_browser_launch_failure(session: &str, error: &OpenPageError) {
    if matches!(error, OpenPageError::BrowserLaunch(_)) {
        let _ = stop_browser_session(session, true);
    }
}

fn rpc_page(session: &str, op: &str, params: Value) -> OpenPageResult<Value> {
    rpc_request_existing(session, Some(session.to_string()), op, params)
}

fn tab_target_id_from_index(session: &str, index: usize) -> OpenPageResult<String> {
    let response = rpc_page(session, "tab.list", Value::Null)?;
    let tabs = response
        .get("tabs")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            OpenPageError::BrowserOperation("tab.list returned no tabs array".to_string())
        })?;
    tabs.get(index.saturating_sub(1))
        .and_then(|tab| tab.get("target_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| OpenPageError::ElementNotFound(format!("tab index out of range: {index}")))
}

fn tab_list(session: &str) -> OpenPageResult<Vec<Value>> {
    let response = rpc_page(session, "tab.list", Value::Null)?;
    response
        .get("tabs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            OpenPageError::BrowserOperation("tab.list returned no tabs array".to_string())
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RecentlyClosedTab {
    url: String,
    title: String,
}

fn recently_closed_tabs_path(session: &str) -> OpenPageResult<PathBuf> {
    Ok(daemon_dir()?.join(format!("{session}.recent-tabs.json")))
}

fn read_recently_closed_tabs(session: &str) -> OpenPageResult<Vec<RecentlyClosedTab>> {
    let path = recently_closed_tabs_path(session)?;
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| OpenPageError::Serialization(err.to_string())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(OpenPageError::Io(err.to_string())),
    }
}

fn write_recently_closed_tabs(session: &str, tabs: &[RecentlyClosedTab]) -> OpenPageResult<()> {
    let path = recently_closed_tabs_path(session)?;
    if tabs.is_empty() {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(OpenPageError::Io(err.to_string())),
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(tabs)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    fs::write(path, bytes)?;
    Ok(())
}

fn record_recently_closed_tabs(session: &str, tabs: &[Value]) -> OpenPageResult<()> {
    let mut stack = read_recently_closed_tabs(session)?;
    for tab in tabs {
        let Some(url) = tab.get("url").and_then(Value::as_str) else {
            continue;
        };
        if url.is_empty() {
            continue;
        }
        stack.push(RecentlyClosedTab {
            url: url.to_string(),
            title: tab
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    if stack.len() > 50 {
        let drop_count = stack.len() - 50;
        stack.drain(0..drop_count);
    }
    write_recently_closed_tabs(session, &stack)
}

fn tabs_selected_for_close(
    session: &str,
    args: &crate::cli::args::TabCloseArgs,
) -> OpenPageResult<Vec<Value>> {
    let tabs = tab_list(session)?;
    if args.others {
        return Ok(tabs
            .into_iter()
            .filter(|tab| !tab.get("active").and_then(Value::as_bool).unwrap_or(false))
            .collect());
    }
    if let Some(target_id) = args.target.as_deref() {
        return Ok(tabs
            .into_iter()
            .filter(|tab| {
                tab.get("target_id")
                    .and_then(Value::as_str)
                    .map(|value| value == target_id)
                    .unwrap_or(false)
            })
            .collect());
    }
    if let Some(index) = args.index {
        return tabs
            .into_iter()
            .find(|tab| {
                tab.get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value == index as u64)
                    .unwrap_or(false)
            })
            .map(|tab| vec![tab])
            .ok_or_else(|| {
                OpenPageError::ElementNotFound(format!("tab index out of range: {index}"))
            });
    }
    tabs.into_iter()
        .find(|tab| tab.get("active").and_then(Value::as_bool) == Some(true))
        .map(|tab| vec![tab])
        .ok_or_else(|| OpenPageError::ElementNotFound("no active tab found".to_string()))
}

fn tab_value_for_duplicate(
    session: &str,
    target: Option<&str>,
    index: Option<usize>,
) -> OpenPageResult<Value> {
    let tabs = tab_list(session)?;
    if let Some(target) = target {
        return tabs
            .into_iter()
            .find(|tab| {
                tab.get("target_id")
                    .and_then(Value::as_str)
                    .map(|value| value == target)
                    .unwrap_or(false)
            })
            .ok_or_else(|| OpenPageError::ElementNotFound(format!("tab not found: {target}")));
    }
    if let Some(index) = index {
        return tabs.get(index.saturating_sub(1)).cloned().ok_or_else(|| {
            OpenPageError::ElementNotFound(format!("tab index out of range: {index}"))
        });
    }
    tabs.into_iter()
        .find(|tab| tab.get("active").and_then(Value::as_bool) == Some(true))
        .ok_or_else(|| OpenPageError::ElementNotFound("no active tab found".to_string()))
}

fn window_target_id_from_selector(session: &str, selector: &str) -> OpenPageResult<String> {
    let response = rpc_page(session, "window.list", Value::Null)?;
    let windows = response
        .get("windows")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            OpenPageError::BrowserOperation("window.list returned no windows array".to_string())
        })?;
    if let Ok(index) = selector.parse::<usize>() {
        return windows
            .get(index.saturating_sub(1))
            .and_then(|window| window.get("target_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| {
                OpenPageError::ElementNotFound(format!("window index out of range: {index}"))
            });
    }
    windows
        .iter()
        .find(|window| {
            window
                .get("window_id")
                .and_then(Value::as_i64)
                .map(|value| value.to_string() == selector)
                .unwrap_or(false)
        })
        .and_then(|window| window.get("target_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| OpenPageError::ElementNotFound(format!("window not found: {selector}")))
}

fn response_result(response: Response) -> OpenPageResult<Value> {
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null))
    } else {
        let error = response
            .error
            .ok_or_else(|| OpenPageError::BrowserOperation("daemon request failed".to_string()))?;
        Err(openpage::protocol::openpage_error_from_response_error(
            error,
        ))
    }
}

fn run_diff(command: DiffCommand) -> OpenPageResult<()> {
    match command {
        DiffCommand::Snapshot(args) => {
            let before = fs::read_to_string(&args.before)
                .map_err(|e| OpenPageError::Io(format!("read before file: {e}")))?;
            let after = fs::read_to_string(&args.after)
                .map_err(|e| OpenPageError::Io(format!("read after file: {e}")))?;
            let result = openpage::diff::diff_snapshots(&before, &after);
            print_json(simple_ok(json!({
                "changed": result.changed,
                "additions": result.additions,
                "removals": result.removals,
                "unchanged": result.unchanged,
                "diff": result.diff,
            })))
        }
        DiffCommand::Screenshot(args) => {
            let baseline = fs::read(&args.baseline)
                .map_err(|e| OpenPageError::Io(format!("read baseline image: {e}")))?;
            let current = fs::read(&args.current)
                .map_err(|e| OpenPageError::Io(format!("read current image: {e}")))?;
            let result = openpage::diff::diff_screenshot(&baseline, &current, args.threshold)
                .map_err(OpenPageError::Io)?;
            let mut payload = json!({
                "matched": result.matched,
                "mismatch_percentage": result.mismatch_percentage,
                "different_pixels": result.different_pixels,
                "total_pixels": result.total_pixels,
            });
            if let Some(dim) = result.dimension_mismatch {
                payload["dimension_mismatch"] = dim;
            }
            print_json(simple_ok(payload))
        }
    }
}

fn print_json(value: Value) -> OpenPageResult<()> {
    print_output_json(&value);
    Ok(())
}

fn print_page_result(session: &str, result: Value) -> OpenPageResult<()> {
    print_page_json(session, result)
}

fn print_page_json(session: &str, result: Value) -> OpenPageResult<()> {
    print_json(simple_ok(with_navigation_followup(session, result)))
}

fn with_browser_start_followup(session: &str, url: Option<&str>, mut result: Value) -> Value {
    let Some(object) = result.as_object_mut() else {
        return result;
    };

    let next_steps = if url.is_some() {
        json!({
            "wait_for_ready": format!(
                "openpage wait-for-ready --session {}",
                shell_quote(session),
            ),
            "snapshot": format!("openpage snapshot --session {}", shell_quote(session)),
            "stop": format!(
                "openpage browser stop --session {}",
                shell_quote(session),
            ),
        })
    } else {
        json!({
            "goto": format!(
                "openpage goto --session {} https://example.com",
                shell_quote(session),
            ),
            "stop": format!(
                "openpage browser stop --session {}",
                shell_quote(session),
            ),
        })
    };

    object.insert("next_steps".to_string(), next_steps);
    result
}

fn with_navigation_followup(session: &str, mut result: Value) -> Value {
    let Some(object) = result.as_object_mut() else {
        return result;
    };
    let Some(token) = object
        .get("navigation_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return result;
    };

    object.insert(
        "wait_for_navigation".to_string(),
        json!({
            "session": session,
            "token": token.clone(),
            "command": format!(
                "openpage wait-for-navigation --session {} --token {}",
                shell_quote(session),
                shell_quote(&token),
            ),
        }),
    );
    result
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'@')
        })
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_scroll_into_view(args: ScrollIntoViewArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.scroll_into_view",
        json!({"locator": args.locator, "center": args.center}),
    )?;
    print_json(simple_ok(json!({"scrolled_into_view": true})))
}

fn run_hover(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.hover",
        json!({"locator": args.locator}),
    )?))
}

fn run_hover_at(args: HoverAtArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.hover_at",
        json!({
            "locator": args.locator,
            "x": args.x,
            "y": args.y,
        }),
    )?))
}

fn run_press(args: PressArgs) -> OpenPageResult<()> {
    print_page_result(
        &args.session,
        rpc_page(
            &args.session,
            "element.press_key",
            json!({"locator": args.locator, "key": args.key.clone()}),
        )?,
    )
}

fn run_select(args: SelectArgs) -> OpenPageResult<()> {
    let text = match args.text.as_slice() {
        [] => Value::Null,
        [value] => Value::String(value.clone()),
        _ => json!(args.text),
    };
    let value = match args.value.as_slice() {
        [] => Value::Null,
        [value] => Value::String(value.clone()),
        _ => json!(args.value),
    };
    let index = match args.index.as_slice() {
        [] => Value::Null,
        [value] => json!(value),
        _ => json!(args.index),
    };
    let selected = rpc_page(
        &args.session,
        "element.select",
        json!({
            "locator": args.locator,
            "text": text,
            "value": value,
            "index": index,
        }),
    )?
    .get("selected")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"selected": selected})))
}

fn run_option_texts(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.option_texts",
        json!({"locator": args.locator}),
    )?))
}

fn run_selected_option(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.selected_option",
        json!({"locator": args.locator}),
    )?))
}

fn run_selected_options(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.selected_options",
        json!({"locator": args.locator}),
    )?))
}

fn run_select_all_options(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.select_all_options",
        json!({"locator": args.locator}),
    )?))
}

fn run_clear_selected_options(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.clear_selected_options",
        json!({"locator": args.locator}),
    )?))
}

fn run_invert_selected_options(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.invert_selected_options",
        json!({"locator": args.locator}),
    )?))
}

fn run_selected_text(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.selected_text",
        Value::Null,
    )?))
}

fn run_select_text(args: SelectTextArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.select_text",
        json!({
            "locator": args.locator,
            "start": args.start,
            "end": args.end,
        }),
    )?))
}

fn run_select_range(args: SelectRangeArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.select_range",
        json!({
            "locator": args.locator,
            "start": args.start,
            "end": args.end,
        }),
    )?))
}

fn run_upload(args: UploadArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "element.upload",
        json!({"locator": args.locator, "files": args.files}),
    )?;
    print_json(simple_ok(json!({"uploaded": true, "files": args.files})))
}

fn run_click_to_download(args: ClickToDownloadArgs) -> OpenPageResult<()> {
    let dir = args
        .dir
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.click_to_download",
        json!({
            "locator": args.locator,
            "dir": dir,
            "rename": args.rename,
            "suffix": args.suffix,
            "timeout_ms": args.timeout,
            "js": args.js,
            "new_tab": args.new_tab,
        }),
    )?))
}

fn run_click_to_upload(args: ClickToUploadArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.click_to_upload",
        json!({
            "locator": args.locator,
            "files": args.files,
            "timeout_ms": args.timeout,
            "js": args.js,
        }),
    )?))
}

fn run_click_for_new_tab(args: ClickForNewTabArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.click_for_new_tab",
        json!({
            "locator": args.locator,
            "timeout_ms": args.timeout,
            "js": args.js,
        }),
    )?))
}

fn run_is_visible(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_visible",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_enabled(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_enabled",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_checked(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_checked",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_selected(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_selected",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_alive(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_alive",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_in_viewport(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_in_viewport",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_whole_in_viewport(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_whole_in_viewport",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_covered(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_covered",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_clickable(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.is_clickable",
        json!({"locator": args.locator}),
    )?))
}

fn run_has_rect(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "element.has_rect",
        json!({"locator": args.locator}),
    )?))
}

fn run_find(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.find",
        json!({"locator": args.locator}),
    )?))
}

fn run_find_in_page(args: FindInPageArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.find_in_page",
        json!({
            "text": args.text,
            "backward": args.backward,
            "case_sensitive": args.case_sensitive,
        }),
    )?))
}

fn run_find_all(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.find_all",
        json!({"locator": args.locator}),
    )?))
}

fn run_locate(args: LocateArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.locate",
        json!({"chain": args.chain.join(" ")}),
    )?))
}

fn run_count(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.count",
        json!({"locator": args.locator}),
    )?))
}

fn run_wait_visible(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_displayed",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"visible": ready, "waited": ready})))
}

fn run_wait_hidden(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_hidden",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"hidden": ready, "waited": ready})))
}

fn run_wait_enabled(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_enabled",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"enabled": ready, "waited": ready})))
}

fn run_wait_disabled(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_disabled",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"disabled": ready, "waited": ready})))
}

fn run_wait_deleted(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_deleted",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"deleted": ready, "waited": ready})))
}

fn run_wait_clickable(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_clickable",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"clickable": ready, "waited": ready})))
}

fn run_wait_has_rect(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_has_rect",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"has_rect": ready, "waited": ready})))
}

fn run_wait_covered(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_covered",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"covered": ready, "waited": ready})))
}

fn run_wait_not_covered(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_not_covered",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"not_covered": ready, "waited": ready})))
}

fn run_wait_stop_moving(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_stop_moving",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"stopped": ready, "waited": ready})))
}

fn run_active_element(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "page.active_element",
        Value::Null,
    )?))
}

fn run_wait_for_new_tab(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let target_id = rpc_page(
        &args.session,
        "wait.new_tab",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("target")
    .cloned()
    .unwrap_or(Value::Null);
    let waited = !target_id.is_null();
    print_json(simple_ok(json!({"waited": waited, "target_id": target_id})))
}

fn run_wait_for_download_begin(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let mission = rpc_page(
        &args.session,
        "wait.download_begin",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("mission")
    .cloned()
    .unwrap_or(Value::Null);
    let waited = !mission.is_null();
    print_json(simple_ok(json!({"waited": waited, "mission": mission})))
}

fn run_wait_for_downloads_done(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let done = rpc_page(
        &args.session,
        "wait.downloads_done",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("done")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": done, "done": done})))
}

fn run_wait_for_alert_closed(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let closed = rpc_page(
        &args.session,
        "wait.alert_closed",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("closed")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": closed, "closed": closed})))
}

fn run_wait_for_load_start(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let started = rpc_page(
        &args.session,
        "wait.load_start",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("started")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": started, "started": started})))
}

fn run_wait_for_doc_loaded(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let loaded = rpc_page(
        &args.session,
        "wait.doc_loaded",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("loaded")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": loaded, "loaded": loaded})))
}

fn run_wait_for_ready(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let result = rpc_page(
        &args.session,
        "wait.ready",
        json!({"timeout_ms": args.timeout}),
    )?;
    let ready = result
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    print_json(simple_ok(json!({
        "waited": ready,
        "ready": ready,
        "ready_state": result.get("ready_state").cloned(),
        "url": result.get("url").cloned(),
    })))
}

fn run_wait_for_navigation(args: WaitForNavigationArgs) -> OpenPageResult<()> {
    let result = rpc_page(
        &args.session,
        "wait.navigation",
        json!({"timeout_ms": args.timeout, "token": args.token}),
    )?;
    let ready = result
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    print_json(simple_ok(json!({
        "waited": ready,
        "ready": ready,
        "ready_state": result.get("ready_state").cloned(),
        "url": result.get("url").cloned(),
        "token": result.get("token").cloned(),
    })))
}

fn run_wait_for_url(args: WaitForUrlArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "wait.url_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let url = rpc_page(&args.session, "page.url", Value::Null)?;
    print_json(simple_ok(
        json!({"waited": true, "url": url.get("url").cloned()}),
    ))
}

fn run_wait_for_title(args: WaitForTitleArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "wait.title_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let title = rpc_page(&args.session, "page.title", Value::Null)?;
    print_json(simple_ok(
        json!({"waited": true, "title": title.get("title").cloned()}),
    ))
}

fn run_wait_for_elements_loaded(args: WaitElementsLoadedArgs) -> OpenPageResult<()> {
    let loaded = rpc_page(
        &args.session,
        "wait.elements_loaded",
        json!({
            "locators": args.locators,
            "any_one": args.any_one,
            "timeout_ms": args.timeout,
        }),
    )?
    .get("loaded")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": loaded, "loaded": loaded})))
}

fn run_wait_for_function(args: WaitForFunctionArgs) -> OpenPageResult<()> {
    let value = rpc_page(
        &args.session,
        "wait.function",
        json!({
            "script": args.script,
            "timeout_ms": args.timeout,
            "interval_ms": args.interval,
        }),
    )?
    .get("result")
    .cloned();
    print_json(simple_ok(json!({"waited": true, "result": value})))
}

fn run_wait_for_text(args: WaitForTextArgs) -> OpenPageResult<()> {
    let _ = rpc_page(
        &args.session,
        "wait.text",
        json!({
            "locator": args.locator,
            "text": args.text,
            "timeout_ms": args.timeout,
            "interval_ms": args.interval,
        }),
    )?;
    print_json(simple_ok(json!({"waited": true, "text": args.text})))
}

fn run_wait_disabled_or_deleted(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_page(
        &args.session,
        "wait.element_disabled_or_deleted",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(
        json!({"disabled_or_deleted": ready, "waited": ready}),
    ))
}

fn run_wait_upload_paths_inputted(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let inputted = rpc_page(
        &args.session,
        "wait.upload_paths_inputted",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("inputted")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": inputted, "inputted": inputted})))
}

fn run_save(args: SaveArgs) -> OpenPageResult<()> {
    let saved = rpc_page(&args.session, "page.save", json!({"path": args.output}))?;
    print_json(simple_ok(json!({
        "saved": true,
        "output": saved.get("path").cloned().unwrap_or(Value::Null),
    })))
}

fn run_pdf(args: PdfArgs) -> OpenPageResult<()> {
    let _ = rpc_page(&args.session, "page.pdf", json!({"path": args.output}))?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_history(command: HistoryCommand) -> OpenPageResult<()> {
    let session = match &command {
        HistoryCommand::List(args) => args.session.clone(),
        HistoryCommand::Go(args) => args.session.clone(),
        HistoryCommand::Clear(args) => args.session.clone(),
    };
    match command {
        HistoryCommand::List(_) => {
            print_json(simple_ok(rpc_page(&session, "history.list", Value::Null)?))
        }
        HistoryCommand::Go(args) => print_json(simple_ok(rpc_page(
            &session,
            "history.go",
            json!({"index": args.index}),
        )?)),
        HistoryCommand::Clear(_) => {
            print_json(simple_ok(rpc_page(&session, "history.clear", Value::Null)?))
        }
    }
}

fn run_storage(command: StorageCommand) -> OpenPageResult<()> {
    let session = match &command {
        StorageCommand::Get(args) => args.session.clone(),
        StorageCommand::Set(args) => args.session.clone(),
    };
    match command {
        StorageCommand::Get(args) => {
            let value = if matches!(args.scope, StorageScope::Local) {
                rpc_page(&session, "page.local_storage", json!({"item": args.key}))?
            } else {
                rpc_page(&session, "page.session_storage", json!({"item": args.key}))?
            };
            print_json(simple_ok(json!({
                "scope": storage_scope_name(&args.scope),
                "key": args.key,
                "value": value.get("value").cloned(),
            })))
        }
        StorageCommand::Set(args) => {
            if matches!(args.scope, StorageScope::Local) {
                let _ = rpc_page(
                    &session,
                    "set.local_storage",
                    json!({"item": args.key, "value": args.value}),
                )?;
            } else {
                let _ = rpc_page(
                    &session,
                    "set.session_storage",
                    json!({"item": args.key, "value": args.value}),
                )?;
            }
            print_json(simple_ok(json!({
                "scope": storage_scope_name(&args.scope),
                "key": args.key,
                "updated": true,
                "removed": args.value.is_none(),
            })))
        }
    }
}

fn storage_scope_name(scope: &StorageScope) -> &'static str {
    match scope {
        StorageScope::Local => "local",
        StorageScope::Session => "session",
    }
}

fn run_permissions(command: PermissionsCommand) -> OpenPageResult<()> {
    match command {
        PermissionsCommand::Set(args) => run_permissions_set(args),
        PermissionsCommand::Reset(args) => print_json(simple_ok(rpc_page(
            &args.session,
            "permissions.reset",
            Value::Null,
        )?)),
    }
}

fn run_permissions_set(args: PermissionSetArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_page(
        &args.session,
        "permissions.set",
        json!({
            "name": args.name.as_descriptor_name(),
            "setting": args.setting.as_cdp_value(),
            "origin": args.origin,
            "embedded_origin": args.embedded_origin,
        }),
    )?))
}

fn run_clear_cache(args: ClearCacheArgs) -> OpenPageResult<()> {
    let any_selected = args.session_storage || args.local_storage || args.cache || args.cookies;
    let session_storage = args.session_storage || !any_selected;
    let local_storage = args.local_storage || !any_selected;
    let cache = args.cache || !any_selected;
    let cookies = args.cookies || !any_selected;
    let _ = rpc_page(
        &args.session,
        "page.clear_cache",
        json!({
            "session_storage": session_storage,
            "local_storage": local_storage,
            "cache": cache,
            "cookies": cookies,
        }),
    )?;
    print_json(simple_ok(json!({
        "cleared": true,
        "session_storage": session_storage,
        "local_storage": local_storage,
        "cache": cache,
        "cookies": cookies,
    })))
}

fn run_cookies(command: CookiesCommand) -> OpenPageResult<()> {
    let session = match &command {
        CookiesCommand::Get(args) | CookiesCommand::Clear(args) => args.session.clone(),
        CookiesCommand::Set(args) => args.session.clone(),
        CookiesCommand::Delete(args) => args.session.clone(),
    };
    match command {
        CookiesCommand::Get(_) => {
            let cookies = rpc_page(&session, "page.cookies", Value::Null)?
                .get("cookies")
                .cloned();
            print_json(simple_ok(json!({"cookies": cookies})))
        }
        CookiesCommand::Set(args) => {
            let url = match args.url {
                Some(u) => Some(u),
                None => rpc_page(&session, "page.url", Value::Null)?
                    .get("url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            };
            let _ = rpc_page(
                &session,
                "cookies.set",
                json!({"name": args.name, "value": args.value, "url": url}),
            )?;
            print_json(simple_ok(json!({"set": true, "name": args.name})))
        }
        CookiesCommand::Delete(args) => {
            let url = match args.url {
                Some(u) => Some(u),
                None => rpc_page(&session, "page.url", Value::Null)?
                    .get("url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            };
            let _ = rpc_page(
                &session,
                "cookies.delete",
                json!({"name": args.name, "url": url}),
            )?;
            print_json(simple_ok(json!({"deleted": true, "name": args.name})))
        }
        CookiesCommand::Clear(_) => {
            let _ = rpc_page(&session, "cookies.clear", Value::Null)?;
            print_json(simple_ok(json!({"cleared": true})))
        }
    }
}

fn run_tab(command: TabCommand) -> OpenPageResult<()> {
    let session = match &command {
        TabCommand::New(args) => args.session.clone(),
        TabCommand::Duplicate(args) => args.session.clone(),
        TabCommand::Reopen(args) => args.session.clone(),
        TabCommand::Close(args) => args.session.clone(),
        TabCommand::List(args) => args.session.clone(),
        TabCommand::Switch(args) => args.session.clone(),
    };
    match command {
        TabCommand::New(args) => print_json(simple_ok(rpc_page(
            &session,
            "tab.new",
            json!({
                "url": args.url,
                "window": args.window,
                "background": args.background,
            }),
        )?)),
        TabCommand::Duplicate(args) => run_tab_duplicate(args),
        TabCommand::Reopen(args) => run_tab_reopen(args),
        TabCommand::Close(args) => {
            let tabs_to_record = tabs_selected_for_close(&session, &args)?;
            if args.others {
                let response = rpc_page(&session, "tab.close", json!({"others": true}))?;
                let closed = response.get("closed").and_then(Value::as_u64).unwrap_or(0) as usize;
                record_recently_closed_tabs(
                    &session,
                    &tabs_to_record[..closed.min(tabs_to_record.len())],
                )?;
                print_json(simple_ok(response))
            } else if let Some(target_id) = args.target {
                let response = rpc_page(&session, "tab.close", json!({"targets": [target_id]}))?;
                let closed = response.get("closed").and_then(Value::as_u64).unwrap_or(0) as usize;
                record_recently_closed_tabs(
                    &session,
                    &tabs_to_record[..closed.min(tabs_to_record.len())],
                )?;
                print_json(simple_ok(response))
            } else if let Some(index) = args.index {
                let target_id = tab_target_id_from_index(&session, index)?;
                let response = rpc_page(&session, "tab.close", json!({"targets": [target_id]}))?;
                let closed = response.get("closed").and_then(Value::as_u64).unwrap_or(0) as usize;
                record_recently_closed_tabs(
                    &session,
                    &tabs_to_record[..closed.min(tabs_to_record.len())],
                )?;
                print_json(simple_ok(response))
            } else {
                let target_id = tabs_to_record
                    .first()
                    .and_then(|tab| tab.get("target_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        OpenPageError::ElementNotFound("no active tab found".to_string())
                    })?;
                let response = rpc_page(&session, "tab.close", json!({"targets": [target_id]}))?;
                let closed = response.get("closed").and_then(Value::as_u64).unwrap_or(0) as usize;
                record_recently_closed_tabs(
                    &session,
                    &tabs_to_record[..closed.min(tabs_to_record.len())],
                )?;
                print_json(simple_ok(response))
            }
        }
        TabCommand::List(_) => print_json(simple_ok(rpc_page(&session, "tab.list", Value::Null)?)),
        TabCommand::Switch(args) => {
            let target_id = if let Ok(index) = args.target.parse::<usize>() {
                tab_target_id_from_index(&session, index)?
            } else {
                args.target
            };
            print_json(simple_ok(rpc_page(
                &session,
                "tab.switch",
                json!({"target_id": target_id}),
            )?))
        }
    }
}

fn run_tab_duplicate(args: TabDuplicateArgs) -> OpenPageResult<()> {
    let tab = tab_value_for_duplicate(&args.session, args.target.as_deref(), args.index)?;
    let url = tab
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenPageError::BrowserOperation(
                "selected tab did not expose a duplicate url".to_string(),
            )
        })?;
    print_json(simple_ok(rpc_page(
        &args.session,
        "tab.new",
        json!({
            "url": url,
            "window": args.window,
            "background": args.background,
        }),
    )?))
}

fn run_tab_reopen(args: TabReopenArgs) -> OpenPageResult<()> {
    let mut stack = read_recently_closed_tabs(&args.session)?;
    let tab = stack.last().cloned().ok_or_else(|| {
        OpenPageError::UnsupportedOperation(
            "no recently closed tab recorded for this session".to_string(),
        )
    })?;
    let response = rpc_page(
        &args.session,
        "tab.new",
        json!({
            "url": tab.url,
            "window": args.window,
            "background": args.background,
        }),
    )?;
    stack.pop();
    write_recently_closed_tabs(&args.session, &stack)?;
    print_json(simple_ok(json!({
        "reopened": true,
        "url": response.get("url").cloned().unwrap_or_else(|| json!(tab.url)),
        "recorded_title": tab.title,
        "target_id": response.get("target_id").cloned(),
        "window": args.window,
        "background": args.background,
    })))
}

fn run_frame(command: FrameCommand) -> OpenPageResult<()> {
    let session = match &command {
        FrameCommand::List(args) => args.session.clone(),
        FrameCommand::Switch(args) => args.session.clone(),
    };
    match command {
        FrameCommand::List(_) => {
            print_json(simple_ok(rpc_page(&session, "frame.list", Value::Null)?))
        }
        FrameCommand::Switch(args) => print_json(simple_ok(rpc_page(
            &session,
            "frame.switch",
            json!({"target": args.target}),
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RecentlyClosedTab, read_recently_closed_tabs, recently_closed_tabs_path,
        record_recently_closed_tabs, tail_log_lines, write_recently_closed_tabs,
    };
    use clap::Parser;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::{BrowserCommand, Cli, Command};
    use crate::error::OpenPageError;
    use openpage::daemon::client::{daemon_dir, pid_path, port_path, version_path};

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
            "openpage-oneshot-test-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn parses_browser_start() {
        let cli = Cli::try_parse_from([
            "openpage",
            "browser",
            "start",
            "--session",
            "agent",
            "--head",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            crate::cli::args::Command::Browser { .. }
        ));
    }

    #[test]
    fn parses_browser_activate() {
        Cli::try_parse_from(["openpage", "browser", "activate", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_browser_logs_tail() {
        let cli = Cli::try_parse_from([
            "openpage",
            "browser",
            "logs",
            "--session",
            "agent",
            "--tail",
            "25",
        ])
        .unwrap();

        match cli.command {
            Command::Browser(BrowserCommand::Logs(args)) => {
                assert_eq!(args.session, "agent");
                assert_eq!(args.tail, Some(25));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_browser_stop_all() {
        let cli = Cli::try_parse_from(["openpage", "browser", "stop", "--all"]).unwrap();

        match cli.command {
            Command::Browser(BrowserCommand::Stop(args)) => {
                assert!(args.all);
                assert_eq!(args.session, None);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn browser_stop_all_sessions_deduplicates_and_keeps_alive_incomplete_sessions() {
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
            incomplete: vec![
                openpage::daemon::client::IncompleteDaemonSession {
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
                },
                openpage::daemon::client::IncompleteDaemonSession {
                    session: "alpha".to_string(),
                    pid_present: true,
                    port_present: true,
                    version_present: false,
                    pid_valid: true,
                    port_valid: true,
                    alive: true,
                    ready: false,
                    log_path: "/tmp/alpha.log".to_string(),
                    log_exists: true,
                    runtime_issue: None,
                },
                openpage::daemon::client::IncompleteDaemonSession {
                    session: "gamma".to_string(),
                    pid_present: true,
                    port_present: true,
                    version_present: false,
                    pid_valid: true,
                    port_valid: true,
                    alive: false,
                    ready: false,
                    log_path: "/tmp/gamma.log".to_string(),
                    log_exists: false,
                    runtime_issue: None,
                },
            ],
            cleaned: Vec::new(),
        };

        assert_eq!(
            super::browser_stop_all_sessions(&inventory),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn browser_inventory_summary_counts_all_categories() {
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

        let summary = super::browser_inventory_summary(&inventory);
        assert_eq!(summary["healthy"], 1);
        assert_eq!(summary["incompatible"], 0);
        assert_eq!(summary["incomplete"], 1);
        assert_eq!(summary["cleaned"], 1);
        assert_eq!(summary["total"], 3);
    }

    #[test]
    fn browser_inventory_payload_includes_state_and_reasons() {
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

        let payload = super::browser_inventory_payload(&inventory);
        assert_eq!(payload["sessions"][0]["kind"], "daemon_session");
        assert_eq!(payload["sessions"][0]["state"], "healthy");
        assert_eq!(payload["sessions"][0]["version_matches_current_cli"], true);
        assert_eq!(payload["sessions"][0]["log_exists"], true);
        assert!(payload["sessions"][0].get("fix").is_none());
        assert_eq!(payload["incomplete"][0]["state"], "incomplete");
        assert_eq!(payload["incomplete"][0]["kind"], "daemon_session");
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
        assert_eq!(payload["cleaned"][0]["kind"], "daemon_session");
        assert_eq!(payload["cleaned"][0]["reason"], "missing version");
        assert_eq!(payload["cleaned"][0]["reasons"], json!(["missing_version"]));
        assert_eq!(payload["cleaned"][0]["log_path"], "/tmp/gamma.log");
        assert_eq!(payload["cleaned"][0]["log_exists"], true);
        assert!(
            payload["cleaned"][0]["fix"]
                .as_str()
                .expect("cleaned fix should be present")
                .contains("browser logs --session gamma --tail 20")
        );
    }

    #[test]
    fn incomplete_session_reasons_report_missing_version_and_not_ready() {
        let incomplete = openpage::daemon::client::IncompleteDaemonSession {
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
        };

        assert_eq!(
            super::incomplete_session_reasons(&incomplete),
            vec!["missing_version", "daemon_not_ready"]
        );
    }

    #[test]
    fn download_final_path_requires_non_empty_path() {
        let path =
            super::download_final_path(&json!({"guid": "done", "final_path": "/tmp/file.txt"}))
                .expect("path should resolve");
        assert_eq!(path, "/tmp/file.txt");

        let error = super::download_final_path(&json!({"guid": "pending", "final_path": ""}))
            .expect_err("empty final_path should fail");
        assert!(
            error
                .to_string()
                .contains("download has no finalized file path yet")
        );
    }

    #[test]
    fn browser_logs_payload_preserves_state_and_reasons() {
        let payload = super::browser_logs_payload(
            json!({
                "session": "beta",
                "alive": true,
                "ready": false,
                "pid": 123,
                "port": 456,
                "version": "0.1.0",
                "log_path": "/tmp/beta.log",
                "log_exists": false,
                "state": "incomplete",
                "reasons": ["missing_version", "daemon_not_ready"],
                "fix": "Run `openpage doctor --quick --fix` ...",
            }),
            true,
            Some(20),
            Some("tail".to_string()),
        );

        assert_eq!(payload["state"], "incomplete");
        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(
            payload["reasons"],
            json!(["missing_version", "daemon_not_ready"])
        );
        assert!(
            payload["fix"]
                .as_str()
                .expect("fix should be preserved")
                .contains("doctor --quick --fix")
        );
        assert_eq!(payload["path"], "/tmp/beta.log");
        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(payload["log_exists"], true);
        assert_eq!(payload["exists"], true);
        assert_eq!(payload["tail"], 20);
        assert_eq!(payload["content"], "tail");
    }

    #[test]
    fn browser_logs_payload_preserves_incompatible_state_and_reasons() {
        let payload = super::browser_logs_payload(
            json!({
                "session": "beta",
                "alive": true,
                "ready": true,
                "pid": 123,
                "port": 456,
                "version": "0.0.1",
                "version_matches_current_cli": false,
                "log_path": "/tmp/beta.log",
                "log_exists": false,
                "state": "incompatible",
                "reasons": ["version_mismatch"],
                "fix": "Run `openpage browser stop --session beta` ...",
            }),
            true,
            Some(5),
            Some("tail".to_string()),
        );

        assert_eq!(payload["state"], "incompatible");
        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(payload["version_matches_current_cli"], false);
        assert_eq!(payload["reasons"], json!(["version_mismatch"]));
        assert!(
            payload["fix"]
                .as_str()
                .expect("fix should be preserved")
                .contains("browser stop --session beta")
        );
        assert_eq!(payload["path"], "/tmp/beta.log");
        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(payload["log_exists"], true);
        assert_eq!(payload["exists"], true);
        assert_eq!(payload["tail"], 5);
        assert_eq!(payload["content"], "tail");
    }

    #[test]
    fn browser_logs_payload_preserves_false_log_exists() {
        let payload = super::browser_logs_payload(
            json!({
                "session": "missing",
                "alive": false,
                "ready": false,
                "log_path": "/tmp/missing.log",
                "log_exists": false,
                "state": "inactive",
            }),
            false,
            Some(20),
            None,
        );

        assert_eq!(payload["path"], "/tmp/missing.log");
        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(payload["log_exists"], false);
        assert_eq!(payload["exists"], false);
        assert_eq!(payload["log_empty"], false);
        assert_eq!(payload["content"], Value::Null);
        assert!(payload.get("log_hint").is_none());
    }

    #[test]
    fn browser_logs_payload_marks_empty_inactive_logs_with_hint() {
        let payload = super::browser_logs_payload(
            json!({
                "session": "failed-start",
                "alive": false,
                "ready": false,
                "log_path": "/tmp/failed-start.log",
                "log_exists": true,
                "state": "inactive",
            }),
            true,
            Some(20),
            Some(String::new()),
        );

        assert_eq!(payload["log_exists"], true);
        assert_eq!(payload["exists"], true);
        assert_eq!(payload["log_empty"], true);
        assert_eq!(payload["content"], "");
        assert!(
            payload["log_hint"]
                .as_str()
                .expect("log hint should be present")
                .contains("original browser_launch error")
        );
    }

    #[test]
    fn browser_logs_payload_sets_daemon_session_kind_when_missing() {
        let payload = super::browser_logs_payload(
            json!({
                "session": "legacy-shape",
                "alive": false,
                "ready": false,
                "log_path": "/tmp/legacy.log",
                "state": "inactive",
            }),
            false,
            None,
            None,
        );

        assert_eq!(payload["kind"], "daemon_session");
        assert_eq!(payload["log_exists"], false);
        assert_eq!(payload["exists"], false);
    }

    #[test]
    fn browser_log_tail_keeps_last_lines() {
        assert_eq!(tail_log_lines("alpha\nbeta\ngamma\n", 2), "beta\ngamma");
        assert_eq!(tail_log_lines("alpha\nbeta\ngamma", 1), "gamma");
    }

    #[test]
    fn browser_log_tail_handles_zero_and_large_limits() {
        assert_eq!(tail_log_lines("alpha\nbeta", 0), "");
        assert_eq!(tail_log_lines("alpha\nbeta", 5), "alpha\nbeta");
    }

    #[test]
    fn parses_goto() {
        Cli::try_parse_from([
            "openpage",
            "goto",
            "https://example.com",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_snapshot() {
        Cli::try_parse_from([
            "openpage",
            "snapshot",
            "--mode",
            "semantic",
            "--format",
            "json",
            "--raw",
            "--depth",
            "4",
            "--selector",
            "#main",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_element_html() {
        Cli::try_parse_from(["openpage", "element-html", "#main", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_locate_chain() {
        Cli::try_parse_from([
            "openpage",
            "locate",
            "@e2",
            ">>",
            "parent",
            ">>",
            "child",
            "a",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_user_agent() {
        Cli::try_parse_from(["openpage", "user-agent", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_status_code() {
        Cli::try_parse_from(["openpage", "status-code", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_ready_state() {
        Cli::try_parse_from(["openpage", "ready-state", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_is_loading() {
        Cli::try_parse_from(["openpage", "is-loading", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_is_headless() {
        Cli::try_parse_from(["openpage", "is-headless", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_screenshot_element() {
        Cli::try_parse_from([
            "openpage",
            "screenshot-element",
            "#hero",
            "hero.png",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_click_with_ref() {
        Cli::try_parse_from(["openpage", "click", "@e5", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_stop_loading() {
        Cli::try_parse_from(["openpage", "stop-loading", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_focus() {
        Cli::try_parse_from(["openpage", "focus", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_value() {
        Cli::try_parse_from(["openpage", "value", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_raw_text() {
        Cli::try_parse_from(["openpage", "raw-text", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_link() {
        Cli::try_parse_from(["openpage", "link", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_open_link() {
        Cli::try_parse_from([
            "openpage",
            "open-link",
            "#go",
            "--background",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_child_count() {
        Cli::try_parse_from(["openpage", "child-count", "#root", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_css_path() {
        Cli::try_parse_from(["openpage", "css-path", "#root", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_xpath() {
        Cli::try_parse_from(["openpage", "xpath", "#root", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_double_click() {
        Cli::try_parse_from(["openpage", "double-click", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_click_at() {
        Cli::try_parse_from([
            "openpage",
            "click-at",
            "#kw",
            "--x",
            "24",
            "--y",
            "12",
            "--button",
            "right",
            "--count",
            "2",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_hover_at() {
        Cli::try_parse_from([
            "openpage",
            "hover-at",
            "#kw",
            "--x",
            "24",
            "--y",
            "12",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_scroll_position() {
        Cli::try_parse_from(["openpage", "scroll-position", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_scroll_element() {
        Cli::try_parse_from([
            "openpage",
            "scroll-element",
            "#pane",
            "down",
            "--pixels",
            "180",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_scroll_element_position() {
        Cli::try_parse_from([
            "openpage",
            "scroll-element-position",
            "#pane",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_active_element() {
        Cli::try_parse_from(["openpage", "active-element", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_storage_get() {
        Cli::try_parse_from([
            "openpage",
            "storage",
            "get",
            "token",
            "--scope",
            "local",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_storage_set() {
        Cli::try_parse_from([
            "openpage",
            "storage",
            "set",
            "token",
            "abc",
            "--scope",
            "session",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_permissions_set() {
        Cli::try_parse_from([
            "openpage",
            "permissions",
            "set",
            "clipboard-read",
            "granted",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_drag() {
        Cli::try_parse_from([
            "openpage",
            "drag",
            "#knob",
            "--dx",
            "120",
            "--dy",
            "8",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_drag_to() {
        Cli::try_parse_from([
            "openpage",
            "drag-to",
            "#item",
            "#dropzone",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_drag_to_point() {
        Cli::try_parse_from([
            "openpage",
            "drag-to-point",
            "#item",
            "--x",
            "320",
            "--y",
            "80",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_drag_in_text() {
        Cli::try_parse_from([
            "openpage",
            "drag-in",
            "#drop",
            "--text",
            "Dragged text",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_key_down() {
        Cli::try_parse_from(["openpage", "key-down", "Shift", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_shortcut() {
        Cli::try_parse_from(["openpage", "shortcut", "Meta", "a", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_copy() {
        Cli::try_parse_from(["openpage", "copy", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_clipboard_write() {
        Cli::try_parse_from([
            "openpage",
            "clipboard",
            "write",
            "hello",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_select_all() {
        Cli::try_parse_from(["openpage", "select-all", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_input() {
        Cli::try_parse_from(["openpage", "input", "hello", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_click_to_download() {
        Cli::try_parse_from([
            "openpage",
            "click-to-download",
            "#export",
            "--dir",
            "/tmp/out",
            "--rename",
            "report",
            "--suffix",
            ".csv",
            "--new-tab",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_click_to_upload() {
        Cli::try_parse_from([
            "openpage",
            "click-to-upload",
            "#picker",
            "a.txt",
            "b.txt",
            "--timeout",
            "4000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_click_for_new_tab() {
        Cli::try_parse_from([
            "openpage",
            "click-for-new-tab",
            "#link",
            "--timeout",
            "4000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_find_all() {
        Cli::try_parse_from(["openpage", "find-all", ".item", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_find_in_page() {
        Cli::try_parse_from([
            "openpage",
            "find-in-page",
            "needle",
            "--backward",
            "--case-sensitive",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_reload_ignore_cache() {
        Cli::try_parse_from(["openpage", "reload", "--ignore-cache", "--session", "agent"])
            .unwrap();
    }

    #[test]
    fn parses_select_range() {
        Cli::try_parse_from([
            "openpage",
            "select-range",
            "#kw",
            "1",
            "4",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_select_text() {
        Cli::try_parse_from([
            "openpage",
            "select-text",
            "#article",
            "--start",
            "2",
            "--end",
            "8",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_selected_text() {
        Cli::try_parse_from(["openpage", "selected-text", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_has_rect() {
        Cli::try_parse_from(["openpage", "has-rect", "#kw", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_count() {
        Cli::try_parse_from(["openpage", "count", ".item", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_wait_clickable() {
        Cli::try_parse_from([
            "openpage",
            "wait-clickable",
            "#go",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_has_rect() {
        Cli::try_parse_from([
            "openpage",
            "wait-has-rect",
            "#go",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_covered() {
        Cli::try_parse_from([
            "openpage",
            "wait-covered",
            "#go",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_not_covered() {
        Cli::try_parse_from([
            "openpage",
            "wait-not-covered",
            "#go",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_stop_moving() {
        Cli::try_parse_from([
            "openpage",
            "wait-stop-moving",
            "#go",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_new_tab() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-new-tab",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_download_begin() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-download-begin",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_downloads_done() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-downloads-done",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_alert_closed() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-alert-closed",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_alert_accept_prompt_text() {
        Cli::try_parse_from([
            "openpage",
            "alert",
            "accept",
            "--prompt-text",
            "Alice",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_alert_has() {
        Cli::try_parse_from(["openpage", "alert", "has", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_wait_for_load_start() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-load-start",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_doc_loaded() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-doc-loaded",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_for_ready_and_navigation() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-ready",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
        Cli::try_parse_from([
            "openpage",
            "wait-for-navigation",
            "--timeout",
            "5000",
            "--token",
            "nav-3",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_navigation_alias_with_token() {
        Cli::try_parse_from([
            "openpage",
            "wait",
            "navigation",
            "--timeout",
            "5000",
            "--token",
            "nav-7",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn wait_condition_aliases_map_to_distinct_wait_ops() {
        assert_eq!(
            super::wait_condition_op("load-start"),
            Some("wait.load_start")
        );
        assert_eq!(
            super::wait_condition_op("doc_loaded"),
            Some("wait.doc_loaded")
        );
        assert_eq!(super::wait_condition_op("loaded"), Some("wait.doc_loaded"));
        assert_eq!(super::wait_condition_op("ready"), Some("wait.ready"));
        assert_eq!(
            super::wait_condition_op("navigation"),
            Some("wait.navigation")
        );
        assert_eq!(super::wait_condition_op("#result"), None);
    }

    #[test]
    fn parses_wait_for_elements_loaded() {
        Cli::try_parse_from([
            "openpage",
            "wait-for-elements-loaded",
            "#a",
            "#b",
            "--any-one",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait_disabled_or_deleted_and_upload_paths_inputted() {
        Cli::try_parse_from([
            "openpage",
            "wait-disabled-or-deleted",
            "#gone",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
        Cli::try_parse_from([
            "openpage",
            "wait-upload-paths-inputted",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_is_clickable() {
        Cli::try_parse_from(["openpage", "is-clickable", "#go", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_type_with_interval() {
        Cli::try_parse_from([
            "openpage",
            "type-with-interval",
            "hello",
            "--interval",
            "0.12",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_wait() {
        Cli::try_parse_from([
            "openpage",
            "wait",
            "element #btn",
            "--session",
            "agent",
            "--timeout",
            "5000",
        ])
        .unwrap();
    }

    #[test]
    fn parses_intercept_start() {
        Cli::try_parse_from(["openpage", "intercept", "start", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_save() {
        Cli::try_parse_from(["openpage", "save", "page.mhtml", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_window_move() {
        Cli::try_parse_from([
            "openpage",
            "window",
            "move",
            "120",
            "48",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_window_list() {
        Cli::try_parse_from(["openpage", "window", "list", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_window_switch() {
        Cli::try_parse_from(["openpage", "window", "switch", "2", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_window_close() {
        Cli::try_parse_from([
            "openpage",
            "window",
            "close",
            "--index",
            "2",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_window_state() {
        Cli::try_parse_from(["openpage", "window", "state", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_zoom_set() {
        Cli::try_parse_from(["openpage", "zoom", "set", "1.25", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_zoom_in() {
        Cli::try_parse_from([
            "openpage",
            "zoom",
            "in",
            "--step",
            "0.2",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_zoom_out() {
        Cli::try_parse_from(["openpage", "zoom", "out", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_zoom_reset() {
        Cli::try_parse_from(["openpage", "zoom", "reset", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_tab_new_window() {
        Cli::try_parse_from([
            "openpage",
            "tab",
            "new",
            "https://example.com",
            "--window",
            "--background",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_tab_duplicate() {
        Cli::try_parse_from([
            "openpage",
            "tab",
            "duplicate",
            "--index",
            "2",
            "--background",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_tab_reopen() {
        Cli::try_parse_from([
            "openpage",
            "tab",
            "reopen",
            "--background",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_tab_close_current() {
        Cli::try_parse_from(["openpage", "tab", "close", "--session", "agent"]).unwrap();
    }

    #[test]
    fn recently_closed_tabs_round_trip() {
        let home = unique_openpage_home("recent-tabs");
        let _guard = EnvVarGuard::set("OPENPAGE_HOME", &home);

        record_recently_closed_tabs(
            "agent",
            &[json!({"url": "https://example.com/", "title": "Example"})],
        )
        .expect("record recent tab");
        let tabs = read_recently_closed_tabs("agent").expect("read recent tabs");
        assert_eq!(
            tabs,
            vec![RecentlyClosedTab {
                url: "https://example.com/".to_string(),
                title: "Example".to_string(),
            }]
        );

        write_recently_closed_tabs("agent", &[]).expect("clear recent tabs");
        assert!(
            !recently_closed_tabs_path("agent")
                .expect("recent tabs path")
                .exists(),
            "clearing recent tabs should remove the sidecar file"
        );
    }

    #[test]
    fn parses_history_list() {
        Cli::try_parse_from(["openpage", "history", "list", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_history_go() {
        Cli::try_parse_from(["openpage", "history", "go", "2", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_history_clear() {
        Cli::try_parse_from(["openpage", "history", "clear", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_downloads_list() {
        Cli::try_parse_from(["openpage", "downloads", "list", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_downloads_last() {
        Cli::try_parse_from(["openpage", "downloads", "last", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_downloads_clear() {
        Cli::try_parse_from(["openpage", "downloads", "clear", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_downloads_cancel() {
        Cli::try_parse_from([
            "openpage",
            "downloads",
            "cancel",
            "guid-1",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn parses_downloads_wait() {
        Cli::try_parse_from([
            "openpage",
            "downloads",
            "wait",
            "hello.txt",
            "--timeout",
            "5000",
            "--session",
            "agent",
        ])
        .unwrap();
    }

    #[test]
    fn finds_download_path_by_filename() {
        let missions = vec![json!({
            "guid": "g1",
            "suggested_filename": "hello.txt",
            "state": "done",
            "final_path": "/tmp/hello.txt",
        })];
        assert_eq!(
            super::find_download_path(&missions, Some("hello.txt"), &[]),
            Some(Value::from("/tmp/hello.txt"))
        );
    }

    #[test]
    fn finds_download_path_for_new_completed_mission() {
        let missions = vec![
            json!({
                "guid": "old",
                "suggested_filename": "old.txt",
                "state": "done",
                "final_path": "/tmp/old.txt",
            }),
            json!({
                "guid": "new",
                "suggested_filename": "new.txt",
                "state": "done",
                "final_path": "/tmp/new.txt",
            }),
        ];
        assert_eq!(
            super::find_download_path(&missions, None, &[String::from("old")]),
            Some(Value::from("/tmp/new.txt"))
        );
    }

    #[test]
    fn response_result_preserves_daemon_error_kind() {
        let error = super::response_result(super::Response::error(
            None,
            "element_not_found",
            "missing #submit",
        ))
        .expect_err("daemon error should not look successful");

        match error {
            OpenPageError::ElementNotFound(message) => {
                assert_eq!(message, "missing #submit");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn response_result_maps_unknown_daemon_error_kind() {
        let error =
            super::response_result(super::Response::error(None, "daemon_state", "not ready"))
                .expect_err("daemon error should not look successful");

        match error {
            OpenPageError::BrowserOperation(message) => {
                assert_eq!(message, "daemon_state: not ready");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn response_result_preserves_structured_fix_without_double_prefix() {
        let detail = "session `inactive-review` is not active. Start it with `openpage browser start --session inactive-review` before retrying.";
        let response = openpage::protocol::response_openpage_error(
            None,
            &OpenPageError::BrowserOperation(detail.to_string()),
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);
        let message = payload["error"]["message"]
            .as_str()
            .expect("message should be a string");

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "inactive-review");
        assert_eq!(
            payload["error"]["fix"],
            "Start it with `openpage browser start --session inactive-review` before retrying."
        );
        assert!(
            message.contains("browser operation failed: session `inactive-review` is not active.")
        );
        assert!(
            !message.contains("browser operation failed: browser operation failed"),
            "daemon error should not be double-prefixed: {message}"
        );
        assert_eq!(payload["error"]["state"], "inactive");
        assert!(payload["error"].get("reasons").is_none());
    }

    #[test]
    fn response_result_reconstructed_error_keeps_state_and_reasons_for_incompatible_session() {
        let detail = "session `mismatch-review` is backed by daemon version 0.0.1 but the current CLI expects 0.1.0. Run `openpage browser stop --session mismatch-review` and then restart that session with the current CLI so its daemon sidecars are recreated with version 0.1.0. Or run `openpage doctor --quick --fix` if you want the CLI to stop the stale daemon for you.";
        let response = openpage::protocol::response_openpage_error(
            None,
            &OpenPageError::BrowserOperation(detail.to_string()),
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "mismatch-review");
        assert_eq!(payload["error"]["state"], "incompatible");
        assert_eq!(payload["error"]["reasons"], json!(["version_mismatch"]));
        assert!(
            payload["error"]["fix"]
                .as_str()
                .expect("fix should be present")
                .contains("browser stop --session mismatch-review")
        );
    }

    #[test]
    fn response_result_uses_structured_session_state_when_message_is_generic() {
        let response = super::Response::error_with_context(
            None,
            "browser_operation",
            "daemon reported inactive session",
            Some(
                "Start it with `openpage browser start --session generic-inactive` before retrying."
                    .to_string(),
            ),
            Some("generic-inactive".to_string()),
            Some("inactive".to_string()),
            None,
            None,
            None,
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "generic-inactive");
        assert_eq!(payload["error"]["state"], "inactive");
        assert_eq!(
            payload["error"]["fix"],
            "Start it with `openpage browser start --session generic-inactive` before retrying."
        );
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("session `generic-inactive` is not active")
        );
    }

    #[test]
    fn response_result_uses_structured_transient_fields_when_message_is_generic() {
        let response = super::Response::error_with_context(
            None,
            "daemon_transient",
            "io error: connection reset by peer",
            Some("Retry the same command.".to_string()),
            Some("retry-review".to_string()),
            None,
            None,
            Some(true),
            Some("retry_same_command".to_string()),
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "daemon_transient");
        assert_eq!(payload["error"]["session"], "retry-review");
        assert_eq!(payload["error"]["retryable"], true);
        assert_eq!(payload["error"]["suggested_action"], "retry_same_command");
        assert_eq!(payload["error"]["fix"], "Retry the same command.");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("daemon transient for session `retry-review`")
        );
    }

    #[test]
    fn response_result_uses_structured_incompatible_state_when_message_is_generic() {
        let response = super::Response::error_with_context(
            None,
            "browser_operation",
            "daemon reported version mismatch",
            Some("Stop and restart the session.".to_string()),
            Some("generic-mismatch".to_string()),
            Some("incompatible".to_string()),
            Some(vec!["version_mismatch".to_string()]),
            None,
            None,
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "generic-mismatch");
        assert_eq!(payload["error"]["state"], "incompatible");
        assert_eq!(payload["error"]["reasons"], json!(["version_mismatch"]));
        assert_eq!(payload["error"]["fix"], "Stop and restart the session.");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("session `generic-mismatch` has a daemon version mismatch")
        );
    }

    #[test]
    fn response_result_uses_structured_busy_state_when_message_is_generic() {
        let response = super::Response::error_with_context(
            None,
            "browser_operation",
            "daemon reported busy session",
            Some("Inspect logs or restart the session.".to_string()),
            Some("generic-busy".to_string()),
            Some("incomplete".to_string()),
            Some(vec!["daemon_unresponsive".to_string()]),
            None,
            None,
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "browser_operation");
        assert_eq!(payload["error"]["session"], "generic-busy");
        assert_eq!(payload["error"]["state"], "incomplete");
        assert_eq!(payload["error"]["reasons"], json!(["daemon_unresponsive"]));
        assert_eq!(
            payload["error"]["fix"],
            "Inspect logs or restart the session."
        );
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("session `generic-busy` is currently busy or unresponsive")
        );
    }

    #[test]
    fn response_result_uses_structured_session_and_fix_for_generic_startup_failure_io() {
        let response = super::Response::error_with_context(
            None,
            "io",
            "daemon startup timed out",
            Some(
                "Run `openpage browser logs --session startup-review --tail 20` to inspect the persisted daemon log, then retry the start command."
                    .to_string(),
            ),
            Some("startup-review".to_string()),
            None,
            None,
            None,
            None,
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(payload["error"]["session"], "startup-review");
        assert_eq!(
            payload["error"]["fix"],
            "Run `openpage browser logs --session startup-review --tail 20` to inspect the persisted daemon log, then retry the start command."
        );
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("daemon for session 'startup-review' startup failure")
        );
    }

    #[test]
    fn response_result_uses_structured_session_for_generic_io_without_fix() {
        let response = super::Response::error_with_context(
            None,
            "io",
            "permission denied",
            None,
            Some("io-review".to_string()),
            None,
            None,
            None,
            None,
        );

        let error =
            super::response_result(response).expect_err("daemon error should not look successful");
        let payload = openpage::protocol::simple_openpage_error(&error);

        assert_eq!(payload["error"]["kind"], "io");
        assert_eq!(payload["error"]["session"], "io-review");
        assert!(payload["error"].get("fix").is_none());
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be a string")
                .contains("daemon for session 'io-review': permission denied")
        );
    }

    #[test]
    fn response_result_preserves_invalid_input_kind_from_daemon_response() {
        let error = super::response_result(super::Response::error(
            None,
            "invalid_input",
            "unsupported snapshot format: xml",
        ))
        .expect_err("daemon error should not look successful");

        let payload = openpage::protocol::simple_openpage_error(&error);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "unsupported snapshot format: xml"
        );
    }

    #[test]
    fn response_result_preserves_invalid_input_kind_for_param_validation_detail() {
        let error = super::response_result(super::Response::error(
            None,
            "invalid_input",
            "history index must be >= 1",
        ))
        .expect_err("daemon error should not look successful");

        let payload = openpage::protocol::simple_openpage_error(&error);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(payload["error"]["message"], "history index must be >= 1");
        assert_eq!(
            payload["error"]["fix"],
            "Use a history index of 1 or greater before retrying."
        );
    }

    #[test]
    fn response_result_preserves_invalid_input_kind_for_missing_string_param_detail() {
        let error = super::response_result(super::Response::error(
            None,
            "invalid_input",
            "missing string param: locator",
        ))
        .expect_err("daemon error should not look successful");

        let payload = openpage::protocol::simple_openpage_error(&error);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(payload["error"]["message"], "missing string param: locator");
    }

    #[test]
    fn response_result_preserves_invalid_input_kind_for_unknown_navigation_token() {
        let error = super::response_result(super::Response::error(
            None,
            "invalid_input",
            "unknown navigation token: definitely-bad",
        ))
        .expect_err("daemon error should not look successful");

        let payload = openpage::protocol::simple_openpage_error(&error);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["message"],
            "unknown navigation token: definitely-bad"
        );
    }

    #[test]
    fn navigation_followup_adds_wait_command_for_navigation_token() {
        let payload = super::with_navigation_followup(
            "review session",
            json!({
                "clicked": true,
                "navigation_token": "nav-42",
            }),
        );

        assert_eq!(payload["clicked"], true);
        assert_eq!(payload["navigation_token"], "nav-42");
        assert_eq!(payload["wait_for_navigation"]["session"], "review session");
        assert_eq!(payload["wait_for_navigation"]["token"], "nav-42");
        assert_eq!(
            payload["wait_for_navigation"]["command"],
            "openpage wait-for-navigation --session 'review session' --token nav-42"
        );
    }

    #[test]
    fn navigation_followup_skips_results_without_navigation_token() {
        let payload = super::with_navigation_followup(
            "review",
            json!({
                "clicked": true,
                "button": "right",
            }),
        );

        assert_eq!(payload["clicked"], true);
        assert_eq!(payload["button"], "right");
        assert!(payload.get("wait_for_navigation").is_none());
    }

    #[test]
    fn browser_start_followup_without_url_points_to_goto_and_stop() {
        let payload = super::with_browser_start_followup(
            "review session",
            None,
            json!({
                "session": "review session",
                "port": 9222,
            }),
        );

        assert_eq!(payload["session"], "review session");
        assert_eq!(
            payload["next_steps"]["goto"],
            "openpage goto --session 'review session' https://example.com"
        );
        assert_eq!(
            payload["next_steps"]["stop"],
            "openpage browser stop --session 'review session'"
        );
    }

    #[test]
    fn browser_start_followup_with_url_points_to_wait_snapshot_and_stop() {
        let payload = super::with_browser_start_followup(
            "review session",
            Some("https://example.com"),
            json!({
                "session": "review session",
                "port": 9222,
                "url": "https://example.com",
            }),
        );

        assert_eq!(
            payload["next_steps"]["wait_for_ready"],
            "openpage wait-for-ready --session 'review session'"
        );
        assert_eq!(
            payload["next_steps"]["snapshot"],
            "openpage snapshot --session 'review session'"
        );
        assert_eq!(
            payload["next_steps"]["stop"],
            "openpage browser stop --session 'review session'"
        );
    }

    #[test]
    fn cleanup_after_browser_launch_failure_removes_stale_sidecars() {
        let home = unique_openpage_home("launch-cleanup");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        std::fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let session = "launch-cleanup";
        std::fs::write(port_path(session).expect("port path"), "12345").expect("write port");
        std::fs::write(pid_path(session).expect("pid path"), "999999").expect("write pid");
        std::fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        super::cleanup_after_browser_launch_failure(
            session,
            &OpenPageError::BrowserLaunch("synthetic launch failure".to_string()),
        );

        assert!(!port_path(session).expect("port path").exists());
        assert!(!pid_path(session).expect("pid path").exists());
        assert!(!version_path(session).expect("version path").exists());
    }

    #[test]
    fn cleanup_after_browser_launch_failure_ignores_non_launch_errors() {
        let home = unique_openpage_home("launch-cleanup-ignore");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        std::fs::create_dir_all(daemon_dir().expect("daemon dir")).expect("create daemon dir");

        let session = "launch-cleanup-ignore";
        std::fs::write(port_path(session).expect("port path"), "12345").expect("write port");
        std::fs::write(pid_path(session).expect("pid path"), "999999").expect("write pid");
        std::fs::write(
            version_path(session).expect("version path"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("write version");

        super::cleanup_after_browser_launch_failure(
            session,
            &OpenPageError::BrowserOperation("synthetic non-launch failure".to_string()),
        );

        assert!(port_path(session).expect("port path").exists());
        assert!(pid_path(session).expect("pid path").exists());
        assert!(version_path(session).expect("version path").exists());
    }

    #[test]
    fn rpc_page_rejects_inactive_session_without_creating_sidecars() {
        let home = unique_openpage_home("inactive-session");
        let _env_guard = EnvVarGuard::set("OPENPAGE_HOME", &home);
        let daemon = daemon_dir().expect("daemon dir path");
        assert!(!daemon.exists(), "test should start without daemon dir");

        let error = super::rpc_page("inactive-review", "page.title", Value::Null)
            .expect_err("inactive session should fail instead of starting a fresh daemon/browser");

        match error {
            OpenPageError::BrowserOperation(message) => {
                assert!(message.contains("is not active"));
                assert!(message.contains("browser start --session inactive-review"));
            }
            other => panic!("expected BrowserOperation, got {other:?}"),
        }

        assert!(
            !port_path("inactive-review").expect("port path").exists(),
            "inactive read command should not create port sidecar"
        );
        assert!(
            !pid_path("inactive-review").expect("pid path").exists(),
            "inactive read command should not create pid sidecar"
        );
        assert!(
            !version_path("inactive-review")
                .expect("version path")
                .exists(),
            "inactive read command should not create version sidecar"
        );
    }

    #[test]
    fn parses_batch_with_commands() {
        Cli::try_parse_from([
            "openpage",
            "batch",
            "--bail",
            "browser start https://example.com --headless",
            "title",
            "browser stop",
        ])
        .unwrap();
    }

    #[test]
    fn parses_batch_without_commands() {
        Cli::try_parse_from(["openpage", "batch", "--bail"]).unwrap();
    }

    #[test]
    fn batch_error_payload_uses_invalid_input_for_nested_parse_errors() {
        let payload = super::batch_error_payload(&OpenPageError::UnsupportedOperation(
            "invalid batch command `page url`: error: unrecognized subcommand 'page'".to_string(),
        ));

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be string")
                .contains("invalid batch command `page url`")
        );
        assert!(
            payload["error"]["fix"]
                .as_str()
                .expect("fix should be string")
                .contains("old `page ...` surface was removed")
        );
    }

    #[test]
    fn batch_error_payload_exposes_fix_for_removed_stdio_surface() {
        let payload = super::batch_error_payload(&OpenPageError::UnsupportedOperation(
            "invalid batch command `serve --stdio`: error: unexpected argument '--stdio' found"
                .to_string(),
        ));

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert_eq!(
            payload["error"]["fix"],
            "Use `openpage serve --session <name>` for the TCP daemon workflow. The removed `serve --stdio` surface is intentionally rejected."
        );
    }

    #[test]
    fn batch_error_payload_uses_invalid_input_for_invalid_stdin_json() {
        let payload = super::batch_error_payload(&OpenPageError::Serialization(
            "invalid batch stdin JSON: expected value at line 1 column 1; expected an array of argv arrays"
                .to_string(),
        ));

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_input");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message should be string")
                .contains("invalid batch stdin JSON:")
        );
    }

    #[test]
    fn batch_error_payload_keeps_unsupported_operation_for_batch_restrictions() {
        let payload = super::batch_error_payload(&OpenPageError::UnsupportedOperation(
            "batch cannot execute `serve`; use top-level `serve` separately".to_string(),
        ));

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert_eq!(
            payload["error"]["message"],
            "unsupported operation: batch cannot execute `serve`; use top-level `serve` separately"
        );
        assert_eq!(
            payload["error"]["fix"],
            "Run `openpage serve --session <name>` as a separate top-level command, then invoke follow-up commands outside `batch`."
        );
    }

    #[test]
    fn batch_error_payload_exposes_fix_for_nested_batch_restriction() {
        let payload = super::batch_error_payload(&OpenPageError::UnsupportedOperation(
            "batch cannot execute nested batch commands".to_string(),
        ));

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "unsupported_operation");
        assert_eq!(
            payload["error"]["fix"],
            "Flatten the command list into a single top-level `openpage batch ...` invocation instead of nesting `batch` inside `batch`."
        );
    }

    #[test]
    fn parses_doctor() {
        Cli::try_parse_from(["openpage", "doctor"]).unwrap();
    }

    #[test]
    fn parses_doctor_quick() {
        Cli::try_parse_from(["openpage", "doctor", "--quick"]).unwrap();
    }

    #[test]
    fn parses_serve_port() {
        Cli::try_parse_from(["openpage", "serve", "--port", "9876"]).unwrap();
    }

    #[test]
    fn rejects_serve_stdio_flag() {
        assert!(
            Cli::try_parse_from(["openpage", "serve", "--stdio"]).is_err(),
            "serve --stdio should remain rejected so TCP stays the only active daemon protocol"
        );
    }

    #[test]
    fn rejects_removed_page_get_command() {
        assert!(
            Cli::try_parse_from(["openpage", "page", "get", "https://example.com"]).is_err(),
            "removed page get command should remain rejected"
        );
    }

    #[test]
    fn rejects_removed_page_url_command() {
        assert!(
            Cli::try_parse_from(["openpage", "page", "url", "--session", "agent"]).is_err(),
            "removed page url command should remain rejected"
        );
    }

    #[test]
    fn rejects_removed_page_title_command() {
        assert!(
            Cli::try_parse_from(["openpage", "page", "title", "--session", "agent"]).is_err(),
            "removed page title command should remain rejected"
        );
    }

    #[test]
    fn rejects_removed_page_screenshot_command() {
        assert!(
            Cli::try_parse_from([
                "openpage",
                "page",
                "screenshot",
                "shot.png",
                "--session",
                "agent",
            ])
            .is_err(),
            "removed page screenshot command should remain rejected"
        );
    }
}
