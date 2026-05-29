use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::rc::Rc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::page::{
    GetNavigationHistoryParams, NavigateToHistoryEntryParams,
};
use serde_json::{Map, Value, json};

use crate::browser::{DownloadFileExistsMode, LaunchOptions, LoadMode};
use crate::cli::args::ServeArgs;
use crate::cli::connection::write_tcp_sidecars;
use crate::cli::protocol::{Request, Response};
use crate::download::DownloadMission;
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::ActionsDragData;
use crate::session::SessionOptions;
use crate::webpage::{WebElement, WebFrame, WebMode, WebPage};

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

fn handle_client(stream: &mut TcpStream, runtime: Rc<RefCell<ServeRuntime>>) -> OpenPageResult<()> {
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
                    Err(err) => crate::cli::protocol::response_openpage_error(id, &err),
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
    webpages: HashMap<String, ServeWebPage>,
    next_webpage_id: u64,
    shutdown: bool,
}

struct ServeWebPage {
    page: WebPage,
    active_frame_target: Option<String>,
}

impl ServeWebPage {
    fn new(page: WebPage) -> Self {
        Self {
            page,
            active_frame_target: None,
        }
    }

    fn current_frame(&self) -> OpenPageResult<Option<WebFrame>> {
        match self.active_frame_target.as_deref() {
            Some(target) if !target.is_empty() => {
                if let Ok(index) = target.parse::<usize>() {
                    self.page.get_frame_context_by_index(index).map(Some)
                } else {
                    self.page.get_frame_context(target).map(Some)
                }
            }
            _ => Ok(None),
        }
    }

    fn clear_frame(&mut self) {
        self.active_frame_target = None;
    }

    fn switch_frame(&mut self, target: Option<String>) {
        self.active_frame_target = target;
    }

    fn switch_target(&mut self, target_id: &str) -> OpenPageResult<()> {
        self.page.activate_tab(target_id)?;
        self.page = self.page.with_target(target_id)?;
        self.clear_frame();
        Ok(())
    }

    fn current_target_id(&self) -> String {
        self.page.target_id()
    }

    fn find(&self, locator: &str) -> OpenPageResult<WebElement> {
        match self.current_frame()? {
            Some(frame) => frame.find(locator),
            None => self.page.find(locator),
        }
    }

    fn find_all(&self, locator: &str) -> OpenPageResult<Vec<WebElement>> {
        match self.current_frame()? {
            Some(frame) => frame.find_all(locator),
            None => self.page.find_all(locator),
        }
    }

    fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        match self.current_frame()? {
            Some(frame) => frame.run_js(script),
            None => self.page.run_js(script),
        }
    }

    fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        match self.current_frame()? {
            Some(frame) => frame.active_element(),
            None => self.page.active_element(),
        }
    }

    fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.current_frame()? {
            Some(frame) => frame.wait_for_doc_loaded(timeout_ms),
            None => self.page.wait_for_doc_loaded(timeout_ms),
        }
    }
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
                let state = self
                    .webpages
                    .remove(&target)
                    .ok_or_else(|| missing_target(&target))?;
                state.page.quit()?;
                Ok(json!({"target": target, "quit": true}))
            }
            _ => {
                let target = required_target(&request)?;
                let page = self
                    .webpages
                    .get_mut(&target)
                    .ok_or_else(|| missing_target(&target))?;
                dispatch_webpage(page, &request.op, &request.params)
            }
        }
    }

    fn create_webpage(
        &mut self,
        target_hint: Option<&str>,
        params: &Value,
    ) -> OpenPageResult<Value> {
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
                "mode": existing.page.mode()?.as_str(),
                "existing": true
            }));
        }
        let mut launch = LaunchOptions::from_ini(None)?;
        launch.headless = optional_bool(params, "headless").unwrap_or(true);
        if let Some(path) = optional_string(params, "browser_path") {
            launch.browser_path = Some(path.into());
        }
        if let Some(path) = optional_string(params, "download_path") {
            launch.download_path = Some(path.into());
        }
        if let Some(path) = optional_string(params, "user_data_dir") {
            launch.user_data_dir = Some(path.into());
        }
        launch.width = optional_u64(params, "width").unwrap_or(1280) as u32;
        launch.height = optional_u64(params, "height").unwrap_or(900) as u32;
        if let Some(no_sandbox) = optional_bool(params, "no_sandbox") {
            launch.no_sandbox = no_sandbox;
        }
        if let Some(load_mode) = optional_str(params, "load_mode") {
            launch.load_mode = LoadMode::parse(load_mode)?;
        }
        if let Some(mode) = optional_str(params, "download_file_exists") {
            launch.download_file_exists = DownloadFileExistsMode::parse(mode)?;
        }

        let session = session_options_from_request(params, None)?;

        let page = WebPage::new(mode, launch, session)?;
        self.webpages
            .insert(target.clone(), ServeWebPage::new(page));
        Ok(json!({"target": target, "mode": mode.as_str()}))
    }
}

