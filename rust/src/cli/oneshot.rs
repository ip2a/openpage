use std::ffi::OsStr;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::browser::Browser;
use crate::cli::args::{
    AlertCommand, AttrArgs, BrowserCommand, BrowserStartArgs, ClickForNewTabArgs,
    ClickToDownloadArgs, ClickToUploadArgs, Command, CookiesCommand, DownloadArgs, DragArgs,
    DragInArgs, DragToArgs, DragToPointArgs, ElementArgs, FillArgs, FrameCommand, GotoArgs,
    InterceptCommand, JsArgs, KeyArgs, PageTextArgs, PdfArgs, PressArgs, ScrollArgs,
    ScrollIntoViewArgs, SelectArgs, ScreenshotArgs, SessionArgs, ShortcutArgs, StorageCommand,
    StorageScope, TabCommand, TypeWithIntervalArgs, UploadArgs, WaitArgs, WaitElementArgs,
    WaitForFunctionArgs, WaitForTextArgs, WaitForTitleArgs, WaitForUrlArgs, WindowCommand,
    WindowMoveArgs,
};
use crate::cli::connection::{
    cleanup_sidecars, daemon_ready as tcp_daemon_ready, read_port, send_request,
};
use crate::cli::protocol::{simple_ok, Request, Response};
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::{ActionsDragData, Frame, Page};

pub fn run(command: Command) -> OpenPageResult<()> {
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
        Command::KeyDown(args) => run_key_down(args),
        Command::KeyUp(args) => run_key_up(args),
        Command::Shortcut(args) => run_shortcut(args),
        Command::Input(args) => run_input(args),
        Command::Type(args) => run_type(args),
        Command::TypeWithInterval(args) => run_type_with_interval(args),
        Command::Drag(args) => run_drag(args),
        Command::DragTo(args) => run_drag_to(args),
        Command::DragToPoint(args) => run_drag_to_point(args),
        Command::DragIn(args) => run_drag_in(args),
        Command::Text(args) => run_text(args),
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
        Command::FindAll(args) => run_find_all(args),
        Command::Count(args) => run_count(args),
        Command::WaitVisible(args) => run_wait_visible(args),
        Command::WaitHidden(args) => run_wait_hidden(args),
        Command::WaitEnabled(args) => run_wait_enabled(args),
        Command::WaitDisabled(args) => run_wait_disabled(args),
        Command::WaitDeleted(args) => run_wait_deleted(args),
        Command::WaitClickable(args) => run_wait_clickable(args),
        Command::ActiveElement(args) => run_active_element(args),
        Command::WaitForUrl(args) => run_wait_for_url(args),
        Command::WaitForTitle(args) => run_wait_for_title(args),
        Command::WaitForFunction(args) => run_wait_for_function(args),
        Command::WaitForText(args) => run_wait_for_text(args),
        Command::Pdf(args) => run_pdf(args),
        Command::Storage(command) => run_storage(command),
        Command::Cookies(command) => run_cookies(command),
        Command::Tab(command) => run_tab(command),
        Command::Frame(command) => run_frame(command),
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
    print_json(simple_ok(json!({"scrolled": true, "direction": args.direction})))
}

fn run_browser(command: BrowserCommand) -> OpenPageResult<()> {
    match command {
        BrowserCommand::Start(args) => start_browser(args),
        BrowserCommand::Stop(args) => stop_browser(args, false),
        BrowserCommand::Status(args) => {
            let status = if tcp_daemon_ready(&args.session) {
                json!({
                    "session": args.session,
                    "alive": true,
                    "port": read_port(&args.session)?,
                    "target": args.session,
                })
            } else {
                json!({"session": args.session, "alive": false})
            };
            print_json(simple_ok(status))
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
    print_json(simple_ok(json!({"loaded": true, "url": url.get("url").cloned()})))
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
    print_json(simple_ok(json!({"back": navigated, "url": url.get("url").cloned()})))
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
    print_json(simple_ok(json!({"forward": navigated, "url": url.get("url").cloned()})))
}

fn run_reload(args: SessionArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "webpage.reload",
        json!({"timeout_ms": 10_000}),
    )?;
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(json!({"reloaded": true, "url": url.get("url").cloned()})))
}

