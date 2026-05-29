use clap::Parser;
use std::io::Read;

use serde_json::{Value, json};

use crate::cli::args::{
    AlertCommand, AttrArgs, BatchArgs, BrowserCommand, BrowserStartArgs, Cli, ClickAtArgs,
    ClickForNewTabArgs, ClickToDownloadArgs, ClickToUploadArgs, Command, CookiesCommand,
    DownloadArgs, DragArgs, DragInArgs, DragToArgs, DragToPointArgs, ElementArgs, FillArgs,
    FindInPageArgs, FrameCommand, GotoArgs, HistoryCommand, InterceptCommand, JsArgs, KeyArgs,
    PageTextArgs, PdfArgs, PressArgs, ScreenshotArgs, ScrollArgs, ScrollIntoViewArgs, SelectArgs,
    SelectRangeArgs, SelectTextArgs, SessionArgs, ShortcutArgs, StorageCommand, StorageScope,
    TabCommand, TypeWithIntervalArgs, UploadArgs, WaitArgs, WaitElementArgs, WaitForFunctionArgs,
    WaitForTextArgs, WaitForTitleArgs, WaitForUrlArgs, WaitTimeoutArgs, WindowCommand,
    WindowMoveArgs,
};
use crate::cli::connection::{
    cleanup_sidecars, daemon_inventory, daemon_ready as tcp_daemon_ready, daemon_status, read_port,
    send_request,
};
use crate::cli::protocol::{Request, Response, format_output_json, simple_error, simple_ok};
use crate::error::{OpenPageError, OpenPageResult};

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
        Command::Goto(args) => run_goto(args),
        Command::Back(args) => run_back(args),
        Command::Forward(args) => run_forward(args),
        Command::Reload(args) => run_reload(args),
        Command::StopLoading(args) => run_stop_loading(args),
        Command::Url(args) => run_url(args),
        Command::Title(args) => run_title(args),
        Command::Html(args) => run_html(args),
        Command::Snapshot(args) => run_snapshot(args),
        Command::Screenshot(args) => run_screenshot(args),
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
        Command::SelectedText(args) => run_selected_text(args),
        Command::Attr(args) => run_attr(args),
        Command::Wait(args) => run_wait(args),
        Command::Intercept(command) => run_intercept(command),
        Command::Js(args) => run_js(args),
        Command::Download(args) => run_download(args),
        Command::Window(command) => run_window(command),
        Command::Alert(command) => run_alert(command),
        Command::Scroll(args) => run_scroll(args),
        Command::ScrollIntoView(args) => run_scroll_into_view(args),
        Command::Hover(args) => run_hover(args),
        Command::Press(args) => run_press(args),
        Command::Select(args) => run_select(args),
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
        Command::Find(args) => run_find(args),
        Command::FindInPage(args) => run_find_in_page(args),
        Command::FindAll(args) => run_find_all(args),
        Command::Count(args) => run_count(args),
        Command::WaitVisible(args) => run_wait_visible(args),
        Command::WaitHidden(args) => run_wait_hidden(args),
        Command::WaitEnabled(args) => run_wait_enabled(args),
        Command::WaitDisabled(args) => run_wait_disabled(args),
        Command::WaitDeleted(args) => run_wait_deleted(args),
        Command::WaitClickable(args) => run_wait_clickable(args),
        Command::ActiveElement(args) => run_active_element(args),
        Command::WaitForNewTab(args) => run_wait_for_new_tab(args),
        Command::WaitForDownloadBegin(args) => run_wait_for_download_begin(args),
        Command::WaitForDownloadsDone(args) => run_wait_for_downloads_done(args),
        Command::WaitForAlertClosed(args) => run_wait_for_alert_closed(args),
        Command::WaitForLoadStart(args) => run_wait_for_load_start(args),
        Command::WaitForUrl(args) => run_wait_for_url(args),
        Command::WaitForTitle(args) => run_wait_for_title(args),
        Command::WaitForFunction(args) => run_wait_for_function(args),
        Command::WaitForText(args) => run_wait_for_text(args),
        Command::Pdf(args) => run_pdf(args),
        Command::History(command) => run_history(command),
        Command::Storage(command) => run_storage(command),
        Command::Cookies(command) => run_cookies(command),
        Command::Tab(command) => run_tab(command),
        Command::Frame(command) => run_frame(command),
        Command::Doctor(_) => unreachable!("doctor is handled by cli::doctor"),
        Command::Batch(_) => unreachable!("batch is handled by oneshot::run"),
        Command::Serve(_) => unreachable!("serve is handled by cli::serve"),
    }
}