fn dispatch_webpage(state: &mut ServeWebPage, op: &str, params: &Value) -> OpenPageResult<Value> {
    let page = &state.page;
    match op {
        "webpage.back" => Ok(json!({"back": page.back(1)?})),
        "webpage.forward" => Ok(json!({"forward": page.forward(1)?})),
        "history.list" => {
            let history = page.execute_cdp(GetNavigationHistoryParams::default())?;
            let current_index = history.current_index as usize;
            let entries = history
                .entries
                .into_iter()
                .enumerate()
                .map(|(index, entry)| {
                    json!({
                        "index": index + 1,
                        "current": index == current_index,
                        "id": entry.id,
                        "url": entry.url,
                        "user_typed_url": entry.user_typed_url,
                        "title": entry.title,
                        "transition_type": entry.transition_type,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "current_index": current_index + 1,
                "entries": entries,
            }))
        }
        "history.go" => {
            let requested_index = optional_u64(params, "index").ok_or_else(|| {
                OpenPageError::BrowserOperation("missing numeric param: index".to_string())
            })? as usize;
            if requested_index == 0 {
                return Err(OpenPageError::BrowserOperation(
                    "history index must be >= 1".to_string(),
                ));
            }
            let history = page.execute_cdp(GetNavigationHistoryParams::default())?;
            let entry = history
                .entries
                .into_iter()
                .nth(requested_index - 1)
                .ok_or_else(|| {
                    OpenPageError::BrowserOperation(format!(
                        "history index out of range: {requested_index}"
                    ))
                })?;
            page.execute_cdp(NavigateToHistoryEntryParams::new(entry.id))?;
            Ok(json!({
                "navigated": true,
                "index": requested_index,
                "id": entry.id,
                "url": entry.url,
                "title": entry.title,
            }))
        }
        "webpage.reload" => {
            page.refresh(false)?;
            state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?;
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
        "webpage.html" => Ok(payload_with_origin_and_title(
            "html",
            json!(page.html()?),
            current_page_origin(state).as_deref(),
            current_page_title(state).as_deref(),
        )),
        "webpage.snapshot" => snapshot_payload(state),
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
        "webpage.run_js" | "page.run_js" => Ok(payload_with_origin(
            "value",
            json!(state.run_js(required_str(params, "script")?)?),
            current_page_origin(state).as_deref(),
        )),
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
            if let Some(values) = optional_string_array(params, "text") {
                actions.type_keys(values)?;
            } else {
                actions.type_keys(required_str(params, "text")?)?;
            }
            Ok(json!({"typed": true}))
        }
        "page.selected_text" => {
            let text = page
                .run_js(
                    "(() => {\
                        const active = document.activeElement;\
                        if (active && typeof active.value === 'string' && typeof active.selectionStart === 'number' && typeof active.selectionEnd === 'number') {\
                            return active.value.slice(active.selectionStart, active.selectionEnd);\
                        }\
                        return window.getSelection ? window.getSelection().toString() : '';\
                    })()",
                )?
                .as_str()
                .unwrap_or_default()
                .to_string();
            Ok(payload_with_origin(
                "text",
                Value::String(text),
                current_page_origin(state).as_deref(),
            ))
        }
        "page.find_in_page" => {
            let query = required_str(params, "text")?;
            if query.trim().is_empty() {
                return Err(OpenPageError::BrowserOperation(
                    "find-in-page text must not be empty".to_string(),
                ));
            }
            let query_json = serde_json::to_string(query)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            let script = format!(
                "(() => {{
                    const found = window.find({query}, {case_sensitive}, {backward}, true, false, false, false);
                    const selection = found && window.getSelection ? window.getSelection().toString() : '';
                    const source = document.body?.innerText || document.documentElement?.innerText || '';
                    const haystack = {case_sensitive} ? source : source.toLowerCase();
                    const needle = {case_sensitive} ? {query} : {query}.toLowerCase();
                    let count = 0;
                    if (needle.length > 0) {{
                        let index = 0;
                        while ((index = haystack.indexOf(needle, index)) !== -1) {{
                            count += 1;
                            index += needle.length;
                        }}
                    }}
                    return {{ found, selection, count }};
                }})()",
                query = query_json,
                case_sensitive = optional_bool(params, "case_sensitive").unwrap_or(false),
                backward = optional_bool(params, "backward").unwrap_or(false),
            );
            let result = page.run_js(&script)?;
            Ok(json!({
                "found": result.get("found").and_then(Value::as_bool).unwrap_or(false),
                "selection": result.get("selection").cloned().unwrap_or(Value::String(String::new())),
                "count": result.get("count").and_then(Value::as_i64).unwrap_or(0),
                "text": query,
            }))
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
            "element": state.active_element()?.map(web_element_to_json).transpose()?
        })),
        "tab.list" => Ok(json!({
            "tabs": page
                .tab_infos()?
                .into_iter()
                .enumerate()
                .map(|(index, tab)| {
                    let active = tab.target_id == state.current_target_id();
                    json!({
                        "index": index + 1,
                        "target_id": tab.target_id,
                        "url": tab.url,
                        "title": tab.title,
                        "type": tab.tab_type,
                        "attached": tab.attached,
                        "active": active,
                    })
                })
                .collect::<Vec<_>>()
        })),
        "tab.new" => {
            let background = optional_bool(params, "background").unwrap_or(false);
            let new_page = page.new_tab(
                optional_str(params, "url"),
                optional_bool(params, "window").unwrap_or(false),
                background,
            )?;
            let target_id = new_page.target_id();
            if !background {
                state.switch_target(&target_id)?;
            }
            Ok(json!({
                "created": true,
                "target_id": target_id,
                "url": new_page.url()?,
                "window": optional_bool(params, "window").unwrap_or(false),
                "background": background,
            }))
        }
        "tab.switch" => {
            let target_id = required_str(params, "target_id")?;
            state.switch_target(target_id)?;
            Ok(json!({"switched": true, "target_id": target_id}))
        }
        "tab.close" => {
            let others = optional_bool(params, "others").unwrap_or(false);
            let mut targets = optional_string_array(params, "targets").unwrap_or_default();
            if others && targets.is_empty() {
                targets.push(state.current_target_id());
            }
            if targets.is_empty() {
                return Err(OpenPageError::BrowserOperation(
                    "tab.close requires targets or others=true".to_string(),
                ));
            }

            let closed = page.close_tabs(&targets, others)?;
            let current_target = state.current_target_id();
            let remaining_tabs = page.tab_infos()?;
            let next_target = if remaining_tabs
                .iter()
                .any(|tab| tab.target_id == current_target)
            {
                None
            } else if let Some(next) = remaining_tabs.first() {
                Some(next.target_id.clone())
            } else {
                Some(page.new_tab(None, false, false)?.target_id())
            };

            state.clear_frame();
            if let Some(target_id) = next_target {
                if target_id != current_target {
                    state.switch_target(&target_id)?;
                }
            }

            Ok(json!({"closed": closed, "others": others}))
        }
        "frame.list" => Ok(json!({
            "frames": page
                .get_frame_contexts(None::<&str>)?
                .into_iter()
                .enumerate()
                .map(|(index, frame)| {
                    json!({
                        "index": index + 1,
                        "id": frame.id(),
                        "name": frame.name().ok().flatten(),
                        "url": frame.url().ok().flatten(),
                        "title": frame.title().ok().flatten(),
                        "parent_id": frame.parent_id().ok().flatten(),
                        "tag": frame.tag().unwrap_or_default(),
                        "attrs": frame.attrs().unwrap_or_default(),
                        "active": state.active_frame_target.as_deref() == Some(frame.id()),
                    })
                })
                .collect::<Vec<_>>()
        })),
        "frame.switch" => {
            let target = required_str(params, "target")?;
            if matches!(target, "main" | "root" | "page") {
                state.clear_frame();
                return Ok(json!({"switched": true, "frame": "main"}));
            }
            let frame = if let Ok(index) = target.parse::<usize>() {
                page.get_frame_context_by_index(index)?
            } else {
                page.get_frame_context(target)?
            };
            state.switch_frame(Some(target.to_string()));
            Ok(json!({
                "switched": true,
                "frame_id": frame.id(),
                "target": target,
            }))
        }
        "webpage.find" => Ok(web_element_to_json(
            state.find(&required_locator_string(params)?)?,
        )?),
        "webpage.find_all" => Ok(json!({
            "elements": state.find_all(&required_locator_string(params)?)?
                .into_iter()
                .map(web_element_to_json)
                .collect::<OpenPageResult<Vec<_>>>()?
        })),
        "webpage.count" => Ok(json!({
            "count": state.find_all(&required_locator_string(params)?)?.len()
        })),
        "webpage.ele.is_visible" | "element.is_visible" => Ok(json!({
            "visible": state.find(&required_locator_string(params)?)?.is_displayed()?
        })),
        "webpage.ele.is_enabled" | "element.is_enabled" => Ok(json!({
            "enabled": state.find(&required_locator_string(params)?)?.is_enabled()?
        })),
        "webpage.ele.is_checked" | "element.is_checked" => Ok(json!({
            "checked": state.find(&required_locator_string(params)?)?.is_checked()?
        })),
        "webpage.ele.is_selected" | "element.is_selected" => Ok(json!({
            "selected": state.find(&required_locator_string(params)?)?.is_selected()?
        })),
        "webpage.ele.is_alive" | "element.is_alive" => Ok(json!({
            "alive": state.find(&required_locator_string(params)?)?.is_alive()?
        })),
        "webpage.ele.is_in_viewport" | "element.is_in_viewport" => Ok(json!({
            "in_viewport": state.find(&required_locator_string(params)?)?.is_in_viewport()?
        })),
        "webpage.ele.is_whole_in_viewport" | "element.is_whole_in_viewport" => Ok(json!({
            "whole_in_viewport": state.find(&required_locator_string(params)?)?.is_whole_in_viewport()?
        })),
        "webpage.ele.is_covered" | "element.is_covered" => Ok(json!({
            "covered": state.find(&required_locator_string(params)?)?.is_covered()?
        })),
        "webpage.ele.is_clickable" | "element.is_clickable" => Ok(json!({
            "clickable": state.find(&required_locator_string(params)?)?.is_clickable()?
        })),
        "webpage.ele.focus" | "element.focus" => {
            state.find(&required_locator_string(params)?)?.focus()?;
            Ok(json!({"focused": true}))
        }
        "webpage.ele.text" | "element.text" => Ok(payload_with_origin(
            "text",
            json!(state.find(&required_locator_string(params)?)?.text()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.html" | "element.html" => Ok(payload_with_origin(
            "html",
            json!(state.find(&required_locator_string(params)?)?.html()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.attr" | "element.attr" => Ok(payload_with_origin(
            "value",
            json!(
                state
                    .find(&required_locator_string(params)?)?
                    .attr(required_str(params, "name")?)?
            ),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.click" | "element.click" => {
            state.find(&required_locator_string(params)?)?.click()?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.click_right" | "element.click_right" => {
            state
                .find(&required_locator_string(params)?)?
                .click_right()?;
            Ok(json!({"clicked": true, "button": "right"}))
        }
        "webpage.ele.click_middle" | "element.click_middle" => {
            state
                .find(&required_locator_string(params)?)?
                .click_middle()?;
            Ok(json!({"clicked": true, "button": "middle"}))
        }
        "webpage.ele.click_multi" | "element.click_multi" => {
            state
                .find(&required_locator_string(params)?)?
                .click_multi(optional_u64(params, "count").unwrap_or(2) as u32)?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.click_at" | "element.click_at" => {
            state.find(&required_locator_string(params)?)?.click_at(
                optional_f64(params, "x"),
                optional_f64(params, "y"),
                optional_str(params, "button").unwrap_or("left"),
                optional_u64(params, "count").unwrap_or(1) as u32,
            )?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.input" | "element.input" => {
            state
                .find(&required_locator_string(params)?)?
                .input(required_str(params, "text")?)?;
            Ok(json!({"input": true}))
        }
        "webpage.ele.select_range" | "element.select_range" => {
            let start = params.get("start").and_then(Value::as_u64).ok_or_else(|| {
                OpenPageError::BrowserOperation("missing numeric param: start".to_string())
            })?;
            let end = params.get("end").and_then(Value::as_u64).ok_or_else(|| {
                OpenPageError::BrowserOperation("missing numeric param: end".to_string())
            })?;
            if end < start {
                return Err(OpenPageError::BrowserOperation(
                    "select-range requires end >= start".to_string(),
                ));
            }
            let start = start as usize;
            let end = end as usize;
            let element = state.find(&required_locator_string(params)?)?;
            let script = format!(
                "(() => {{
                    if (typeof this.setSelectionRange !== 'function') {{
                        throw new Error('select-range only supports input and textarea elements');
                    }}
                    const value = typeof this.value === 'string' ? this.value : '';
                    const length = value.length;
                    const start = Math.min({start}, length);
                    const end = Math.min({end}, length);
                    this.focus();
                    this.setSelectionRange(start, end);
                    return {{
                        start: this.selectionStart ?? start,
                        end: this.selectionEnd ?? end,
                        text: value.slice(start, end),
                    }};
                }})()",
                start = start,
                end = end,
            );
            element.run_js(&script)?;
            let actual_start = element
                .property("selectionStart")?
                .and_then(|value| value.as_u64())
                .unwrap_or(start as u64) as usize;
            let actual_end = element
                .property("selectionEnd")?
                .and_then(|value| value.as_u64())
                .unwrap_or(end as u64) as usize;
            let value = element.value()?.unwrap_or_default();
            Ok(json!({
                "start": actual_start,
                "end": actual_end,
                "text": value.get(actual_start..actual_end).unwrap_or_default(),
            }))
        }
        "webpage.ele.select_text" | "element.select_text" => {
            let start = optional_u64(params, "start").map(|value| value as usize);
            let end = optional_u64(params, "end").map(|value| value as usize);
            if matches!((start, end), (Some(s), Some(e)) if e < s) {
                return Err(OpenPageError::BrowserOperation(
                    "select-text requires end >= start".to_string(),
                ));
            }
            let element = state.find(&required_locator_string(params)?)?;
            let tag = element.tag()?.to_ascii_lowercase();
            if matches!(tag.as_str(), "input" | "textarea") {
                let value = element.value()?.unwrap_or_default();
                let length = value.len();
                let actual_start = start.unwrap_or(0).min(length);
                let actual_end = end.unwrap_or(length).min(length);
                let script = format!(
                    "(() => {{
                        this.focus();
                        this.setSelectionRange({start}, {end});
                        return true;
                    }})()",
                    start = actual_start,
                    end = actual_end,
                );
                element.run_js(&script)?;
                return Ok(json!({
                    "start": actual_start,
                    "end": actual_end,
                    "text": value.get(actual_start..actual_end).unwrap_or_default(),
                }));
            }

            let start_raw = start.unwrap_or(0);
            let end_expr = end.map_or("Number.MAX_SAFE_INTEGER".to_string(), |value| {
                value.to_string()
            });
            let select_script = format!(
                "(() => {{
                    const root = this;
                    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {{
                        acceptNode(node) {{
                            if (!node.nodeValue) return NodeFilter.FILTER_REJECT;
                            const parent = node.parentElement;
                            if (!parent) return NodeFilter.FILTER_REJECT;
                            const tag = parent.tagName;
                            if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'NOSCRIPT') {{
                                return NodeFilter.FILTER_REJECT;
                            }}
                            return NodeFilter.FILTER_ACCEPT;
                        }}
                    }});

                    const nodes = [];
                    let total = 0;
                    while (true) {{
                        const node = walker.nextNode();
                        if (!node) break;
                        nodes.push(node);
                        total += node.nodeValue.length;
                    }}

                    if (nodes.length === 0) {{
                        const selection = window.getSelection();
                        if (selection) selection.removeAllRanges();
                        return 0;
                    }}

                    const start = Math.min({start}, total);
                    const end = Math.min({end}, total);
                    const locate = (offset) => {{
                        let remaining = offset;
                        for (const node of nodes) {{
                            const length = node.nodeValue.length;
                            if (remaining <= length) {{
                                return {{ node, offset: remaining }};
                            }}
                            remaining -= length;
                        }}
                        const last = nodes[nodes.length - 1];
                        return {{ node: last, offset: last.nodeValue.length }};
                    }};

                    const startPos = locate(start);
                    const endPos = locate(end);
                    const range = document.createRange();
                    range.setStart(startPos.node, startPos.offset);
                    range.setEnd(endPos.node, endPos.offset);

                    const selection = window.getSelection();
                    if (selection) {{
                        selection.removeAllRanges();
                        selection.addRange(range);
                    }}

                    root.scrollIntoView({{ block: 'center', inline: 'nearest' }});
                    return total;
                }})()",
                start = start_raw,
                end = end_expr,
            );
            let total = element.run_js(&select_script)?.as_u64().unwrap_or(0) as usize;
            let selected_text = page
                .run_js("window.getSelection ? window.getSelection().toString() : ''")?
                .as_str()
                .unwrap_or_default()
                .to_string();
            let actual_start = element
                .run_js(
                    "(() => {\
                        const selection = window.getSelection();\
                        if (!selection || selection.rangeCount === 0) return 0;\
                        const range = selection.getRangeAt(0);\
                        if (!this.contains(range.startContainer)) return 0;\
                        const prefix = range.cloneRange();\
                        prefix.selectNodeContents(this);\
                        prefix.setEnd(range.startContainer, range.startOffset);\
                        return prefix.toString().length;\
                    })()",
                )?
                .as_u64()
                .unwrap_or(start_raw as u64) as usize;
            let actual_end = element
                .run_js(
                    "(() => {\
                        const selection = window.getSelection();\
                        if (!selection || selection.rangeCount === 0) return 0;\
                        const range = selection.getRangeAt(0);\
                        if (!this.contains(range.endContainer)) return 0;\
                        const prefix = range.cloneRange();\
                        prefix.selectNodeContents(this);\
                        prefix.setEnd(range.endContainer, range.endOffset);\
                        return prefix.toString().length;\
                    })()",
                )?
                .as_u64()
                .unwrap_or(end.unwrap_or(total) as u64) as usize;
            Ok(json!({
                "start": actual_start,
                "end": actual_end,
                "text": selected_text,
            }))
        }
        "webpage.ele.clear" | "element.clear" => {
            state.find(&required_locator_string(params)?)?.clear()?;
            Ok(json!({"cleared": true}))
        }
        "webpage.ele.submit" | "element.submit" => {
            state.find(&required_locator_string(params)?)?.submit()?;
            Ok(json!({"submitted": true}))
        }
        "webpage.ele.check" | "element.check" => {
            state
                .find(&required_locator_string(params)?)?
                .set_checked(true)?;
            Ok(json!({"checked": true}))
        }
        "webpage.ele.uncheck" | "element.uncheck" => {
            state
                .find(&required_locator_string(params)?)?
                .set_checked(false)?;
            Ok(json!({"checked": false}))
        }
        "webpage.ele.hover" | "element.hover" => {
            state.find(&required_locator_string(params)?)?.hover()?;
            Ok(json!({"hovered": true}))
        }
        "webpage.ele.press_key" | "element.press_key" => {
            state
                .find(&required_locator_string(params)?)?
                .press_key(required_str(params, "key")?)?;
            Ok(json!({"pressed": true}))
        }
        "webpage.ele.select" | "element.select" => {
            let element = state.find(&required_locator_string(params)?)?;
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
            state
                .find(&required_locator_string(params)?)?
                .set_file_input_files(&files)?;
            Ok(json!({"uploaded": true}))
        }
        "webpage.ele.click_to_download" | "element.click_to_download" => {
            let mission = state
                .find(&required_locator_string(params)?)?
                .clicker()
                .to_download(
                    optional_str(params, "dir"),
                    optional_str(params, "rename"),
                    optional_str(params, "suffix"),
                    params.get("suffix").is_some(),
                    optional_u64(params, "timeout_ms"),
                    optional_bool(params, "js").unwrap_or(false),
                    optional_bool(params, "new_tab").unwrap_or(false),
                )?;
            Ok(json!({
                "download_started": mission.is_some(),
                "mission": mission.map(mission_to_json).transpose()?,
            }))
        }
        "webpage.ele.click_to_upload" | "element.click_to_upload" => {
            let uploaded = state
                .find(&required_locator_string(params)?)?
                .clicker()
                .to_upload(
                    &required_string_array(params, "files")?,
                    optional_u64(params, "timeout_ms"),
                    optional_bool(params, "js").unwrap_or(false),
                )?;
            Ok(json!({"uploaded": uploaded}))
        }
        "webpage.ele.click_for_new_tab" | "element.click_for_new_tab" => {
            let new_page = state
                .find(&required_locator_string(params)?)?
                .clicker()
                .for_new_tab(
                    optional_u64(params, "timeout_ms"),
                    optional_bool(params, "js").unwrap_or(false),
                )?;
            match new_page {
                Some(new_page) => {
                    let target_id = new_page.target_id();
                    state.switch_target(&target_id)?;
                    Ok(json!({
                        "created": true,
                        "switched": true,
                        "target_id": target_id,
                        "url": new_page.url()?,
                    }))
                }
                None => Ok(json!({"created": false})),
            }
        }
        "webpage.ele.scroll_into_view" | "element.scroll_into_view" => {
            let element = state.find(&required_locator_string(params)?)?;
            if optional_bool(params, "center").unwrap_or(false) {
                element.scroll_to_center()?;
            } else {
                element.scroll_to_see(None)?;
            }
            Ok(json!({"scrolled_into_view": true}))
        }
        "webpage.ele.drag" | "element.drag" => {
            state.find(&required_locator_string(params)?)?.drag(
                optional_f64(params, "dx").unwrap_or(0.0),
                optional_f64(params, "dy").unwrap_or(0.0),
                optional_f64(params, "duration").unwrap_or(0.5),
            )?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.drag_to" | "element.drag_to" => {
            let source = state.find(&required_locator_string(params)?)?;
            let target_locator = normalize_locator(required_str(params, "target")?);
            let target = state.find(target_locator.as_ref())?;
            source.drag_to_element(&target, optional_f64(params, "duration").unwrap_or(0.5))?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.drag_to_point" | "element.drag_to_point" => {
            state
                .find(&required_locator_string(params)?)?
                .drag_to_point(
                    optional_f64(params, "x").unwrap_or(0.0),
                    optional_f64(params, "y").unwrap_or(0.0),
                    optional_f64(params, "duration").unwrap_or(0.5),
                )?;
            Ok(json!({"dragged": true}))
        }
        "webpage.ele.run_js" | "element.run_js" => Ok(json!({
            "value": state.find(&required_locator_string(params)?)?.run_js(required_str(params, "script")?)?
        })),
        "webpage.ele.screenshot" | "element.screenshot" => {
            state
                .find(&required_locator_string(params)?)?
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
            "loaded": state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
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
                state,
                required_str(params, "script")?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_u64(params, "interval_ms").unwrap_or(200),
            )?
        })),
        "wait.text" => Ok(json!({
            "waited": wait_for_text_match(
                state,
                &required_locator_string(params)?,
                required_str(params, "text")?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_u64(params, "interval_ms").unwrap_or(200),
            )?
        })),
        "wait.locator" => Ok(json!({
            "waited": wait_for_locator(
                state,
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
                optional_u64(params, "interval_ms").unwrap_or(100),
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
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_displayed(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_hidden" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_hidden(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_enabled" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_enabled(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_disabled" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?.wait_until_disabled(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_deleted" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_deleted(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_clickable" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_clickable(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "webpage.drag_in" | "page.drag_in" => {
            let target = state.find(&required_str(params, "target")?)?;
            let drag_data = if let Some(text) = optional_str(params, "text") {
                ActionsDragData::text(text)
            } else if params.get("files").is_some() {
                ActionsDragData::files(required_string_array(params, "files")?)
            } else {
                return Err(OpenPageError::UnsupportedOperation(
                    "drag-in requires text or files".to_string(),
                ));
            };
            page.actions()?.drag_in(&target, drag_data)?;
            Ok(json!({"dragged": true}))
        }
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

fn session_options_from_request(
    params: &Value,
    ini_path: Option<&Path>,
) -> OpenPageResult<SessionOptions> {
    let mut session = SessionOptions::from_ini(ini_path)?;
    if let Some(timeout_secs) = optional_u64(params, "timeout_secs") {
        session.set_timeout(timeout_secs);
    }
    if params.get("user_agent").is_some() {
        session.set_user_agent(optional_string(params, "user_agent"));
    }
    Ok(session)
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
    if input.len() > 1 && input.starts_with('e') && input[1..].chars().all(|c| c.is_ascii_digit()) {
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

fn snapshot_payload(state: &ServeWebPage) -> OpenPageResult<Value> {
    let snapshot = state.run_js(agent_snapshot_script())?;

    let origin = current_page_origin(state);
    let title = current_page_title(state);

    let mut payload = payload_object(
        "snapshot",
        snapshot.clone(),
        origin.as_deref(),
        title.as_deref(),
    );

    if let Some(entries) = snapshot.as_array() {
        payload.insert(
            "text".to_string(),
            Value::String(format_snapshot_text(
                entries,
                title.as_deref(),
                origin.as_deref(),
            )),
        );
        payload.insert("refs".to_string(), Value::Object(snapshot_refs(entries)));
        payload.insert("interactive_count".to_string(), json!(entries.len()));
    }

    Ok(Value::Object(payload))
}

fn payload_with_origin(key: &str, value: Value, origin: Option<&str>) -> Value {
    Value::Object(payload_object(key, value, origin, None))
}

fn payload_with_origin_and_title(
    key: &str,
    value: Value,
    origin: Option<&str>,
    title: Option<&str>,
) -> Value {
    Value::Object(payload_object(key, value, origin, title))
}

fn payload_object(
    key: &str,
    value: Value,
    origin: Option<&str>,
    title: Option<&str>,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert(key.to_string(), value);
    if let Some(origin) = origin {
        payload.insert("origin".to_string(), Value::String(origin.to_string()));
    }
    if let Some(title) = title {
        payload.insert("title".to_string(), Value::String(title.to_string()));
    }
    payload
}

fn current_page_origin(state: &ServeWebPage) -> Option<String> {
    state
        .page
        .url()
        .ok()
        .flatten()
        .filter(|value| !value.is_empty())
}

fn current_page_title(state: &ServeWebPage) -> Option<String> {
    state
        .page
        .title()
        .ok()
        .flatten()
        .filter(|value| !value.is_empty())
}

fn format_snapshot_text(entries: &[Value], title: Option<&str>, origin: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(title) = title {
        lines.push(format!("Page: {title}"));
    }
    if let Some(origin) = origin {
        lines.push(format!("URL: {origin}"));
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }

    if entries.is_empty() {
        lines.push("No interactive elements found".to_string());
        return lines.join("\n");
    }

    for entry in entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let ref_id = obj.get("ref").and_then(Value::as_str).unwrap_or("?");
        let tag = obj.get("tag").and_then(Value::as_str).unwrap_or("unknown");
        let text = obj.get("text").and_then(Value::as_str).unwrap_or("");
        let attrs = obj.get("attrs").and_then(Value::as_object);

        let mut line = format!("@{ref_id} [{tag}]");
        if !text.is_empty() {
            line.push(' ');
            line.push('"');
            line.push_str(&escape_snapshot_value(text));
            line.push('"');
        }

        if let Some(attrs) = attrs {
            for key in [
                "placeholder",
                "href",
                "value",
                "type",
                "name",
                "id",
                "class",
            ] {
                if let Some(value) = attrs
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    line.push(' ');
                    line.push_str(key);
                    line.push_str("=\"");
                    line.push_str(&escape_snapshot_value(value));
                    line.push('"');
                }
            }
        }

        lines.push(line);
    }

    lines.join("\n")
}

fn snapshot_refs(entries: &[Value]) -> Map<String, Value> {
    let mut refs = Map::new();
    for entry in entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some(ref_id) = obj.get("ref").and_then(Value::as_str) else {
            continue;
        };

        let mut ref_obj = Map::new();
        if let Some(tag) = obj.get("tag").and_then(Value::as_str) {
            ref_obj.insert("tag".to_string(), Value::String(tag.to_string()));
        }
        if let Some(text) = obj.get("text").and_then(Value::as_str) {
            ref_obj.insert("text".to_string(), Value::String(text.to_string()));
        }
        if let Some(attrs) = obj.get("attrs").and_then(Value::as_object) {
            ref_obj.insert("attrs".to_string(), Value::Object(attrs.clone()));
        }
        refs.insert(ref_id.to_string(), Value::Object(ref_obj));
    }
    refs
}

fn escape_snapshot_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn format_snapshot_text_includes_title_origin_refs_and_attrs() {
        let entries = vec![
            json!({
                "ref": "e1",
                "tag": "button",
                "text": "Go",
                "attrs": {"id": "go"}
            }),
            json!({
                "ref": "e2",
                "tag": "input",
                "text": "",
                "attrs": {"placeholder": "Email", "type": "text"}
            }),
        ];

        let text = format_snapshot_text(&entries, Some("Example"), Some("https://example.com"));
        assert!(text.contains("Page: Example"));
        assert!(text.contains("URL: https://example.com"));
        assert!(text.contains("@e1 [button] \"Go\" id=\"go\""));
        assert!(text.contains("@e2 [input] placeholder=\"Email\" type=\"text\""));
    }

    #[test]
    fn snapshot_refs_builds_ref_index() {
        let entries = vec![json!({
            "ref": "e3",
            "tag": "a",
            "text": "More",
            "attrs": {"href": "https://example.com"}
        })];

        let refs = snapshot_refs(&entries);
        assert_eq!(refs["e3"]["tag"], "a");
        assert_eq!(refs["e3"]["text"], "More");
        assert_eq!(refs["e3"]["attrs"]["href"], "https://example.com");
    }

    #[test]
    fn payload_with_origin_includes_origin_and_value() {
        let payload = payload_with_origin(
            "text",
            Value::String("hello".to_string()),
            Some("about:blank"),
        );

        assert_eq!(payload["text"], "hello");
        assert_eq!(payload["origin"], "about:blank");
        assert!(payload.get("title").is_none());
    }

    #[test]
    fn payload_with_origin_omits_empty_fields_when_missing() {
        let payload = payload_with_origin("value", json!(true), None);

        assert_eq!(payload["value"], true);
        assert!(payload.get("origin").is_none());
        assert!(payload.get("title").is_none());
    }

    #[test]
    fn payload_with_origin_and_title_includes_both_fields() {
        let payload = payload_with_origin_and_title(
            "html",
            Value::String("<main/>".to_string()),
            Some("https://example.com/path"),
            Some("Example"),
        );

        assert_eq!(payload["html"], "<main/>");
        assert_eq!(payload["origin"], "https://example.com/path");
        assert_eq!(payload["title"], "Example");
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "openpage-serve-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn session_options_from_request_uses_ini_defaults_when_params_omit_fields() {
        let dir = make_temp_dir("session-ini-defaults");
        let ini_path = dir.join("session.ini");
        let mut expected = SessionOptions::default();
        expected
            .set_timeout(21)
            .set_user_agent(Some("OpenPage/ServeIni".to_string()))
            .set_download_path("downloads")
            .set_retry(Some(4), Some(250));
        expected
            .save(Some(ini_path.as_path()))
            .expect("write session options ini");

        let options = session_options_from_request(&json!({}), Some(ini_path.as_path()))
            .expect("load session options from ini");

        assert_eq!(options.timeout_secs, 21);
        assert_eq!(options.user_agent.as_deref(), Some("OpenPage/ServeIni"));
        assert_eq!(options.download_path, std::path::PathBuf::from("downloads"));
        assert_eq!(options.retry_times, 4);
        assert_eq!(options.retry_interval_millis, 250);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_from_request_overrides_explicit_params() {
        let dir = make_temp_dir("session-request-overrides");
        let ini_path = dir.join("session.ini");
        let mut expected = SessionOptions::default();
        expected
            .set_timeout(21)
            .set_user_agent(Some("OpenPage/ServeIni".to_string()));
        expected
            .save(Some(ini_path.as_path()))
            .expect("write session options ini");

        let options = session_options_from_request(
            &json!({
                "timeout_secs": 5,
                "user_agent": "OpenPage/Request"
            }),
            Some(ini_path.as_path()),
        )
        .expect("override session options from request");

        assert_eq!(options.timeout_secs, 5);
        assert_eq!(options.user_agent.as_deref(), Some("OpenPage/Request"));

        let _ = fs::remove_dir_all(&dir);
    }
}

fn optional_string_array(params: &Value, key: &str) -> Option<Vec<String>> {
    params.get(key).and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect()
    })
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
    state: &ServeWebPage,
    script: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> OpenPageResult<Value> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let expression = format!("({script})");
    loop {
        let value = state.run_js(&expression)?;
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
    state: &ServeWebPage,
    locator: &str,
    text: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> OpenPageResult<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Ok(element) = state.find(locator) {
            if element.text()?.is_some_and(|value| value.contains(text)) {
                return Ok(true);
            }
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-text timed out after {timeout_ms}ms: locator={locator}, text={text}"
            )));
        }
        sleep(Duration::from_millis(interval_ms));
    }
}

fn wait_for_locator(
    state: &ServeWebPage,
    locator: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> OpenPageResult<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if state.find(locator).is_ok() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-locator timed out after {timeout_ms}ms: {locator}"
            )));
        }
        sleep(Duration::from_millis(interval_ms));
    }
}
