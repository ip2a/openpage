use std::cell::RefCell;
use std::borrow::Cow;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;

use serde_json::{Value, json};

use crate::browser::{DownloadFileExistsMode, LaunchOptions, LoadMode};
use crate::cli::args::ServeArgs;
use crate::cli::connection::write_tcp_sidecars;
use crate::cli::protocol::{Request, Response};
use crate::download::DownloadMission;
use crate::error::{OpenPageError, OpenPageResult};
use crate::session::SessionOptions;
use crate::webpage::{WebElement, WebMode, WebPage};

pub fn run(args: ServeArgs) -> OpenPageResult<()> {
    run_tcp(args.port.unwrap_or(0), &args.session)
}

fn run_tcp(port: u16, session: &str) -> OpenPageResult<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    let _sidecars = write_tcp_sidecars(session, address.port())?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "listening": address.to_string(),
            "mode": "tcp",
            "session": session
        }))
        .unwrap()
    );

    let runtime = Rc::new(RefCell::new(ServeRuntime::default()));

    for stream in listener.incoming() {
        let mut stream = stream.map_err(|err| OpenPageError::Io(err.to_string()))?;
        let runtime_for_client = Rc::clone(&runtime);
        if let Err(err) = handle_client(&mut stream, runtime_for_client) {
            let _ = serde_json::to_writer(
                &stream,
                &Response::error(None, "tcp_error", err.to_string()),
            );
            let _ = stream.write_all(b"\n");
        }
        if runtime.borrow().shutdown {
            break;
        }
    }
    Ok(())
}

fn handle_client(
    stream: &mut TcpStream,
    runtime: Rc<RefCell<ServeRuntime>>,
) -> OpenPageResult<()> {
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = std::io::BufReader::new(&*stream)
            .read_line(&mut buf)
            .map_err(|err| OpenPageError::Io(err.to_string()))?;
        if n == 0 {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let id = request.id.clone();
                let mut runtime = runtime.borrow_mut();
                match runtime.dispatch(request) {
                    Ok(result) => Response::ok(id, result),
                    Err(err) => Response::error(id, "openpage", err.to_string()),
                }
            }
            Err(err) => Response::error(None, "invalid_json", err.to_string()),
        };
        serde_json::to_writer(&mut *stream, &response)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        if runtime.borrow().shutdown {
            break;
        }
    }
    Ok(())
}

#[derive(Default)]
struct ServeRuntime {
    webpages: HashMap<String, WebPage>,
    next_webpage_id: u64,
    shutdown: bool,
}

impl ServeRuntime {
    fn dispatch(&mut self, request: Request) -> OpenPageResult<Value> {
        match request.op.as_str() {
            "daemon.shutdown" => {
                self.shutdown = true;
                Ok(json!({"shutdown": true}))
            }
            "webpage.create" => self.create_webpage(request.target.as_deref(), &request.params),
            "webpage.quit" => {
                let target = required_target(&request)?;
                let page = self
                    .webpages
                    .remove(&target)
                    .ok_or_else(|| missing_target(&target))?;
                page.quit()?;
                Ok(json!({"target": target, "quit": true}))
            }
            _ => {
                let target = required_target(&request)?;
                let page = self
                    .webpages
                    .get(&target)
                    .ok_or_else(|| missing_target(&target))?;
                dispatch_webpage(page, &request.op, &request.params)
            }
        }
    }