fn run_scroll(args: ScrollArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
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

fn run_browser(command: BrowserCommand) -> OpenPageResult<()> {
    match command {
        BrowserCommand::Start(args) => start_browser(args),
        BrowserCommand::Stop(args) => stop_browser(args, false),
        BrowserCommand::List => {
            let inventory = daemon_inventory()?;
            print_json(simple_ok(json!({
                "sessions": inventory.sessions,
                "incomplete": inventory.incomplete,
                "cleaned": inventory.cleaned,
            })))
        }
        BrowserCommand::Status(args) => {
            let status = daemon_status(&args.session)?;
            print_json(simple_ok(json!(status)))
        }
    }
}

fn run_goto(args: GotoArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "webpage.get",
        json!({
            "url": args.url,
        }),
    )?;
    if args.wait {
        let _ = rpc_webpage(
            &args.session,
            "wait.doc_loaded",
            json!({
                "timeout_ms": 10_000,
            }),
        )?;
    }
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(
        json!({"loaded": true, "url": url.get("url").cloned()}),
    ))
}

fn run_back(args: SessionArgs) -> OpenPageResult<()> {
    let navigated = rpc_webpage(&args.session, "webpage.back", Value::Null)?
        .get("back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if navigated {
        let _ = rpc_webpage(
            &args.session,
            "wait.doc_loaded",
            json!({"timeout_ms": 10_000}),
        )?;
    }
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(
        json!({"back": navigated, "url": url.get("url").cloned()}),
    ))
}

fn run_forward(args: SessionArgs) -> OpenPageResult<()> {
    let navigated = rpc_webpage(&args.session, "webpage.forward", Value::Null)?
        .get("forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if navigated {
        let _ = rpc_webpage(
            &args.session,
            "wait.doc_loaded",
            json!({"timeout_ms": 10_000}),
        )?;
    }
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(
        json!({"forward": navigated, "url": url.get("url").cloned()}),
    ))
}

fn run_reload(args: SessionArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "webpage.reload",
        json!({"timeout_ms": 10_000}),
    )?;
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(
        json!({"reloaded": true, "url": url.get("url").cloned()}),
    ))
}

fn run_stop_loading(args: SessionArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(&args.session, "webpage.stop_loading", Value::Null)?;
    print_json(simple_ok(json!({"stopped_loading": true})))
}

fn run_url(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.url",
        Value::Null,
    )?))
}

fn run_title(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.title",
        Value::Null,
    )?))
}

fn run_html(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.html",
        Value::Null,
    )?))
}

fn run_snapshot(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.snapshot",
        Value::Null,
    )?))
}

fn run_screenshot(args: ScreenshotArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.screenshot",
        json!({
            "path": args.output,
            "full_page": args.full_page,
        }),
    )?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_click(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.click",
        json!({"locator": args.locator}),
    )?))
}

fn run_fill(args: FillArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.input",
        json!({"locator": args.locator, "text": args.text}),
    )?))
}

fn run_focus(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.focus",
        json!({"locator": args.locator}),
    )?))
}

fn run_clear(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.clear",
        json!({"locator": args.locator}),
    )?))
}

fn run_submit(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.submit",
        json!({"locator": args.locator}),
    )?))
}

fn run_check(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.check",
        json!({"locator": args.locator}),
    )?))
}

fn run_uncheck(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.uncheck",
        json!({"locator": args.locator}),
    )?))
}

fn run_right_click(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.click_right",
        json!({"locator": args.locator}),
    )?))
}

fn run_middle_click(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.click_middle",
        json!({"locator": args.locator}),
    )?))
}

fn run_double_click(args: ElementArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "element.click_multi",
        json!({"locator": args.locator, "count": 2}),
    )?;
    print_json(simple_ok(json!({"clicked": true, "count": 2})))
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
    let _ = rpc_webpage(
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
    print_json(simple_ok(json!({
        "clicked": true,
        "button": button,
        "count": count,
    })))
}

fn run_key_down(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.key_down",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(
        json!({"dispatched": true, "event": "keydown", "key": args.key}),
    ))
}

fn run_key_up(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.key_up",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(
        json!({"dispatched": true, "event": "keyup", "key": args.key}),
    ))
}

