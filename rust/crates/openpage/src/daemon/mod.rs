use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::page::{
    GetNavigationHistoryParams, NavigateToHistoryEntryParams, ResetNavigationHistoryParams,
};
use serde_json::{Map, Value, json};

use crate::browser::{DownloadFileExistsMode, LoadMode};
use crate::config::{ConfigOverrides, load_resolved_config};
use crate::download::DownloadMission;
use crate::element::Element;
use crate::error::{ErrorDiagnostic, OpenPageError, OpenPageResult};
use crate::page::{ActionsDragData, PageNavigationSnapshot};
use crate::page::{Frame, Page};
use crate::protocol::{Request, Response};
use crate::recorder::{RecordedAction, RecordedFlow, RecordedTarget, RecordedValue, RecordedWait};
use crate::settings::wait_timeout_result;

use ref_registry::{RefRegistry, parse_ref};
use revision::RevisionRegistry;

pub mod client;
mod operations;
mod ref_registry;
mod revision;
mod snapshot;

pub fn run_tcp(port: u16, session: &str) -> OpenPageResult<()> {
    run_tcp_inner(port, session)
}

fn run_tcp_inner(port: u16, session: &str) -> OpenPageResult<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    let _sidecars = client::write_tcp_sidecars(session, address.port())?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "listening": address.to_string(),
            "mode": "tcp",
            "session": session
        }))
        .unwrap()
    );

    let runtime = Rc::new(RefCell::new(ServeState::default()));

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

fn handle_client(stream: &mut TcpStream, runtime: Rc<RefCell<ServeState>>) -> OpenPageResult<()> {
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);

    loop {
        buf.clear();
        let n = reader
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
                    Err(err) => crate::protocol::response_openpage_error(id, &err),
                }
            }
            Err(err) => Response::error(None, "invalid_json", err.to_string()),
        };
        serde_json::to_writer(reader.get_mut(), &response)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        reader.get_mut().write_all(b"\n")?;
        reader.get_mut().flush()?;

        if runtime.borrow().shutdown {
            break;
        }
    }
    Ok(())
}

#[derive(Default)]
struct ServeState {
    pages: HashMap<String, ServePage>,
    next_page_id: u64,
    shutdown: bool,
}

struct ServePage {
    page: Page,
    attached: bool,
    active_frame_target: Option<String>,
    refs: RefCell<RefRegistry>,
    revisions: RevisionRegistry,
    navigation_baseline: Option<NavigationBaseline>,
    navigation_tickets: HashMap<String, NavigationTicket>,
    next_navigation_ticket_id: u64,
}