fn run_stop_loading(args: SessionArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(&args.session, "webpage.stop_loading", Value::Null)?;
    print_json(simple_ok(json!({"stopped_loading": true})))
}

fn run_url(args: SessionArgs) -> OpenPageResult<()> {
    print_json(simple_ok(rpc_webpage(&args.session, "webpage.url", Value::Null)?))
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

fn run_key_down(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.key_down",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(json!({"dispatched": true, "event": "keydown", "key": args.key})))
}

fn run_key_up(args: KeyArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.key_up",
        json!({"key": args.key.clone()}),
    )?;
    print_json(simple_ok(json!({"dispatched": true, "event": "keyup", "key": args.key})))
}

fn run_shortcut(args: ShortcutArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "page.type_keys",
        json!({"text": args.keys.clone()}),
    )?;
    print_json(simple_ok(json!({"pressed": true, "keys": args.keys})))
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
    let (_browser, page, record) = open_page(&args.session)?;
    let mut actions = page.actions()?;
    let target = context_find(&page, &record, &args.target)?;
    if let Some(text) = args.text {
        actions.drag_in(&target, ActionsDragData::text(text))?;
        print_json(simple_ok(json!({"dragged": true, "target": args.target, "kind": "text"})))
    } else if !args.files.is_empty() {
        actions.drag_in(&target, ActionsDragData::files(args.files.clone()))?;
        print_json(simple_ok(json!({"dragged": true, "target": args.target, "kind": "files"})))
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
    let (_browser, page, record) = open_page(&args.session)?;
    let condition = args.condition.trim();
    if condition.eq_ignore_ascii_case("navigation")
        || condition.eq_ignore_ascii_case("load")
        || condition.eq_ignore_ascii_case("loaded")
    {
        page.wait_for_doc_loaded(args.timeout)?;
    } else if let Some(locator) = condition.strip_prefix("element ") {
        let _ = context_wait_for(&page, &record, locator.trim(), args.timeout)?;
    } else {
        let _ = context_wait_for(&page, &record, condition, args.timeout)?;
    }
    print_json(simple_ok(json!({"waited": true, "condition": condition})))
}