fn run_shortcut(args: ShortcutArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
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
    let _ = rpc_webpage(
        &args.session,
        "page.type_keys",
        json!({"text": [modifier, key]}),
    )?;
    print_json(simple_ok(json!({result_key: true, "key": key})))
}

fn run_input(args: PageTextArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.input",
        json!({"text": args.text.clone()}),
    )?;
    print_json(simple_ok(json!({"input": true, "text": args.text})))
}

fn run_type(args: PageTextArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.type",
        json!({"text": args.text.clone()}),
    )?;
    print_json(simple_ok(json!({"typed": true, "text": args.text})))
}

fn run_type_with_interval(args: TypeWithIntervalArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
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
    let _ = rpc_webpage(
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
    let _ = rpc_webpage(
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
    let _ = rpc_webpage(
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
        let _ = rpc_webpage(
            &args.session,
            "page.drag_in",
            json!({"target": args.target, "text": text}),
        )?;
        print_json(simple_ok(
            json!({"dragged": true, "target": args.target, "kind": "text"}),
        ))
    } else if !args.files.is_empty() {
        let _ = rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.text",
        json!({"locator": args.locator}),
    )?))
}

fn run_attr(args: AttrArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.attr",
        json!({"locator": args.locator, "name": args.name}),
    )?))
}

fn run_wait(args: WaitArgs) -> OpenPageResult<()> {
    let condition = args.condition.trim();
    if condition.eq_ignore_ascii_case("navigation")
        || condition.eq_ignore_ascii_case("load")
        || condition.eq_ignore_ascii_case("loaded")
    {
        let _ = rpc_webpage(
            &args.session,
            "wait.doc_loaded",
            json!({"timeout_ms": args.timeout}),
        )?;
    } else {
        let locator = condition
            .strip_prefix("element ")
            .map(str::trim)
            .unwrap_or(condition);
        let _ = rpc_webpage(
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
            let _ = rpc_webpage(&session, "intercept.start", Value::Null)?;
            print_json(simple_ok(json!({"intercept": "started"})))
        }
        InterceptCommand::Stop(_) => {
            let _ = rpc_webpage(&session, "intercept.stop", Value::Null)?;
            print_json(simple_ok(json!({"intercept": "stopped"})))
        }
        InterceptCommand::Status(_) => {
            let status = rpc_webpage(&session, "intercept.status", Value::Null)?;
            print_json(simple_ok(json!({
                "listening": status.get("listening").cloned(),
                "paused": status.get("paused").cloned(),
            })))
        }
    }
}

fn run_js(args: JsArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "page.run_js",
        json!({"script": args.script}),
    )?))
}

