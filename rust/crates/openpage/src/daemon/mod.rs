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

use crate::browser::{BrowserTabReference, DownloadFileExistsMode, LoadMode};
use crate::config::{ConfigValueSource, RuntimeOverrides, load_resolved_config, openpage_home};
use crate::download::DownloadMission;
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::{ActionsDragData, PageNavigationSnapshot};
use crate::protocol::{Request, Response};
use crate::recorder::{RecordedAction, RecordedFlow, RecordedTarget, RecordedValue};
use crate::session::SessionOptions;
use crate::settings::wait_timeout_result;
use crate::webpage::{WebElement, WebFrame, WebMode, WebPage};

pub mod client;
mod operations;

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
struct ServeRuntime {
    webpages: HashMap<String, ServeWebPage>,
    next_webpage_id: u64,
    shutdown: bool,
}

struct ServeWebPage {
    page: WebPage,
    active_frame_target: Option<String>,
    refs: RefCell<RefRegistry>,
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

impl ServeWebPage {
    fn new(page: WebPage) -> Self {
        Self {
            page,
            active_frame_target: None,
            refs: RefCell::new(RefRegistry::default()),
            navigation_baseline: None,
            navigation_tickets: HashMap::new(),
            next_navigation_ticket_id: 1,
        }
    }