fn run_intercept(command: InterceptCommand) -> OpenPageResult<()> {
    let session = match &command {
        InterceptCommand::Start(args) | InterceptCommand::Stop(args) | InterceptCommand::Status(args) => {
            args.session.clone()
        }
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
        WindowCommand::State(args) => {
            print_json(simple_ok(rpc_webpage(&args.session, "window.state", Value::Null)?))
        }
        WindowCommand::Location(args) => {
            print_json(simple_ok(rpc_webpage(&args.session, "window.location", Value::Null)?))
        }
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
            print_json(simple_ok(json!({"window": true, "width": args.width, "height": args.height})))
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
    print_json(simple_ok(json!({"moved": true, "left": args.left, "top": args.top})))
}

fn run_alert(command: AlertCommand) -> OpenPageResult<()> {
    match command {
        AlertCommand::Accept(args) => {
            let text = rpc_webpage(
                &args.session,
                "alert.handle",
                json!({"accept": true, "timeout_ms": 10_000}),
            )?
            .get("text")
            .cloned();
            print_json(simple_ok(json!({"accepted": true, "text": text})))
        }
        AlertCommand::Dismiss(args) => {
            let text = rpc_webpage(
                &args.session,
                "alert.handle",
                json!({"accept": false, "timeout_ms": 10_000}),
            )?
            .get("text")
            .cloned();
            print_json(simple_ok(json!({"dismissed": true, "text": text})))
        }
        AlertCommand::Text(args) => {
            let text = rpc_webpage(&args.session, "alert.text", Value::Null)?
                .get("text")
                .cloned();
            print_json(simple_ok(json!({"text": text})))
        }
    }
}

fn do_start_browser(args: &BrowserStartArgs) -> OpenPageResult<(SessionRecord, bool)> {
    if !args.replace {
        if let Ok(mut record) = load_session(&args.session) {
            if debugger_version(&record.debugger_url).is_ok() {
                record.last_used_at_ms = now_ms();
                let _ = save_session(&record);
                return Ok((record, false));
            }
        }
    }

    if args.replace {
        let stop_args = SessionArgs {
            session: args.session.clone(),
        };
        let _ = stop_browser(stop_args, true);
    }

    let port = match args.port {
        Some(port) => port,
        None => free_port()?,
    };
    let debugger_url = format!("http://127.0.0.1:{port}");
    let user_data_dir = args
        .user_data_dir
        .clone()
        .unwrap_or(session_profile_dir(&args.session)?);
    fs::create_dir_all(&user_data_dir)?;

    let browser_path = args
        .browser_path
        .clone()
        .or_else(default_browser_path)
        .ok_or_else(|| {
            OpenPageError::BrowserLaunch(
                "could not find a Chrome/Chromium executable; pass --browser-path".to_string(),
            )
        })?;

    let mut command = ProcessCommand::new(&browser_path);
    command
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg(format!("--window-size={},{}" , args.width, args.height))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-background-networking")
        .arg("--disable-popup-blocking")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let headless = args.headless && !args.head;
    if args.no_sandbox {
        command.arg("--no-sandbox").arg("--disable-setuid-sandbox");
    }
    if headless {
        command
            .arg("--headless=new")
            .arg("--hide-scrollbars")
            .arg("--mute-audio");
    }
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command.arg("about:blank");

    let child = command
        .spawn()
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;

    wait_for_debugger(&debugger_url, Duration::from_secs(15))?;
    let now = now_ms();
    let record = SessionRecord {
        name: args.session.clone(),
        debugger_url: debugger_url.clone(),
        user_data_dir: Some(user_data_dir),
        default_target_id: current_target_id(&debugger_url)?,
        default_frame_target: None,
        pid: Some(child.id()),
        created_at_ms: now,
        last_used_at_ms: now,
    };
    save_session(&record)?;
    let _ = std::thread::spawn({
        let session = args.session.clone();
        move || idle_watchdog(session)
    });
    Ok((record, true))
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
    let _ = fs::remove_file(session_file(&args.session)?);
    if quiet {
        Ok(())
    } else {
        print_json(simple_ok(json!({"stopped": true, "session": args.session})))
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

fn open_page(session: &str) -> OpenPageResult<(Browser, Page, SessionRecord)> {
    let mut record = match load_session(session) {
        Ok(r) => r,
        Err(_) => {
            let args = BrowserStartArgs {
                url: None,
                session: session.to_string(),
                browser_path: None,
                user_data_dir: None,
                port: None,
                head: false,
                headless: true,
                width: 1280,
                height: 900,
                no_sandbox: false,
                replace: false,
            };
            let (record, _) = do_start_browser(&args)?;
            record
        }
    };
    let browser = Browser::connect(&record.debugger_url)?;

    if let Some(target_id) = &record.default_target_id {
        if let Ok(page) = browser.get_page(target_id) {
            record.last_used_at_ms = now_ms();
            let _ = save_session(&record);
            return Ok((browser, page, record));
        }
    }

    if let Some(page) = browser.pages()?.into_iter().next() {
        record.default_target_id = Some(page.target_id());
        record.last_used_at_ms = now_ms();
        save_session(&record)?;
        return Ok((browser, page, record));
    }

    let page = browser.new_page(None)?;
    record.default_target_id = Some(page.target_id());
    record.last_used_at_ms = now_ms();
    save_session(&record)?;
    Ok((browser, page, record))
}

fn set_active_target(session: &str, target_id: &str) -> OpenPageResult<()> {
    let mut record = load_session(session)?;
    record.default_target_id = Some(target_id.to_string());
    record.default_frame_target = None;
    save_session(&record)
}

fn active_frame(page: &Page, record: &SessionRecord) -> OpenPageResult<Option<Frame>> {
    match record.default_frame_target.as_deref() {
        Some(target) => resolve_frame_target(page, target).map(Some),
        None => Ok(None),
    }
}

fn resolve_frame_target(page: &Page, target: &str) -> OpenPageResult<Frame> {
    if let Ok(index) = target.parse::<usize>() {
        page.get_frame_context_by_index(index)
    } else {
        page.get_frame_context(target)
    }
}

fn context_find(page: &Page, record: &SessionRecord, locator: &str) -> OpenPageResult<crate::element::Element> {
    match active_frame(page, record)? {
        Some(frame) => frame.find(locator),
        None => page.find(locator),
    }
}

fn context_wait_for(
    page: &Page,
    record: &SessionRecord,
    locator: &str,
    timeout_ms: u64,
) -> OpenPageResult<crate::element::Element> {
    match active_frame(page, record)? {
        Some(frame) => {
            if frame.wait_for_doc_loaded(timeout_ms)? {
                frame.find(locator)
            } else {
                Err(OpenPageError::Timeout(format!(
                    "frame wait_for timed out after {timeout_ms}ms: {locator}"
                )))
            }
        }
        None => page.wait_for(locator, timeout_ms),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionRecord {
    name: String,
    debugger_url: String,
    user_data_dir: Option<PathBuf>,
    default_target_id: Option<String>,
    #[serde(default)]
    default_frame_target: Option<String>,
    pid: Option<u32>,
    created_at_ms: u128,
    last_used_at_ms: u128,
}

fn save_session(record: &SessionRecord) -> OpenPageResult<()> {
    let path = session_file(&record.name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(record)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    fs::write(path, data)?;
    Ok(())
}

fn load_session(session: &str) -> OpenPageResult<SessionRecord> {
    let path = session_file(session)?;
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|err| OpenPageError::Serialization(err.to_string()))
}

fn session_file(session: &str) -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?
        .join("sessions")
        .join(format!("{session}.json")))
}

fn session_profile_dir(session: &str) -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?.join("profiles").join(session))
}

fn openpage_home() -> OpenPageResult<PathBuf> {
    if let Some(value) = std::env::var_os("OPENPAGE_HOME") {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(value).join(".openpage"));
    }
    Err(OpenPageError::Io(
        "OPENPAGE_HOME or HOME must be set".to_string(),
    ))
}