fn run_download(args: DownloadArgs) -> OpenPageResult<()> {
    let path = rpc_webpage(
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

fn run_window(command: WindowCommand) -> OpenPageResult<()> {
    match command {
        WindowCommand::State(args) => print_json(simple_ok(rpc_webpage(
            &args.session,
            "window.state",
            Value::Null,
        )?)),
        WindowCommand::Location(args) => print_json(simple_ok(rpc_webpage(
            &args.session,
            "window.location",
            Value::Null,
        )?)),
        WindowCommand::Max(args) => {
            let _ = rpc_webpage(&args.session, "window.max", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "maximized"})))
        }
        WindowCommand::Min(args) => {
            let _ = rpc_webpage(&args.session, "window.min", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "minimized"})))
        }
        WindowCommand::Fullscreen(args) => {
            let _ = rpc_webpage(&args.session, "window.full", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "fullscreen"})))
        }
        WindowCommand::Normal(args) => {
            let _ = rpc_webpage(&args.session, "window.normal", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "state": "normal"})))
        }
        WindowCommand::Hide(args) => {
            let _ = rpc_webpage(&args.session, "window.hide", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "visible": false})))
        }
        WindowCommand::Show(args) => {
            let _ = rpc_webpage(&args.session, "window.show", Value::Null)?;
            print_json(simple_ok(json!({"window": true, "visible": true})))
        }
        WindowCommand::Size(args) => {
            let _ = rpc_webpage(
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

fn run_window_move(args: WindowMoveArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
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
            let text = rpc_webpage(
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
            let text = rpc_webpage(
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
            let has_alert = rpc_webpage(&args.session, "alert.has", Value::Null)?
                .get("has_alert")
                .cloned();
            print_json(simple_ok(json!({"has_alert": has_alert})))
        }
        AlertCommand::Text(args) => {
            let text = rpc_webpage(&args.session, "alert.text", Value::Null)?
                .get("text")
                .cloned();
            print_json(simple_ok(json!({"text": text})))
        }
    }
}

fn start_browser(args: BrowserStartArgs) -> OpenPageResult<()> {
    let headless = args.headless && !args.head;
    let create = rpc_request(
        &args.session,
        Some(args.session.clone()),
        "webpage.create",
        json!({
            "session": args.session,
            "headless": headless,
            "browser_path": args.browser_path,
            "user_data_dir": args.user_data_dir,
            "width": args.width,
            "height": args.height,
            "no_sandbox": args.no_sandbox,
        }),
    )?;

    if let Some(url) = &args.url {
        let _ = rpc_webpage(
            &args.session,
            "webpage.get",
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

    if !existing {
        print_json(simple_ok(json!({
            "session": args.session,
            "target": create.get("target").cloned(),
            "port": port,
            "headless": headless,
            "url": args.url,
        })))
    } else {
        print_json(simple_ok(json!({
            "session": args.session,
            "already_running": true,
            "target": create.get("target").cloned(),
            "port": port,
            "url": args.url,
        })))
    }
}

fn stop_browser(args: SessionArgs, quiet: bool) -> OpenPageResult<()> {
    if tcp_daemon_ready(&args.session) {
        let _ = rpc_request(
            &args.session,
            Some(args.session.clone()),
            "webpage.quit",
            Value::Null,
        );
        let _ = rpc_request(&args.session, None, "daemon.shutdown", Value::Null);
    }
    let _ = cleanup_sidecars(&args.session);
    if quiet {
        Ok(())
    } else {
        print_json(simple_ok(json!({"stopped": true, "session": args.session})))
    }
}

fn run_batch(args: BatchArgs) -> OpenPageResult<i32> {
    let commands = batch_commands(&args)?;
    let mut had_error = false;

    for command_args in commands {
        if command_args.is_empty() {
            continue;
        }

        let command = match parse_batch_command(&command_args) {
            Ok(command) => command,
            Err(err) => {
                print_json(simple_error("openpage", err.to_string()))?;
                had_error = true;
                if args.bail {
                    break;
                }
                continue;
            }
        };

        if let Err(err) = run_single(command) {
            print_json(simple_error("openpage", err.to_string()))?;
            had_error = true;
            if args.bail {
                break;
            }
        }
    }

    Ok(if had_error { 1 } else { 0 })
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

fn rpc_webpage(session: &str, op: &str, params: Value) -> OpenPageResult<Value> {
    let _ = rpc_request(
        session,
        Some(session.to_string()),
        "webpage.create",
        json!({
            "session": session,
            "headless": true,
        }),
    )?;
    rpc_request(session, Some(session.to_string()), op, params)
}

fn tab_target_id_from_index(session: &str, index: usize) -> OpenPageResult<String> {
    let response = rpc_webpage(session, "tab.list", Value::Null)?;
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

fn response_result(response: Response) -> OpenPageResult<Value> {
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null))
    } else {
        let message = response
            .error
            .map(|error| format!("{}: {}", error.kind, error.message))
            .unwrap_or_else(|| "daemon request failed".to_string());
        Err(OpenPageError::BrowserOperation(message))
    }
}

fn print_json(value: Value) -> OpenPageResult<()> {
    println!(
        "{}",
        format_output_json(&value).map_err(|err| OpenPageError::Serialization(err.to_string()))?
    );
    Ok(())
}

fn run_scroll_into_view(args: ScrollIntoViewArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "element.scroll_into_view",
        json!({"locator": args.locator, "center": args.center}),
    )?;
    print_json(simple_ok(json!({"scrolled_into_view": true})))
}

fn run_hover(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.hover",
        json!({"locator": args.locator}),
    )?))
}

fn run_press(args: PressArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "element.press_key",
        json!({"locator": args.locator, "key": args.key.clone()}),
    )?;
    print_json(simple_ok(json!({"pressed": true, "key": args.key})))
}

fn run_select(args: SelectArgs) -> OpenPageResult<()> {
    let selected = rpc_webpage(
        &args.session,
        "element.select",
        json!({
            "locator": args.locator,
            "text": args.text,
            "value": args.value,
            "index": args.index,
        }),
    )?
    .get("selected")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"selected": selected})))
}

