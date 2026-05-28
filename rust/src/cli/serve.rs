use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::browser::{DownloadFileExistsMode, LaunchOptions, LoadMode};
use crate::cli::args::ServeArgs;
use crate::cli::protocol::{Request, Response};
use crate::download::DownloadMission;
use crate::error::{OpenPageError, OpenPageResult};
use crate::session::SessionOptions;
use crate::webpage::{WebMode, WebPage};

pub fn run(args: ServeArgs) -> OpenPageResult<()> {
    if !args.stdio {
        return Err(OpenPageError::UnsupportedOperation(
            "only serve --stdio is supported".to_string(),
        ));
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut runtime = ServeRuntime::default();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let id = request.id.clone();
                match runtime.dispatch(request) {
                    Ok(result) => Response::ok(id, result),
                    Err(err) => Response::error(id, "openpage", err.to_string()),
                }
            }
            Err(err) => Response::error(None, "invalid_json", err.to_string()),
        };
        serde_json::to_writer(&mut stdout, &response)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;

        if runtime.shutdown {
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
            "webpage.create" => self.create_webpage(&request.params),
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

    fn create_webpage(&mut self, params: &Value) -> OpenPageResult<Value> {
        let mode = optional_str(params, "mode")
            .map(WebMode::parse)
            .transpose()?
            .unwrap_or(WebMode::Driver);
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
        self.next_webpage_id += 1;
        let target = format!("wp_{}", self.next_webpage_id);
        self.webpages.insert(target.clone(), page);
        Ok(json!({"target": target, "mode": mode.as_str()}))
    }
}

fn dispatch_webpage(page: &WebPage, op: &str, params: &Value) -> OpenPageResult<Value> {
    match op {
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
        "webpage.json" => Ok(json!({"json": page.json()?})),
        "webpage.cookies" => Ok(json!({"cookies": page.cookies()?})),
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
        "webpage.run_js" | "page.run_js" => {
            Ok(json!({"value": page.run_js(required_str(params, "script")?)?}))
        }
        "webpage.screenshot" | "page.screenshot" => {
            page.save_screenshot(
                required_str(params, "path")?,
                optional_bool(params, "full_page").unwrap_or(false),
            )?;
            Ok(json!({"saved": true}))
        }
        "webpage.ele.text" | "element.text" => {
            Ok(json!({"text": page.find(required_locator(params)?)?.text()?}))
        }
        "webpage.ele.html" | "element.html" => {
            Ok(json!({"html": page.find(required_locator(params)?)?.html()?}))
        }
        "webpage.ele.attr" | "element.attr" => Ok(json!({
            "value": page.find(required_locator(params)?)?.attr(required_str(params, "name")?)?
        })),
        "webpage.ele.click" | "element.click" => {
            page.find(required_locator(params)?)?.click()?;
            Ok(json!({"clicked": true}))
        }
        "webpage.ele.input" | "element.input" => {
            page.find(required_locator(params)?)?
                .input(required_str(params, "text")?)?;
            Ok(json!({"input": true}))
        }
        "webpage.ele.clear" | "element.clear" => {
            page.find(required_locator(params)?)?.clear()?;
            Ok(json!({"cleared": true}))
        }
        "webpage.ele.run_js" | "element.run_js" => Ok(json!({
            "value": page.find(required_locator(params)?)?.run_js(required_str(params, "script")?)?
        })),
        "webpage.ele.screenshot" | "element.screenshot" => {
            page.find(required_locator(params)?)?
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
        "wait.eles_loaded" | "wait.elements_loaded" => Ok(json!({
            "loaded": page.wait_for_elements_loaded(
                &required_string_array(params, "locators")?,
                optional_bool(params, "any_one").unwrap_or(false),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_displayed" => Ok(json!({
            "ready": page.wait_for_ele_displayed(
                required_locator(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_hidden" => Ok(json!({
            "ready": page.wait_for_ele_hidden(
                required_locator(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_enabled" => Ok(json!({
            "ready": page.wait_for_ele_enabled(
                required_locator(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_deleted" => Ok(json!({
            "ready": page.wait_for_ele_deleted(
                required_locator(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_clickable" => Ok(json!({
            "ready": page.wait_for_ele_clickable(
                required_locator(params)?,
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

fn optional_i64(params: &Value, key: &str) -> Option<i64> {
    params.get(key).and_then(Value::as_i64)
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