fn free_port() -> OpenPageResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn wait_for_debugger(url: &str, timeout: Duration) -> OpenPageResult<()> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/version", url.trim_end_matches('/'));
    let deadline = Instant::now() + timeout;
    loop {
        if client
            .get(&endpoint)
            .send()
            .is_ok_and(|resp| resp.status().is_success())
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "browser debugger did not become ready: {endpoint}"
            )));
        }
        sleep(Duration::from_millis(100));
    }
}

fn debugger_version(url: &str) -> OpenPageResult<Value> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/version", url.trim_end_matches('/'));
    client
        .get(&endpoint)
        .send()
        .map_err(|err| OpenPageError::Http(err.to_string()))?
        .json()
        .map_err(|err| OpenPageError::Serialization(err.to_string()))
}

#[derive(Debug, Deserialize)]
struct DebugTarget {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

fn current_target_id(url: &str) -> OpenPageResult<Option<String>> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/list", url.trim_end_matches('/'));
    let targets: Vec<DebugTarget> = client
        .get(&endpoint)
        .send()
        .map_err(|err| OpenPageError::Http(err.to_string()))?
        .json()
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    Ok(targets
        .into_iter()
        .find(|target| target.kind == "page")
        .map(|target| target.id))
}

fn default_browser_path() -> Option<PathBuf> {
    for var in ["OPENPAGE_BROWSER_PATH", "CHROME", "CHROME_PATH"] {
        if let Some(path) = std::env::var_os(var).map(PathBuf::from) {
            if path.exists() {
                return Some(path);
            }
        }
    }

    for path in platform_browser_paths() {
        if path.exists() {
            return Some(path);
        }
    }

    find_on_path("google-chrome")
        .or_else(|| find_on_path("chromium"))
        .or_else(|| find_on_path("chromium-browser"))
        .or_else(|| find_on_path("chrome"))
        .or_else(|| find_on_path("msedge"))
}