    fn current_frame(&self) -> OpenPageResult<Option<WebFrame>> {
        match self.active_frame_target.as_deref() {
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
        self.page = self.page.with_target(target_id)?;
        self.clear_frame();
        self.refs.borrow_mut().clear();
        Ok(())
    }

    fn current_target_id(&self) -> String {
        self.page.target_id()
    }

    fn find(&self, locator: &str) -> OpenPageResult<WebElement> {
        if let Some(ref_id) = parse_ref(locator) {
            return self.find_ref(ref_id);
        }
        self.find_raw(locator)
    }

    fn find_raw(&self, locator: &str) -> OpenPageResult<WebElement> {
        match self.current_frame()? {
            Some(frame) => frame.find(locator),
            None => self.page.find(locator),
        }
    }

    fn find_all(&self, locator: &str) -> OpenPageResult<Vec<WebElement>> {
        if let Some(ref_id) = parse_ref(locator) {
            return Ok(vec![self.find_ref(ref_id)?]);
        }
        self.find_all_raw(locator)
    }

    fn find_all_raw(&self, locator: &str) -> OpenPageResult<Vec<WebElement>> {
        match self.current_frame()? {
            Some(frame) => frame.find_all(locator),
            None => self.page.find_all(locator),
        }
    }

    fn find_ref(&self, ref_id: &str) -> OpenPageResult<WebElement> {
        let target = self.refs.borrow().get(ref_id).cloned().ok_or_else(|| {
            OpenPageError::ElementNotFound(format!(
                "unknown ref @{ref_id}; run `openpage snapshot` or `openpage find` to refresh refs"
            ))
        })?;
        if target.target_id != self.current_target_id()
            || target.frame_target != self.active_frame_target
        {
            return Err(OpenPageError::ElementNotFound(format!(
                "ref @{ref_id} belongs to another page or frame; run `openpage snapshot` again"
            )));
        }
        if let Some(element) = self.find_ref_by_locator_hints(&target) {
            self.refresh_ref_target(ref_id, &element)?;
            return Ok(element);
        }
        if let Some(element) = self.reresolve_ref_target(&target)? {
            self.refresh_ref_target(ref_id, &element)?;
            return Ok(element);
        }
        Err(OpenPageError::ElementNotFound(format!(
            "ref @{ref_id} is stale and could not be re-resolved; run `openpage snapshot` again"
        )))
    }

    fn register_element(&self, element: &WebElement) -> OpenPageResult<String> {
        let css_path = element.css_path().ok().filter(|value| !value.is_empty());
        let xpath = element.xpath().ok().filter(|value| !value.is_empty());
        if css_path.is_none() && xpath.is_none() {
            return Err(OpenPageError::ElementNotFound(
                "element has no stable locator hints".to_string(),
            ));
        }
        let tag = element.tag().ok();
        let attrs = element.attrs().ok().map(compact_element_attrs);
        let text = element
            .text()
            .ok()
            .flatten()
            .map(|value| clip_agent_text(&value, 120));
        let role = tag
            .as_deref()
            .zip(attrs.as_ref())
            .map(|(tag, attrs)| element_role(tag, attrs));
        let name = attrs
            .as_ref()
            .and_then(|attrs| element_name(text.as_deref(), attrs));
        Ok(self.refs.borrow_mut().register(RefTarget {
            target_id: self.current_target_id(),
            frame_target: self.active_frame_target.clone(),
            css_path,
            xpath,
            role,
            tag,
            name,
            text,
        }))
    }

    fn find_ref_by_locator_hints(&self, target: &RefTarget) -> Option<WebElement> {
        if let Some(css_path) = target.css_path.as_deref().filter(|value| !value.is_empty())
            && let Ok(element) = self.find_raw(&format!("css:{css_path}"))
        {
            return Some(element);
        }
        if let Some(xpath) = target.xpath.as_deref().filter(|value| !value.is_empty())
            && let Ok(element) = self.find_raw(&format!("xpath:{xpath}"))
        {
            return Some(element);
        }
        None
    }

    fn reresolve_ref_target(&self, target: &RefTarget) -> OpenPageResult<Option<WebElement>> {
        let mut queries = Vec::new();
        for value in [target.name.as_deref(), target.text.as_deref()] {
            let Some(value) = value
                .map(normalize_agent_text)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if !queries.contains(&value) {
                queries.push(value);
            }
        }

        for query in queries {
            let locator = format!("text={query}");
            let elements = match self.find_all_raw(&locator) {
                Ok(elements) => elements,
                Err(_) => continue,
            };
            let mut matches = Vec::new();
            for element in elements {
                if candidate_matches_ref_target(&element, target)? {
                    matches.push(element);
                }
            }
            if matches.len() == 1 {
                return Ok(matches.into_iter().next());
            }
        }

        Ok(None)
    }

    fn refresh_ref_target(&self, ref_id: &str, element: &WebElement) -> OpenPageResult<()> {
        let css_path = element.css_path().ok().filter(|value| !value.is_empty());
        let xpath = element.xpath().ok().filter(|value| !value.is_empty());
        let tag = element.tag().ok();
        let attrs = element.attrs().ok().map(compact_element_attrs);
        let text = element
            .text()
            .ok()
            .flatten()
            .map(|value| clip_agent_text(&value, 120));
        let role = tag
            .as_deref()
            .zip(attrs.as_ref())
            .map(|(tag, attrs)| element_role(tag, attrs));
        let name = attrs
            .as_ref()
            .and_then(|attrs| element_name(text.as_deref(), attrs));

        self.refs.borrow_mut().register_as(
            ref_id.to_string(),
            RefTarget {
                target_id: self.current_target_id(),
                frame_target: self.active_frame_target.clone(),
                css_path,
                xpath,
                role,
                tag,
                name,
                text,
            },
        );
        Ok(())
    }

    fn register_snapshot_entries(&self, entries: &mut [Value]) {
        self.refs.borrow_mut().clear();
        let target_id = self.current_target_id();
        let frame_target = self.active_frame_target.clone();
        for entry in entries {
            let Some(obj) = entry.as_object_mut() else {
                continue;
            };
            let Some(ref_id) = obj.get("ref").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            let css_path = obj
                .remove("_cssPath")
                .and_then(|value| value.as_str().map(ToString::to_string))
                .filter(|value| !value.is_empty());
            let xpath = obj
                .remove("_xpath")
                .and_then(|value| value.as_str().map(ToString::to_string))
                .filter(|value| !value.is_empty());
            self.refs.borrow_mut().register_as(
                ref_id,
                RefTarget {
                    target_id: target_id.clone(),
                    frame_target: frame_target.clone(),
                    css_path,
                    xpath,
                    role: obj
                        .get("role")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    tag: obj
                        .get("tag")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    name: obj
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    text: obj
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                },
            );
        }
    }

    fn element_payload(&self, element: WebElement) -> OpenPageResult<Value> {
        let ref_id = self.register_element(&element)?;
        web_element_to_json(element, Some(ref_id))
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

#[derive(Default)]
struct RefRegistry {
    next_id: u64,
    refs: HashMap<String, RefTarget>,
    by_key: HashMap<String, String>,
}

#[derive(Clone)]
struct RefTarget {
    target_id: String,
    frame_target: Option<String>,
    css_path: Option<String>,
    xpath: Option<String>,
    role: Option<String>,
    tag: Option<String>,
    name: Option<String>,
    text: Option<String>,
}

impl RefRegistry {
    fn clear(&mut self) {
        self.next_id = 0;
        self.refs.clear();
        self.by_key.clear();
    }

    fn get(&self, ref_id: &str) -> Option<&RefTarget> {
        self.refs.get(ref_id)
    }

    fn register(&mut self, target: RefTarget) -> String {
        let key = target.key();
        if let Some(ref_id) = self.by_key.get(&key) {
            self.refs.insert(ref_id.clone(), target);
            return ref_id.clone();
        }
        self.next_id += 1;
        let ref_id = format!("e{}", self.next_id);
        self.register_as(ref_id.clone(), target);
        ref_id
    }

    fn register_as(&mut self, ref_id: String, target: RefTarget) {
        if let Some(number) = ref_id
            .strip_prefix('e')
            .and_then(|value| value.parse::<u64>().ok())
        {
            self.next_id = self.next_id.max(number);
        }
        self.by_key.insert(target.key(), ref_id.clone());
        self.refs.insert(ref_id, target);
    }
}

impl RefTarget {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.target_id,
            self.frame_target.as_deref().unwrap_or(""),
            self.css_path.as_deref().unwrap_or(""),
            self.xpath.as_deref().unwrap_or(""),
            self.role.as_deref().unwrap_or(""),
            self.tag.as_deref().unwrap_or(""),
            self.name.as_deref().unwrap_or(""),
            self.text.as_deref().unwrap_or("")
        )
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

fn collect_window_infos(
    page: &WebPage,
    current_target: &str,
) -> OpenPageResult<Vec<ServeWindowInfo>> {
    let mut windows: Vec<ServeWindowInfo> = Vec::new();
    let mut indices = HashMap::<i64, usize>::new();

    for tab in page.tab_infos()? {
        let tab_page = page.with_target(&tab.target_id)?;
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

fn window_list_payload(page: &WebPage, current_target: &str) -> OpenPageResult<Value> {
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

fn apply_session_default_user_data_dir(
    launch: &mut crate::browser::LaunchOptions,
    user_data_dir_source: ConfigValueSource,
    session: &str,
    explicit_user_data_dir: bool,
) -> OpenPageResult<()> {
    if explicit_user_data_dir || user_data_dir_source != ConfigValueSource::BuiltInDefault {
        return Ok(());
    }

    let session = session.trim();
    if session.is_empty() {
        return Ok(());
    }

    launch.user_data_dir = Some(openpage_home()?.join("profiles").join(session));
    Ok(())
}

fn apply_runtime_default_debugger_port(
    launch: &mut crate::browser::LaunchOptions,
    debugger_source: ConfigValueSource,
    explicit_local_port: Option<u16>,
) {
    if debugger_source == ConfigValueSource::BuiltInDefault && explicit_local_port.is_none() {
        launch.set_local_port(0);
    }
}

fn dispatch_webpage(state: &mut ServeWebPage, op: &str, params: &Value) -> OpenPageResult<Value> {
    if op != "wait.navigation" {
        state.discard_stale_navigation_baseline();
    }
    let page = state.page.clone();
    match op {
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
        "webpage.back" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({"back": page.back(1)?, "navigation_token": navigation_token});
            state.clear_navigation_baseline();
            Ok(result)
        }
        "webpage.forward" => {
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
        "webpage.reload" => {
            let navigation_token = state.record_navigation_baseline();
            page.refresh(optional_bool(params, "ignore_cache").unwrap_or(false))?;
            state.wait_for_doc_loaded(optional_u64(params, "timeout_ms").unwrap_or(10_000))?;
            state.clear_navigation_baseline();
            Ok(json!({"reloaded": true, "navigation_token": navigation_token}))
        }
        "webpage.stop_loading" => {
            page.stop_loading()?;
            Ok(json!({"stopped_loading": true}))
        }
        "webpage.get" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({
                "loaded": page.get(required_str(params, "url")?)?,
                "navigation_token": navigation_token,
            });
            state.clear_navigation_baseline();
            Ok(result)
        }
        "webpage.post" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({
                "loaded": page.post(required_str(params, "url")?)?,
                "navigation_token": navigation_token,
            });
            state.clear_navigation_baseline();
            Ok(result)
        }
        "webpage.post_json" => {
            let navigation_token = state.record_navigation_baseline();
            let result = json!({
                "loaded": page.post_json(required_str(params, "url")?, params.get("payload").cloned())?,
                "navigation_token": navigation_token,
            });
            state.clear_navigation_baseline();
            Ok(result)
        }
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
        "webpage.snapshot" => snapshot_payload(state, params),
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
            let remaining_tabs = page.tab_infos()?;
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
        "webpage.window_state" | "window.state" => Ok(json!({"state": page.window_state()?})),
        "webpage.window_size" | "window.size" => {
            let (width, height) = page.window_size()?;
            Ok(json!({"width": width, "height": height}))
        }
        "webpage.window_location" | "window.location" => {
            let (left, top) = page.window_location()?;
            Ok(json!({"left": left, "top": top}))
        }
        "webpage.zoom_get" | "zoom.get" => Ok(json!({"factor": page.zoom_factor()?})),
        "webpage.zoom_set" | "zoom.set" => {
            page.set_zoom_factor(required_f64(params, "factor")?)?;
            Ok(json!({"factor": page.zoom_factor()?}))
        }
        "webpage.zoom_reset" | "zoom.reset" => {
            page.reset_zoom_factor()?;
            Ok(json!({"factor": page.zoom_factor()?}))
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
        "webpage.scroll_position" | "page.scroll_position" => {
            let (x, y) = page.scroll_position()?;
            Ok(json!({"x": x, "y": y}))
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
        "webpage.save" | "page.save" => {
            let mut path = std::path::PathBuf::from(required_str(params, "path")?);
            if path.extension().is_none() {
                path.set_extension("mhtml");
            }
            page.save(Some(path.as_path()), None, false)?;
            Ok(json!({"saved": true, "path": path}))
        }
        "webpage.screenshot" | "page.screenshot" => {
            page.save_screenshot(
                required_str(params, "path")?,
                optional_bool(params, "full_page").unwrap_or(false),
            )?;
            Ok(json!({"saved": true}))
        }
        "webpage.active_element" => Ok(json!({
            "element": state
                .active_element()?
                .map(|element| state.element_payload(element))
                .transpose()?
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
            let remaining_tabs = page.tab_infos()?;
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
                return Ok(json!({"switched": true, "frame": "main"}));
            }
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
        "webpage.find" => {
            let element = state.find(&required_locator_string(params)?)?;
            Ok(state.element_payload(element)?)
        }
        "webpage.find_all" => {
            let elements = state.find_all(&required_locator_string(params)?)?;
            let payloads = elements
                .into_iter()
                .map(|element| state.element_payload(element))
                .collect::<OpenPageResult<Vec<_>>>()?;
            Ok(json!({"elements": payloads}))
        }
        "webpage.locate" => locate_chain_payload(state, required_str(params, "chain")?),
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
        "webpage.ele.has_rect" | "element.has_rect" => Ok(json!({
            "has_rect": state.find(&required_locator_string(params)?)?.has_rect()?
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
        "webpage.ele.value" | "element.value" => Ok(payload_with_origin(
            "value",
            json!(state.find(&required_locator_string(params)?)?.value()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.raw_text" | "element.raw_text" => Ok(payload_with_origin(
            "raw_text",
            json!(state.find(&required_locator_string(params)?)?.raw_text()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.link" | "element.link" => Ok(payload_with_origin(
            "link",
            json!(state.find(&required_locator_string(params)?)?.link()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.child_count" | "element.child_count" => Ok(payload_with_origin(
            "child_count",
            json!(
                state
                    .find(&required_locator_string(params)?)?
                    .child_count()?
            ),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.css_path" | "element.css_path" => Ok(payload_with_origin(
            "css_path",
            json!(state.find(&required_locator_string(params)?)?.css_path()?),
            current_page_origin(state).as_deref(),
        )),
        "webpage.ele.xpath" | "element.xpath" => Ok(payload_with_origin(
            "xpath",
            json!(state.find(&required_locator_string(params)?)?.xpath()?),
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
            let navigation_token = state.record_navigation_baseline();
            state.find(&required_locator_string(params)?)?.click()?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
        }
        "webpage.ele.click_right" | "element.click_right" => {
            state
                .find(&required_locator_string(params)?)?
                .click_right()?;
            Ok(json!({"clicked": true, "button": "right"}))
        }
        "webpage.ele.click_middle" | "element.click_middle" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .click_middle()?;
            Ok(json!({"clicked": true, "button": "middle", "navigation_token": navigation_token}))
        }
        "webpage.ele.click_multi" | "element.click_multi" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .click_multi(optional_u64(params, "count").unwrap_or(2) as u32)?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
        }
        "webpage.ele.click_at" | "element.click_at" => {
            let navigation_token = state.record_navigation_baseline();
            state.find(&required_locator_string(params)?)?.click_at(
                optional_f64(params, "x"),
                optional_f64(params, "y"),
                optional_str(params, "button").unwrap_or("left"),
                optional_u64(params, "count").unwrap_or(1) as u32,
            )?;
            Ok(json!({"clicked": true, "navigation_token": navigation_token}))
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
            let navigation_token = state.record_navigation_baseline();
            state.find(&required_locator_string(params)?)?.submit()?;
            Ok(json!({"submitted": true, "navigation_token": navigation_token}))
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
        "webpage.ele.hover_at" | "element.hover_at" => {
            state
                .find(&required_locator_string(params)?)?
                .hover_with_offset(optional_f64(params, "x"), optional_f64(params, "y"))?;
            Ok(json!({"hovered": true}))
        }
        "webpage.ele.press_key" | "element.press_key" => {
            let navigation_token = state.record_navigation_baseline();
            state
                .find(&required_locator_string(params)?)?
                .press_key(required_str(params, "key")?)?;
            Ok(json!({"pressed": true, "navigation_token": navigation_token}))
        }
        "webpage.ele.select" | "element.select" => {
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
        "webpage.ele.option_texts" | "element.option_texts" => {
            let options = state
                .find(&required_locator_string(params)?)?
                .option_texts()?;
            Ok(json!({"options": options}))
        }
        "webpage.ele.selected_option" | "element.selected_option" => {
            let option = state
                .find(&required_locator_string(params)?)?
                .selected_option()?;
            Ok(json!({"option": option}))
        }
        "webpage.ele.selected_options" | "element.selected_options" => {
            let options = state
                .find(&required_locator_string(params)?)?
                .selected_options()?;
            Ok(json!({"options": options}))
        }
        "webpage.ele.select_all_options" | "element.select_all_options" => {
            state
                .find(&required_locator_string(params)?)?
                .select_all()?;
            Ok(json!({"selected_all": true}))
        }
        "webpage.ele.clear_selected_options" | "element.clear_selected_options" => {
            state
                .find(&required_locator_string(params)?)?
                .clear_selected()?;
            Ok(json!({"cleared": true}))
        }
        "webpage.ele.invert_selected_options" | "element.invert_selected_options" => {
            state
                .find(&required_locator_string(params)?)?
                .invert_selected()?;
            Ok(json!({"inverted": true}))
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
                    let (target_id, url) = match new_page {
                        BrowserTabReference::Page(page) => (page.target_id(), json!(page.url()?)),
                        BrowserTabReference::WebPage(page) => {
                            (page.target_id(), json!(page.url()?))
                        }
                        BrowserTabReference::Id(id) => (id, Value::Null),
                    };
                    state.switch_target(&target_id)?;
                    Ok(json!({
                        "created": true,
                        "switched": true,
                        "target_id": target_id,
                        "url": url,
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
        "webpage.ele.scroll_position" | "element.scroll_position" => {
            let position = state
                .find(&required_locator_string(params)?)?
                .rect_scroll_position()?;
            Ok(json!({
                "x": position.map(|(x, _)| x),
                "y": position.map(|(_, y)| y),
            }))
        }
        "webpage.ele.scroll" | "element.scroll" => {
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
        "webpage.clear_finished_downloads" | "downloads.clear" => Ok(json!({
            "removed": page.clear_finished_downloads()?
        })),
        "webpage.cancel_download" => {
            page.cancel_download(required_str(params, "guid")?)?;
            Ok(json!({"cancelled": true}))
        }
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
            "ready": wait_for_deleted(
                state,
                &required_locator_string(params)?,
                optional_u64(params, "timeout_ms").unwrap_or(10_000),
            )?
        })),
        "wait.ele_clickable" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_clickable(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_has_rect" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_has_rect(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_covered" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_covered(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_not_covered" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_not_covered(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_stop_moving" => Ok(json!({
            "ready": state.find(&required_locator_string(params)?)?
                .wait_until_stop_moving(optional_u64(params, "timeout_ms").unwrap_or(10_000))?
        })),
        "wait.ele_disabled_or_deleted" => Ok(json!({
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

fn session_options_from_request(
    params: &Value,
    mut session: SessionOptions,
) -> OpenPageResult<SessionOptions> {
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
    let normalized = normalize_locator_shorthand(locator);
    if normalized == locator {
        Cow::Borrowed(locator)
    } else {
        Cow::Owned(normalized)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSnapshotMode {
    Interactive,
    Semantic,
    All,
}

impl AgentSnapshotMode {
    fn parse(value: Option<&str>) -> OpenPageResult<Self> {
        match value.unwrap_or("interactive") {
            "interactive" => Ok(Self::Interactive),
            "semantic" => Ok(Self::Semantic),
            "all" => Ok(Self::All),
            other => Err(OpenPageError::UnsupportedOperation(format!(
                "unsupported snapshot mode: {other}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Semantic => "semantic",
            Self::All => "all",
        }
    }

    fn default_depth(self) -> usize {
        match self {
            Self::Interactive => 10,
            Self::Semantic => 8,
            Self::All => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSnapshotFormat {
    Text,
    Json,
}

impl AgentSnapshotFormat {
    fn parse(value: Option<&str>) -> OpenPageResult<Self> {
        match value.unwrap_or("text") {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(OpenPageError::UnsupportedOperation(format!(
                "unsupported snapshot format: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
struct AgentSnapshotOptions {
    mode: AgentSnapshotMode,
    format: AgentSnapshotFormat,
    raw: bool,
    depth: usize,
    selector: Option<String>,
}

impl AgentSnapshotOptions {
    fn from_params(params: &Value) -> OpenPageResult<Self> {
        let mode = AgentSnapshotMode::parse(optional_str(params, "mode"))?;
        let format = AgentSnapshotFormat::parse(optional_str(params, "format"))?;
        let depth = optional_u64(params, "depth")
            .map(|value| value as usize)
            .unwrap_or_else(|| mode.default_depth());

        Ok(Self {
            mode,
            format,
            raw: optional_bool(params, "raw").unwrap_or(false),
            depth,
            selector: optional_string(params, "selector"),
        })
    }
}

fn agent_snapshot_script(options: &AgentSnapshotOptions) -> OpenPageResult<String> {
    let options_json = serde_json::to_string(&json!({
        "mode": options.mode.as_str(),
        "depth": options.depth,
        "selector": options.selector,
        "maxEntries": 200,
    }))
    .map_err(|err| OpenPageError::Serialization(err.to_string()))?;

    Ok(format!(
        r#"
        (() => {{
            const options = {options_json};
            const interactiveTags = new Set(['a', 'button', 'input', 'textarea', 'select', 'option', 'summary']);
            const interactiveRoles = new Set(['button', 'link', 'checkbox', 'radio', 'switch', 'tab', 'menuitem', 'option', 'textbox', 'combobox', 'searchbox']);
            const semanticTags = new Set(['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'main', 'nav', 'article', 'section', 'aside', 'label']);
            const semanticRoles = new Set(['heading', 'main', 'navigation', 'article', 'region', 'cell', 'gridcell', 'columnheader', 'rowheader', 'listitem']);
            const cleanText = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const clipText = (value, limit = 80) => cleanText(value).slice(0, limit);
            const cssEscape = (value) => {{
                if (globalThis.CSS && typeof globalThis.CSS.escape === 'function') return globalThis.CSS.escape(value);
                return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => `\\${{ch}}`);
            }};
            const roleOf = (el) => {{
                const explicit = cleanText(el.getAttribute('role')).toLowerCase();
                if (explicit) return explicit;
                const tag = el.tagName.toLowerCase();
                if (tag === 'a' && el.hasAttribute('href')) return 'link';
                if (tag === 'button') return 'button';
                if (tag === 'textarea') return 'textbox';
                if (tag === 'select') return 'combobox';
                if (tag === 'option') return 'option';
                if (tag === 'input') {{
                    const type = cleanText(el.getAttribute('type')).toLowerCase();
                    if (type === 'checkbox') return 'checkbox';
                    if (type === 'radio') return 'radio';
                    if (type === 'button' || type === 'submit' || type === 'reset') return 'button';
                    if (type === 'search') return 'searchbox';
                    return 'textbox';
                }}
                if (/^h[1-6]$/.test(tag)) return 'heading';
                return tag;
            }};
            const labelText = (el) => {{
                if (!el.labels || el.labels.length === 0) return '';
                return clipText(Array.from(el.labels)
                    .map(label => label.innerText || label.textContent || '')
                    .join(' '));
            }};
            const accessibleName = (el) => {{
                const aria = clipText(el.getAttribute('aria-label') || '');
                if (aria) return aria;
                const title = clipText(el.getAttribute('title') || '');
                if (title) return title;
                const alt = clipText(el.getAttribute('alt') || '');
                if (alt) return alt;
                const label = labelText(el);
                if (label) return label;
                const value = clipText(el.getAttribute('value') || '');
                if (value && ['input', 'option'].includes(el.tagName.toLowerCase())) return value;
                return clipText(el.innerText || el.textContent || '');
            }};
            const isVisible = (el) => {{
                const rect = el.getBoundingClientRect();
                if (rect.width === 0 || rect.height === 0) return false;
                const style = getComputedStyle(el);
                return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity || 1) !== 0;
            }};
            const isInteractive = (el) => {{
                const tag = el.tagName.toLowerCase();
                if (interactiveTags.has(tag)) return true;
                if (el.onclick || el.hasAttribute('onclick')) return true;
                if (el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1') return true;
                if (getComputedStyle(el).cursor === 'pointer') return true;
                if (el.isContentEditable) return true;
                return interactiveRoles.has(roleOf(el));
            }};
            const isSemantic = (el) => {{
                const tag = el.tagName.toLowerCase();
                if (semanticTags.has(tag)) return true;
                return semanticRoles.has(roleOf(el));
            }};
            const includeElement = (el) => {{
                if (!(el instanceof HTMLElement) || !isVisible(el)) return false;
                if (options.mode === 'interactive') return isInteractive(el);
                if (options.mode === 'semantic') return isInteractive(el) || isSemantic(el);
                return isInteractive(el) || isSemantic(el) || !!accessibleName(el);
            }};
            const nearestHeading = (el) => {{
                let node = el.previousElementSibling;
                while (node) {{
                    if (/^H[1-6]$/.test(node.tagName)) return clipText(node.innerText || node.textContent || '');
                    node = node.previousElementSibling;
                }}
                const parent = el.parentElement;
                if (!parent) return '';
                const heading = parent.querySelector('h1,h2,h3,h4,h5,h6');
                return heading ? clipText(heading.innerText || heading.textContent || '') : '';
            }};
            const cssPathOf = (el) => {{
                if (!(el instanceof Element)) return '';
                const parts = [];
                let node = el;
                while (node && node.nodeType === Node.ELEMENT_NODE) {{
                    const tag = node.tagName.toLowerCase();
                    if (node.id) {{
                        parts.unshift(`${{tag}}#${{cssEscape(node.id)}}`);
                        break;
                    }}
                    let nth = 1;
                    let sib = node;
                    while ((sib = sib.previousElementSibling)) nth += 1;
                    parts.unshift(`${{tag}}:nth-child(${{nth}})`);
                    node = node.parentElement;
                }}
                return parts.join(' > ');
            }};
            const xpathOf = (el) => {{
                if (!(el instanceof Element)) return '';
                const parts = [];
                let node = el;
                while (node && node.nodeType === Node.ELEMENT_NODE) {{
                    const tag = node.tagName.toLowerCase();
                    let index = 1;
                    let sib = node;
                    while ((sib = sib.previousElementSibling)) {{
                        if (sib.tagName.toLowerCase() === tag) index += 1;
                    }}
                    parts.unshift(`${{tag}}[${{index}}]`);
                    node = node.parentElement;
                }}
                return '/' + parts.join('/');
            }};
            const root = options.selector ? document.querySelector(options.selector) : document.body;
            if (!root) return {{ entries: [], truncated: false, error: options.selector ? `selector not found: ${{options.selector}}` : null, options }};
            const snapshot = [];
            const visit = (el, depth) => {{
                if (!el || snapshot.length >= options.maxEntries || depth > options.depth) return;
                if (includeElement(el)) snapshot.push({{ el, depth }});
                Array.from(el.children || []).forEach(child => visit(child, depth + 1));
            }};
            visit(root, 0);
            const entries = [];
            snapshot.forEach((item, i) => {{
                const el = item.el;
                const ref = 'e' + (i + 1);
                const attrs = {{}};
                for (const attr of ['id', 'name', 'type', 'placeholder', 'href', 'role', 'aria-label', 'title', 'alt', 'value']) {{
                    if (!el.hasAttribute(attr)) continue;
                    const value = cleanText(el.getAttribute(attr));
                    if (value) attrs[attr] = value;
                }}
                const rect = el.getBoundingClientRect();
                const entry = {{
                    ref,
                    role: roleOf(el),
                    tag: el.tagName.toLowerCase(),
                    name: accessibleName(el),
                    text: clipText(el.innerText || el.textContent || ''),
                    attrs,
                    depth: item.depth,
                    _cssPath: cssPathOf(el),
                    _xpath: xpathOf(el),
                    state: {{
                        visible: true,
                        disabled: !!el.disabled,
                        checked: !!el.checked,
                        selected: !!el.selected,
                        focused: document.activeElement === el,
                        inViewport: rect.bottom > 0 && rect.right > 0 && rect.top < window.innerHeight && rect.left < window.innerWidth,
                    }},
                }};
                const label = labelText(el);
                if (label) entry.label = label;
                const heading = nearestHeading(el);
                if (heading && heading !== entry.name && heading !== entry.text) entry.context = heading;
                entries.push(entry);
            }});
            return {{
                entries,
                truncated: snapshot.length >= options.maxEntries,
                error: null,
                options,
            }};
        }})()
    "#
    ))
}

fn snapshot_payload(state: &mut ServeWebPage, params: &Value) -> OpenPageResult<Value> {
    let options = AgentSnapshotOptions::from_params(params)?;
    let snapshot = state.run_js(&agent_snapshot_script(&options)?)?;

    let origin = current_page_origin(state);
    let title = current_page_title(state);
    let mut entries = snapshot
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    state.register_snapshot_entries(&mut entries);

    let mut payload = payload_object(
        "text",
        Value::String(format_snapshot_text(
            &entries,
            title.as_deref(),
            origin.as_deref(),
        )),
        origin.as_deref(),
        title.as_deref(),
    );
    payload.insert("refs".to_string(), Value::Object(snapshot_refs(&entries)));
    payload.insert("count".to_string(), json!(entries.len()));
    payload.insert("mode".to_string(), json!(options.mode.as_str()));
    payload.insert("depth".to_string(), json!(options.depth));
    if let Some(selector) = options.selector {
        payload.insert("selector".to_string(), json!(selector));
    }
    if snapshot
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        payload.insert("truncated".to_string(), json!(true));
    }
    if let Some(error) = snapshot.get("error").and_then(Value::as_str) {
        if !error.is_empty() {
            payload.insert("warning".to_string(), json!(error));
        }
    }
    if options.raw || options.format == AgentSnapshotFormat::Json {
        payload.insert("snapshot".to_string(), Value::Array(entries));
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
        let role = obj.get("role").and_then(Value::as_str).unwrap_or("element");
        let tag = obj.get("tag").and_then(Value::as_str).unwrap_or("unknown");
        let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
        let text = obj.get("text").and_then(Value::as_str).unwrap_or("");
        let attrs = obj.get("attrs").and_then(Value::as_object);
        let label = obj.get("label").and_then(Value::as_str).unwrap_or("");
        let context = obj.get("context").and_then(Value::as_str).unwrap_or("");
        let state = obj.get("state").and_then(Value::as_object);
        let disabled = state
            .and_then(|state| state.get("disabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let checked = state
            .and_then(|state| state.get("checked"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let selected = state
            .and_then(|state| state.get("selected"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let focused = state
            .and_then(|state| state.get("focused"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let in_viewport = state
            .and_then(|state| state.get("inViewport"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let indent = obj
            .get("depth")
            .and_then(Value::as_u64)
            .map(|depth| "  ".repeat(depth.min(6) as usize))
            .unwrap_or_default();
        let display = if !name.is_empty() { name } else { text };
        let mut line = format!("{indent}@{ref_id} {role} [{tag}]");
        if !display.is_empty() {
            line.push(' ');
            line.push('"');
            line.push_str(&escape_snapshot_value(display));
            line.push('"');
        }
        if !label.is_empty() {
            line.push(' ');
            line.push_str("label=\"");
            line.push_str(&escape_snapshot_value(label));
            line.push('"');
        }

        if let Some(attrs) = attrs {
            for key in [
                "type",
                "placeholder",
                "href",
                "role",
                "aria-label",
                "alt",
                "title",
                "value",
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
        if checked {
            line.push_str(" checked");
        }
        if selected {
            line.push_str(" selected");
        }
        if disabled {
            line.push_str(" disabled");
        }
        if focused {
            line.push_str(" focused");
        }
        if in_viewport {
            line.push_str(" in_viewport");
        }
        if !context.is_empty() {
            line.push_str(" context=\"");
            line.push_str(&escape_snapshot_value(context));
            line.push('"');
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
        if let Some(role) = obj.get("role").and_then(Value::as_str) {
            ref_obj.insert("role".to_string(), Value::String(role.to_string()));
        }
        if let Some(tag) = obj.get("tag").and_then(Value::as_str) {
            ref_obj.insert("tag".to_string(), Value::String(tag.to_string()));
        }
        if let Some(name) = obj.get("name").and_then(Value::as_str) {
            ref_obj.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(text) = obj.get("text").and_then(Value::as_str) {
            ref_obj.insert("text".to_string(), Value::String(text.to_string()));
        }
        if let Some(label) = obj.get("label").and_then(Value::as_str) {
            ref_obj.insert("label".to_string(), Value::String(label.to_string()));
        }
        if let Some(attrs) = obj.get("attrs").and_then(Value::as_object) {
            ref_obj.insert("attrs".to_string(), Value::Object(attrs.clone()));
        }
        if let Some(state) = obj.get("state").and_then(Value::as_object) {
            ref_obj.insert("state".to_string(), Value::Object(state.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn format_snapshot_text_includes_title_origin_refs_and_attrs() {
        let entries = vec![
            json!({
                "ref": "e1",
                "role": "button",
                "tag": "button",
                "name": "Go",
                "text": "Go",
                "attrs": {"id": "go"},
                "state": {"inViewport": true}
            }),
            json!({
                "ref": "e2",
                "role": "textbox",
                "tag": "input",
                "name": "Email",
                "text": "",
                "label": "Email",
                "attrs": {"placeholder": "Email", "type": "text"},
                "state": {"disabled": true}
            }),
            json!({
                "ref": "e3",
                "role": "checkbox",
                "tag": "input",
                "text": "",
                "attrs": {"type": "checkbox"},
                "state": {"checked": true}
            }),
        ];

        let text = format_snapshot_text(&entries, Some("Example"), Some("https://example.com"));
        assert!(text.contains("Page: Example"));
        assert!(text.contains("URL: https://example.com"));
        assert!(text.contains("@e1 button [button] \"Go\" id=\"go\" in_viewport"));
        assert!(text
            .contains("@e2 textbox [input] \"Email\" label=\"Email\" type=\"text\" placeholder=\"Email\" disabled"));
        assert!(text.contains("@e3 checkbox [input] type=\"checkbox\" checked"));
    }

    #[test]
    fn snapshot_refs_builds_ref_index() {
        let entries = vec![json!({
            "ref": "e3",
            "role": "link",
            "tag": "a",
            "name": "Learn more",
            "text": "More",
            "label": "Learn more",
            "attrs": {"href": "https://example.com"},
            "state": {"selected": true}
        })];

        let refs = snapshot_refs(&entries);
        assert_eq!(refs["e3"]["role"], "link");
        assert_eq!(refs["e3"]["tag"], "a");
        assert_eq!(refs["e3"]["name"], "Learn more");
        assert_eq!(refs["e3"]["text"], "More");
        assert_eq!(refs["e3"]["label"], "Learn more");
        assert_eq!(refs["e3"]["attrs"]["href"], "https://example.com");
        assert_eq!(refs["e3"]["state"]["selected"], true);
    }

    #[test]
    fn locator_chain_step_parses_operation_and_locator() {
        let (op, locator) = parse_locator_chain_step("child text:\"Learn more\"").unwrap();
        assert_eq!(op, "child");
        assert_eq!(locator.as_deref(), Some("text=Learn more"));

        let (op, locator) = parse_locator_chain_step("parent").unwrap();
        assert_eq!(op, "parent");
        assert!(locator.is_none());
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

    #[test]
    fn ready_snapshot_settled_requires_complete_document() {
        let settled = ReadySnapshot {
            ready_state: "complete".to_string(),
            has_document: true,
            url: Some("https://example.com/next".to_string()),
        };
        assert!(settled.is_settled());

        let interactive = ReadySnapshot {
            ready_state: "interactive".to_string(),
            has_document: true,
            url: Some("https://example.com/next".to_string()),
        };
        assert!(!interactive.is_settled());

        let missing_document = ReadySnapshot {
            ready_state: "complete".to_string(),
            has_document: false,
            url: Some("https://example.com/next".to_string()),
        };
        assert!(!missing_document.is_settled());
    }

    #[test]
    fn navigation_transition_observed_requires_url_change_or_loading_state() {
        let same_complete = ReadySnapshot {
            ready_state: "complete".to_string(),
            has_document: true,
            url: Some("https://example.com/current".to_string()),
        };
        assert!(!navigation_transition_observed(
            Some("https://example.com/current"),
            &same_complete
        ));

        let same_interactive = ReadySnapshot {
            ready_state: "interactive".to_string(),
            has_document: true,
            url: Some("https://example.com/current".to_string()),
        };
        assert!(navigation_transition_observed(
            Some("https://example.com/current"),
            &same_interactive
        ));

        let changed_complete = ReadySnapshot {
            ready_state: "complete".to_string(),
            has_document: true,
            url: Some("https://example.com/next".to_string()),
        };
        assert!(navigation_transition_observed(
            Some("https://example.com/current"),
            &changed_complete
        ));

        assert!(!navigation_transition_observed(None, &same_complete));
        assert!(navigation_transition_observed(None, &same_interactive));
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

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.as_ref() {
                unsafe {
                    std::env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn session_options_from_request_uses_base_defaults_when_params_omit_fields() {
        let mut base = SessionOptions::default();
        base.set_timeout(21)
            .set_user_agent(Some("OpenPage/ServeIni".to_string()))
            .set_download_path("downloads")
            .set_retry(Some(4), Some(250));

        let options =
            session_options_from_request(&json!({}), base).expect("load session options from base");

        assert_eq!(options.timeout_secs, 21);
        assert_eq!(options.user_agent.as_deref(), Some("OpenPage/ServeIni"));
        assert_eq!(options.download_path, std::path::PathBuf::from("downloads"));
        assert_eq!(options.retry_times, 4);
        assert_eq!(options.retry_interval_millis, 250);
    }

    #[test]
    fn session_options_from_request_overrides_explicit_params() {
        let mut base = SessionOptions::default();
        base.set_timeout(21)
            .set_user_agent(Some("OpenPage/ServeIni".to_string()));

        let options = session_options_from_request(
            &json!({
                "timeout_secs": 5,
                "user_agent": "OpenPage/Request"
            }),
            base,
        )
        .expect("override session options from request");

        assert_eq!(options.timeout_secs, 5);
        assert_eq!(options.user_agent.as_deref(), Some("OpenPage/Request"));
    }

    #[test]
    fn apply_session_default_user_data_dir_scopes_builtin_default_by_session() {
        let home = make_temp_dir("session-profile");
        let _guard = EnvVarGuard::set("OPENPAGE_HOME", home.to_string_lossy().as_ref());
        let mut launch = crate::browser::LaunchOptions::default();

        apply_session_default_user_data_dir(
            &mut launch,
            ConfigValueSource::BuiltInDefault,
            "review",
            false,
        )
        .expect("apply session profile");

        assert_eq!(
            launch.user_data_dir.as_deref(),
            Some(home.join("profiles").join("review").as_path())
        );
    }

    #[test]
    fn apply_session_default_user_data_dir_preserves_explicit_source() {
        let mut launch = crate::browser::LaunchOptions::default();
        launch.user_data_dir = Some(PathBuf::from("/tmp/explicit-profile"));

        apply_session_default_user_data_dir(
            &mut launch,
            ConfigValueSource::UserConfig,
            "review",
            false,
        )
        .expect("preserve configured profile");

        assert_eq!(
            launch.user_data_dir.as_deref(),
            Some(PathBuf::from("/tmp/explicit-profile").as_path())
        );
    }

    #[test]
    fn apply_runtime_default_debugger_port_uses_dynamic_port_for_builtin_default() {
        let mut launch = crate::browser::LaunchOptions::default();
        launch.set_address("127.0.0.1:9222");
        launch.user_data_dir = Some(PathBuf::from("/tmp/session-profile"));

        apply_runtime_default_debugger_port(&mut launch, ConfigValueSource::BuiltInDefault, None);

        assert_eq!(launch.remote_debugging_port, Some(0));
        assert_eq!(launch.address.as_deref(), Some("127.0.0.1:0"));
        assert_eq!(
            launch.user_data_dir.as_deref(),
            Some(PathBuf::from("/tmp/session-profile").as_path())
        );
        assert!(!launch.auto_port);
    }

    #[test]
    fn apply_runtime_default_debugger_port_preserves_explicit_port_and_configured_source() {
        let mut explicit_port = crate::browser::LaunchOptions::default();
        explicit_port.set_address("127.0.0.1:9222");

        apply_runtime_default_debugger_port(
            &mut explicit_port,
            ConfigValueSource::BuiltInDefault,
            Some(9555),
        );

        assert_eq!(explicit_port.remote_debugging_port, Some(9222));
        assert_eq!(explicit_port.address.as_deref(), Some("127.0.0.1:9222"));

        let mut configured = crate::browser::LaunchOptions::default();
        configured.set_address("127.0.0.1:9444");

        apply_runtime_default_debugger_port(&mut configured, ConfigValueSource::UserConfig, None);

        assert_eq!(configured.remote_debugging_port, Some(9444));
        assert_eq!(configured.address.as_deref(), Some("127.0.0.1:9444"));
    }

    #[test]
    fn handle_client_keeps_follow_up_ndjson_lines_on_same_connection() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let address = listener.local_addr().expect("listener addr");

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            handle_client(&mut stream, Rc::new(RefCell::new(ServeRuntime::default())))
                .expect("handle client");
        });

        let mut client = TcpStream::connect(address).expect("connect test client");
        client
            .write_all(
                br#"{"id":"1","op":"webpage.url","target":"missing"}
{"id":"2","op":"webpage.title","target":"missing"}
"#,
            )
            .expect("write requests");
        client
            .shutdown(Shutdown::Write)
            .expect("shutdown write half");

        let mut raw = String::new();
        client
            .read_to_string(&mut raw)
            .expect("read daemon responses");
        server.join().expect("join server thread");

        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(
            lines.len(),
            2,
            "expected one response per NDJSON line: {raw}"
        );

        let first: Value = serde_json::from_str(lines[0]).expect("parse first response");
        let second: Value = serde_json::from_str(lines[1]).expect("parse second response");

        assert_eq!(first["id"], "1");
        assert_eq!(second["id"], "2");
        assert_eq!(first["ok"], false);
        assert_eq!(second["ok"], false);
        assert_eq!(first["error"]["kind"], "browser_operation");
        assert_eq!(second["error"]["kind"], "browser_operation");
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

fn locate_chain_payload(state: &mut ServeWebPage, chain: &str) -> OpenPageResult<Value> {
    let (element, steps) = resolve_locator_chain(state, chain)?;
    let mut payload = state.element_payload(element)?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("chain".to_string(), Value::String(chain.to_string()));
        obj.insert("steps".to_string(), json!(steps));
    }
    Ok(payload)
}

fn resolve_locator_chain(
    state: &ServeWebPage,
    chain: &str,
) -> OpenPageResult<(WebElement, Vec<String>)> {
    let parts = chain
        .split(">>")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
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
        element = match op {
            "parent" => match locator {
                Some(locator) => element.parent_with(locator.as_str(), 1)?,
                None => element.parent()?,
            },
            "child" => match locator {
                Some(locator) => element.child_with(Some(locator.as_str()), 1)?,
                None => element.child()?,
            },
            "prev" | "previous" => match locator {
                Some(locator) => element.prev_with(Some(locator.as_str()), 1)?,
                None => element.prev()?,
            },
            "next" => match locator {
                Some(locator) => element.next_with(Some(locator.as_str()), 1)?,
                None => element.next()?,
            },
            "before" => match locator {
                Some(locator) => element.before_with(Some(locator.as_str()), 1)?,
                None => element.before()?,
            },
            "after" => match locator {
                Some(locator) => element.after_with(Some(locator.as_str()), 1)?,
                None => element.after()?,
            },
            other => {
                return Err(OpenPageError::UnsupportedLocator(format!(
                    "unsupported locator chain step: {other}"
                )));
            }
        };
        steps.push(part.to_string());
    }

    Ok((element, steps))
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

fn web_element_to_json(element: WebElement, ref_id: Option<String>) -> OpenPageResult<Value> {
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

fn candidate_matches_ref_target(element: &WebElement, target: &RefTarget) -> OpenPageResult<bool> {
    let tag = element.tag()?;
    if let Some(expected) = target.tag.as_deref()
        && tag != expected
    {
        return Ok(false);
    }

    let text = element.text()?.unwrap_or_default();
    let normalized_text = normalize_agent_text(&text);
    let attrs = compact_element_attrs(element.attrs()?);
    let role = element_role(&tag, &attrs);
    if let Some(expected) = target.role.as_deref()
        && role != expected
    {
        return Ok(false);
    }

    let name = element_name(Some(&normalized_text), &attrs).unwrap_or_default();
    if let Some(expected) = target.name.as_deref() {
        let expected = normalize_agent_text(expected);
        if !expected.is_empty() && name != expected && !name.starts_with(&expected) {
            return Ok(false);
        }
    }
    if let Some(expected) = target.text.as_deref() {
        let expected = normalize_agent_text(expected);
        if !expected.is_empty()
            && normalized_text != expected
            && !normalized_text.starts_with(&expected)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn wait_for_ready_payload(state: &ServeWebPage, timeout_ms: u64) -> OpenPageResult<Value> {
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
    state: &mut ServeWebPage,
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

fn ready_snapshot(state: &ServeWebPage) -> OpenPageResult<ReadySnapshot> {
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

fn wait_for_deleted(state: &ServeWebPage, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
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
            return wait_timeout_result("wait.ele_deleted", timeout_ms);
        }
        sleep(Duration::from_millis(50));
    }
}

fn wait_for_disabled_or_deleted(
    state: &ServeWebPage,
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
            return wait_timeout_result("wait.ele_disabled_or_deleted", timeout_ms);
        }
        sleep(Duration::from_millis(50));
    }
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

fn replay_recorded_flow(state: &mut ServeWebPage, params: &Value) -> OpenPageResult<Value> {
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
    let find = |target: &RecordedTarget| match state.page.find(&target.locator) {
        Ok(element) => Ok(element),
        Err(primary_error) => {
            for locator in &target.fallbacks {
                if let Ok(element) = state.page.find(locator) {
                    return Ok(element);
                }
            }
            Err(primary_error)
        }
    };
    let mut replayed = 0;
    for step in flow.steps {
        match step.action {
            RecordedAction::Goto { url } => {
                state.page.goto(&url)?;
            }
            RecordedAction::Click { target } => {
                find(&target)?.click()?;
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