fn run_selected_text(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "page.selected_text",
        Value::Null,
    )?))
}

fn run_select_text(args: SelectTextArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
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
    let _ = rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_visible",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_enabled(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_enabled",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_checked(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_checked",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_selected(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_selected",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_alive(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_alive",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_in_viewport(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_in_viewport",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_whole_in_viewport(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_whole_in_viewport",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_covered(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_covered",
        json!({"locator": args.locator}),
    )?))
}

fn run_is_clickable(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "element.is_clickable",
        json!({"locator": args.locator}),
    )?))
}

fn run_find(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.find",
        json!({"locator": args.locator}),
    )?))
}

fn run_find_in_page(args: FindInPageArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
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
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.find_all",
        json!({"locator": args.locator}),
    )?))
}

fn run_count(args: ElementArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.count",
        json!({"locator": args.locator}),
    )?))
}

fn run_wait_visible(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_displayed",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"visible": ready, "waited": ready})))
}

fn run_wait_hidden(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_hidden",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"hidden": ready, "waited": ready})))
}

fn run_wait_enabled(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_enabled",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"enabled": ready, "waited": ready})))
}

fn run_wait_disabled(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_disabled",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"disabled": ready, "waited": ready})))
}

fn run_wait_deleted(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_deleted",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"deleted": ready, "waited": ready})))
}

fn run_wait_clickable(args: WaitElementArgs) -> OpenPageResult<()> {
    let ready = rpc_webpage(
        &args.session,
        "wait.ele_clickable",
        json!({"locator": args.locator, "timeout_ms": args.timeout}),
    )?
    .get("ready")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"clickable": ready, "waited": ready})))
}

fn run_active_element(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(
        &args.session,
        "webpage.active_element",
        Value::Null,
    )?))
}