    fn create_webpage(&mut self, target_hint: Option<&str>, params: &Value) -> OpenPageResult<Value> {
        let mode = optional_str(params, "mode")
            .map(WebMode::parse)
            .transpose()?
            .unwrap_or(WebMode::Driver);
        let target = target_hint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| optional_str(params, "session").map(ToOwned::to_owned))
            .unwrap_or_else(|| {
                self.next_webpage_id += 1;
                format!("wp_{}", self.next_webpage_id)
            });
        if let Some(existing) = self.webpages.get(&target) {
            return Ok(json!({
                "target": target,
                "mode": existing.mode()?.as_str(),
                "existing": true
            }));
        }
        let mut launch = LaunchOptions::default();
        launch.headless = optional_bool(params, "headless").unwrap_or(true);
        launch.browser_path = optional_string(params, "browser_path").map(Into::into);
        launch.download_path = optional_string(params, "download_path").map(Into::into);
        launch.user_data_dir = optional_string(params, "user_data_dir").map(Into::into);
        launch.width = optional_u64(params, "width").unwrap_or(1280) as u32;
        launch.height = optional_u64(params, "height").unwrap_or(900) as u32;
        launch.no_sandbox = optional_bool(params, "no_sandbox").unwrap_or(false);
        if let Some(load_mode) = optional_str(params, "load_mode") {
            launch.load_mode = LoadMode::parse(load_mode)?;
        }
        if let Some(mode) = optional_str(params, "download_file_exists") {
            launch.download_file_exists = DownloadFileExistsMode::parse(mode)?;
        }

        let session = SessionOptions {
            timeout_secs: optional_u64(params, "timeout_secs").unwrap_or(10),
            user_agent: optional_string(params, "user_agent"),
            ..SessionOptions::default()
        };

        let page = WebPage::new(mode, launch, session)?;
        self.webpages.insert(target.clone(), page);
        Ok(json!({"target": target, "mode": mode.as_str()}))
    }
}