#[derive(Clone, Debug)]
enum NavigationBaseline {
    Page {
        started_seq: u64,
        url: Option<String>,
    },
    Frame {
        url: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct NavigationTicket {
    baseline: NavigationBaseline,
    target_id: String,
    frame_target: Option<String>,
}

impl ServePage {
    fn new(page: Page, attached: bool) -> Self {
        Self {
            page,
            attached,
            active_frame_target: None,
            refs: RefCell::new(RefRegistry::default()),
            revisions: RevisionRegistry::default(),
            navigation_baseline: None,
            navigation_tickets: HashMap::new(),
            next_navigation_ticket_id: 1,
        }
    }

    fn current_frame(&self) -> OpenPageResult<Option<Frame>> {
        match self.active_frame_target.as_deref() {
            Some(target) if target.starts_with("frames:") => {
                let mut frames = target
                    .strip_prefix("frames:")
                    .unwrap_or_default()
                    .split('\u{1f}');
                let first = frames.next().filter(|value| !value.is_empty());
                let mut current = match first {
                    Some(locator) => self.page.get_frame_context(locator)?,
                    None => return Ok(None),
                };
                for locator in frames {
                    current = current.get_frame_context(locator)?;
                }
                Ok(Some(current))
            }
            Some(target) if !target.is_empty() => {
                if let Some(frame_id) = target.strip_prefix("id:") {
                    return self
                        .page
                        .get_frame_contexts(None::<&str>)?
                        .into_iter()
                        .find(|frame| frame.id() == frame_id)
                        .map(Some)
                        .ok_or_else(|| {
                            OpenPageError::ElementNotFound(format!(
                                "frame not found for id: {frame_id}"
                            ))
                        });
                }
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
        self.clear_navigation_baseline();
        self.clear_navigation_tickets();
    }

    fn switch_frame(&mut self, target: Option<String>) {
        self.active_frame_target = target;
        self.clear_navigation_baseline();
        self.clear_navigation_tickets();
    }

    fn switch_target(&mut self, target_id: &str) -> OpenPageResult<()> {
        self.page.activate_tab(target_id)?;
        self.page = self
            .page
            .browser()
            .ok_or_else(|| OpenPageError::BrowserOperation("page has no browser".to_string()))?
            .get_page(target_id)?;
        self.clear_frame();
        self.refs.borrow_mut().clear();
        Ok(())
    }

    fn current_target_id(&self) -> String {
        self.page.target_id()
    }

    fn current_revision(&self) -> String {
        self.revisions.current(&self.current_target_id())
    }

    fn bump_revision(&mut self) -> String {
        self.revisions.bump(&self.current_target_id())
    }

    fn validate_expected_revision(
        &self,
        operation: &str,
        locator: &str,
        expected_revision: Option<&str>,
    ) -> OpenPageResult<()> {
        validate_expected_revision(
            operation,
            locator,
            &self.current_revision(),
            expected_revision,
        )
    }

    fn find_revisioned(
        &self,
        operation: &str,
        locator: &str,
        expected_revision: Option<&str>,
    ) -> OpenPageResult<Element> {
        if parse_ref(locator).is_none() {
            return self.find(locator);
        }
        self.validate_expected_revision(operation, locator, expected_revision)?;
        self.find(locator).map_err(|error| {
            let message = match error.root() {
                OpenPageError::ElementNotFound(message) => message.clone(),
                _ => return error,
            };
            OpenPageError::ElementNotFound(message).diagnosed(ErrorDiagnostic {
                operation: Some(operation.to_string()),
                locator: Some(locator.to_string()),
                current_revision: Some(self.current_revision()),
                expected_revision: expected_revision.map(ToString::to_string),
                failure_reason: Some("stale_ref".to_string()),
                ..ErrorDiagnostic::default()
            })
        })
    }

    fn find(&self, locator: &str) -> OpenPageResult<Element> {
        if let Some(ref_id) = parse_ref(locator) {
            return self.find_ref(ref_id);
        }
        self.find_raw(locator)
    }

    fn find_raw(&self, locator: &str) -> OpenPageResult<Element> {
        match self.current_frame()? {
            Some(frame) => frame.find(locator),
            None => self.page.find(locator),
        }
    }

    fn find_all(&self, locator: &str) -> OpenPageResult<Vec<Element>> {
        if let Some(ref_id) = parse_ref(locator) {
            return Ok(vec![self.find_ref(ref_id)?]);
        }
        self.find_all_raw(locator)
    }

    fn find_all_raw(&self, locator: &str) -> OpenPageResult<Vec<Element>> {
        match self.current_frame()? {
            Some(frame) => frame.find_all(locator),
            None => self.page.find_all(locator),
        }
    }

    fn element_payload(&self, element: Element) -> OpenPageResult<Value> {
        let ref_id = self.register_element(&element)?;
        element_to_json(element, Some(ref_id))
    }

    fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        match self.current_frame()? {
            Some(frame) => frame.run_js(script),
            None => self.page.run_js(script),
        }
    }

    fn current_context_url(&self) -> Option<String> {
        self.run_js("window.location.href")
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .filter(|value| !value.is_empty())
    }

    fn record_navigation_baseline(&mut self) -> String {
        let baseline = self.capture_navigation_baseline();
        let token = self.next_navigation_ticket();
        self.navigation_baseline = Some(baseline.clone());
        self.navigation_tickets.insert(
            token.clone(),
            NavigationTicket {
                baseline,
                target_id: self.current_target_id(),
                frame_target: self.active_frame_target.clone(),
            },
        );
        self.prune_navigation_tickets();
        token
    }

    fn clear_navigation_baseline(&mut self) {
        self.navigation_baseline = None;
    }

    fn clear_navigation_tickets(&mut self) {
        self.navigation_tickets.clear();
    }

    fn capture_navigation_baseline(&self) -> NavigationBaseline {
        if self.current_frame().ok().flatten().is_some() {
            return NavigationBaseline::Frame {
                url: self.current_context_url(),
            };
        }

        match self.page.navigation_snapshot() {
            Ok(snapshot) => NavigationBaseline::Page {
                started_seq: snapshot.started_seq,
                url: snapshot.current_url.or_else(|| self.current_context_url()),
            },
            Err(_) => NavigationBaseline::Frame {
                url: self.current_context_url(),
            },
        }
    }

    fn discard_stale_navigation_baseline(&mut self) {
        let Some(baseline) = self.navigation_baseline.as_ref() else {
            return;
        };
        match baseline {
            NavigationBaseline::Page { started_seq, url } => {
                if let Ok(snapshot) = self.page.navigation_snapshot()
                    && page_navigation_transition_observed(*started_seq, &snapshot)
                    && page_navigation_settled(*started_seq, &snapshot)
                {
                    self.clear_navigation_baseline();
                    return;
                }
                let Ok(snapshot) = ready_snapshot(self) else {
                    return;
                };
                if snapshot.is_settled()
                    && navigation_transition_observed(url.as_deref(), &snapshot)
                {
                    self.clear_navigation_baseline();
                }
            }
            NavigationBaseline::Frame { url } => {
                let Ok(snapshot) = ready_snapshot(self) else {
                    return;
                };
                if snapshot.is_settled()
                    && navigation_transition_observed(url.as_deref(), &snapshot)
                {
                    self.clear_navigation_baseline();
                }
            }
        }
    }

    fn navigation_baseline_for_wait(
        &mut self,
        token: Option<&str>,
    ) -> OpenPageResult<NavigationBaseline> {
        match token {
            Some(token) => {
                let ticket = self.navigation_tickets.get(token).cloned().ok_or_else(|| {
                    OpenPageError::BrowserOperation(format!("unknown navigation token: {token}"))
                })?;
                if ticket.target_id != self.current_target_id()
                    || ticket.frame_target != self.active_frame_target
                {
                    return Err(OpenPageError::BrowserOperation(format!(
                        "navigation token {token} belongs to another page or frame"
                    )));
                }
                Ok(ticket.baseline)
            }
            None => Ok(self
                .navigation_baseline
                .take()
                .unwrap_or_else(|| self.capture_navigation_baseline())),
        }
    }

    fn consume_navigation_token(&mut self, token: Option<&str>) {
        let Some(token) = token else {
            return;
        };
        self.navigation_tickets.remove(token);
    }

    fn next_navigation_ticket(&mut self) -> String {
        let token = format!("nav-{}", self.next_navigation_ticket_id);
        self.next_navigation_ticket_id += 1;
        token
    }

    fn prune_navigation_tickets(&mut self) {
        const MAX_NAVIGATION_TICKETS: usize = 32;
        if self.navigation_tickets.len() <= MAX_NAVIGATION_TICKETS {
            return;
        }
        let floor = self
            .next_navigation_ticket_id
            .saturating_sub(MAX_NAVIGATION_TICKETS as u64);
        self.navigation_tickets.retain(|token, _| {
            token
                .strip_prefix("nav-")
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|value| value >= floor)
        });
    }
    fn active_element(&self) -> OpenPageResult<Option<Element>> {
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

struct ServeWindowInfo {
    window_id: i64,
    state: String,
    left: i64,
    top: i64,
    width: i64,
    height: i64,
    active: bool,
    target_id: String,
    tabs: Vec<Value>,
}

fn collect_window_infos(page: &Page, current_target: &str) -> OpenPageResult<Vec<ServeWindowInfo>> {
    let mut windows: Vec<ServeWindowInfo> = Vec::new();
    let mut indices = HashMap::<i64, usize>::new();

    let browser = page
        .browser()
        .ok_or_else(|| OpenPageError::BrowserOperation("page has no browser".to_string()))?;
    for tab in browser.tab_infos()? {
        let tab_page = browser.get_page(&tab.target_id)?;
        let window_id = tab_page.window_id()?;
        let active = tab.target_id == current_target;
        let tab_json = json!({
            "target_id": tab.target_id,
            "url": tab.url,
            "title": tab.title,
            "type": tab.tab_type,
            "attached": tab.attached,
            "active": active,
        });

        if let Some(index) = indices.get(&window_id).copied() {
            let entry = &mut windows[index];
            entry.active |= active;
            if active {
                entry.target_id = tab_json
                    .get("target_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
            }
            entry.tabs.push(tab_json);
            continue;
        }

        let (left, top) = tab_page.window_location()?;
        let (width, height) = tab_page.window_size()?;
        let state = tab_page.window_state()?;
        let target_id = tab_json
            .get("target_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        indices.insert(window_id, windows.len());
        windows.push(ServeWindowInfo {
            window_id,
            state,
            left,
            top,
            width,
            height,
            active,
            target_id,
            tabs: vec![tab_json],
        });
    }

    Ok(windows)
}

fn window_list_payload(page: &Page, current_target: &str) -> OpenPageResult<Value> {
    let windows = collect_window_infos(page, current_target)?;
    Ok(json!({
        "windows": windows
            .into_iter()
            .enumerate()
            .map(|(index, window)| {
                let target_ids = window
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.get("target_id").and_then(Value::as_str))
                    .collect::<Vec<_>>();
                json!({
                    "index": index + 1,
                    "window_id": window.window_id,
                    "target_id": window.target_id,
                    "state": window.state,
                    "left": window.left,
                    "top": window.top,
                    "width": window.width,
                    "height": window.height,
                    "active": window.active,
                    "tab_count": target_ids.len(),
                    "target_ids": target_ids,
                    "tabs": window.tabs,
                })
            })
            .collect::<Vec<_>>()
    }))
}

fn resolve_window_info<'a>(
    windows: &'a [ServeWindowInfo],
    selector: Option<&str>,
) -> OpenPageResult<&'a ServeWindowInfo> {
    match selector {
        Some(selector) => {
            if let Ok(index) = selector.parse::<usize>() {
                return windows.get(index.saturating_sub(1)).ok_or_else(|| {
                    OpenPageError::ElementNotFound(format!("window index out of range: {index}"))
                });
            }
            windows
                .iter()
                .find(|window| {
                    window.window_id.to_string() == selector
                        || window.target_id == selector
                        || window.tabs.iter().any(|tab| {
                            tab.get("target_id")
                                .and_then(Value::as_str)
                                .map(|target_id| target_id == selector)
                                .unwrap_or(false)
                        })
                })
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound(format!("window not found: {selector}"))
                })
        }
        None => windows
            .iter()
            .find(|window| window.active)
            .or_else(|| windows.first())
            .ok_or_else(|| {
                OpenPageError::ElementNotFound("no browser windows available".to_string())
            }),
    }
}

fn dispatch_page(state: &mut ServePage, op: &str, params: &Value) -> OpenPageResult<Value> {
    if op != "wait.navigation" {
        state.discard_stale_navigation_baseline();
    }
    let page = state.page.clone();
    let mut result = match op {
        "recorder.start" => {
            page.recorder().start()?;
            Ok(json!(page.recorder().status()?))
        }
        "recorder.replay" => replay_recorded_flow(state, params),
        "recorder.stop" => {
            let flow = page.recorder().stop()?;
            Ok(serde_json::to_value(flow)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?)
        }
        "recorder.steps" => Ok(serde_json::to_value(page.recorder().flow()?)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?),
        "recorder.clear" => {
            page.recorder().clear()?;
            Ok(json!({"cleared": true}))
        }
        "recorder.status" => Ok(json!(page.recorder().status()?)),
        "page.back" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({"back": page.back(1)?, "navigation_token": navigation_token});
            state.clear_navigation_baseline();
            Ok(result)
        }
        "page.forward" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({"forward": page.forward(1)?, "navigation_token": navigation_token});
            state.clear_navigation_baseline();
            Ok(result)
        }
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
            let navigation_token = state.record_navigation_baseline();
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
            let result = json!({
                "navigated": true,
                "index": requested_index,
                "id": entry.id,
                "url": entry.url,
                "title": entry.title,
                "navigation_token": navigation_token,
            });
            state.clear_navigation_baseline();
            Ok(result)
        }
        "history.clear" => {
            page.execute_cdp(ResetNavigationHistoryParams::default())?;
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
                "cleared": true,
                "current_index": current_index + 1,
                "entries": entries,
            }))
        }
        "page.goto" => {
            let url = required_str(params, "url")?;
            let navigation_token = state.record_navigation_baseline();
            page.goto(url)?;
            if optional_bool(params, "wait").unwrap_or(true) {
                state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?;
            }
            state.clear_navigation_baseline();
            Ok(json!({"navigated": true, "url": url, "navigation_token": navigation_token}))
        }
        "page.reload" => {
            let navigation_token = state.record_navigation_baseline();
            page.refresh(optional_bool(params, "ignore_cache").unwrap_or(false))?;
            state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?;
            state.clear_navigation_baseline();
            Ok(json!({"reloaded": true, "navigation_token": navigation_token}))
        }
        "page.stop_loading" => {
            page.stop_loading()?;
            Ok(json!({"stopped_loading": true}))
        }
        "page.url" => Ok(json!({"url": page.url()?})),
        "page.title" => Ok(json!({"title": page.title()?})),
        "page.html" => Ok(payload_with_origin_and_title(
            "html",
            json!(page.html()?),
            current_page_origin(state).as_deref(),
            current_page_title(state).as_deref(),
        )),
        "page.snapshot" => snapshot::snapshot_payload(state, params),
        "page.cookies" => Ok(json!({"cookies": page.cookies()?})),
        "page.set_cookie" | "cookies.set" => {
            page.set_cookie(
                required_str(params, "name")?,
                required_str(params, "value")?,
                optional_str(params, "url"),
                optional_str(params, "domain"),
                optional_str(params, "path"),
            )?;
            Ok(json!({"set": true}))
        }
        "page.remove_cookie" | "cookies.delete" => {
            page.remove_cookie(
                required_str(params, "name")?,
                optional_str(params, "url"),
                optional_str(params, "domain"),
                optional_str(params, "path"),
            )?;
            Ok(json!({"deleted": true}))
        }
        "page.clear_cookies" | "cookies.clear" => {
            page.clear_cookies()?;
            Ok(json!({"cleared": true}))
        }
        "page.user_agent" => Ok(json!({"user_agent": page.user_agent()?})),
        "page.ready_state" => Ok(json!({"ready_state": page.ready_state()?})),
        "page.is_loading" => Ok(json!({"is_loading": page.is_loading()?})),
        "page.is_alive" => Ok(json!({"is_alive": page.is_alive()?})),
        "page.tabs" => Ok(json!({"count": page.tabs_count()?, "ids": page.tab_ids()?})),
        "page.download_path" => Ok(json!({"download_path": page.download_path()?})),
        "page.set_download_path" | "set.download_path" => {
            page.set_download_path(required_str(params, "path")?)?;
            Ok(json!({"set": true}))
        }
        "page.download_file_exists_mode" => Ok(json!({
            "mode": page.download_file_exists_mode()?
        })),
        "page.set_download_file_exists_mode" | "set.download_file_exists_mode" => {
            page.set_download_file_exists_mode(DownloadFileExistsMode::parse(required_str(
                params, "mode",
            )?)?)?;
            Ok(json!({"set": true}))
        }
        "page.load_mode" => Ok(json!({"load_mode": page.load_mode()?})),
        "page.set_load_mode" | "set.load_mode" => {
            page.set_load_mode(LoadMode::parse(required_str(params, "mode")?)?)?;
            Ok(json!({"set": true}))
        }
        "page.set_blocked_urls" | "set.blocked_urls" => {
            page.set_blocked_urls(&required_string_array(params, "patterns")?)?;
            Ok(json!({"set": true}))
        }
        "page.set_upload_files" | "set.upload_files" => {
            page.set_upload_files(&required_string_array(params, "files")?)?;
            Ok(json!({"set": true}))
        }
        "page.set_headers" | "set.headers" => {
            let headers = required_headers(params, "headers")?;
            page.set_headers(&headers)?;
            Ok(json!({"set": true}))
        }
        "page.set_user_agent" | "set.user_agent" => {
            page.set_user_agent(
                required_str(params, "user_agent")?,
                optional_str(params, "platform"),
            )?;
            Ok(json!({"set": true}))
        }
        "page.local_storage" => Ok(json!({
            "value": page.local_storage(optional_str(params, "item"))?
        })),
        "page.session_storage" => Ok(json!({
            "value": page.session_storage(optional_str(params, "item"))?
        })),
        "page.set_local_storage" | "set.local_storage" => {
            page.set_local_storage(required_str(params, "item")?, optional_str(params, "value"))?;
            Ok(json!({"set": true}))
        }
        "page.set_session_storage" | "set.session_storage" => {
            page.set_session_storage(required_str(params, "item")?, optional_str(params, "value"))?;
            Ok(json!({"set": true}))
        }
        "permissions.set" => {
            let name = required_str(params, "name")?;
            let setting = required_str(params, "setting")?;
            let origin = page.set_permission(
                name,
                setting,
                optional_str(params, "origin"),
                optional_str(params, "embedded_origin"),
            )?;
            Ok(json!({
                "set": true,
                "name": name,
                "setting": setting,
                "origin": origin,
            }))
        }
        "permissions.reset" => {
            page.reset_permissions()?;
            Ok(json!({"reset": true}))
        }
        "page.activate" => {
            page.activate()?;
            Ok(json!({"activated": true}))
        }
        "window.list" => window_list_payload(&page, &state.current_target_id()),
        "window.switch" => {
            let target_id = required_str(params, "target_id")?;
            state.switch_target(target_id)?;
            Ok(json!({"switched": true, "target_id": target_id}))
        }
        "window.close" => {
            let windows = collect_window_infos(&page, &state.current_target_id())?;
            let selected =
                resolve_window_info(windows.as_slice(), optional_str(params, "target_id"))?;
            let targets = selected
                .tabs
                .iter()
                .filter_map(|tab| tab.get("target_id").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let closed = page.close_tabs(&targets, false)?;
            let current_target = state.current_target_id();
            let remaining_tabs = page
                .browser()
                .ok_or_else(|| OpenPageError::BrowserOperation("page has no browser".to_string()))?
                .tab_infos()?;
            let next_target = if remaining_tabs
                .iter()
                .any(|tab| tab.target_id == current_target)
            {
                None
            } else if let Some(next) = remaining_tabs.first() {
                Some(next.target_id.clone())
            } else {
                Some(page.new_tab(None, false, false, false)?.target_id())
            };

            state.clear_frame();
            if let Some(target_id) = next_target {
                if target_id != current_target {
                    state.switch_target(&target_id)?;
                }
            }

            Ok(json!({
                "closed": closed,
                "window_id": selected.window_id,
                "targets": targets,
            }))
        }
        "page.window_state" | "window.state" => Ok(json!({"state": page.window_state()?})),
        "page.window_size" | "window.size" => {
            let (width, height) = page.window_size()?;
            Ok(json!({"width": width, "height": height}))
        }
        "page.window_location" | "window.location" => {
            let (left, top) = page.window_location()?;
            Ok(json!({"left": left, "top": top}))
        }
        "page.zoom_get" | "zoom.get" => Ok(json!({"factor": page.zoom_factor()?})),
        "page.zoom_set" | "zoom.set" => {
            page.set_zoom_factor(required_f64(params, "factor")?)?;
            Ok(json!({"factor": page.zoom_factor()?}))
        }
        "page.zoom_reset" | "zoom.reset" => {
            page.reset_zoom_factor()?;
            Ok(json!({"factor": page.zoom_factor()?}))
        }
        "page.window_max" | "window.max" => {
            page.window_max()?;
            Ok(json!({"set": true}))
        }
        "page.window_min" | "window.min" | "window.mini" => {
            page.window_min()?;
            Ok(json!({"set": true}))
        }
        "page.window_full" | "window.full" => {
            page.window_full()?;
            Ok(json!({"set": true}))
        }
        "page.window_normal" | "window.normal" => {
            page.window_normal()?;
            Ok(json!({"set": true}))
        }
        "page.window_hide" | "window.hide" => {
            page.window_hide()?;
            Ok(json!({"set": true}))
        }
        "page.window_show" | "window.show" => {
            page.window_show()?;
            Ok(json!({"set": true}))
        }
        "page.window_size_set" | "window.size_set" => {
            page.window_size_set(
                optional_i64(params, "width"),
                optional_i64(params, "height"),
            )?;
            Ok(json!({"set": true}))
        }
        "page.window_location_set" | "window.location_set" => {
            page.window_location_set(optional_i64(params, "left"), optional_i64(params, "top"))?;
            Ok(json!({"set": true}))
        }
        "page.scroll_position" => {
            let (x, y) = page.scroll_position()?;
            Ok(json!({"x": x, "y": y}))
        }
        "page.scroll" => {
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
        "page.run_js" => Ok(payload_with_origin(
            "value",
            json!(state.run_js(required_str(params, "script")?)?),
            current_page_origin(state).as_deref(),
        )),
        "clipboard.read" => Ok(payload_with_origin(
            "text",
            Value::String(page.clipboard_read_text()?),
            current_page_origin(state).as_deref(),
        )),
        "clipboard.write" => {
            page.clipboard_write_text(required_str(params, "text")?)?;
            Ok(payload_with_origin(
                "written",
                Value::Bool(true),
                current_page_origin(state).as_deref(),
            ))
        }
        "page.download_url" => {
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
        "page.pdf" => {
            page.save_pdf(required_str(params, "path")?)?;
            Ok(json!({"saved": true}))
        }
        "page.save" => {
            let mut path = std::path::PathBuf::from(required_str(params, "path")?);
            if path.extension().is_none() {
                path.set_extension("mhtml");
            }
            page.save(Some(path.as_path()), None, false)?;
            Ok(json!({"saved": true, "path": path}))
        }
        "page.screenshot" => {
            page.save_screenshot(
                required_str(params, "path")?,
                optional_bool(params, "full_page").unwrap_or(false),
            )?;
            Ok(json!({"saved": true}))
        }
        "page.active_element" => Ok(json!({
            "element": state
                .active_element()?
                .map(|element| state.element_payload(element))
                .transpose()?
        })),
        "tab.list" => Ok(json!({
            "tabs": page
                .browser().ok_or_else(|| OpenPageError::BrowserOperation("page has no browser".to_string()))?.tab_infos()?
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
            let new_context = optional_bool(params, "context").unwrap_or(false);
            let new_page = page.new_tab(
                optional_str(params, "url"),
                optional_bool(params, "window").unwrap_or(false),
                background,
                new_context,
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
                "context": new_context,
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
            let remaining_tabs = page
                .browser()
                .ok_or_else(|| OpenPageError::BrowserOperation("page has no browser".to_string()))?
                .tab_infos()?;
            let next_target = if remaining_tabs
                .iter()
                .any(|tab| tab.target_id == current_target)
            {
                None
            } else if let Some(next) = remaining_tabs.first() {
                Some(next.target_id.clone())
            } else {
                Some(page.new_tab(None, false, false, false)?.target_id())
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
                        "active": state
                            .active_frame_target
                            .as_deref()
                            .and_then(|target| target.strip_prefix("id:"))
                            == Some(frame.id()),
                    })
                })
                .collect::<Vec<_>>()
        })),
        "frame.switch" => {
            let target = required_str(params, "target")?;
            if matches!(target, "main" | "root" | "page") {
                state.clear_frame();
                Ok(json!({"switched": true, "frame": "main"}))
            } else {
                let frame = if let Ok(index) = target.parse::<usize>() {
                    page.get_frame_context_by_index(index)?
                } else {
                    page.get_frame_context(target)?
                };
                state.switch_frame(Some(format!("id:{}", frame.id())));
                Ok(json!({
                    "switched": true,
                    "frame_id": frame.id(),
                    "target": target,
                }))
            }
        }
        "page.find" => {
            let element = state.find(&required_locator_string(params)?)?;
            Ok(state.element_payload(element)?)
        }
        "page.find_all" => {
            let elements = state.find_all(&required_locator_string(params)?)?;
            let payloads = elements
                .into_iter()
                .map(|element| state.element_payload(element))
                .collect::<OpenPageResult<Vec<_>>>()?;
            Ok(json!({"elements": payloads}))
        }
        "page.locate" => locate_chain_payload(state, required_str(params, "chain")?),
        "page.count" => Ok(json!({
            "count": state.find_all(&required_locator_string(params)?)?.len()
        })),
        "page.ele.is_visible" | "element.is_visible" => Ok(json!({
            "visible": state.find(&required_locator_string(params)?)?.is_displayed()?
        })),
        "page.ele.is_enabled" | "element.is_enabled" => Ok(json!({
            "enabled": state.find(&required_locator_string(params)?)?.is_enabled()?
        })),
        "page.ele.is_checked" | "element.is_checked" => Ok(json!({
            "checked": state.find(&required_locator_string(params)?)?.is_checked()?
        })),
        "page.ele.is_selected" | "element.is_selected" => Ok(json!({
            "selected": state.find(&required_locator_string(params)?)?.is_selected()?
        })),
        "page.ele.is_alive" | "element.is_alive" => Ok(json!({
            "alive": state.find(&required_locator_string(params)?)?.is_alive()?
        })),
        "page.ele.is_in_viewport" | "element.is_in_viewport" => Ok(json!({
            "in_viewport": state.find(&required_locator_string(params)?)?.is_in_viewport()?
        })),
        "page.ele.is_whole_in_viewport" | "element.is_whole_in_viewport" => Ok(json!({
            "whole_in_viewport": state.find(&required_locator_string(params)?)?.is_whole_in_viewport()?
        })),
        "page.ele.is_covered" | "element.is_covered" => Ok(json!({
            "covered": state.find(&required_locator_string(params)?)?.is_covered()?
        })),
        "page.ele.is_clickable" | "element.is_clickable" => Ok(json!({
            "clickable": state.find(&required_locator_string(params)?)?.is_clickable()?
        })),
        "page.ele.has_rect" | "element.has_rect" => Ok(json!({
            "has_rect": state.find(&required_locator_string(params)?)?.has_rect()?
        })),
        "page.ele.focus" | "element.focus" => {
            state.find(&required_locator_string(params)?)?.focus()?;
            Ok(json!({"focused": true}))
        }
        "page.ele.text" | "element.text" => Ok(payload_with_origin(
            "text",
            json!(state.find(&required_locator_string(params)?)?.text()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.value" | "element.value" => Ok(payload_with_origin(
            "value",
            json!(state.find(&required_locator_string(params)?)?.value()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.raw_text" | "element.raw_text" => Ok(payload_with_origin(
            "raw_text",
            json!(state.find(&required_locator_string(params)?)?.raw_text()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.link" | "element.link" => Ok(payload_with_origin(
            "link",
            json!(state.find(&required_locator_string(params)?)?.link()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.child_count" | "element.child_count" => Ok(payload_with_origin(
            "child_count",
            json!(
                state
                    .find(&required_locator_string(params)?)?
                    .child_count()?
            ),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.css_path" | "element.css_path" => Ok(payload_with_origin(
            "css_path",
            json!(state.find(&required_locator_string(params)?)?.css_path()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.xpath" | "element.xpath" => Ok(payload_with_origin(
            "xpath",
            json!(state.find(&required_locator_string(params)?)?.xpath()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.html" | "element.html" => Ok(payload_with_origin(
            "html",
            json!(state.find(&required_locator_string(params)?)?.html()?),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.attr" | "element.attr" => Ok(payload_with_origin(
            "value",
            json!(
                state
                    .find(&required_locator_string(params)?)?
                    .attr(required_str(params, "name")?)?
            ),
            current_page_origin(state).as_deref(),
        )),
        "page.ele.click" | "element.click" => {
            let locator = required_locator_string(params)?;
            let navigation_token = state.record_navigation_baseline();
            state
                .find_revisioned("click", &locator, optional_str(params, "expected_revision"))?
                .click()?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
        }
        "page.ele.click_right" | "element.click_right" => {
            state
                .find(&required_locator_string(params)?)?
                .click_right()?;
            Ok(json!({"clicked": true, "button": "right"}))
        }
        "page.ele.click_middle" | "element.click_middle" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .click_middle()?;
            Ok(json!({"clicked": true, "button": "middle", "navigation_token": navigation_token}))
        }
        "page.ele.click_multi" | "element.click_multi" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .click_multi(optional_u64(params, "count").unwrap_or(2) as u32)?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
        }
        "page.ele.click_at" | "element.click_at" => {
            let navigation_token = state.record_navigation_baseline();
            state.find(&required_locator_string(params)?)?.click_at(
                optional_f64(params, "x"),
                optional_f64(params, "y"),
                optional_str(params, "button").unwrap_or("left"),
                optional_u64(params, "count").unwrap_or(1) as u32,
            )?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
        }
        "page.ele.input" | "element.input" => {
            let locator = required_locator_string(params)?;
            state
                .find_revisioned("fill", &locator, optional_str(params, "expected_revision"))?
                .input(required_str(params, "text")?)?;
            Ok(json!({"input": true}))
        }
        "page.ele.select_range" | "element.select_range" => {
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
        "page.ele.select_text" | "element.select_text" => {
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
        "page.ele.clear" | "element.clear" => {
            state.find(&required_locator_string(params)?)?.clear()?;
            Ok(json!({"cleared": true}))
        }
        "page.ele.submit" | "element.submit" => {
            let navigation_token = state.record_navigation_baseline();
            state.find(&required_locator_string(params)?)?.submit()?;
            Ok(json!({"submitted": true, "navigation_token": navigation_token}))
        }
        "page.ele.check" | "element.check" => {
            state
                .find(&required_locator_string(params)?)?
                .set_checked(true)?;
            Ok(json!({"checked": true}))
        }
        "page.ele.uncheck" | "element.uncheck" => {
            state
                .find(&required_locator_string(params)?)?
                .set_checked(false)?;
            Ok(json!({"checked": false}))
        }
        "page.ele.hover" | "element.hover" => {
            state.find(&required_locator_string(params)?)?.hover()?;
            Ok(json!({"hovered": true}))
        }
        "page.ele.hover_at" | "element.hover_at" => {
            state
                .find(&required_locator_string(params)?)?
                .hover_with_offset(optional_f64(params, "x"), optional_f64(params, "y"))?;
            Ok(json!({"hovered": true}))
        }
        "page.ele.press_key" | "element.press_key" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .press_key(required_str(params, "key")?)?;
            Ok(json!({"pressed": true, "navigation_token": navigation_token}))
        }
        "page.ele.select" | "element.select" => {
            let element = state.find(&required_locator_string(params)?)?;
            let selected = if params.get("text").is_some() {
                element.select_by_text(select_string_values(params, "text")?)?
            } else if params.get("value").is_some() {
                element.select_by_value(select_string_values(params, "value")?)?
            } else if params.get("index").is_some() {
                element.select_by_index(select_index_values(params, "index")?)?
            } else {
                return Err(OpenPageError::BrowserOperation(
                    "select requires one of: text, value, index".to_string(),
                ));
            };
            Ok(json!({"selected": selected}))
        }
        "page.ele.option_texts" | "element.option_texts" => {
            let options = state
                .find(&required_locator_string(params)?)?
                .option_texts()?;
            Ok(json!({"options": options}))
        }
        "page.ele.selected_option" | "element.selected_option" => {
            let option = state
                .find(&required_locator_string(params)?)?
                .selected_option()?;
            Ok(json!({"option": option}))
        }
        "page.ele.selected_options" | "element.selected_options" => {
            let options = state
                .find(&required_locator_string(params)?)?
                .selected_options()?;
            Ok(json!({"options": options}))
        }
        "page.ele.select_all_options" | "element.select_all_options" => {
            state
                .find(&required_locator_string(params)?)?
                .select_all()?;
            Ok(json!({"selected_all": true}))
        }
        "page.ele.clear_selected_options" | "element.clear_selected_options" => {
            state
                .find(&required_locator_string(params)?)?
                .clear_selected()?;
            Ok(json!({"cleared": true}))
        }
        "page.ele.invert_selected_options" | "element.invert_selected_options" => {
            state
                .find(&required_locator_string(params)?)?
                .invert_selected()?;
            Ok(json!({"inverted": true}))
        }
        "page.ele.upload" | "element.upload" => {
            let files = required_string_array(params, "files")?;
            state
                .find(&required_locator_string(params)?)?
                .set_file_input_files(&files)?;
            Ok(json!({"uploaded": true}))
        }
        "page.ele.click_to_download" | "element.click_to_download" => {
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
        "page.ele.click_to_upload" | "element.click_to_upload" => {
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
        "page.ele.click_for_new_tab" | "element.click_for_new_tab" => {
            let new_page = state
                .find(&required_locator_string(params)?)?
                .clicker()
                .for_new_tab(
                    optional_u64(params, "timeout_ms"),
                    optional_bool(params, "js").unwrap_or(false),
                )?;
            let Some(new_page) = new_page else {
                return Ok(json!({"created": false}));
            };
            let target_id = new_page.target_id();
            let url = json!(new_page.url()?);
            state.switch_target(&target_id)?;
            Ok(json!({
                "created": true,
                "switched": true,
                "target_id": target_id,
                "url": url,
            }))
        }
        "page.ele.scroll_into_view" | "element.scroll_into_view" => {
            let element = state.find(&required_locator_string(params)?)?;
            if optional_bool(params, "center").unwrap_or(false) {
                element.scroll_to_center()?;
            } else {
                element.scroll_to_see(None)?;
            }
            Ok(json!({"scrolled_into_view": true}))
        }
        "page.ele.scroll_position" | "element.scroll_position" => {
            let position = state
                .find(&required_locator_string(params)?)?
                .rect_scroll_position()?;
            Ok(json!({
                "x": position.map(|(x, _)| x),
                "y": position.map(|(_, y)| y),
            }))
        }
        "page.ele.scroll" | "element.scroll" => {
            let element = state.find(&required_locator_string(params)?)?;
            match required_str(params, "direction")? {
                "down" => element.scroll_down(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "up" => element.scroll_up(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "left" => element.scroll_left(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "right" => element.scroll_right(optional_f64(params, "pixels").unwrap_or(300.0))?,
                "top" => element.scroll_to_top()?,
                "bottom" => element.scroll_to_bottom()?,
                "half" => element.scroll_to_half()?,
                "rightmost" => element.scroll_to_rightmost()?,
                "leftmost" => element.scroll_to_leftmost()?,
                "location" => element.scroll_to_location(
                    optional_f64(params, "x").unwrap_or(0.0),
                    optional_f64(params, "y").unwrap_or(0.0),
                )?,
                other => {
                    return Err(OpenPageError::UnsupportedLocator(format!(
                        "unknown element scroll direction: {other}"
                    )));
                }
            }
            Ok(json!({"scrolled": true}))
        }
        "page.ele.drag" | "element.drag" => {
            state.find(&required_locator_string(params)?)?.drag(
                optional_f64(params, "dx").unwrap_or(0.0),
                optional_f64(params, "dy").unwrap_or(0.0),
                optional_f64(params, "duration").unwrap_or(0.5),
            )?;
            Ok(json!({"dragged": true}))
        }
        "page.ele.drag_to_point" | "element.drag_to_point" => {
            state
                .find(&required_locator_string(params)?)?
                .drag_to_point(
                    optional_f64(params, "x").unwrap_or(0.0),
                    optional_f64(params, "y").unwrap_or(0.0),
                    optional_f64(params, "duration").unwrap_or(0.5),
                )?;
            Ok(json!({"dragged": true}))
        }
        "page.ele.run_js" | "element.run_js" => Ok(json!({
            "value": state.find(&required_locator_string(params)?)?.run_js(required_str(params, "script")?)?
        })),
        "page.ele.screenshot" | "element.screenshot" => {
            state
                .find(&required_locator_string(params)?)?
                .save_screenshot(required_str(params, "path")?)?;
            Ok(json!({"saved": true}))
        }
        "page.handle_alert" | "alert.handle" => Ok(json!({
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
        "page.set_next_alert_action" | "alert.set_next_action" => {
            page.set_next_alert_action(
                optional_bool(params, "accept").unwrap_or(true),
                optional_str(params, "prompt_text"),
            )?;
            Ok(json!({"set": true}))
        }
        "page.set_auto_alert_action" | "alert.set_auto_action" => {
            page.set_auto_alert_action(
                optional_bool(params, "accept"),
                optional_str(params, "prompt_text"),
            )?;
            Ok(json!({"set": true}))
        }
        "page.has_alert" | "alert.has" => Ok(json!({"has_alert": page.has_alert()?})),
        "page.wait_for_alert_closed" | "wait.alert_closed" => Ok(json!({
            "closed": page.wait_for_alert_closed(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.load_start" => Ok(json!({
            "started": page.wait_for_load_start(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.doc_loaded" => Ok(json!({
            "loaded": state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ready" => {
            wait_for_ready_payload(state, optional_u64(params, "timeout_ms").unwrap_or(10_000))
        }
        "wait.navigation" => wait_for_navigation_payload(
            state,
            optional_u64(params, "timeout_ms").unwrap_or(10_000),
            optional_str(params, "token"),
        ),
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
        "wait.elements_loaded" => Ok(json!({
            "loaded": page.wait_for_elements_loaded(
                &required_string_array(params, "locators")?,
                optional_bool(params, "any_one").unwrap_or(false),
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.element_displayed" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_displayed(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_hidden" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_hidden(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_enabled" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_enabled(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_disabled" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?.wait_until_disabled(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.element_deleted" => Ok(json!({
            "ready": wait_for_deleted(
                state,
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.element_clickable" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_clickable(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_has_rect" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_has_rect(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_covered" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_covered(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_not_covered" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_not_covered(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_stop_moving" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_stop_moving(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.element_disabled_or_deleted" => Ok(json!({
            "ready": wait_for_disabled_or_deleted(
                state,
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.upload_paths_inputted" => Ok(json!({
            "inputted": page.wait_for_upload_paths_inputted(
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "page.drag_in" => {
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
    }?;

    let revision = if operation_bumps_revision(op) {
        state.bump_revision()
    } else {
        state.current_revision()
    };
    if let Some(payload) = result.as_object_mut() {
        payload.insert("revision".to_string(), Value::String(revision));
    }
    Ok(result)
}

fn validate_expected_revision(
    operation: &str,
    locator: &str,
    current_revision: &str,
    expected_revision: Option<&str>,
) -> OpenPageResult<()> {
    let Some(expected_revision) = expected_revision else {
        return Ok(());
    };
    if expected_revision == current_revision {
        return Ok(());
    }

    Err(OpenPageError::ElementNotFound(format!(
        "revision {expected_revision} is stale, current is {current_revision}"
    ))
    .diagnosed(ErrorDiagnostic {
        operation: Some(operation.to_string()),
        locator: Some(locator.to_string()),
        current_revision: Some(current_revision.to_string()),
        expected_revision: Some(expected_revision.to_string()),
        failure_reason: Some("stale_ref".to_string()),
        ..ErrorDiagnostic::default()
    }))
}

fn operation_bumps_revision(op: &str) -> bool {
    matches!(
        op,
        "recorder.replay"
            | "page.back"
            | "page.forward"
            | "history.go"
            | "page.goto"
            | "page.reload"
            | "page.activate"
            | "page.run_js"
            | "page.input"
            | "page.type"
            | "page.type_with_interval"
            | "tab.new"
            | "tab.switch"
            | "tab.close"
            | "window.switch"
            | "window.close"
            | "frame.switch"
            | "page.ele.click"
            | "element.click"
            | "page.ele.click_right"
            | "element.click_right"
            | "page.ele.click_middle"
            | "element.click_middle"
            | "page.ele.click_multi"
            | "element.click_multi"
            | "page.ele.click_at"
            | "element.click_at"
            | "page.ele.input"
            | "element.input"
            | "page.ele.clear"
            | "element.clear"
            | "page.ele.submit"
            | "element.submit"
            | "page.ele.check"
            | "element.check"
            | "page.ele.uncheck"
            | "element.uncheck"
            | "page.ele.press_key"
            | "element.press_key"
            | "page.ele.select"
            | "element.select"
            | "page.ele.select_all_options"
            | "element.select_all_options"
            | "page.ele.clear_selected_options"
            | "element.clear_selected_options"
            | "page.ele.invert_selected_options"
            | "element.invert_selected_options"
            | "page.ele.upload"
            | "element.upload"
            | "page.ele.click_to_upload"
            | "element.click_to_upload"
            | "page.ele.click_for_new_tab"
            | "element.click_for_new_tab"
            | "page.ele.drag"
            | "element.drag"
            | "page.ele.drag_to_point"
            | "element.drag_to_point"
            | "page.ele.run_js"
            | "element.run_js"
            | "page.drag_in"
    )
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

fn required_f64(params: &Value, key: &str) -> OpenPageResult<f64> {
    params
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing number param: {key}")))
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
    let normalized = normalize_locator_shorthand(locator);
    if normalized == locator {
        Cow::Borrowed(locator)
    } else {
        Cow::Owned(normalized)
    }
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

fn current_page_origin(state: &ServePage) -> Option<String> {
    state.page.url().ok().filter(|value| !value.is_empty())
}

fn current_page_title(state: &ServePage) -> Option<String> {
    state.page.title().ok().filter(|value| !value.is_empty())
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

fn select_string_values(params: &Value, key: &str) -> OpenPageResult<Vec<String>> {
    let value = params
        .get(key)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing param: {key}")))?;
    if let Some(text) = value.as_str() {
        return Ok(vec![text.to_string()]);
    }
    if let Some(values) = value.as_array() {
        return values
            .iter()
            .map(|value| {
                value.as_str().map(ToString::to_string).ok_or_else(|| {
                    OpenPageError::BrowserOperation(format!(
                        "array param must contain only strings: {key}"
                    ))
                })
            })
            .collect();
    }
    Err(OpenPageError::BrowserOperation(format!(
        "{key} must be a string or string array"
    )))
}

fn select_index_values(params: &Value, key: &str) -> OpenPageResult<Vec<usize>> {
    let value = params
        .get(key)
        .ok_or_else(|| OpenPageError::BrowserOperation(format!("missing param: {key}")))?;
    if let Some(index) = value.as_u64() {
        return Ok(vec![index as usize]);
    }
    if let Some(values) = value.as_array() {
        return values
            .iter()
            .map(|value| {
                value.as_u64().map(|value| value as usize).ok_or_else(|| {
                    OpenPageError::BrowserOperation(format!(
                        "array param must contain only integers: {key}"
                    ))
                })
            })
            .collect();
    }
    Err(OpenPageError::BrowserOperation(format!(
        "{key} must be an integer or integer array"
    )))
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

fn locate_chain_payload(state: &mut ServePage, chain: &str) -> OpenPageResult<Value> {
    let (element, steps) = resolve_locator_chain(state, chain)?;
    let mut payload = state.element_payload(element)?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("chain".to_string(), Value::String(chain.to_string()));
        obj.insert("steps".to_string(), json!(steps));
    }
    Ok(payload)
}

fn resolve_locator_chain(state: &ServePage, chain: &str) -> OpenPageResult<(Element, Vec<String>)> {
    let parts = split_locator_chain(chain)?;
    if parts.is_empty() {
        return Err(OpenPageError::UnsupportedLocator(
            "locator chain is empty".to_string(),
        ));
    }

    let root = normalize_locator_shorthand(parts[0]);
    let mut element = state.find(&root)?;
    let mut steps = vec![format!("root {root}")];

    for part in parts.iter().skip(1) {
        let (op, locator) = parse_locator_chain_step(part)?;
        element = match match op {
            "parent" => match locator {
                Some(locator) => element.parent_with(locator.as_str(), 1),
                None => element.parent(),
            },
            "child" => match locator {
                Some(locator) => element.child_with(Some(locator.as_str()), 1),
                None => element.child(),
            },
            "prev" | "previous" => match locator {
                Some(locator) => element.prev_with(Some(locator.as_str()), 1),
                None => element.prev(),
            },
            "next" => match locator {
                Some(locator) => element.next_with(Some(locator.as_str()), 1),
                None => element.next(),
            },
            "before" => match locator {
                Some(locator) => element.before_with(Some(locator.as_str()), 1),
                None => element.before(),
            },
            "after" => match locator {
                Some(locator) => element.after_with(Some(locator.as_str()), 1),
                None => element.after(),
            },
            other => {
                return Err(OpenPageError::UnsupportedLocator(format!(
                    "unsupported locator chain step: {other}"
                )));
            }
        } {
            Ok(element) => element,
            Err(error) => {
                return Err(OpenPageError::UnsupportedLocator(format!(
                    "locator chain step `{part}` failed: {error}"
                )));
            }
        };
        steps.push(part.to_string());
    }

    Ok((element, steps))
}

fn split_locator_chain(chain: &str) -> OpenPageResult<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut nesting = 0usize;
    let bytes = chain.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let character = bytes[index] as char;
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            index += 1;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '[' | '(' | '{' => nesting += 1,
            ']' | ')' | '}' => {
                nesting = nesting.checked_sub(1).ok_or_else(|| {
                    OpenPageError::UnsupportedLocator(
                        "unbalanced locator chain delimiters".to_string(),
                    )
                })?
            }
            '>' if nesting == 0 && bytes.get(index + 1) == Some(&b'>') => {
                let part = chain[start..index].trim();
                if part.is_empty() {
                    return Err(OpenPageError::UnsupportedLocator(
                        "locator chain contains an empty step".to_string(),
                    ));
                }
                parts.push(part);
                index += 2;
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }

    if quote.is_some() || nesting != 0 || escaped {
        return Err(OpenPageError::UnsupportedLocator(
            "unterminated locator chain quote or delimiter".to_string(),
        ));
    }
    let part = chain[start..].trim();
    if part.is_empty() {
        return Err(OpenPageError::UnsupportedLocator(
            "locator chain contains an empty step".to_string(),
        ));
    }
    parts.push(part);
    Ok(parts)
}

fn parse_locator_chain_step(step: &str) -> OpenPageResult<(&str, Option<String>)> {
    let mut parts = step.splitn(2, char::is_whitespace);
    let op = parts.next().unwrap_or_default().trim();
    if op.is_empty() {
        return Err(OpenPageError::UnsupportedLocator(
            "empty locator chain step".to_string(),
        ));
    }
    let locator = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_locator_shorthand);
    Ok((op, locator))
}

fn normalize_locator_shorthand(locator: &str) -> String {
    let trimmed = locator.trim();
    if let Some(value) = trimmed.strip_prefix("text:") {
        let value = value.trim().trim_matches('"').trim_matches('\'');
        return format!("text={value}");
    }
    trimmed.to_string()
}

fn element_to_json(element: Element, ref_id: Option<String>) -> OpenPageResult<Value> {
    let tag = element.tag()?;
    let text = element.text()?.map(|value| clip_agent_text(&value, 120));
    let attrs = compact_element_attrs(element.attrs()?);
    let role = element_role(&tag, &attrs);
    let name = element_name(text.as_deref(), &attrs);

    let mut payload = json!({
        "tag": tag,
        "role": role,
        "name": name,
        "text": text,
        "attrs": attrs,
        "state": {
            "visible": element.is_displayed().ok(),
            "enabled": element.is_enabled().ok(),
            "in_viewport": element.is_in_viewport().ok(),
            "has_rect": element.has_rect().ok(),
        },
    });
    if let Some(ref_id) = ref_id.filter(|value| !value.is_empty())
        && let Some(obj) = payload.as_object_mut()
    {
        obj.insert("ref".to_string(), Value::String(ref_id));
    }
    Ok(payload)
}

fn clip_agent_text(value: &str, limit: usize) -> String {
    let normalized = normalize_agent_text(value);
    if normalized.chars().count() <= limit {
        return normalized;
    }
    let byte_limit = normalized
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(normalized.len());
    normalized[..byte_limit].to_string()
}

fn normalize_agent_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compact_element_attrs(attrs: Vec<(String, String)>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in attrs {
        if !matches!(
            key.as_str(),
            "id" | "class"
                | "name"
                | "type"
                | "placeholder"
                | "href"
                | "src"
                | "role"
                | "aria-label"
                | "title"
                | "alt"
                | "value"
        ) {
            continue;
        }
        let value = clip_agent_text(&value, 120);
        if !value.is_empty() {
            out.insert(key, Value::String(value));
        }
    }
    out
}

fn element_role(tag: &str, attrs: &Map<String, Value>) -> String {
    if let Some(role) = attrs.get("role").and_then(Value::as_str) {
        if !role.is_empty() {
            return role.to_string();
        }
    }

    match tag {
        "a" if attrs.get("href").is_some() => "link",
        "button" => "button",
        "textarea" => "textbox",
        "select" => "combobox",
        "option" => "option",
        "input" => match attrs
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "checkbox" => "checkbox",
            "radio" => "radio",
            "button" | "submit" | "reset" => "button",
            "search" => "searchbox",
            _ => "textbox",
        },
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        _ => tag,
    }
    .to_string()
}

fn element_name(text: Option<&str>, attrs: &Map<String, Value>) -> Option<String> {
    for key in ["aria-label", "title", "alt", "placeholder", "value"] {
        if let Some(value) = attrs.get(key).and_then(Value::as_str) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    text.filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn wait_for_ready_payload(state: &ServePage, timeout_ms: u64) -> OpenPageResult<Value> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));

    loop {
        if let Ok(snapshot) = ready_snapshot(state) {
            if snapshot.is_settled() {
                return Ok(json!({
                    "ready": true,
                    "ready_state": snapshot.ready_state,
                    "url": snapshot.url,
                }));
            }
        }

        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-ready timed out after {timeout_ms}ms"
            )));
        }
        sleep(Duration::from_millis(50));
    }
}

fn wait_for_navigation_payload(
    state: &mut ServePage,
    timeout_ms: u64,
    token: Option<&str>,
) -> OpenPageResult<Value> {
    let baseline = state.navigation_baseline_for_wait(token)?;
    let from_url = match &baseline {
        NavigationBaseline::Page { url, .. } | NavigationBaseline::Frame { url } => url.clone(),
    };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    let mut saw_transition = false;

    loop {
        let page_snapshot = match &baseline {
            NavigationBaseline::Page { .. } => state.page.navigation_snapshot().ok(),
            NavigationBaseline::Frame { .. } => None,
        };

        let ready = ready_snapshot(state);
        if let NavigationBaseline::Page { started_seq, .. } = &baseline
            && page_snapshot
                .as_ref()
                .is_some_and(|snapshot| page_navigation_transition_observed(*started_seq, snapshot))
        {
            saw_transition = true;
        }

        match ready {
            Ok(snapshot) => {
                if navigation_transition_observed(from_url.as_deref(), &snapshot) {
                    saw_transition = true;
                }

                let settled = match &baseline {
                    NavigationBaseline::Page { started_seq, .. } => {
                        page_snapshot
                            .as_ref()
                            .is_some_and(|page| page_navigation_settled(*started_seq, page))
                            || (navigation_transition_observed(from_url.as_deref(), &snapshot)
                                && snapshot.is_settled())
                    }
                    NavigationBaseline::Frame { .. } => {
                        navigation_transition_observed(from_url.as_deref(), &snapshot)
                            && snapshot.is_settled()
                    }
                };

                if saw_transition && settled {
                    state.consume_navigation_token(token);
                    return Ok(json!({
                        "navigated": true,
                        "ready": true,
                        "ready_state": snapshot.ready_state,
                        "from_url": from_url,
                        "url": snapshot.url,
                        "token": token,
                    }));
                }
            }
            Err(_) => {
                if let NavigationBaseline::Frame { .. } = &baseline {
                    saw_transition = true;
                }
            }
        }

        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "wait-for-navigation timed out after {timeout_ms}ms"
            )));
        }
        sleep(Duration::from_millis(50));
    }
}

struct ReadySnapshot {
    ready_state: String,
    has_document: bool,
    url: Option<String>,
}

impl ReadySnapshot {
    fn is_settled(&self) -> bool {
        self.ready_state == "complete" && self.has_document
    }
}

fn navigation_transition_observed(from_url: Option<&str>, snapshot: &ReadySnapshot) -> bool {
    from_url.is_some_and(|url| snapshot.url.as_deref() != Some(url))
        || snapshot.ready_state != "complete"
}

fn page_navigation_transition_observed(
    started_seq: u64,
    snapshot: &PageNavigationSnapshot,
) -> bool {
    snapshot.started_seq > started_seq
}

fn page_navigation_settled(started_seq: u64, snapshot: &PageNavigationSnapshot) -> bool {
    snapshot.started_seq > started_seq && snapshot.settled_seq >= snapshot.started_seq
}

fn ready_snapshot(state: &ServePage) -> OpenPageResult<ReadySnapshot> {
    let value = state.run_js(
        r#"(() => ({
            readyState: document.readyState,
            hasDocument: !!document.documentElement,
            url: window.location.href
        }))()"#,
    )?;
    Ok(ReadySnapshot {
        ready_state: value
            .get("readyState")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        has_document: value
            .get("hasDocument")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        url: value
            .get("url")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty()),
    })
}

fn wait_for_deleted(state: &ServePage, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        match state.find(locator) {
            Ok(element) => match element.is_alive() {
                Ok(false) | Err(_) => return Ok(true),
                Ok(true) => {}
            },
            Err(OpenPageError::ElementNotFound(_)) => return Ok(true),
            Err(err) => return Err(err),
        }

        if Instant::now() >= deadline {
            return wait_timeout_result("wait.element_deleted", timeout_ms);
        }
        sleep(Duration::from_millis(50));
    }
}

fn wait_for_disabled_or_deleted(
    state: &ServePage,
    locator: &str,
    timeout_ms: u64,
) -> OpenPageResult<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        match state.find(locator) {
            Ok(element) => {
                if !element.is_alive().unwrap_or(false) || !element.is_enabled().unwrap_or(false) {
                    return Ok(true);
                }
            }
            Err(OpenPageError::ElementNotFound(_)) => return Ok(true),
            Err(err) => return Err(err),
        }

        if Instant::now() >= deadline {
            return wait_timeout_result("wait.element_disabled_or_deleted", timeout_ms);
        }
        sleep(Duration::from_millis(50));
    }
}

fn wait_for_function_result(
    state: &ServePage,
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
    state: &ServePage,
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
    state: &ServePage,
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

fn step_target_frames(action: &RecordedAction) -> Option<&[String]> {
    match action {
        RecordedAction::Click { target }
        | RecordedAction::Fill { target, .. }
        | RecordedAction::Select { target, .. }
        | RecordedAction::Check { target, .. } => Some(&target.frames),
        RecordedAction::Press {
            target: Some(target),
            ..
        } => Some(&target.frames),
        RecordedAction::Goto { .. } | RecordedAction::Press { target: None, .. } => None,
    }
}

fn replay_recorded_flow(state: &mut ServePage, params: &Value) -> OpenPageResult<Value> {
    let flow_value = params
        .get("flow")
        .cloned()
        .unwrap_or_else(|| params.clone());
    let flow: RecordedFlow = serde_json::from_value(flow_value)
        .map_err(|err| OpenPageError::BrowserOperation(format!("invalid recorded flow: {err}")))?;
    let secrets = params
        .get("secrets")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let page = state.page.clone();
    let find = |target: &RecordedTarget| match page.find(&target.locator) {
        Ok(element) => Ok(element),
        Err(error) => Err(error),
    };
    let mut replayed = 0;
    for step in flow.steps {
        if step_target_frames(&step.action).is_some() {
            let frames = step_target_frames(&step.action).unwrap_or_default();
            state.switch_frame(Some(format!("frames:{}", frames.join("\u{1f}"))));
        } else {
            state.clear_frame();
        }
        match step.action {
            RecordedAction::Goto { url } => {
                state.page.goto(&url)?;
            }
            RecordedAction::Click { target } => {
                let navigation_token = if step.wait_after == Some(RecordedWait::Navigation) {
                    Some(state.record_navigation_baseline())
                } else {
                    None
                };
                find(&target)?.click()?;
                if let Some(token) = navigation_token {
                    wait_for_navigation_payload(state, 30_000, Some(&token))?;
                }
            }
            RecordedAction::Fill { target, value } => {
                let text = match value {
                    RecordedValue::Text(text) => text,
                    RecordedValue::Secret { secret } => secrets
                        .get(&secret)
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            OpenPageError::BrowserOperation(format!(
                                "missing runtime secret: {secret}"
                            ))
                        })?
                        .to_string(),
                };
                find(&target)?.input(text)?;
            }
            RecordedAction::Select { target, values } => {
                find(&target)?.select_by_value(values)?;
            }
            RecordedAction::Check { target, checked } => {
                find(&target)?.set_checked(checked)?;
            }
            RecordedAction::Press { target, key } => {
                if let Some(target) = target {
                    find(&target)?.press_key(&key)?;
                } else {
                    state.page.actions()?.type_keys(key)?;
                }
            }
        }
        replayed += 1;
    }
    Ok(json!({"replayed": replayed, "version": flow.version}))
}

#[cfg(test)]
mod locator_chain_tests {
    use super::{
        normalize_locator_shorthand, operation_bumps_revision, parse_locator_chain_step,
        split_locator_chain, validate_expected_revision,
    };
    use crate::protocol::{openpage_error_kind, simple_openpage_error};

    #[test]
    fn preserves_delimiters_inside_locator_values() {
        assert_eq!(
            split_locator_chain(r#"root >> child text:"A >> B" >> next [data-value="x>>y"]"#)
                .unwrap(),
            vec![
                "root",
                "child text:\"A >> B\"",
                "next [data-value=\"x>>y\"]"
            ]
        );
    }

    #[test]
    fn rejects_empty_or_unterminated_steps() {
        for chain in [
            ">> parent",
            "root >>",
            "root >> >> parent",
            "root >> text:\"unfinished",
        ] {
            assert!(
                split_locator_chain(chain).is_err(),
                "accepted malformed chain: {chain}"
            );
        }
    }

    #[test]
    fn parses_step_and_normalizes_text_shorthand() {
        let (op, locator) = parse_locator_chain_step("child text:\"Learn more\"").unwrap();
        assert_eq!(op, "child");
        assert_eq!(locator.as_deref(), Some("text=Learn more"));
        assert_eq!(
            normalize_locator_shorthand("text:'Save >> now'"),
            "text=Save >> now"
        );
    }
    #[test]
    fn accepts_matching_or_omitted_revision() {
        assert!(validate_expected_revision("click", "@e1", "r_2", Some("r_2")).is_ok());
        assert!(validate_expected_revision("click", "@e1", "r_2", None).is_ok());
    }

    #[test]
    fn rejects_stale_revision_with_resnapshot_contract() {
        let error = validate_expected_revision("fill", "@e1", "r_2", Some("r_1"))
            .expect_err("stale revision must fail");
        assert_eq!(openpage_error_kind(&error), "stale_ref");

        let payload = simple_openpage_error(&error);
        assert_eq!(payload["error"]["retryable"], true);
        assert_eq!(payload["error"]["suggested_action"], "re-snapshot");
        assert_eq!(payload["error"]["current_revision"], "r_2");
        assert_eq!(payload["error"]["expected_revision"], "r_1");
        assert_eq!(payload["error"]["operation"], "fill");
        assert_eq!(payload["error"]["locator"], "@e1");
    }
    #[test]
    fn revision_bumps_only_for_page_state_changes() {
        for operation in [
            "page.goto",
            "page.reload",
            "element.click",
            "element.input",
            "page.run_js",
            "frame.switch",
        ] {
            assert!(operation_bumps_revision(operation), "{operation}");
        }
        for operation in [
            "page.snapshot",
            "page.url",
            "page.title",
            "page.screenshot",
            "element.attr",
        ] {
            assert!(!operation_bumps_revision(operation), "{operation}");
        }
    }
}