fn run_wait_for_new_tab(args: WaitTimeoutArgs) -> OpenPageResult<()> {
    let target_id = rpc_webpage(
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
    let mission = rpc_webpage(
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
    let done = rpc_webpage(
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
    let closed = rpc_webpage(
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
    let started = rpc_webpage(
        &args.session,
        "wait.load_start",
        json!({"timeout_ms": args.timeout}),
    )?
    .get("started")
    .and_then(Value::as_bool)
    .unwrap_or(false);
    print_json(simple_ok(json!({"waited": started, "started": started})))
}

fn run_wait_for_url(args: WaitForUrlArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "wait.url_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(
        json!({"waited": true, "url": url.get("url").cloned()}),
    ))
}

fn run_wait_for_title(args: WaitForTitleArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "wait.title_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let title = rpc_webpage(&args.session, "webpage.title", Value::Null)?;
    print_json(simple_ok(
        json!({"waited": true, "title": title.get("title").cloned()}),
    ))
}

fn run_wait_for_function(args: WaitForFunctionArgs) -> OpenPageResult<()> {
    let value = rpc_webpage(
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
    let _ = rpc_webpage(
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

fn run_pdf(args: PdfArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(&args.session, "page.pdf", json!({"path": args.output}))?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_history(command: HistoryCommand) -> OpenPageResult<()> {
    let session = match &command {
        HistoryCommand::List(args) => args.session.clone(),
        HistoryCommand::Go(args) => args.session.clone(),
    };
    match command {
        HistoryCommand::List(_) => print_json(simple_ok(rpc_webpage(
            &session,
            "history.list",
            Value::Null,
        )?)),
        HistoryCommand::Go(args) => print_json(simple_ok(rpc_webpage(
            &session,
            "history.go",
            json!({"index": args.index}),
        )?)),
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
                rpc_webpage(&session, "webpage.local_storage", json!({"item": args.key}))?
            } else {
                rpc_webpage(
                    &session,
                    "webpage.session_storage",
                    json!({"item": args.key}),
                )?
            };
            print_json(simple_ok(json!({
                "scope": storage_scope_name(&args.scope),
                "key": args.key,
                "value": value.get("value").cloned(),
            })))
        }
        StorageCommand::Set(args) => {
            if matches!(args.scope, StorageScope::Local) {
                let _ = rpc_webpage(
                    &session,
                    "set.local_storage",
                    json!({"item": args.key, "value": args.value}),
                )?;
            } else {
                let _ = rpc_webpage(
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

fn run_cookies(command: CookiesCommand) -> OpenPageResult<()> {
    let session = match &command {
        CookiesCommand::Get(args) | CookiesCommand::Clear(args) => args.session.clone(),
        CookiesCommand::Set(args) => args.session.clone(),
        CookiesCommand::Delete(args) => args.session.clone(),
    };
    match command {
        CookiesCommand::Get(_) => {
            let cookies = rpc_webpage(&session, "webpage.cookies", Value::Null)?
                .get("cookies")
                .cloned();
            print_json(simple_ok(json!({"cookies": cookies})))
        }
        CookiesCommand::Set(args) => {
            let url = match args.url {
                Some(u) => Some(u),
                None => rpc_webpage(&session, "webpage.url", Value::Null)?
                    .get("url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            };
            let _ = rpc_webpage(
                &session,
                "cookies.set",
                json!({"name": args.name, "value": args.value, "url": url}),
            )?;
            print_json(simple_ok(json!({"set": true, "name": args.name})))
        }
        CookiesCommand::Delete(args) => {
            let url = match args.url {
                Some(u) => Some(u),
                None => rpc_webpage(&session, "webpage.url", Value::Null)?
                    .get("url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            };
            let _ = rpc_webpage(
                &session,
                "cookies.delete",
                json!({"name": args.name, "url": url}),
            )?;
            print_json(simple_ok(json!({"deleted": true, "name": args.name})))
        }
        CookiesCommand::Clear(_) => {
            let _ = rpc_webpage(&session, "cookies.clear", Value::Null)?;
            print_json(simple_ok(json!({"cleared": true})))
        }
    }
}

fn run_tab(command: TabCommand) -> OpenPageResult<()> {
    let session = match &command {
        TabCommand::New(args) => args.session.clone(),
        TabCommand::Close(args) => args.session.clone(),
        TabCommand::List(args) => args.session.clone(),
        TabCommand::Switch(args) => args.session.clone(),
    };
    match command {
        TabCommand::New(args) => print_json(simple_ok(rpc_webpage(
            &session,
            "tab.new",
            json!({
                "url": args.url,
                "window": args.window,
                "background": args.background,
            }),
        )?)),
        TabCommand::Close(args) => {
            if args.others {
                print_json(simple_ok(rpc_webpage(
                    &session,
                    "tab.close",
                    json!({"others": true}),
                )?))
            } else if let Some(target_id) = args.target {
                print_json(simple_ok(rpc_webpage(
                    &session,
                    "tab.close",
                    json!({"targets": [target_id]}),
                )?))
            } else if let Some(index) = args.index {
                let target_id = tab_target_id_from_index(&session, index)?;
                print_json(simple_ok(rpc_webpage(
                    &session,
                    "tab.close",
                    json!({"targets": [target_id]}),
                )?))
            } else {
                Err(OpenPageError::UnsupportedOperation(
                    "tab close requires --target, --index, or --others".to_string(),
                ))
            }
        }
        TabCommand::List(_) => {
            print_json(simple_ok(rpc_webpage(&session, "tab.list", Value::Null)?))
        }
        TabCommand::Switch(args) => {
            let target_id = if let Ok(index) = args.target.parse::<usize>() {
                tab_target_id_from_index(&session, index)?
            } else {
                args.target
            };
            print_json(simple_ok(rpc_webpage(
                &session,
                "tab.switch",
                json!({"target_id": target_id}),
            )?))
        }
    }
}

fn run_frame(command: FrameCommand) -> OpenPageResult<()> {
    let session = match &command {
        FrameCommand::List(args) => args.session.clone(),
        FrameCommand::Switch(args) => args.session.clone(),
    };
    match command {
        FrameCommand::List(_) => {
            print_json(simple_ok(rpc_webpage(&session, "frame.list", Value::Null)?))
        }
        FrameCommand::Switch(args) => print_json(simple_ok(rpc_webpage(
            &session,
            "frame.switch",
            json!({"target": args.target}),
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::Cli;

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
        Cli::try_parse_from(["openpage", "snapshot", "--session", "agent"]).unwrap();
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
    fn parses_window_state() {
        Cli::try_parse_from(["openpage", "window", "state", "--session", "agent"]).unwrap();
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
    fn parses_history_list() {
        Cli::try_parse_from(["openpage", "history", "list", "--session", "agent"]).unwrap();
    }

    #[test]
    fn parses_history_go() {
        Cli::try_parse_from(["openpage", "history", "go", "2", "--session", "agent"]).unwrap();
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
}