fn dispatch_webpage(page: &WebPage, op: &str, params: &Value) -> OpenPageResult<Value> {
    match op {
        "webpage.back" => Ok(json!({"back": page.back(1)?})),
        "webpage.forward" => Ok(json!({"forward": page.forward(1)?})),
        "webpage.reload" => {
            page.refresh(false)?;
            page.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?;
            Ok(json!({"reloaded": true}))
        }
        "webpage.stop_loading" => {
            page.stop_loading()?;
            Ok(json!({"stopped_loading": true}))
        }
        "webpage.get" => Ok(json!({"loaded": page.get(required_str(params, "url")?)?})),
        "webpage.post" => Ok(json!({"loaded": page.post(required_str(params, "url")?)?})),
        "webpage.post_json" => Ok(json!({
            "loaded": page.post_json(required_str(params, "url")?, params.get("payload").cloned())?
        })),
        "webpage.change_mode" => {
            let mode = optional_str(params, "mode")
                .map(WebMode::parse)
                .transpose()?;
            let go = optional_bool(params, "go").unwrap_or(true);
            let copy_cookies = optional_bool(params, "copy_cookies").unwrap_or(true);
            page.change_mode(mode, go, copy_cookies)?;
            Ok(json!({"mode": page.mode()?.as_str()}))
        }
        "webpage.mode" => Ok(json!({"mode": page.mode()?.as_str()})),
        "webpage.url" => Ok(json!({"url": page.url()?})),
        "webpage.title" => Ok(json!({"title": page.title()?})),
        "webpage.html" => Ok(json!({"html": page.html()?})),
        "webpage.snapshot" => Ok(json!({"snapshot": page.run_js(agent_snapshot_script())?})),
        "webpage.json" => Ok(json!({"json": page.json()?})),
        "webpage.cookies" => Ok(json!({"cookies": page.cookies()?})),
        "webpage.set_cookie" | "cookies.set" => {
            page.set_cookie(
                required_str(params, "name")?,
                required_str(params, "value")?,
                optional_str(params, "url"),
                optional_str(params, "domain"),
                optional_str(params, "path"),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.remove_cookie" | "cookies.delete" => {
            page.remove_cookie(
                required_str(params, "name")?,
                optional_str(params, "url"),
                optional_str(params, "domain"),
                optional_str(params, "path"),
            )?;
            Ok(json!({"deleted": true}))
        }
        "webpage.clear_cookies" | "cookies.clear" => {
            page.clear_cookies()?;
            Ok(json!({"cleared": true}))
        }
        "webpage.user_agent" => Ok(json!({"user_agent": page.user_agent()?})),
        "webpage.status_code" => Ok(json!({"status_code": page.status_code()?})),
        "webpage.ready_state" => Ok(json!({"ready_state": page.ready_state()?})),
        "webpage.is_loading" => Ok(json!({"is_loading": page.is_loading()?})),
        "webpage.is_alive" => Ok(json!({"is_alive": page.is_alive()?})),
        "webpage.is_headless" => Ok(json!({"is_headless": page.is_headless()})),
        "webpage.is_existed" => Ok(json!({"is_existed": page.is_existed()?})),
        "webpage.is_incognito" => Ok(json!({"is_incognito": page.is_incognito()?})),
        "webpage.tabs" => Ok(json!({"count": page.tabs_count()?, "ids": page.tab_ids()?})),
        "webpage.download_path" => Ok(json!({"download_path": page.download_path()?})),
        "webpage.set_download_path" | "set.download_path" => {
            page.set_download_path(required_str(params, "path")?)?;
            Ok(json!({"set": true}))
        }
        "webpage.current_tab_download_path" => Ok(json!({
            "download_path": page.current_tab_download_path()?
        })),
        "webpage.set_current_tab_download_path" | "set.current_tab_download_path" => {
            page.set_current_tab_download_path(required_str(params, "path")?)?;
            Ok(json!({"set": true}))
        }
        "webpage.download_file_exists_mode" => Ok(json!({
            "mode": page.download_file_exists_mode()?
        })),
        "webpage.set_download_file_exists_mode" | "set.download_file_exists_mode" => {
            page.set_download_file_exists_mode(DownloadFileExistsMode::parse(required_str(
                params, "mode",
            )?)?)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_current_tab_download_file_exists_mode"
        | "set.current_tab_download_file_exists_mode" => {
            page.set_current_tab_download_file_exists_mode(DownloadFileExistsMode::parse(
                required_str(params, "mode")?,
            )?)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_current_tab_download_filename" | "set.current_tab_download_filename" => {
            page.set_current_tab_download_filename(
                optional_str(params, "rename"),
                optional_str(params, "suffix"),
                params.get("suffix").is_some(),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.load_mode" => Ok(json!({"load_mode": page.load_mode()?})),
        "webpage.set_load_mode" | "set.load_mode" => {
            page.set_load_mode(LoadMode::parse(required_str(params, "mode")?)?)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_blocked_urls" | "set.blocked_urls" => {
            page.set_blocked_urls(&required_string_array(params, "patterns")?)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_upload_files" | "set.upload_files" => {
            page.set_upload_files(&required_string_array(params, "files")?)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_headers" | "set.headers" => {
            let headers = required_headers(params, "headers")?;
            page.set_headers(&headers)?;
            Ok(json!({"set": true}))
        }
        "webpage.set_user_agent" | "set.user_agent" => {
            page.set_user_agent(
                required_str(params, "user_agent")?,
                optional_str(params, "platform"),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.local_storage" => Ok(json!({
            "value": page.local_storage(optional_str(params, "item"))?
        })),
        "webpage.session_storage" => Ok(json!({
            "value": page.session_storage(optional_str(params, "item"))?
        })),
        "webpage.set_local_storage" | "set.local_storage" => {
            page.set_local_storage(required_str(params, "item")?, optional_str(params, "value"))?;
            Ok(json!({"set": true}))
        }
        "webpage.set_session_storage" | "set.session_storage" => {
            page.set_session_storage(required_str(params, "item")?, optional_str(params, "value"))?;
            Ok(json!({"set": true}))
        }
        "webpage.activate" => {
            page.activate()?;
            Ok(json!({"activated": true}))
        }
        "webpage.cookies_to_session" => {
            page.cookies_to_session(optional_bool(params, "copy_user_agent").unwrap_or(true))?;
            Ok(json!({"copied": true}))
        }
        "webpage.cookies_to_browser" => {
            page.cookies_to_browser()?;
            Ok(json!({"copied": true}))
        }
        "webpage.window_state" | "window.state" => Ok(json!({"state": page.window_state()?})),
        "webpage.window_size" | "window.size" => {
            let (width, height) = page.window_size()?;
            Ok(json!({"width": width, "height": height}))
        }
        "webpage.window_location" | "window.location" => {
            let (left, top) = page.window_location()?;
            Ok(json!({"left": left, "top": top}))
        }
        "webpage.window_max" | "window.max" => {
            page.window_max()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_min" | "window.min" | "window.mini" => {
            page.window_min()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_full" | "window.full" => {
            page.window_full()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_normal" | "window.normal" => {
            page.window_normal()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_hide" | "window.hide" => {
            page.window_hide()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_show" | "window.show" => {
            page.window_show()?;
            Ok(json!({"set": true}))
        }
        "webpage.window_size_set" | "window.size_set" => {
            page.window_size_set(
                optional_i64(params, "width"),
                optional_i64(params, "height"),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.window_location_set" | "window.location_set" => {
            page.window_location_set(optional_i64(params, "left"), optional_i64(params, "top"))?;
            Ok(json!({"set": true}))
        }
        "webpage.scroll" | "page.scroll" => {
            match required_str(params, "direction")? {
                "down" => page.scroll_down(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "up" => page.scroll_up(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "left" => page.scroll_left(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "right" => page.scroll_right(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "top" => page.scroll_to_top()?,
                "bottom" => page.scroll_to_bottom()?,
                "half" => page.scroll_to_half()?,
                "rightmost" => page.scroll_to_rightmost()?,
                "leftmost" => page.scroll_to_leftmost()?,
                "location" => page.scroll_to_location(
                    optional_f64(params, "x").unwrap_or(0.0),
                    optional_f64(params, "y").unwrap_or(0.0),
                )?,
                other => {
                    return Err(OpenPageError::UnsupportedLocator(format!(
                        "unknown scroll direction: {other}"
                    )));
                }
            }
            Ok(json!({"scrolled": true}))
        }
        "webpage.run_js" | "page.run_js" => {
            Ok(json!({"value": page.run_js(required_str(params, "script")?)?}))
        }
        "webpage.download_url" | "page.download_url" => {
            let path = if let Some(output) = optional_str(params, "path") {
                page.download_to(required_str(params, "url")?, output)?
            } else {
                page.download(required_str(params, "url")?)?
            };
            Ok(json!({"downloaded": true, "path": path}))
        }
        "page.key_down" => {
            let mut actions = page.actions()?;
            actions.key_down(required_str(params, "key")?)?;
            Ok(json!({"dispatched": true}))
        }
        "page.key_up" => {
            let mut actions = page.actions()?;
            actions.key_up(required_str(params, "key")?)?;
            Ok(json!({"dispatched": true}))
        }
        "page.type_keys" => {
            let mut actions = page.actions()?;
            actions.type_keys(required_str(params, "text")?)?;
            Ok(json!({"typed": true}))
        }
        "page.input" => {
            let mut actions = page.actions()?;
            actions.input(required_str(params, "text")?)?;
            Ok(json!({"input": true}))
        }
        "page.type" => {
            let mut actions = page.actions()?;
            actions.r#type(required_str(params, "text")?)?;
            Ok(json!({"typed": true}))
        }
        "page.type_with_interval" => {
            let mut actions = page.actions()?;
            actions.type_with_interval(
                required_str(params, "text")?,
                optional_f64(params, "interval").unwrap_or(0.1),
            )?;
            Ok(json!({"typed": true}))
        }
        "webpage.pdf" | "page.pdf" => {
            page.save_pdf(required_str(params, "path")?)?;
            Ok(json!({"saved": true}))
        }
        "webpage.screenshot" | "page.screenshot" => {
            page.save_screenshot(
                required_str(params, "path")?,
                optional_bool(params, "full_page").unwrap_or(false),
            )?;
            Ok(json!({"saved": true}))
        }
        "webpage.active_element" => Ok(json!({
            "element": page.active_element()?.map(web_element_to_json).transpose()?
        })),
        "webpage.find" => Ok(web_element_to_json(page.find(&required_locator_string(params)?)?)?),
        "webpage.find_all" => Ok(json!({
            "elements": page.find_all(&required_locator_string(params)?)?
                .into_iter()
                .map(web_element_to_json)
                .collect::<OpenPageResult<Vec<_>>>()?
        })),
        "webpage.count" => Ok(json!({
            "count": page.find_all(&required_locator_string(params)?)?.len()
        })),
        "webpage.ele.is_visible" | "element.is_visible" => Ok(json!({
            "visible": page.find(&required_locator_string(params)?)?.is_displayed()?
        })),
        "webpage.ele.is_enabled" | "element.is_enabled" => Ok(json!({
            "enabled": page.find(&required_locator_string(params)?)?.is_enabled()?
        })),
        "webpage.ele.is_checked" | "element.is_checked" => Ok(json!({
            "checked": page.find(&required_locator_string(params)?)?.is_checked()?
        })),
        "webpage.ele.is_selected" | "element.is_selected" => Ok(json!({
            "selected": page.find(&required_locator_string(params)?)?.is_selected()?
        })),
        "webpage.ele.is_alive" | "element.is_alive" => Ok(json!({
            "alive": page.find(&required_locator_string(params)?)?.is_alive()?
        })),
        "webpage.ele.is_in_viewport" | "element.is_in_viewport" => Ok(json!({
            "in_viewport": page.find(&required_locator_string(params)?)?.is_in_viewport()?
        })),
        "webpage.ele.is_whole_in_viewport" | "element.is_whole_in_viewport" => Ok(json!({
            "whole_in_viewport": page.find(&required_locator_string(params)?)?.is_whole_in_viewport()?
        })),
        "webpage.ele.is_covered" | "element.is_covered" => Ok(json!({
            "covered": page.find(&required_locator_string(params)?)?.is_covered()?
        })),
        "webpage.ele.is_clickable" | "element.is_clickable" => Ok(json!({
            "clickable": page.find(&required_locator_string(params)?)?.is_clickable()?
        })),
        "webpage.ele.focus" | "element.focus" => {
            page.find(&required_locator_string(params)?)?.focus()?;
            Ok(json!({"focused": true}))
        }
        "webpage.ele.text" | "element.text" => {
            Ok(json!({"text": page.find(&required_locator_string(params)?)?.text()?}))
        }
        "webpage.ele.html" | "element.html" => {
            Ok(json!({"html": page.find(&required_locator_string(params)?)?.html()?}))
        }
        "webpage.ele.attr" | "element.attr" => Ok(json!({
            "value": page.find(&required_locator_string(params)?)?.attr(required_str(params, "name")?)?
        })),
        "webpage.ele.click" | "element.click" => {
            page.find(&required_locator_string(params)?)?.click()?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.click_right" | "element.click_right" => {
            page.find(&required_locator_string(params)?)?.click_right()?;
            Ok(json!({"clicked": true, "button": "right"}))
        }
        "webpage.ele.click_middle" | "element.click_middle" => {
            page.find(&required_locator_string(params)?)?.click_middle()?;
            Ok(json!({"clicked": true, "button": "middle"}))
        }
        "webpage.ele.click_multi" | "element.click_multi" => {
            page.find(&required_locator_string(params)?)?
                .click_multi(optional_u64(params, "count").unwrap_or(2) as u32)?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.input" | "element.input" => {
            page.find(&required_locator_string(params)?)?
                .input(required_str(params, "text")?)?;
            Ok(json!({"input": true}))
        }
        "webpage.ele.clear" | "element.clear" => {
            page.find(&required_locator_string(params)?)?.clear()?;
            Ok(json!({"cleared": true}))
        }
        "webpage.ele.submit" | "element.submit" => {
            page.find(&required_locator_string(params)?)?.submit()?;
            Ok(json!({"submitted": true}))
        }
        "webpage.ele.check" | "element.check" => {
            page.find(&required_locator_string(params)?)?.set_checked(true)?;
            Ok(json!({"checked": true}))
        }
        "webpage.ele.uncheck" | "element.uncheck" => {
            page.find(&required_locator_string(params)?)?.set_checked(false)?;
            Ok(json!({"checked": false}))
        }
        "webpage.ele.hover" | "element.hover" => {
            page.find(&required_locator_string(params)?)?.hover()?;
            Ok(json!({"hovered": true}))
        }
        "webpage.ele.press_key" | "element.press_key" => {
            page.find(&required_locator_string(params)?)?
                .press_key(required_str(params, "key")?)?;
            Ok(json!({"pressed": true}))
        }
        "webpage.ele.select" | "element.select" => {
            let element = page.find(&required_locator_string(params)?)?;
            let selected = if let Some(text) = optional_str(params, "text") {
                element.select_by_text(text)?
            } else if let Some(value) = optional_str(params, "value") {
                element.select_by_value(value)?
            } else if let Some(index) = optional_u64(params, "index") {
                element.select_by_index(index as usize)?
            } else {
                return Err(OpenPageError::BrowserOperation(
                    "select requires one of: text, value, index".to_string(),
                ));
            };
            Ok(json!({"selected": selected}))
        }
        "webpage.ele.upload" | "element.upload" => {
            let files = required_string_array(params, "files")?;
            page.find(&required_locator_string(params)?)?
                .set_file_input_files(&files)?;
            Ok(json!({"uploaded": true}))
        }
        "webpage.ele.scroll_into_view" | "element.scroll_into_view" => {
            let element = page.find(&required_locator_string(params)?)?;
            if optional_bool(params, "center").unwrap_or(false) {
                element.scroll_to_center()?;
            } else {
                element.scroll_to_see(None)?;
            }
            Ok(json!({"scrolled_into_view": true}))
        }
        "webpage.ele.drag" | "element.drag" => {
            page.find(&required_locator_string(params)?)?.drag(
                optional_f64(params, "dx").unwrap_or(0.0),
                optional_f64(params, "dy").unwrap_or(0.0),
                optional_f64(params, "duration").unwrap_or(0.5),
            )?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.drag_to" | "element.drag_to" => {
            let source = page.find(&required_locator_string(params)?)?;
            let target_locator = normalize_locator(required_str(params, "target")?);
            let target = page.find(target_locator.as_ref())?;
            source.drag_to_element(&target, optional_f64(params, "duration").unwrap_or(0.5))?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.drag_to_point" | "element.drag_to_point" => {
            page.find(&required_locator_string(params)?)?.drag_to_point(
                optional_f64(params, "x").unwrap_or(0.0),
                optional_f64(params, "y").unwrap_or(0.0),
                optional_f64(params, "duration").unwrap_or(0.5),
            )?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.run_js" | "element.run_js" => Ok(json!({
            "value": page.find(&required_locator_string(params)?)?.run_js(required_str(params, "script")?)?
        })),
        "webpage.ele.screenshot" | "element.screenshot" => {
            page.find(&required_locator_string(params)?)?
                .save_screenshot(required_str(params, "path")?)?;
            Ok(json!({"saved": true}))
        }
        "webpage.wait_for_download" | "wait.download" => Ok(json!({
            "path": page.wait_for_download(
                optional_str(params, "filename"),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "webpage.download_missions" => {
            Ok(json!({"missions": missions_to_json(page.download_missions()?)?}))
        }
        "webpage.last_download" => Ok(json!({
            "mission": page.last_download()?.map(mission_to_json).transpose()?
        })),
        "webpage.wait_for_new_tab" | "wait.new_tab" => Ok(json!({
            "target": page.wait_for_new_tab(
                optional_str(params, "current_tab_id"),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "webpage.wait_for_download_begin" | "wait.download_begin" => Ok(json!({
            "mission": page.wait_for_download_begin(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_bool(params, "cancel_it").unwrap_or(false),
            )?.map(mission_to_json).transpose()?
        })),
        "webpage.wait_for_downloads_done" | "wait.downloads_done" => Ok(json!({
            "done": page.wait_for_downloads_done(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_bool(params, "cancel_if_timeout").unwrap_or(false),
            )?
        })),
        "webpage.handle_alert" | "alert.handle" => Ok(json!({
            "text": page.handle_alert(
                optional_bool(params, "accept").unwrap_or(true),
                optional_str(params, "prompt_text"),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "intercept.start" => {
            page.interceptor().start(None, false, None, None)?;
            Ok(json!({"intercept": "started"}))
        }
        "intercept.stop" => {
            page.interceptor().stop()?;
            Ok(json!({"intercept": "stopped"}))
        }
        "intercept.status" => Ok(json!({
            "listening": page.interceptor().is_listening()?,
            "paused": page.interceptor().is_paused()?,
        })),
        "alert.text" => Ok(json!({
            "text": page.alert_text()?
        })),
        "webpage.set_next_alert_action" | "alert.set_next_action" => {
            page.set_next_alert_action(
                optional_bool(params, "accept").unwrap_or(true),
                optional_str(params, "prompt_text"),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.set_auto_alert_action" | "alert.set_auto_action" => {
            page.set_auto_alert_action(
                optional_bool(params, "accept"),
                optional_str(params, "prompt_text"),
            )?;
            Ok(json!({"set": true}))
        }
        "webpage.has_alert" | "alert.has" => Ok(json!({"has_alert": page.has_alert()?})),
        "webpage.wait_for_alert_closed" | "wait.alert_closed" => Ok(json!({
            "closed": page.wait_for_alert_closed(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.load_start" => Ok(json!({
            "started": page.wait_for_load_start(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.doc_loaded" => Ok(json!({
            "loaded": page.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.url_change" => Ok(json!({
            "changed": page.wait_for_url_change(
                required_str(params, "text")?,
                optional_bool(params, "exclude").unwrap_or(false),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.title_change" => Ok(json!({
            "changed": page.wait_for_title_change(
                required_str(params, "text")?,
                optional_bool(params, "exclude").unwrap_or(false),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.function" => Ok(json!({
            "result": wait_for_function_result(
                page,
                required_str(params, "script")?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_u64(params, "interval_ms").unwrap_or(200),
            )?
        })),
        "wait.text" => Ok(json!({
            "waited": wait_for_text_match(
                page,
                &required_locator_string(params)?,
                required_str(params, "text")?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_u64(params, "interval_ms").unwrap_or(200),
            )?
        })),
        "wait.eles_loaded" | "wait.elements_loaded" => Ok(json!({
            "loaded": page.wait_for_elements_loaded(
                &required_string_array(params, "locators")?,
                optional_bool(params, "any_one").unwrap_or(false),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_displayed" => Ok(json!({
            "ready": page.wait_for_ele_displayed(
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_hidden" => Ok(json!({
            "ready": page.wait_for_ele_hidden(
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_enabled" => Ok(json!({
            "ready": page.wait_for_ele_enabled(
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_disabled" => Ok(json!({
            "ready": page.find(&required_locator_string(params)?)?.wait_until_disabled(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_deleted" => Ok(json!({
            "ready": page.wait_for_ele_deleted(
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_clickable" => Ok(json!({
            "ready": page.wait_for_ele_clickable(
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        _ => Err(OpenPageError::UnsupportedOperation(format!(
            "unsupported op: {op}"
        ))),
    }
}

fn required_target(request: &Request) -> OpenPageResult<String> {
    request
        .target
        .clone()
        .ok_or_else(|| OpenPageError::BrowserOperation("missing target".to_string()))
}

fn missing_target(target: &str) -> OpenPageError {
    OpenPageError::BrowserOperation(format!("unknown target: {target}"))
}

fn required_locator(params: &Value) -> OpenPageResult<&str> {
    required_str(params, "locator")
}

fn required_locator_string(params: &Value) -> OpenPageResult<String> {
    Ok(normalize_locator(required_locator(params)?).into_owned())
}

fn required_str<'a>(params: &'a Value, key: &str) -> OpenPageResult<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing string param: {key}")))
}

fn optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn optional_string(params: &Value, key: &str) -> Option<String> {
    optional_str(params, key).map(ToString::to_string)
}

fn optional_bool(params: &Value, key: &str) -> Option<bool> {
    params.get(key).and_then(Value::as_bool)
}

fn optional_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}

fn optional_f64(params: &Value, key: &str) -> Option<f64> {
    params.get(key).and_then(Value::as_f64)
}

fn optional_i64(params: &Value, key: &str) -> Option<i64> {
    params.get(key).and_then(Value::as_i64)
}

fn normalize_locator(locator: &str) -> Cow<'_, str> {
    let trimmed = locator.trim();
    if let Some(reference) = parse_ref(trimmed) {
        return Cow::Owned(format!(r#"[data-op-ref="{}"]"#, reference));
    }
    Cow::Borrowed(locator)
}

fn parse_ref(input: &str) -> Option<&str> {
    if let Some(stripped) = input.strip_prefix('@') {
        return parse_plain_ref(stripped);
    }
    if let Some(stripped) = input.strip_prefix("ref=") {
        return parse_plain_ref(stripped);
    }
    parse_plain_ref(input)
}

fn parse_plain_ref(input: &str) -> Option<&str> {
    if input.len() > 1
        && input.starts_with('e')
        && input[1..].chars().all(|c| c.is_ascii_digit())
    {
        Some(input)
    } else {
        None
    }
}

fn agent_snapshot_script() -> &'static str {
    r#"
        (() => {
            const interactive = ['a','button','input','textarea','select','option'];
            const elements = Array.from(document.querySelectorAll('*'))
                .filter(el => interactive.includes(el.tagName.toLowerCase())
                    || el.onclick
                    || el.getAttribute('role') === 'button');
            const snapshot = [];
            elements.forEach((el, i) => {
                const ref = 'e' + (i + 1);
                el.setAttribute('data-op-ref', ref);
                const attrs = {};
                for (const attr of ['id','class','name','type','placeholder','href','value']) {
                    if (el.hasAttribute(attr)) attrs[attr] = el.getAttribute(attr);
                }
                snapshot.push({
                    ref: ref,
                    tag: el.tagName.toLowerCase(),
                    text: (el.innerText || '').trim().substring(0, 80),
                    attrs: attrs
                });
            });
            return snapshot;
        })()
    "#
}

fn required_string_array(params: &Value, key: &str) -> OpenPageResult<Vec<String>> {
    let values = params
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing array param: {key}")))?;
    values
        .iter()
        .map(|value| {
            value.as_str().map(ToString::to_string).ok_or_else(|| {
                OpenPageError::BrowserOperation(format!(
                    "array param must contain only strings: {key}"
                ))
            })
        })
        .collect()
}

fn required_headers(params: &Value, key: &str) -> OpenPageResult<Vec<(String, String)>> {
    let value = params
        .get(key)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing headers param: {key}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("{key} must be an object")))?;
    object
        .iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.clone(), value.to_string()))
                .ok_or_else(|| {
                    OpenPageError::BrowserOperation(format!(
                        "header values must be strings: {name}"
                    ))
                })
        })
        .collect()
}

fn missions_to_json(missions: Vec<DownloadMission>) -> OpenPageResult<Vec<Value>> {
    missions.into_iter().map(mission_to_json).collect()
}

fn mission_to_json(mission: DownloadMission) -> OpenPageResult<Value> {
    Ok(json!({
        "guid": mission.guid(),
        "url": mission.url()?,
        "suggested_filename": mission.suggested_filename()?,
        "state": mission.state()?,
        "received_bytes": mission.received_bytes()?,
        "total_bytes": mission.total_bytes()?,
        "final_path": mission.final_path()?,
    }))
}

fn web_element_to_json(element: WebElement) -> OpenPageResult<Value> {
    Ok(json!({
        "tag": element.tag()?,
        "text": element.text()?,
        "attrs": element.attrs()?,
        "html": element.html()?,
    }))
}

fn wait_for_function_result(
    page: &WebPage,
    script: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> OpenPageResult<Value> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let expression = format!("({script})");
    loop {
        let value = page.run_js(&expression)?;
        if value.as_bool() == Some(true) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-function timed out after {timeout_ms}ms"
            )));
        }
        sleep(Duration::from_millis(interval_ms));
    }
}

fn wait_for_text_match(
    page: &WebPage,
    locator: &str,
    text: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> OpenPageResult<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let script = format!(
        "(document.querySelector('{}')?.innerText || '').includes('{}')",
        locator.replace('\'', "\\'"),
        text.replace('\'', "\\'")
    );
    loop {
        let value = page.run_js(&script)?;
        if value.as_bool() == Some(true) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-text timed out after {timeout_ms}ms: locator={locator}, text={text}"
            )));
        }
        sleep(Duration::from_millis(interval_ms));
    }
}