fn platform_browser_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
    ]
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let path = dir.join(name);
        if is_executable(&path) {
            return Some(path);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    path.is_file() || path.extension() == Some(OsStr::new("exe"))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn idle_watchdog(session: String) {
    let check_interval = Duration::from_secs(60);
    let idle_timeout = Duration::from_secs(300);
    loop {
        sleep(check_interval);
        match load_session(&session) {
            Ok(record) => {
                let idle = now_ms().saturating_sub(record.last_used_at_ms);
                if idle >= idle_timeout.as_millis() as u128 {
                    let _ = stop_browser(SessionArgs { session }, true);
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

fn print_json(value: Value) -> OpenPageResult<()> {
    println!(
        "{}",
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?
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

fn run_upload(args: UploadArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "element.upload",
        json!({"locator": args.locator, "files": args.files}),
    )?;
    print_json(simple_ok(json!({"uploaded": true, "files": args.files})))
}

fn run_click_to_download(args: ClickToDownloadArgs) -> OpenPageResult<()> {
    let (_browser, page, record) = open_page(&args.session)?;
    let element = context_find(&page, &record, &args.locator)?;
    let dir = args.dir.as_ref().map(|path| path.to_string_lossy().into_owned());
    let mission = element.clicker().to_download(
        dir.as_deref(),
        args.rename.as_deref(),
        args.suffix.as_deref(),
        args.suffix.is_some(),
        Some(args.timeout),
        args.js,
        args.new_tab,
    )?;
    let mission = match mission {
        Some(mission) => json!({
            "guid": mission.guid(),
            "url": mission.url()?,
            "suggested_filename": mission.suggested_filename()?,
            "state": mission.state()?,
            "received_bytes": mission.received_bytes()?,
            "total_bytes": mission.total_bytes()?,
            "final_path": mission.final_path()?,
        }),
        None => Value::Null,
    };
    print_json(simple_ok(json!({
        "download_started": !mission.is_null(),
        "mission": mission,
    })))
}

fn run_click_to_upload(args: ClickToUploadArgs) -> OpenPageResult<()> {
    let (_browser, page, record) = open_page(&args.session)?;
    let uploaded = context_find(&page, &record, &args.locator)?
        .clicker()
        .to_upload(&args.files, Some(args.timeout), args.js)?;
    print_json(simple_ok(json!({
        "uploaded": uploaded,
        "files": args.files,
    })))
}

fn run_click_for_new_tab(args: ClickForNewTabArgs) -> OpenPageResult<()> {
    let (_browser, page, record) = open_page(&args.session)?;
    let new_page = context_find(&page, &record, &args.locator)?
        .clicker()
        .for_new_tab(Some(args.timeout), args.js)?;
    match new_page {
        Some(page) => {
            let target_id = page.target_id();
            set_active_target(&args.session, &target_id)?;
            print_json(simple_ok(json!({
                "created": true,
                "switched": true,
                "target_id": target_id,
                "url": page.url()?,
            })))
        }
        None => print_json(simple_ok(json!({"created": false}))),
    }
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

fn run_wait_for_url(args: WaitForUrlArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "wait.url_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let url = rpc_webpage(&args.session, "webpage.url", Value::Null)?;
    print_json(simple_ok(json!({"waited": true, "url": url.get("url").cloned()})))
}

fn run_wait_for_title(args: WaitForTitleArgs) -> OpenPageResult<()> {
    let _ = rpc_webpage(
        &args.session,
        "wait.title_change",
        json!({"text": args.text, "exclude": args.exclude, "timeout_ms": args.timeout}),
    )?;
    let title = rpc_webpage(&args.session, "webpage.title", Value::Null)?;
    print_json(simple_ok(json!({"waited": true, "title": title.get("title").cloned()})))
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
    let _ = rpc_webpage(
        &args.session,
        "page.pdf",
        json!({"path": args.output}),
    )?;
    print_json(simple_ok(json!({"saved": true, "output": args.output})))
}

fn run_storage(command: StorageCommand) -> OpenPageResult<()> {
    let session = match &command {
        StorageCommand::Get(args) => args.session.clone(),
        StorageCommand::Set(args) => args.session.clone(),
    };
    match command {
        StorageCommand::Get(args) => {
            let value = if matches!(args.scope, StorageScope::Local) {
                rpc_webpage(
                    &session,
                    "webpage.local_storage",
                    json!({"item": args.key}),
                )?
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
    let (browser, _page, record) = open_page(&session)?;
    match command {
        TabCommand::New(args) => {
            let new_page = browser.new_tab(args.url.as_deref(), args.window, args.background)?;
            let target_id = new_page.target_id();
            if !args.background {
                set_active_target(&session, &target_id)?;
            }
            print_json(simple_ok(json!({
                "created": true,
                "target_id": target_id,
                "url": new_page.url()?,
                "window": args.window,
                "background": args.background,
            })))
        }
        TabCommand::Close(args) => {
            if args.others {
                let current = record.default_target_id.unwrap_or_default();
                let closed = browser.close_tabs(&[current], true)?;
                print_json(simple_ok(json!({"closed": closed, "others": true})))
            } else if let Some(target) = args.target {
                let closed = browser.close_tabs(&[target], false)?;
                print_json(simple_ok(json!({"closed": closed})))
            } else if let Some(index) = args.index {
                let pages = browser.pages()?;
                let target = pages
                    .into_iter()
                    .nth(index.saturating_sub(1))
                    .ok_or_else(|| {
                        OpenPageError::ElementNotFound(format!("tab index out of range: {index}"))
                    })?;
                let closed = browser.close_tabs(&[target.target_id()], false)?;
                print_json(simple_ok(json!({"closed": closed})))
            } else {
                Err(OpenPageError::UnsupportedOperation(
                    "tab close requires --target, --index, or --others".to_string(),
                ))
            }
        }
        TabCommand::List(_) => {
            let tabs: Vec<Value> = browser
                .tab_infos()?
                .into_iter()
                .map(|t| {
                    json!({
                        "target_id": t.target_id,
                        "url": t.url,
                        "title": t.title,
                        "type": t.tab_type,
                    })
                })
                .collect();
            print_json(simple_ok(json!({"tabs": tabs})))
        }
        TabCommand::Switch(args) => {
            let target_id = if let Ok(index) = args.target.parse::<usize>() {
                let pages = browser.pages()?;
                pages
                    .into_iter()
                    .nth(index.saturating_sub(1))
                    .ok_or_else(|| {
                        OpenPageError::ElementNotFound(format!(
                            "tab index out of range: {index}"
                        ))
                    })?
                    .target_id()
            } else {
                args.target
            };
            browser.activate_tab(&target_id)?;
            set_active_target(&session, &target_id)?;
            print_json(simple_ok(json!({"switched": true, "target_id": target_id})))
        }
    }
}

fn run_frame(command: FrameCommand) -> OpenPageResult<()> {
    let session = match &command {
        FrameCommand::List(args) => args.session.clone(),
        FrameCommand::Switch(args) => args.session.clone(),
    };
    let (_browser, page, _record) = open_page(&session)?;
    match command {
        FrameCommand::List(_) => {
            let frames = page.get_frames(None::<&str>).unwrap_or_default();
            let list: Vec<Value> = frames
                .into_iter()
                .map(|f| {
                    json!({
                        "tag": f.tag().unwrap_or_default(),
                        "attrs": f.attrs().unwrap_or_default(),
                    })
                })
                .collect();
            print_json(simple_ok(json!({"frames": list})))
        }
        FrameCommand::Switch(args) => {
            if matches!(args.target.as_str(), "main" | "root" | "page") {
                let mut record = load_session(&session)?;
                record.default_frame_target = None;
                save_session(&record)?;
                return print_json(simple_ok(json!({
                    "switched": true,
                    "frame": "main",
                })));
            }
            let frame = if let Ok(index) = args.target.parse::<usize>() {
                page.get_frame_context_by_index(index)?
            } else {
                page.get_frame_context(&args.target)?
            };
            let mut record = load_session(&session)?;
            record.default_frame_target = Some(args.target.clone());
            save_session(&record)?;
            print_json(simple_ok(json!({
                "switched": true,
                "frame_id": frame.id(),
            })))
        }
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
        Cli::try_parse_from([
            "openpage",
            "shortcut",
            "Meta",
            "a",
            "--session",
            "agent",
        ])
        .unwrap();
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
    fn parses_serve_port() {
        Cli::try_parse_from(["openpage", "serve", "--port", "9876"]).unwrap();
    }
}
