use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex as StdMutex, MutexGuard as StdMutexGuard};
use std::thread::sleep;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::browser::{
    Bounds, GetWindowForTargetParams, GetWindowForTargetReturns, PermissionDescriptor,
    PermissionSetting, SetWindowBoundsParams, WindowState,
};
use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, DescribeNodeParams, GetFrameOwnerParams, RemoveAttributeParams,
    RequestNodeParams, ResolveNodeParams, SetAttributeValueParams,
};
use chromiumoxide::cdp::browser_protocol::emulation::SetUserAgentOverrideParams;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    InsertTextParams, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::network::{
    BlockPattern, ClearBrowserCacheParams, ClearBrowserCookiesParams, CookieParam, CookieSameSite,
    DeleteCookiesParams, EnableParams as NetworkEnableParams, Headers, SetBlockedUrLsParams,
    SetCookiesParams, SetExtraHttpHeadersParams,
};
use chromiumoxide::cdp::browser_protocol::page::SetLifecycleEventsEnabledParams;
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureSnapshotFormat,
    CaptureSnapshotParams, EventFrameNavigated, EventLifecycleEvent, EventNavigatedWithinDocument,
    Frame as CdpPageFrame, FrameId, FrameTree, GetNavigationHistoryParams,
    NavigateToHistoryEntryParams, PrintToPdfParams, ReloadParams,
    RemoveScriptToEvaluateOnNewDocumentParams, StopLoadingParams, Viewport as ClipViewport,
};
use chromiumoxide::cdp::browser_protocol::page::{GetFrameTreeParams, GetLayoutMetricsParams};
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, ExecutionContextId};
use chromiumoxide::keys;
use chromiumoxide::layout::Point;
use chromiumoxide::page::{Page as OxPage, ScreenshotParams};
use chromiumoxide::{Command, Method};
use futures::StreamExt;
use publicsuffix::{List as PublicSuffixList, Psl};
use serde::Serialize;
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio::time::timeout as tokio_timeout;
use url::Url;

use crate::alert::AlertTracker;
use crate::browser::{
    Browser, BrowserTabReference, BrowserTabSelector, BrowserTabTargetsInput, BrowserTabTypeInput,
    DownloadFileExistsMode, LoadMode,
};
use crate::console::Console;
use crate::download::DownloadMission;
use crate::element::{
    Element, ElementDragTarget, ElementResource, load_javascript_source,
    resolve_javascript_timeout_ms,
};
use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfigHandle, elements_one_should_raise_when_missing,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::intercept::Interceptor;
use crate::listener::Listener;
use crate::locator::{
    Locator, LocatorBatchInput, LocatorInput, LocatorKind, LocatorMatch, collect_locator_matches,
    parse_locator_batch_input,
};
use crate::screencast::Screencast;
use crate::session::{
    CookieEntry, CookieInput, HeadersInput, SessionCookieParam, SessionElement, SessionOptions,
    SessionPage, SessionXPathResult, cookie_input_to_params_allow_missing_scope,
    cookies_from_header, parse_headers_input, snapshot_find, snapshot_find_all,
    snapshot_query_xpath, snapshot_root,
};
use crate::settings::{
    action_click_times_positive_message, action_element_missing_clickable_rect_message,
    action_element_missing_rect_location_message, action_type_interval_non_negative_message,
    action_wait_seconds_non_negative_message, browser_backed_page_only_message,
    build_file_url_failed_message, cdp_timeout_duration, clipboard_secure_context_required_message,
    component_state_lock_poisoned_message, default_none_element_runtime_config,
    drag_in_file_path_empty_message, drag_in_requires_file_path_message,
    frame_element_missing_frame_id_message, frame_element_not_found_message,
    frame_execution_context_unavailable_message, frame_html_unavailable_message,
    frame_index_must_start_message, frame_index_out_of_range_message,
    invalid_cookie_same_site_message, invalid_file_url_message, invalid_url_message,
    javascript_execution_timed_out_message, launched_browser_only_message,
    navigation_history_index_out_of_bounds_message, no_new_tab_message,
    page_connect_timed_out_message, page_operation_failed_message,
    permission_origin_required_message, permission_origin_scheme_message,
    permission_setting_invalid_message, resolved_frame_owner_missing_object_id_message,
    screenshot_clip_complete_message, screenshot_clip_order_message,
    session_backed_element_driver_target_message,
    session_backed_web_element_driver_actions_message, singleton_tab_obj_enabled,
    suffixes_list_path, timeout_duration_millis, timeout_error,
    timeout_must_be_non_negative_message, unsupported_key_message, value_did_not_return_message,
    value_pair_entry_not_number_message, value_returned_non_string_entry_message,
    wait_for_locator_timed_out_message, wait_timeout_result, zoom_factor_must_be_positive_message,
};
use crate::shadow_root::ShadowRoot;
use crate::upload::{UploadFilesInput, UploadTracker};
use crate::webpage::{WebElement, WebFrame};
use crate::window::{activate_app, set_app_visibility};

const PAGE_MARKER_ATTRIBUTE: &str = "data-openpage-page-marker";
static NEXT_PAGE_MARKER: AtomicU64 = AtomicU64::new(1);
const DEFAULT_PAGE_LOAD_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_SCRIPT_TIMEOUT_MS: u64 = 30_000;
const ACTION_MODIFIER_ALT: i64 = 1;
const ACTION_MODIFIER_CTRL: i64 = 2;
const ACTION_MODIFIER_META: i64 = 4;
const ACTION_MODIFIER_SHIFT: i64 = 8;
const PAGE_ZOOM_MANAGED_ATTRIBUTE: &str = "data-openpage-zoom-managed";
const PAGE_ZOOM_ORIGINAL_ATTRIBUTE: &str = "data-openpage-zoom-original";

fn page_operation_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::PageOperation(page_operation_failed_message(operation, &err.to_string()))
}

async fn run_page_future_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| page_operation_error(operation, err))
}

async fn run_page_lookup_future_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| {
            OpenPageError::ElementNotFound(page_operation_failed_message(
                operation,
                &err.to_string(),
            ))
        })
}

async fn register_navigation_listener_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    run_page_future_with_cdp_timeout(future, operation).await
}

#[derive(Clone, Debug)]
pub struct Page {
    runtime: Arc<Runtime>,
    inner: OxPage,
    browser: Option<Browser>,
    navigation: NavigationTracker,
    interceptor: Interceptor,
    console: Console,
    screencast: Screencast,
    alerts: AlertTracker,
    uploader: UploadTracker,
    load_mode: Arc<std::sync::Mutex<LoadMode>>,
    init_scripts: Arc<std::sync::Mutex<Vec<String>>>,
    browser_pid: Option<u32>,
    none_element_config: ElementsOneRuntimeConfigHandle,
    frame_none_element_configs:
        Arc<std::sync::Mutex<HashMap<String, ElementsOneRuntimeConfigHandle>>>,
}

#[derive(Clone)]
pub struct Frame {
    page: Page,
    frame_id: String,
    frame_element: Arc<Element>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

#[derive(Clone, Debug)]
pub struct DisconnectedPage {
    browser: Browser,
    target_id: String,
}

impl DisconnectedPage {
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Page> {
        if wait_ms > 0 {
            sleep(Duration::from_millis(wait_ms));
        }
        let browser = self.browser.reconnect()?;
        browser.get_page(&self.target_id)
    }
}

#[derive(Clone, Debug)]
pub struct DisconnectedFrame {
    page: DisconnectedPage,
    frame_dom_id: Option<String>,
    frame_dom_name: Option<String>,
    frame_xpath: Option<String>,
    frame_css_path: Option<String>,
    frame_backend_node_id: BackendNodeId,
}

#[derive(Clone, Debug, Default)]
pub struct PageNavigationSnapshot {
    pub started_seq: u64,
    pub settled_seq: u64,
    pub main_frame_id: Option<String>,
    pub current_loader_id: Option<String>,
    pub current_url: Option<String>,
}

#[derive(Debug, Default)]
struct NavigationState {
    snapshot: PageNavigationSnapshot,
    loader_started: HashMap<String, u64>,
    last_error: Option<String>,
    lifecycle_task: Option<JoinHandle<()>>,
    frame_navigated_task: Option<JoinHandle<()>>,
    same_document_task: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct NavigationShared {
    state: StdMutex<NavigationState>,
}

impl NavigationShared {
    fn new(snapshot: PageNavigationSnapshot) -> Self {
        Self {
            state: StdMutex::new(NavigationState {
                snapshot,
                ..NavigationState::default()
            }),
        }
    }
}

#[derive(Clone, Debug)]
struct NavigationTracker {
    shared: Arc<NavigationShared>,
}

impl DisconnectedFrame {
    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Frame> {
        let page = self.page.reconnect(wait_ms)?;
        if let Some(id) = self.frame_dom_id.as_deref()
            && !id.is_empty()
        {
            let locator = format!("css:#{id}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Some(name) = self.frame_dom_name.as_deref()
            && !name.is_empty()
        {
            let locator = format!(r#"css:iframe[name="{name}"],frame[name="{name}"]"#);
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Some(xpath) = self.frame_xpath.as_deref()
            && !xpath.is_empty()
        {
            let locator = format!("xpath:{xpath}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Some(css_path) = self.frame_css_path.as_deref()
            && !css_path.is_empty()
        {
            let locator = format!("css:{css_path}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }

        let frame_element = page.resolve_dom_backend_node_id(self.frame_backend_node_id)?;
        page.get_frame_context(&frame_element)
    }
}

impl NavigationTracker {
    fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        let snapshot = initial_navigation_snapshot(runtime.as_ref(), &page).unwrap_or_default();
        let shared = Arc::new(NavigationShared::new(snapshot));
        let tracker = Self {
            shared: Arc::clone(&shared),
        };

        let _ = execute_page_command_blocking(
            runtime.as_ref(),
            &page,
            SetLifecycleEventsEnabledParams::new(true),
            "Page::set_lifecycle_events_enabled()",
        );

        let lifecycle_shared = Arc::clone(&shared);
        let lifecycle_page = page.clone();
        let lifecycle_task = runtime.spawn(async move {
            let mut events = match register_navigation_listener_with_cdp_timeout(
                lifecycle_page.event_listener::<EventLifecycleEvent>(),
                "register navigation lifecycle listener",
            )
            .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_navigation_last_error(&lifecycle_shared, err.to_string());
                    return;
                }
            };

            while let Some(event) = events.next().await {
                update_navigation_from_lifecycle(&lifecycle_shared, &event);
            }
        });

        let frame_shared = Arc::clone(&shared);
        let frame_page = page.clone();
        let frame_navigated_task = runtime.spawn(async move {
            let mut events = match register_navigation_listener_with_cdp_timeout(
                frame_page.event_listener::<EventFrameNavigated>(),
                "register navigation frame listener",
            )
            .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_navigation_last_error(&frame_shared, err.to_string());
                    return;
                }
            };

            while let Some(event) = events.next().await {
                update_navigation_from_frame_navigated(&frame_shared, &event);
            }
        });

        let same_document_shared = Arc::clone(&shared);
        let same_document_page = page;
        let same_document_task = runtime.spawn(async move {
            let mut events = match register_navigation_listener_with_cdp_timeout(
                same_document_page.event_listener::<EventNavigatedWithinDocument>(),
                "register navigation same-document listener",
            )
            .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_navigation_last_error(&same_document_shared, err.to_string());
                    return;
                }
            };

            while let Some(event) = events.next().await {
                update_navigation_from_same_document(&same_document_shared, &event);
            }
        });

        if let Ok(mut state) = tracker.shared.state.lock() {
            state.lifecycle_task = Some(lifecycle_task);
            state.frame_navigated_task = Some(frame_navigated_task);
            state.same_document_task = Some(same_document_task);
        }

        tracker
    }

    fn snapshot(&self) -> OpenPageResult<PageNavigationSnapshot> {
        self.shared
            .state
            .lock()
            .map(|state| state.snapshot.clone())
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "page navigation state",
                    "页面导航状态",
                ))
            })
    }
}

fn initial_navigation_snapshot(
    runtime: &Runtime,
    page: &OxPage,
) -> OpenPageResult<PageNavigationSnapshot> {
    let tree = execute_page_command_blocking(
        runtime,
        page,
        GetFrameTreeParams::default(),
        "Page::initial_navigation_snapshot()",
    )?;
    Ok(PageNavigationSnapshot {
        main_frame_id: Some(tree.frame_tree.frame.id.as_ref().to_string()),
        current_loader_id: Some(tree.frame_tree.frame.loader_id.as_ref().to_string()),
        current_url: Some(cdp_frame_url(&tree.frame_tree.frame)),
        ..PageNavigationSnapshot::default()
    })
}

fn set_navigation_last_error(shared: &Arc<NavigationShared>, detail: String) {
    if let Ok(mut state) = shared.state.lock() {
        state.last_error = Some(detail);
    }
}

fn navigation_state_lock<'a>(
    shared: &'a Arc<NavigationShared>,
) -> Option<StdMutexGuard<'a, NavigationState>> {
    shared.state.lock().ok()
}

fn is_main_frame_event(state: &NavigationState, frame_id: &str) -> bool {
    state
        .snapshot
        .main_frame_id
        .as_deref()
        .map(|current| current == frame_id)
        .unwrap_or(true)
}

fn mark_navigation_started(
    state: &mut NavigationState,
    loader_id: Option<&str>,
    url: Option<String>,
) -> u64 {
    if let Some(loader_id) = loader_id
        && let Some(seq) = state.loader_started.get(loader_id)
    {
        if let Some(url) = url {
            state.snapshot.current_url = Some(url);
        }
        state.snapshot.current_loader_id = Some(loader_id.to_string());
        return *seq;
    }

    state.snapshot.started_seq += 1;
    let seq = state.snapshot.started_seq;
    if let Some(loader_id) = loader_id {
        state.loader_started.insert(loader_id.to_string(), seq);
        state.snapshot.current_loader_id = Some(loader_id.to_string());
    }
    if let Some(url) = url {
        state.snapshot.current_url = Some(url);
    }
    seq
}

fn mark_navigation_settled(state: &mut NavigationState, seq: u64) {
    state.snapshot.settled_seq = state.snapshot.settled_seq.max(seq);
    state.loader_started.retain(|_, value| *value > seq);
}

fn settle_loader(state: &mut NavigationState, loader_id: &str) {
    if let Some(seq) = state.loader_started.get(loader_id).copied() {
        mark_navigation_settled(state, seq);
    }
}

fn update_navigation_from_frame_navigated(
    shared: &Arc<NavigationShared>,
    event: &EventFrameNavigated,
) {
    let Some(mut state) = navigation_state_lock(shared) else {
        return;
    };
    if event.frame.parent_id.is_some() {
        return;
    }

    state.snapshot.main_frame_id = Some(event.frame.id.as_ref().to_string());
    let loader_id = event.frame.loader_id.as_ref().to_string();
    let url = cdp_frame_url(&event.frame);
    let _ = mark_navigation_started(&mut state, Some(&loader_id), Some(url));
}

fn update_navigation_from_lifecycle(shared: &Arc<NavigationShared>, event: &EventLifecycleEvent) {
    let Some(mut state) = navigation_state_lock(shared) else {
        return;
    };
    let frame_id = event.frame_id.as_ref().to_string();
    if !is_main_frame_event(&state, &frame_id) {
        return;
    }
    if state.snapshot.main_frame_id.is_none() {
        state.snapshot.main_frame_id = Some(frame_id);
    }

    let loader_id = event.loader_id.as_ref().to_string();
    match event.name.as_str() {
        "init" => {
            let _ = mark_navigation_started(&mut state, Some(&loader_id), None);
        }
        "load" => {
            settle_loader(&mut state, &loader_id);
        }
        _ => {}
    }
}

fn update_navigation_from_same_document(
    shared: &Arc<NavigationShared>,
    event: &EventNavigatedWithinDocument,
) {
    let Some(mut state) = navigation_state_lock(shared) else {
        return;
    };
    let frame_id = event.frame_id.as_ref().to_string();
    if !is_main_frame_event(&state, &frame_id) {
        return;
    }
    if state.snapshot.main_frame_id.is_none() {
        state.snapshot.main_frame_id = Some(frame_id);
    }

    let seq = mark_navigation_started(&mut state, None, Some(event.url.clone()));
    mark_navigation_settled(&mut state, seq);
}

fn cdp_frame_url(frame: &CdpPageFrame) -> String {
    match frame.url_fragment.as_deref() {
        Some(fragment) if !fragment.is_empty() => format!("{}{}", frame.url, fragment),
        _ => frame.url.clone(),
    }
}

pub struct PageScroller<'a> {
    page: &'a Page,
}

pub struct PageSetter<'a> {
    page: &'a Page,
}

pub struct PageCookieSetter<'a> {
    page: &'a Page,
}

pub struct PageWindowSetter<'a> {
    page: &'a Page,
}

pub struct PageLoadModeSetter<'a> {
    page: &'a Page,
}

pub struct FrameScroller<'a> {
    frame: &'a Frame,
}

pub struct FrameSetter<'a> {
    frame: &'a Frame,
}

pub struct FrameCookieSetter<'a> {
    frame: &'a Frame,
}

pub struct FrameStates<'a> {
    frame: &'a Frame,
}

pub struct FrameWait<'a> {
    frame: &'a Frame,
}

pub struct FrameRect<'a> {
    frame: &'a Frame,
}

pub struct Actions {
    page: Page,
    curr_x: f64,
    curr_y: f64,
    modifiers: i64,
    pressed_buttons: i64,
}

#[derive(Debug, Clone, Serialize)]
struct DispatchDragEventParams {
    #[serde(rename = "type")]
    event_type: &'static str,
    x: f64,
    y: f64,
    data: ActionsDragPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    modifiers: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ActionsDragPayload {
    items: Vec<ActionsDragItem>,
    #[serde(rename = "dragOperationsMask")]
    drag_operations_mask: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct ActionsDragItem {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
}

pub enum PageElementTarget<'a> {
    Locator(LocatorInput<'a>),
    Element(&'a Element),
    SessionElement(&'a SessionElement),
    WebElement(&'a WebElement),
    OwnedElement(Element),
    OwnedSessionElement(SessionElement),
    OwnedWebElement(WebElement),
}

pub enum ActionsTarget<'a> {
    Locator(LocatorInput<'a>),
    Element(&'a Element),
    WebElement(&'a WebElement),
    OwnedElement(Element),
    OwnedWebElement(WebElement),
    Coordinates(f64, f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionsInput<'a> {
    Single(Cow<'a, str>),
    Many(Vec<Cow<'a, str>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionsDragData<'a> {
    Files(ActionsInput<'a>),
    Text {
        text: Cow<'a, str>,
        title: Option<Cow<'a, str>>,
        base_url: Option<Cow<'a, str>>,
    },
}

#[derive(Clone)]
pub enum PageFrameTarget<'a> {
    Locator(LocatorInput<'a>),
    Index(isize),
    Element(&'a Element),
    WebElement(&'a WebElement),
    Frame(&'a Frame),
    WebFrame(&'a WebFrame),
    OwnedFrame(Frame),
    OwnedWebFrame(WebFrame),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageElementInfo {
    tag: String,
    properties: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageElementContent<'a> {
    Html(Cow<'a, str>),
    Info(PageElementInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSaveContent {
    Mhtml(String),
    Pdf(Vec<u8>),
}

enum ResolvedPageElementTarget<'a> {
    Owned(Element),
    Borrowed(&'a Element),
}

impl<'a> From<&'a str> for PageElementTarget<'a> {
    fn from(value: &'a str) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a String> for PageElementTarget<'a> {
    fn from(value: &'a String) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<(&'a str, &'a str)> for PageElementTarget<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a Element> for PageElementTarget<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Element(value)
    }
}

impl From<Element> for PageElementTarget<'_> {
    fn from(value: Element) -> Self {
        Self::OwnedElement(value)
    }
}

impl<'a> From<&'a SessionElement> for PageElementTarget<'a> {
    fn from(value: &'a SessionElement) -> Self {
        Self::SessionElement(value)
    }
}

impl From<SessionElement> for PageElementTarget<'_> {
    fn from(value: SessionElement) -> Self {
        Self::OwnedSessionElement(value)
    }
}

impl<'a> From<&'a WebElement> for PageElementTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
    }
}

impl From<WebElement> for PageElementTarget<'_> {
    fn from(value: WebElement) -> Self {
        Self::OwnedWebElement(value)
    }
}

impl<'a> From<&'a str> for ActionsTarget<'a> {
    fn from(value: &'a str) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a String> for ActionsTarget<'a> {
    fn from(value: &'a String) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<(&'a str, &'a str)> for ActionsTarget<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a Element> for ActionsTarget<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Element(value)
    }
}

impl From<Element> for ActionsTarget<'_> {
    fn from(value: Element) -> Self {
        Self::OwnedElement(value)
    }
}

impl<'a> From<&'a WebElement> for ActionsTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
    }
}

impl From<WebElement> for ActionsTarget<'_> {
    fn from(value: WebElement) -> Self {
        Self::OwnedWebElement(value)
    }
}

impl From<(i32, i32)> for ActionsTarget<'_> {
    fn from(value: (i32, i32)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(i64, i64)> for ActionsTarget<'_> {
    fn from(value: (i64, i64)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(usize, usize)> for ActionsTarget<'_> {
    fn from(value: (usize, usize)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(f64, f64)> for ActionsTarget<'_> {
    fn from(value: (f64, f64)) -> Self {
        Self::Coordinates(value.0, value.1)
    }
}

impl<'a> From<&'a str> for ActionsInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Single(Cow::Borrowed(value))
    }
}

impl<'a> ActionsDragData<'a> {
    pub fn files<I>(files: I) -> Self
    where
        I: Into<ActionsInput<'a>>,
    {
        Self::Files(files.into())
    }

    pub fn text<S>(text: S) -> Self
    where
        S: Into<Cow<'a, str>>,
    {
        Self::Text {
            text: text.into(),
            title: None,
            base_url: None,
        }
    }

    pub fn link<S, T>(text: S, title: T) -> Self
    where
        S: Into<Cow<'a, str>>,
        T: Into<Cow<'a, str>>,
    {
        Self::Text {
            text: text.into(),
            title: Some(title.into()),
            base_url: None,
        }
    }

    pub fn html<S, B>(text: S, base_url: B) -> Self
    where
        S: Into<Cow<'a, str>>,
        B: Into<Cow<'a, str>>,
    {
        Self::Text {
            text: text.into(),
            title: None,
            base_url: Some(base_url.into()),
        }
    }
}

impl Method for DispatchDragEventParams {
    fn identifier(&self) -> Cow<'static, str> {
        Cow::Borrowed("Input.dispatchDragEvent")
    }
}

impl Command for DispatchDragEventParams {
    type Response = Value;
}

impl<'a> From<&'a String> for ActionsInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Single(Cow::Borrowed(value.as_str()))
    }
}

impl From<String> for ActionsInput<'_> {
    fn from(value: String) -> Self {
        Self::Single(Cow::Owned(value))
    }
}

impl<'a> From<&'a [String]> for ActionsInput<'a> {
    fn from(value: &'a [String]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| Cow::Borrowed(item.as_str()))
                .collect(),
        )
    }
}

impl<'a> From<&'a Vec<String>> for ActionsInput<'a> {
    fn from(value: &'a Vec<String>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<Vec<String>> for ActionsInput<'_> {
    fn from(value: Vec<String>) -> Self {
        Self::Many(value.into_iter().map(Cow::Owned).collect())
    }
}

impl<'a> From<&'a [&'a str]> for ActionsInput<'a> {
    fn from(value: &'a [&'a str]) -> Self {
        Self::Many(value.iter().copied().map(Cow::Borrowed).collect())
    }
}

impl<'a> From<Vec<&'a str>> for ActionsInput<'a> {
    fn from(value: Vec<&'a str>) -> Self {
        Self::Many(value.into_iter().map(Cow::Borrowed).collect())
    }
}

impl<'a, const N: usize> From<[&'a str; N]> for ActionsInput<'a> {
    fn from(value: [&'a str; N]) -> Self {
        Self::Many(value.into_iter().map(Cow::Borrowed).collect())
    }
}

impl<'a, const N: usize> From<&'a [&'a str; N]> for ActionsInput<'a> {
    fn from(value: &'a [&'a str; N]) -> Self {
        Self::Many(value.iter().copied().map(Cow::Borrowed).collect())
    }
}

impl<'a> From<&'a str> for PageFrameTarget<'a> {
    fn from(value: &'a str) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a String> for PageFrameTarget<'a> {
    fn from(value: &'a String) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<(&'a str, &'a str)> for PageFrameTarget<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl From<usize> for PageFrameTarget<'_> {
    fn from(value: usize) -> Self {
        Self::Index(value as isize)
    }
}

impl From<isize> for PageFrameTarget<'_> {
    fn from(value: isize) -> Self {
        Self::Index(value)
    }
}

impl From<i32> for PageFrameTarget<'_> {
    fn from(value: i32) -> Self {
        Self::Index(value as isize)
    }
}

impl From<i64> for PageFrameTarget<'_> {
    fn from(value: i64) -> Self {
        Self::Index(value as isize)
    }
}

impl<'a> From<&'a Element> for PageFrameTarget<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Element(value)
    }
}

impl<'a> From<&'a WebElement> for PageFrameTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
    }
}

impl<'a> From<&'a Frame> for PageFrameTarget<'a> {
    fn from(value: &'a Frame) -> Self {
        Self::Frame(value)
    }
}

impl From<Frame> for PageFrameTarget<'_> {
    fn from(value: Frame) -> Self {
        Self::OwnedFrame(value)
    }
}

impl<'a> From<&'a WebFrame> for PageFrameTarget<'a> {
    fn from(value: &'a WebFrame) -> Self {
        Self::WebFrame(value)
    }
}

impl From<WebFrame> for PageFrameTarget<'_> {
    fn from(value: WebFrame) -> Self {
        Self::OwnedWebFrame(value)
    }
}

impl<'a> From<&'a Page> for BrowserTabSelector<'a> {
    fn from(value: &'a Page) -> Self {
        Self::Id(Cow::Owned(value.target_id()))
    }
}

impl From<Page> for BrowserTabSelector<'_> {
    fn from(value: Page) -> Self {
        Self::Id(Cow::Owned(value.target_id()))
    }
}

impl<'a> From<&'a Page> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Page) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<Page> for BrowserTabTargetsInput<'_> {
    fn from(value: Page) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl<'a> From<&'a [&'a Page]> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a [&'a Page]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| BrowserTabSelector::from(*item))
                .collect(),
        )
    }
}

impl<'a> From<&'a Vec<&'a Page>> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Vec<&'a Page>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a str> for PageElementContent<'a> {
    fn from(value: &'a str) -> Self {
        Self::Html(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for PageElementContent<'a> {
    fn from(value: &'a String) -> Self {
        Self::Html(Cow::Borrowed(value.as_str()))
    }
}

impl From<String> for PageElementContent<'_> {
    fn from(value: String) -> Self {
        Self::Html(Cow::Owned(value))
    }
}

impl<'a, T> From<T> for PageElementContent<'a>
where
    T: Into<PageElementInfo>,
{
    fn from(value: T) -> Self {
        Self::Info(value.into())
    }
}

impl PageElementInfo {
    fn from_string_pairs<K, V, I>(tag: impl Into<String>, properties: I) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
        I: IntoIterator<Item = (K, V)>,
    {
        Self {
            tag: tag.into(),
            properties: properties
                .into_iter()
                .map(|(name, value)| {
                    (
                        name.as_ref().to_string(),
                        Value::String(value.as_ref().to_string()),
                    )
                })
                .collect(),
        }
    }

    fn from_value_pairs<K, V, I>(tag: impl Into<String>, properties: I) -> Self
    where
        K: AsRef<str>,
        V: Into<Value>,
        I: IntoIterator<Item = (K, V)>,
    {
        Self {
            tag: tag.into(),
            properties: properties
                .into_iter()
                .map(|(name, value)| (name.as_ref().to_string(), value.into()))
                .collect(),
        }
    }

    fn tag(&self) -> &str {
        &self.tag
    }
}

impl<'a> From<(&'a str, &'a [(&'a str, &'a str)])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(&'a str, &'a str)])) -> Self {
        Self::from_string_pairs(value.0, value.1.iter().copied())
    }
}

impl<'a> From<(&'a str, &'a Vec<(&'a str, &'a str)>)> for PageElementInfo {
    fn from(value: (&'a str, &'a Vec<(&'a str, &'a str)>)) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, Vec<(&'a str, &'a str)>)> for PageElementInfo {
    fn from(value: (&'a str, Vec<(&'a str, &'a str)>)) -> Self {
        Self::from_string_pairs(value.0, value.1)
    }
}

impl<'a, const N: usize> From<(&'a str, [(&'a str, &'a str); N])> for PageElementInfo {
    fn from(value: (&'a str, [(&'a str, &'a str); N])) -> Self {
        Self::from_string_pairs(value.0, value.1)
    }
}

impl<'a, const N: usize> From<(&'a str, &'a [(&'a str, &'a str); N])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(&'a str, &'a str); N])) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, &'a [(String, String)])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(String, String)])) -> Self {
        Self::from_string_pairs(
            value.0,
            value
                .1
                .iter()
                .map(|(name, item)| (name.as_str(), item.as_str())),
        )
    }
}

impl<'a> From<(&'a str, &'a Vec<(String, String)>)> for PageElementInfo {
    fn from(value: (&'a str, &'a Vec<(String, String)>)) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, &'a HashMap<String, String>)> for PageElementInfo {
    fn from(value: (&'a str, &'a HashMap<String, String>)) -> Self {
        Self::from_string_pairs(
            value.0,
            value
                .1
                .iter()
                .map(|(name, item)| (name.as_str(), item.as_str())),
        )
    }
}

impl From<(String, Vec<(String, String)>)> for PageElementInfo {
    fn from(value: (String, Vec<(String, String)>)) -> Self {
        Self::from_string_pairs(
            value.0,
            value.1.into_iter().map(|(name, item)| (name, item)),
        )
    }
}

impl<'a> From<(&'a str, &'a [(&'a str, Value)])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(&'a str, Value)])) -> Self {
        Self::from_value_pairs(
            value.0,
            value.1.iter().map(|(name, item)| (*name, item.clone())),
        )
    }
}

impl<'a> From<(&'a str, &'a Vec<(&'a str, Value)>)> for PageElementInfo {
    fn from(value: (&'a str, &'a Vec<(&'a str, Value)>)) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, Vec<(&'a str, Value)>)> for PageElementInfo {
    fn from(value: (&'a str, Vec<(&'a str, Value)>)) -> Self {
        Self::from_value_pairs(value.0, value.1)
    }
}

impl<'a, const N: usize> From<(&'a str, [(&'a str, Value); N])> for PageElementInfo {
    fn from(value: (&'a str, [(&'a str, Value); N])) -> Self {
        Self::from_value_pairs(value.0, value.1)
    }
}

impl<'a, const N: usize> From<(&'a str, &'a [(&'a str, Value); N])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(&'a str, Value); N])) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, &'a [(String, Value)])> for PageElementInfo {
    fn from(value: (&'a str, &'a [(String, Value)])) -> Self {
        Self::from_value_pairs(
            value.0,
            value
                .1
                .iter()
                .map(|(name, item)| (name.as_str(), item.clone())),
        )
    }
}

impl<'a> From<(&'a str, &'a Vec<(String, Value)>)> for PageElementInfo {
    fn from(value: (&'a str, &'a Vec<(String, Value)>)) -> Self {
        Self::from((value.0, value.1.as_slice()))
    }
}

impl<'a> From<(&'a str, &'a HashMap<String, Value>)> for PageElementInfo {
    fn from(value: (&'a str, &'a HashMap<String, Value>)) -> Self {
        Self::from_value_pairs(
            value.0,
            value
                .1
                .iter()
                .map(|(name, item)| (name.as_str(), item.clone())),
        )
    }
}

impl From<(String, Vec<(String, Value)>)> for PageElementInfo {
    fn from(value: (String, Vec<(String, Value)>)) -> Self {
        Self::from_value_pairs(
            value.0,
            value.1.into_iter().map(|(name, item)| (name, item)),
        )
    }
}

impl ResolvedPageElementTarget<'_> {
    fn element(&self) -> &Element {
        match self {
            Self::Owned(element) => element,
            Self::Borrowed(element) => element,
        }
    }

    fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.element().set_attr(name, value)
    }

    fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        self.element().remove_attr(name)
    }
}

impl Frame {
    pub(crate) fn new(
        page: Page,
        frame_id: String,
        frame_element: Element,
        none_element_config: ElementsOneRuntimeConfigHandle,
    ) -> Self {
        Self {
            page,
            frame_id,
            frame_element: Arc::new(frame_element),
            none_element_config,
        }
    }

    pub fn id(&self) -> &str {
        &self.frame_id
    }

    pub fn frame_id(&self) -> &str {
        self.id()
    }

    pub fn frame_element(&self) -> &Element {
        &self.frame_element
    }

    pub fn frame_ele(&self) -> &Element {
        self.frame_element()
    }

    pub fn owner(&self) -> &Page {
        &self.page
    }

    pub fn page(&self) -> &Page {
        self.owner()
    }

    pub fn tab(&self) -> &Page {
        &self.page
    }

    pub fn tab_id(&self) -> String {
        self.page.target_id()
    }

    pub fn scroll(&self) -> FrameScroller<'_> {
        FrameScroller { frame: self }
    }

    pub fn set(&self) -> FrameSetter<'_> {
        FrameSetter { frame: self }
    }

    pub fn states(&self) -> FrameStates<'_> {
        FrameStates { frame: self }
    }

    pub fn wait(&self) -> FrameWait<'_> {
        FrameWait { frame: self }
    }

    pub fn rect(&self) -> FrameRect<'_> {
        FrameRect { frame: self }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.link()
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.frame_element.tag()
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.frame_element.attrs()
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.frame_element.attr(name)
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.frame_element.property(name)
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.text()
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.raw_text()
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.value()
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.frame_element.comments()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.frame_element.texts(text_node_only)
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        self.frame_element.src(timeout_ms, base64_to_bytes)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<PathBuf> {
        self.frame_element.save(path, name, timeout_ms, rename)
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        self.frame_element.style(name, pseudo)
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        self.frame_element.pseudo_before()
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        self.frame_element.pseudo_after()
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        self.frame_element.scroll_to_see(center)
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        self.frame_element.scroll_to_center()
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.frame_element.css_path()
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.frame_element.xpath()
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        self.find_all("xpath:./*").map(|elements| elements.len())
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame_element.sr()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame_element.shadow_root()
    }

    pub fn name(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_name_by_id(&self.frame_id)
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_url_by_id(&self.frame_id)
    }

    pub fn parent_id(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_parent_id(&self.frame_id)
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.run_js("document.title")?, "frame title")
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.page.download_path()
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.page
            .download_with_cookie_scope(url, scope_url.as_deref())
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.page
            .download_to_with_cookie_scope(url, path, scope_url.as_deref())
    }

    pub fn inner_html(&self) -> OpenPageResult<String> {
        value_as_string(
            self.run_js("document.documentElement ? document.documentElement.outerHTML : ''")?,
            "frame inner html",
        )
    }

    pub fn html(&self) -> OpenPageResult<String> {
        let tag = self.frame_element.tag()?;
        let outer_html = self
            .frame_element
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(frame_html_unavailable_message()))?;
        let inner_html = self.inner_html()?;
        Ok(compose_frame_html(&tag, &outer_html, &inner_html))
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        let script = load_javascript_source(expression)?;
        match script {
            Cow::Borrowed(expression) => self.page.evaluate_in_frame(&self.frame_id, expression),
            Cow::Owned(script) => self.run_js_with_options(&script, &[], false, None),
        }
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_with_options(script, args, as_expr, None)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.page
            .evaluate_in_frame_with_options(&self.frame_id, &expression, timeout_ms, true)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        self.run_js_loaded_with_args(script, &[], false)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_loaded_with_options(script, args, as_expr, None)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let _ = self.wait_for_doc_loaded(self.page.navigation_page_load_timeout_ms()?);
        self.run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        self.run_async_js_with_args(script, &[], false)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        self.run_async_js_with_options(script, args, as_expr, None)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.page
            .evaluate_in_frame_with_options(&self.frame_id, &expression, timeout_ms, false)
            .map(|_| ())
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        self.page.add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        self.page.remove_init_js(script_id)
    }

    pub fn refresh(&self) -> OpenPageResult<()> {
        self.refresh_with_options(false)
    }

    pub fn refresh_with_options(&self, ignore_cache: bool) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ window.location.reload({ignore_cache}); return true; }})()",
            ignore_cache = if ignore_cache { "true" } else { "false" },
        );
        self.run_js(&script).map(|_| ())
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        self.goto(url).map(|_| true)
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        let url = normalize_navigation_target(url)?;
        let old_url = self.url().ok().flatten();
        let timeout_ms = self.page.navigation_page_load_timeout_ms()?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let script = format!(
            "(() => {{ window.location.href = {url}; return true; }})()",
            url = serde_json::to_string(&url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        );
        self.run_js(&script)?;

        if self.page.load_mode_value()? == LoadMode::None {
            return Ok(());
        }

        loop {
            let current_url = self.url().ok().flatten();
            if current_url.as_deref() == Some(url.as_str())
                || (current_url.is_some() && current_url != old_url)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(page_connect_timed_out_message(&url)));
            }
            sleep(Duration::from_millis(50));
        }

        if self.wait_for_doc_loaded(remaining_timeout_ms(deadline))? {
            Ok(())
        } else {
            Err(OpenPageError::Timeout(page_connect_timed_out_message(&url)))
        }
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        let page = self.page.reconnect(wait_ms)?;
        if let Ok(Some(id)) = self.frame_element.attr("id")
            && !id.is_empty()
        {
            let locator = format!("css:#{id}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Ok(Some(name)) = self.frame_element.attr("name")
            && !name.is_empty()
        {
            let locator = format!(r#"css:iframe[name="{name}"],frame[name="{name}"]"#);
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Ok(xpath) = self.frame_element.xpath()
            && !xpath.is_empty()
        {
            let locator = format!("xpath:{xpath}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        if let Ok(css_path) = self.frame_element.css_path()
            && !css_path.is_empty()
        {
            let locator = format!("css:{css_path}");
            if let Ok(frame) = page.get_frame_context(locator.as_str()) {
                return Ok(frame);
            }
        }
        let frame_element =
            page.resolve_dom_backend_node_id(self.frame_element.backend_node_id())?;
        page.get_frame_context(&frame_element)
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedFrame> {
        let frame_dom_id = self.frame_element.attr("id")?;
        let frame_dom_name = self.frame_element.attr("name")?;
        let frame_xpath = self
            .frame_element
            .xpath()
            .ok()
            .filter(|xpath| !xpath.is_empty());
        let frame_css_path = self
            .frame_element
            .css_path()
            .ok()
            .filter(|css_path| !css_path.is_empty());
        Ok(DisconnectedFrame {
            page: self.page.disconnect()?,
            frame_dom_id,
            frame_dom_name,
            frame_xpath,
            frame_css_path,
            frame_backend_node_id: self.frame_element.backend_node_id(),
        })
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        self.frame_element.remove_attr(name)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame_element.set_attr(name, value)
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame_element.set_property(name, value)
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame_element.set_style(name, value)
    }

    pub fn click(&self) -> OpenPageResult<()> {
        self.frame_element.click()
    }

    pub fn click_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame_element
            .click_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame_element
            .click_left_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        self.frame_element
            .click_at(offset_x, offset_y, button, count)
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        self.frame_element.click_multi(times)
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        self.frame_element.click_left()
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        self.frame_element.click_right()
    }

    pub fn input<'a, I>(&self, text: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element.input(text)
    }

    pub fn input_with_options<'a, I>(&self, text: I, clear: bool, by_js: bool) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element.input_with_options(text, clear, by_js)
    }

    pub fn input_keys_with_options<'a, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element
            .input_keys_with_options(values, clear, by_js)
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        self.frame_element.press_key(key)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.frame_element.clear()
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.clear_with_mode(by_js)
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        self.frame_element.submit()
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        self.frame_element.focus()
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        self.frame_element.hover()
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        self.frame_element.hover_with_offset(offset_x, offset_y)
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame_element.drag(offset_x, offset_y, duration_secs)
    }

    pub fn drag_to<'a, T>(&self, target: T, duration_secs: f64) -> OpenPageResult<()>
    where
        T: Into<ElementDragTarget<'a>>,
    {
        self.frame_element.drag_to(target, duration_secs)
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame_element.drag_to_point(x, y, duration_secs)
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        self.frame_element.set_checked(checked)
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.check(uncheck, by_js)
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.uncheck(by_js)
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.page.set_tab_download_path(path)
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.page.set_tab_download_file_exists_mode(mode)
    }

    pub fn set_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.page.set_tab_when_download_file_exists(mode)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.page
            .set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_download_filename(rename, suffix, suffix_specified)
    }

    pub fn click_to_download<'a, L>(
        &self,
        locator: L,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_to_download(
            locator,
            save_path,
            rename,
            suffix,
            suffix_specified,
            timeout_ms,
            by_js,
            new_tab,
        )
    }

    pub fn click_to_upload<'a, 'b, L, F>(
        &self,
        locator: L,
        files: F,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorInput<'a>>,
        F: Into<UploadFilesInput<'b>>,
    {
        self.page.click_to_upload(locator, files, timeout_ms, by_js)
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_for_new_tab(locator, timeout_ms, by_js)
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_middle(locator, timeout_ms, get_tab)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.page.wait_for_upload_paths_inputted(timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.page.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.page
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn active_element(&self) -> OpenPageResult<Option<Element>> {
        let marker = next_page_marker();
        let script = format!(
            "(() => {{ \
                const active = document.activeElement; \
                if (!active || !(active instanceof Element)) return null; \
                active.setAttribute({attr}, {marker}); \
                return {marker}; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(&marker)?,
        );
        match self.run_js(&script)? {
            Value::Null => Ok(None),
            Value::String(_) => {
                let element = self.page.find(&marker_xpath(&marker))?;
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                Ok(Some(element))
            }
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "frame active element",
                "a string or null",
                "字符串或 null",
                &other.to_string(),
            ))),
        }
    }

    pub fn active_ele(&self) -> OpenPageResult<Option<Element>> {
        self.active_element()
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let marker = next_page_marker();
        let script = frame_find_script(&locator, &marker)?;
        match self.run_js(&script)? {
            Value::Null => Err(OpenPageError::ElementNotFound(
                frame_element_not_found_message(locator.raw()),
            )),
            Value::String(_) => {
                let element = self.page.find(&marker_xpath(&marker))?;
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                Ok(element)
            }
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "frame find()",
                "a string or null",
                "字符串或 null",
                &other.to_string(),
            ))),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let batch = next_page_marker();
        let script = frame_find_all_script(&locator, &batch)?;
        let markers = value_as_string_vec(self.run_js(&script)?, "frame find_all() result")?;
        let mut elements = Vec::with_capacity(markers.len());
        for marker in markers {
            let element = self.page.find(&marker_xpath(&marker))?;
            let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
            elements.push(element);
        }
        Ok(elements)
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        match &target {
            PageFrameTarget::Frame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok((*frame).clone());
            }
            PageFrameTarget::WebFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.frame().clone());
            }
            PageFrameTarget::OwnedFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.clone());
            }
            PageFrameTarget::OwnedWebFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.frame().clone());
            }
            _ => {}
        }
        self.page.frame_from_element(self.get_frame_ele(target)?)
    }

    pub fn get_frame_with_timeout<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame(target.clone()) {
                Ok(frame) => return Ok(frame),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.get_frame(index)
    }

    pub fn get_frame_by_index_with_timeout(
        &self,
        index: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<Frame> {
        self.get_frame_with_timeout(index, timeout_ms)
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.resolve_frame_target(target.into())
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_ele(target.clone()) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame element",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_ele_by_index(&self, index: usize) -> OpenPageResult<Element> {
        self.get_frame_ele(index)
    }

    pub fn get_frame_ele_by_index_with_timeout(
        &self,
        index: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<Element> {
        self.get_frame_ele_with_timeout(index, timeout_ms)
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frame_eles(locator)?
            .into_iter()
            .map(|element| self.page.frame_from_element(element))
            .collect()
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frames(Some(locator.as_str())) {
                Ok(frames) if !frames.is_empty() => return Ok(frames),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frames",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        self.find_all(locator.as_str())
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_eles(Some(locator.as_str())) {
                Ok(elements) if !elements.is_empty() => return Ok(elements),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frame elements",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.get_frame(target)
    }

    pub fn get_frame_context_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.get_frame_by_index(index)
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frames(locator)
    }

    pub fn parent(&self) -> OpenPageResult<Element> {
        self.frame_element.parent()
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<Element> {
        self.frame_element.parent_level(level)
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.parent_with(locator, index)
    }

    pub fn child(&self) -> OpenPageResult<Element> {
        self.frame_element.child()
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.child_with(locator, index)
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.children()
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.children_with(locator)
    }

    pub fn prev(&self) -> OpenPageResult<Element> {
        self.frame_element.prev()
    }

    pub fn prev_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.prev_with(locator, index)
    }

    pub fn next(&self) -> OpenPageResult<Element> {
        self.frame_element.next()
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.next_with(locator, index)
    }

    pub fn before(&self) -> OpenPageResult<Element> {
        self.frame_element.before()
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.before_with(locator, index)
    }

    pub fn after(&self) -> OpenPageResult<Element> {
        self.frame_element.after()
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.after_with(locator, index)
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.prevs()
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.prevs_with(locator)
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.nexts()
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.nexts_with(locator)
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.befores()
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.befores_with(locator)
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.afters()
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.afters_with(locator)
    }

    pub fn over(&self) -> OpenPageResult<Option<Element>> {
        self.frame_element.over()
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<Element>> {
        self.frame_element.over_with_timeout(timeout_ms)
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.offset(locator, x, y, timeout_ms)
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.east(locator, pixels, index)
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.south(locator, pixels, index)
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.west(locator, pixels, index)
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.north(locator, pixels, index)
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        self.frame_element
            .screenshot_bytes(scroll_to_center, timeout_ms)
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        self.frame_element
            .screenshot_base64(scroll_to_center, timeout_ms)
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<PathBuf> {
        self.frame_element
            .get_screenshot(path, name, scroll_to_center, timeout_ms)
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.frame_element.save_screenshot(path)
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, 0), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight / 2), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollWidth, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(0, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollTo({x}, {y}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {}), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {pixels}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({}, 0), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({pixels}, 0), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        value_as_f64_pair(
            self.run_js(
                "[document.documentElement.scrollLeft, document.documentElement.scrollTop]",
            )?,
            "frame scroll position",
        )
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { const rect = document.documentElement.getBoundingClientRect(); return [rect.left, rect.top]; })()",
            )?,
            "frame viewport location",
        )
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_screen_location()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_size()
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_location()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { \
                    const doc = document.documentElement; \
                    const body = document.body; \
                    const width = Math.max(doc ? doc.scrollWidth : 0, body ? body.scrollWidth : 0); \
                    const height = Math.max(doc ? doc.scrollHeight : 0, body ? body.scrollHeight : 0); \
                    return [width, height]; \
                })()",
            )?,
            "frame size",
        )
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        let Some((left, top)) = self.viewport_location()? else {
            return Ok(None);
        };
        let Some((width, height)) = self.viewport_size()? else {
            return Ok(None);
        };
        Ok(Some([
            (left, top),
            (left + width, top),
            (left + width, top + height),
            (left, top + height),
        ]))
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        let Some((left, top)) = self.location()? else {
            return Ok(None);
        };
        let Some((width, height)) = self.size()? else {
            return Ok(None);
        };
        Ok(Some([
            (left, top),
            (left + width, top),
            (left + width, top + height),
            (left, top + height),
        ]))
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.run_js("document.readyState")?, "frame ready state")
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(self.ready_state()?.as_deref() != Some("complete"))
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.frame_element.is_alive()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.frame_element.is_displayed()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.frame_element.is_enabled()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.frame_element.has_rect()
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame_element.is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame_element.is_whole_in_viewport()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.frame_element.is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.frame_element.is_clickable()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.page.has_alert()
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            if self.ready_state()?.as_deref() == Some("complete") {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_displayed(timeout_ms)
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_hidden(timeout_ms)
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_enabled(timeout_ms)
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_disabled(timeout_ms)
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_deleted(timeout_ms)
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_clickable(timeout_ms)
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_has_rect(timeout_ms)
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_covered(timeout_ms)
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_not_covered(timeout_ms)
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element
            .wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let Some(mut size) = self.frame_element.rect_size()? else {
            return Ok(false);
        };
        let Some(mut location) = self.frame_element.rect_location()? else {
            return Ok(false);
        };
        while Instant::now() < deadline {
            sleep(Duration::from_millis(100));
            let Some(next_size) = self.frame_element.rect_size()? else {
                return Ok(false);
            };
            let Some(next_location) = self.frame_element.rect_location()? else {
                return Ok(false);
            };
            if next_size == size && next_location == location {
                return Ok(true);
            }
            size = next_size;
            location = next_location;
        }
        Ok(false)
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        snapshot_root(&self.inner_html()?)
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find(&self.inner_html()?, locator.raw())
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find_all(&self.inner_html()?, locator.raw())
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        snapshot_query_xpath(&self.inner_html()?, expression)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<Element>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn listener(&self) -> Listener {
        Listener::new_for_frame(
            Arc::clone(&self.page.runtime),
            self.page.inner.clone(),
            self.frame_id.clone(),
        )
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn console(&self) -> Console {
        self.page.console()
    }

    fn resolve_frame_target<'a>(&self, target: PageFrameTarget<'a>) -> OpenPageResult<Element> {
        match target {
            PageFrameTarget::Locator(locator) => {
                let locator = frame_locator_input(locator)?;
                self.find(locator.as_str())
            }
            PageFrameTarget::Index(index) => self.frame_element_by_index(index),
            PageFrameTarget::Element(element) => {
                find_frame_element_from_object(&self.page, element)
            }
            PageFrameTarget::WebElement(element) => match element {
                WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                    find_frame_element_from_object(&self.page, element)
                }
                WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                    session_backed_element_driver_target_message(
                        "WebElement",
                        "frame frame",
                        "frame 元素定位",
                    ),
                )),
            },
            PageFrameTarget::Frame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::WebFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::OwnedFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::OwnedWebFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
        }
    }

    fn frame_element_by_index(&self, index: isize) -> OpenPageResult<Element> {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                frame_index_must_start_message(),
            ));
        }
        let frames = self.get_frame_eles(None::<&str>)?;
        let resolved_index = if index > 0 {
            (index as usize).checked_sub(1)
        } else {
            frames.len().checked_sub(index.unsigned_abs())
        };
        resolved_index
            .and_then(|resolved_index| frames.into_iter().nth(resolved_index))
            .ok_or_else(|| OpenPageError::ElementNotFound(frame_index_out_of_range_message(index)))
    }
}

impl FrameScroller<'_> {
    pub fn to_top(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_top()
    }

    pub fn to_bottom(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_bottom()
    }

    pub fn to_half(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_half()
    }

    pub fn to_rightmost(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_rightmost()
    }

    pub fn to_leftmost(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_leftmost()
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.frame.scroll_to_location(x, y)
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_up(pixels)
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_down(pixels)
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_left(pixels)
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_right(pixels)
    }

    pub fn to_see(&self, element: &Element, center: Option<bool>) -> OpenPageResult<()> {
        element.scroll_to_see(center)
    }

    pub fn to_center(&self, element: &Element) -> OpenPageResult<()> {
        element.scroll_to_center()
    }
}

impl PageScroller<'_> {
    pub fn to_top(&self) -> OpenPageResult<()> {
        self.page.scroll_to_top()
    }

    pub fn to_bottom(&self) -> OpenPageResult<()> {
        self.page.scroll_to_bottom()
    }

    pub fn to_half(&self) -> OpenPageResult<()> {
        self.page.scroll_to_half()
    }

    pub fn to_rightmost(&self) -> OpenPageResult<()> {
        self.page.scroll_to_rightmost()
    }

    pub fn to_leftmost(&self) -> OpenPageResult<()> {
        self.page.scroll_to_leftmost()
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.page.scroll_to_location(x, y)
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_up(pixels)
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_down(pixels)
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_left(pixels)
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<()> {
        self.page.scroll_right(pixels)
    }
}

impl PageSetter<'_> {
    pub fn window(&self) -> PageWindowSetter<'_> {
        PageWindowSetter { page: self.page }
    }

    pub fn cookie(&self) -> PageCookieSetter<'_> {
        PageCookieSetter { page: self.page }
    }

    pub fn load_mode(&self) -> PageLoadModeSetter<'_> {
        PageLoadModeSetter { page: self.page }
    }

    pub fn blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.page.set_blocked_urls(patterns)
    }

    pub fn headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        self.page.set_headers(headers)
    }

    pub fn user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        self.page.set_user_agent(user_agent, platform)
    }

    pub fn session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.page.set_session_storage(item, value)
    }

    pub fn local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.page.set_local_storage(item, value)
    }

    pub fn auto_handle_alert(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.set_auto_alert_action(accept, prompt_text)
    }

    pub fn cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }

    pub fn download_path(&self, path: &str) -> OpenPageResult<()> {
        self.page.set_download_path(path)
    }

    pub fn download_file_exists(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()> {
        self.page.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.page.when_download_file_exists(mode)
    }

    pub fn download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page
            .set_download_file_name(rename, suffix, suffix.is_some())
    }

    pub fn upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_files(files)
    }

    pub fn upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_paths(files)
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        self.page.activate()
    }

    pub fn retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        self.page.set_retry(retry_times, retry_interval_secs)
    }

    pub fn retry_times(&self, times: usize) -> OpenPageResult<()> {
        self.page.set_retry(Some(times), None)
    }

    pub fn retry_interval(&self, interval_secs: f64) -> OpenPageResult<()> {
        self.page.set_retry(None, Some(interval_secs))
    }

    pub fn timeout(&self, timeout_secs: f64) -> OpenPageResult<()> {
        self.page.set_timeouts(Some(timeout_secs), None, None)
    }

    pub fn timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        self.page
            .set_timeouts(base_secs, page_load_secs, script_secs)
    }
}

impl PageCookieSetter<'_> {
    pub fn set<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn remove(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }
}

impl PageWindowSetter<'_> {
    pub fn max(&self) -> OpenPageResult<()> {
        self.page.window_max()
    }

    pub fn mini(&self) -> OpenPageResult<()> {
        self.page.window_min()
    }

    pub fn full(&self) -> OpenPageResult<()> {
        self.page.window_full()
    }

    pub fn normal(&self) -> OpenPageResult<()> {
        self.page.window_normal()
    }

    pub fn size(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        self.page.window_size_set(width, height)
    }

    pub fn location(&self, x: Option<i64>, y: Option<i64>) -> OpenPageResult<()> {
        self.page.window_location_set(x, y)
    }

    pub fn hide(&self) -> OpenPageResult<()> {
        self.page.window_hide()
    }

    pub fn show(&self) -> OpenPageResult<()> {
        self.page.window_show()
    }
}

impl PageLoadModeSetter<'_> {
    pub fn normal(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::Normal)
    }

    pub fn eager(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::Eager)
    }

    pub fn none(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(LoadMode::None)
    }
}

impl FrameSetter<'_> {
    pub fn cookie(&self) -> FrameCookieSetter<'_> {
        FrameCookieSetter { frame: self.frame }
    }

    pub fn cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.frame.set_cookies(cookies)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.frame.clear_cookies()
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.frame.remove_cookie(name, url, domain, path)
    }

    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_attr(name, value)
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame.set_property(name, value)
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_style(name, value)
    }

    pub fn upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame.set_upload_files(files)
    }

    pub fn upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame.set_upload_paths(files)
    }

    pub fn download_path(&self, path: &str) -> OpenPageResult<()> {
        self.frame.set_download_path(path)
    }

    pub fn download_file_exists(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()> {
        self.frame.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.frame.set_when_download_file_exists(mode)
    }

    pub fn download_file_name(
        &self,
        name: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.frame
            .set_download_filename(name, suffix, suffix_specified)
    }
}

impl FrameCookieSetter<'_> {
    pub fn set<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.frame.set_cookies(cookies)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.frame.clear_cookies()
    }

    pub fn remove(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.frame.remove_cookie(name, url, domain, path)
    }
}

impl FrameStates<'_> {
    pub fn is_loading(&self) -> OpenPageResult<bool> {
        self.frame.is_loading()
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.frame.is_alive()
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        self.frame.ready_state()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.frame.is_displayed()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.frame.is_enabled()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.frame.has_rect()
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame.is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame.is_whole_in_viewport()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.frame.is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.frame.is_clickable()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.frame.has_alert()
    }
}

impl FrameWait<'_> {
    pub fn doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_for_doc_loaded(timeout_ms)
    }

    pub fn download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.frame.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn downloads_done(&self, timeout_ms: u64, cancel_if_timeout: bool) -> OpenPageResult<bool> {
        self.frame
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_displayed(timeout_ms)
    }

    pub fn hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_hidden(timeout_ms)
    }

    pub fn enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_enabled(timeout_ms)
    }

    pub fn disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_disabled(timeout_ms)
    }

    pub fn deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_deleted(timeout_ms)
    }

    pub fn clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_clickable(timeout_ms)
    }

    pub fn has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_has_rect(timeout_ms)
    }

    pub fn covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_covered(timeout_ms)
    }

    pub fn not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_not_covered(timeout_ms)
    }

    pub fn disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_stop_moving(timeout_ms)
    }

    pub fn upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_for_upload_paths_inputted(timeout_ms)
    }
}

impl FrameRect<'_> {
    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.location()
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.viewport_location()
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.screen_location()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.size()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.viewport_size()
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame.corners()
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame.viewport_corners()
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        self.frame.scroll_position()
    }
}

impl Actions {
    pub fn new(page: Page) -> Self {
        Self {
            page,
            curr_x: 0.0,
            curr_y: 0.0,
            modifiers: 0,
            pressed_buttons: 0,
        }
    }

    pub fn owner(&self) -> &Page {
        &self.page
    }

    pub fn curr_x(&self) -> i64 {
        self.curr_x.round() as i64
    }

    pub fn curr_y(&self) -> i64 {
        self.curr_y.round() as i64
    }

    pub fn move_to<'a, T>(
        &mut self,
        target: T,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        let (x, y) = resolve_actions_target_point(&self.page, target.into(), offset_x, offset_y)?;
        self.move_pointer_to(x, y, duration_secs)?;
        Ok(self)
    }

    pub fn r#move(
        &mut self,
        offset_x: f64,
        offset_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self> {
        self.move_pointer_to(
            self.curr_x + offset_x,
            self.curr_y + offset_y,
            duration_secs,
        )?;
        Ok(self)
    }

    pub fn move_by(
        &mut self,
        offset_x: f64,
        offset_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self> {
        self.r#move(offset_x, offset_y, duration_secs)
    }

    pub fn up(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(0.0, -pixels, 0.5)
    }

    pub fn down(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(0.0, pixels, 0.5)
    }

    pub fn left(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(-pixels, 0.0, 0.5)
    }

    pub fn right(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(pixels, 0.0, 0.5)
    }

    pub fn click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Left, times)?;
        Ok(self)
    }

    pub fn r_click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Right, times)?;
        Ok(self)
    }

    pub fn m_click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Middle, times)?;
        Ok(self)
    }

    pub fn hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Left)?;
        Ok(self)
    }

    pub fn release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Left)?;
        Ok(self)
    }

    pub fn r_hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Right)?;
        Ok(self)
    }

    pub fn r_release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Right)?;
        Ok(self)
    }

    pub fn m_hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Middle)?;
        Ok(self)
    }

    pub fn m_release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Middle)?;
        Ok(self)
    }

    pub fn scroll<'a, T>(
        &mut self,
        delta_y: f64,
        delta_x: f64,
        on_target: Option<T>,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        let mut event = DispatchMouseEventParams::new(
            DispatchMouseEventType::MouseWheel,
            self.curr_x,
            self.curr_y,
        );
        event.buttons = Some(self.pressed_buttons);
        event.modifiers = Some(self.modifiers);
        event.delta_x = Some(delta_x);
        event.delta_y = Some(delta_y);
        self.dispatch_mouse_event(event)?;
        Ok(self)
    }

    pub fn key_down(&mut self, key: &str) -> OpenPageResult<&mut Self> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(key)))?;
        let next_modifiers = self.modifiers | action_modifier_bit(definition.key).unwrap_or(0);
        self.dispatch_key_event(action_build_key_event(&definition, next_modifiers, false))?;
        self.modifiers = next_modifiers;
        Ok(self)
    }

    pub fn key_up(&mut self, key: &str) -> OpenPageResult<&mut Self> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(key)))?;
        let next_modifiers = self.modifiers & !action_modifier_bit(definition.key).unwrap_or(0);
        self.dispatch_key_event(action_build_key_event(&definition, next_modifiers, true))?;
        self.modifiers = next_modifiers;
        Ok(self)
    }

    pub fn input<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), true, None)?;
        Ok(self)
    }

    pub fn r#type<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), false, None)?;
        Ok(self)
    }

    pub fn type_with_interval<'a, I>(
        &mut self,
        input: I,
        interval_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), false, Some(interval_secs))?;
        Ok(self)
    }

    pub fn type_keys<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.r#type(input)
    }

    pub fn type_keys_with_interval<'a, I>(
        &mut self,
        input: I,
        interval_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.type_with_interval(input, interval_secs)
    }

    pub fn wait(&mut self, second: f64, scope: Option<f64>) -> OpenPageResult<&mut Self> {
        if second.is_sign_negative() {
            return Err(OpenPageError::PageOperation(
                action_wait_seconds_non_negative_message(),
            ));
        }
        let wait_secs = match scope {
            Some(end) => action_wait_duration_secs(second, end),
            None => second,
        };
        sleep(Duration::from_secs_f64(wait_secs.max(0.0)));
        Ok(self)
    }

    pub fn drag_in<'a, T>(
        &mut self,
        target: T,
        data: ActionsDragData<'a>,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        let (x, y) = resolve_actions_target_point(&self.page, target.into(), None, None)?;
        let payload = action_drag_payload(data)?;
        self.dispatch_drag_event("dragEnter", x, y, payload.clone())?;
        self.dispatch_drag_event("dragOver", x, y, payload.clone())?;
        self.dispatch_drag_event("drop", x, y, payload)?;
        Ok(self)
    }

    fn perform_actions_input(
        &mut self,
        input: ActionsInput<'_>,
        prefer_insert_text: bool,
        interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let values = actions_input_values(input);
        if values.is_empty() {
            return Ok(());
        }

        if let Some(interval_secs) = interval_secs {
            if interval_secs.is_sign_negative() {
                return Err(OpenPageError::PageOperation(
                    action_type_interval_non_negative_message(),
                ));
            }
        }

        let mut transient_keys = Vec::new();
        let result = (|| -> OpenPageResult<()> {
            for value in values {
                if value.is_empty() {
                    continue;
                }
                if action_modifier_bit(&value).is_some() {
                    self.key_down(&value)?;
                    transient_keys.push(value);
                    continue;
                }
                let effective_modifiers = self.modifiers;
                if keys::get_key_definition(&value).is_some() {
                    let effective_value = action_effective_key_value(&value, effective_modifiers);
                    self.press_key_value(effective_value.as_ref(), effective_modifiers)?;
                    action_sleep_interval(interval_secs);
                    continue;
                }
                if effective_modifiers != 0 {
                    for ch in value.chars() {
                        let key = ch.to_string();
                        let effective_value = action_effective_key_value(&key, effective_modifiers);
                        self.press_key_value(effective_value.as_ref(), effective_modifiers)?;
                        action_sleep_interval(interval_secs);
                    }
                    continue;
                }
                if prefer_insert_text {
                    self.insert_text_value(&value)?;
                    action_sleep_interval(interval_secs);
                } else {
                    self.type_text_value(&value, interval_secs)?;
                }
            }
            Ok(())
        })();

        let mut cleanup_error = None;
        for key in transient_keys {
            if let Err(err) = self.key_up(&key) {
                if cleanup_error.is_none() {
                    cleanup_error = Some(err);
                }
            }
        }

        match (result, cleanup_error) {
            (Err(err), _) => Err(err),
            (Ok(()), Some(err)) => Err(err),
            (Ok(()), None) => Ok(()),
        }
    }

    fn move_pointer_to(
        &mut self,
        target_x: f64,
        target_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<()> {
        let path = action_move_path(self.curr_x, self.curr_y, target_x, target_y, duration_secs);
        let pause = action_move_pause(duration_secs, path.len());
        let path_len = path.len();
        for (index, point) in path.into_iter().enumerate() {
            let mut moved =
                DispatchMouseEventParams::new(DispatchMouseEventType::MouseMoved, point.x, point.y);
            moved.buttons = Some(self.pressed_buttons);
            moved.modifiers = Some(self.modifiers);
            self.dispatch_mouse_event(moved)?;
            self.curr_x = point.x;
            self.curr_y = point.y;
            if index + 1 < path_len {
                if let Some(pause) = pause {
                    sleep(pause);
                }
            }
        }
        Ok(())
    }

    fn press_button(&mut self, button: MouseButton) -> OpenPageResult<()> {
        let next_buttons = self.pressed_buttons | action_mouse_buttons(&button);
        let mut pressed = DispatchMouseEventParams::new(
            DispatchMouseEventType::MousePressed,
            self.curr_x,
            self.curr_y,
        );
        pressed.button = Some(button);
        pressed.buttons = Some(next_buttons);
        pressed.modifiers = Some(self.modifiers);
        pressed.click_count = Some(1);
        self.dispatch_mouse_event(pressed)?;
        self.pressed_buttons = next_buttons;
        Ok(())
    }

    fn release_button(&mut self, button: MouseButton) -> OpenPageResult<()> {
        let next_buttons = self.pressed_buttons & !action_mouse_buttons(&button);
        let mut released = DispatchMouseEventParams::new(
            DispatchMouseEventType::MouseReleased,
            self.curr_x,
            self.curr_y,
        );
        released.button = Some(button);
        released.buttons = Some(next_buttons);
        released.modifiers = Some(self.modifiers);
        released.click_count = Some(1);
        self.dispatch_mouse_event(released)?;
        self.pressed_buttons = next_buttons;
        Ok(())
    }

    fn dispatch_click(&mut self, button: MouseButton, times: u32) -> OpenPageResult<()> {
        if times == 0 {
            return Err(OpenPageError::PageOperation(
                action_click_times_positive_message(),
            ));
        }
        let pressed_buttons = self.pressed_buttons | action_mouse_buttons(&button);
        for click_count in 1..=times {
            let mut pressed = DispatchMouseEventParams::new(
                DispatchMouseEventType::MousePressed,
                self.curr_x,
                self.curr_y,
            );
            pressed.button = Some(button.clone());
            pressed.buttons = Some(pressed_buttons);
            pressed.modifiers = Some(self.modifiers);
            pressed.click_count = Some(click_count.into());
            self.dispatch_mouse_event(pressed)?;

            let mut released = DispatchMouseEventParams::new(
                DispatchMouseEventType::MouseReleased,
                self.curr_x,
                self.curr_y,
            );
            released.button = Some(button.clone());
            released.buttons = Some(self.pressed_buttons);
            released.modifiers = Some(self.modifiers);
            released.click_count = Some(click_count.into());
            self.dispatch_mouse_event(released)?;
        }
        Ok(())
    }

    fn dispatch_mouse_event(&self, event: DispatchMouseEventParams) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_mouse_event()",
        )?;
        Ok(())
    }

    fn dispatch_key_event(&self, event: DispatchKeyEventParams) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_key_event()",
        )?;
        Ok(())
    }

    fn dispatch_drag_event(
        &self,
        event_type: &'static str,
        x: f64,
        y: f64,
        data: ActionsDragPayload,
    ) -> OpenPageResult<()> {
        let event = DispatchDragEventParams {
            event_type,
            x,
            y,
            data,
            modifiers: Some(self.modifiers),
        };
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_drag_event()",
        )?;
        Ok(())
    }

    fn insert_text_value(&self, value: &str) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            InsertTextParams::new(value.to_string()),
            "Actions::insert_text_value()",
        )?;
        Ok(())
    }

    fn type_text_value(&self, value: &str, interval_secs: Option<f64>) -> OpenPageResult<()> {
        for ch in value.chars() {
            self.press_key_value(ch.to_string().as_str(), self.modifiers)?;
            action_sleep_interval(interval_secs);
        }
        Ok(())
    }

    fn press_key_value(&self, value: &str, modifiers: i64) -> OpenPageResult<()> {
        let definition = keys::get_key_definition(value)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(value)))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, false))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, true))
    }
}

fn browser_backed_page_method_message(method_name: &str) -> String {
    browser_backed_page_only_message(&format!("{method_name}()"))
}

impl Page {
    pub(crate) fn new(runtime: Arc<Runtime>, inner: OxPage) -> Self {
        Self::new_with_load_mode(runtime, inner, LoadMode::Normal)
    }

    pub(crate) fn new_with_load_mode(
        runtime: Arc<Runtime>,
        inner: OxPage,
        load_mode: LoadMode,
    ) -> Self {
        let navigation = NavigationTracker::new(Arc::clone(&runtime), inner.clone());
        let interceptor = Interceptor::new(Arc::clone(&runtime), inner.clone());
        let console = Console::new(Arc::clone(&runtime), inner.clone());
        let screencast = Screencast::new(Arc::clone(&runtime), inner.clone());
        let alerts = AlertTracker::new(Arc::clone(&runtime), inner.clone());
        let uploader = UploadTracker::new(Arc::clone(&runtime), inner.clone());
        Self {
            runtime,
            inner,
            browser: None,
            navigation,
            interceptor,
            console,
            screencast,
            alerts,
            uploader,
            load_mode: Arc::new(std::sync::Mutex::new(load_mode)),
            init_scripts: Arc::new(std::sync::Mutex::new(Vec::new())),
            browser_pid: None,
            none_element_config: Arc::new(std::sync::Mutex::new(
                default_none_element_runtime_config(),
            )),
            frame_none_element_configs: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn with_browser_pid(mut self, browser_pid: Option<u32>) -> Self {
        self.browser_pid = browser_pid;
        self
    }

    pub(crate) fn with_browser(mut self, browser: Browser) -> Self {
        self.browser = Some(browser);
        self
    }

    pub(crate) fn set_runtime_load_mode(&self, load_mode: LoadMode) -> OpenPageResult<()> {
        self.load_mode
            .lock()
            .map(|mut mode| *mode = load_mode)
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "page load mode",
                    "页面加载模式",
                ))
            })
    }

    pub fn browser(&self) -> Option<&Browser> {
        self.browser.as_ref()
    }

    pub fn navigation_snapshot(&self) -> OpenPageResult<PageNavigationSnapshot> {
        self.navigation.snapshot()
    }

    fn browser_backed_ref(&self, method_name: &str) -> OpenPageResult<&Browser> {
        self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_method_message(method_name))
        })
    }

    fn cloned_none_element_config(
        &self,
        handle: &ElementsOneRuntimeConfigHandle,
    ) -> OpenPageResult<ElementsOneRuntimeConfigHandle> {
        handle
            .lock()
            .map(|config| Arc::new(std::sync::Mutex::new(config.clone())))
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    fn frame_none_element_config(
        &self,
        frame_id: &str,
    ) -> OpenPageResult<ElementsOneRuntimeConfigHandle> {
        let fresh_config = self.cloned_none_element_config(&self.none_element_config)?;
        if !singleton_tab_obj_enabled() {
            return Ok(fresh_config);
        }

        self.prune_frame_none_element_configs()?;
        let mut configs = self.frame_none_element_configs.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "frame none element config cache",
                "frame 未找到元素配置缓存",
            ))
        })?;
        if let Some(config) = configs.get(frame_id) {
            return Ok(Arc::clone(config));
        }
        configs.insert(frame_id.to_string(), Arc::clone(&fresh_config));
        Ok(fresh_config)
    }

    fn prune_frame_none_element_configs(&self) -> OpenPageResult<()> {
        let live_frame_ids: HashSet<String> =
            self.download_scope_frame_ids()?.into_iter().collect();
        let mut configs = self.frame_none_element_configs.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "frame none element config cache",
                "frame 未找到元素配置缓存",
            ))
        })?;
        configs.retain(|frame_id, _| live_frame_ids.contains(frame_id));
        Ok(())
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn actions(&self) -> OpenPageResult<Actions> {
        let _ = self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?);
        Ok(Actions::new(self.clone()))
    }

    pub fn new_actions(&self) -> Actions {
        Actions::new(self.clone())
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        let (retry_times, retry_interval_millis) = self.navigation_retry_config()?;
        let load_mode = self.load_mode_value()?;
        let url = normalize_navigation_target(url)?;
        let mut last_err = None;

        for attempt in 0..=retry_times {
            match self.goto_once(&url, load_mode) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_err = Some(err);
                    if attempt == retry_times {
                        break;
                    }
                }
            }

            if retry_interval_millis > 0 {
                sleep(Duration::from_millis(retry_interval_millis));
            }
        }

        Err(last_err
            .unwrap_or_else(|| OpenPageError::Timeout(page_connect_timed_out_message(&url))))
    }

    fn goto_once(&self, url: &str, load_mode: LoadMode) -> OpenPageResult<()> {
        let supports_script_navigation = url.starts_with("http://") || url.starts_with("https://");
        let page_load_timeout_ms = self.navigation_page_load_timeout_ms()?;
        let deadline = Instant::now() + Duration::from_millis(page_load_timeout_ms.max(1));

        match load_mode {
            LoadMode::Normal => {
                self.navigate_via_cdp(&url)?;
                if self.wait_for_doc_loaded(page_load_timeout_ms)?
                    && self.wait_for_dom_ready(remaining_timeout_ms(deadline))?
                {
                    Ok(())
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::Eager if supports_script_navigation => {
                self.navigate_via_script(&url)?;
                if self.wait_for_ready_state_change(page_load_timeout_ms, true)? {
                    let _ = self.stop_loading();
                    if self.wait_for_dom_ready(remaining_timeout_ms(deadline))? {
                        Ok(())
                    } else {
                        Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                    }
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::None if supports_script_navigation => {
                self.navigate_via_script(&url)?;
                Ok(())
            }
            LoadMode::Eager => {
                self.navigate_via_cdp(&url)?;
                if self.wait_for_ready_state_change(page_load_timeout_ms, true)? {
                    Ok(())
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::None => {
                self.navigate_via_cdp(&url)?;
                Ok(())
            }
        }
    }

    fn navigation_retry_config(&self) -> OpenPageResult<(usize, u64)> {
        match &self.browser {
            Some(browser) => Ok((browser.retry_times()?, browser.retry_interval_millis()?)),
            None => Ok((0, 0)),
        }
    }

    fn navigation_page_load_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(browser.timeouts()?.page_load),
            None => Ok(DEFAULT_PAGE_LOAD_TIMEOUT_MS),
        }
    }

    fn javascript_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(browser.timeouts()?.script),
            None => Ok(DEFAULT_SCRIPT_TIMEOUT_MS),
        }
    }

    fn implicit_wait_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(resolve_implicit_wait_timeout_ms(Some(
                browser.timeouts()?.implicit_wait,
            ))),
            None => Ok(resolve_implicit_wait_timeout_ms(None)),
        }
    }

    pub fn url(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            Ok(
                run_page_future_with_cdp_timeout(self.inner.url(), "read url")
                    .await?
                    .unwrap_or_default(),
            )
        })
    }

    pub fn title(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            Ok(
                run_page_future_with_cdp_timeout(self.inner.get_title(), "read title")
                    .await?
                    .unwrap_or_default(),
            )
        })
    }

    pub fn target_id(&self) -> String {
        self.inner.target_id().as_ref().to_string()
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        self.browser_backed_ref("tabs_count")?.tabs_count()
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        self.browser_backed_ref("tab_ids")?.tab_ids()
    }

    pub fn get_tab<'a, I, T>(
        &self,
        id_or_num: Option<I>,
        title: Option<&str>,
        url: Option<&str>,
        tab_type: Option<T>,
        as_id: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>>
    where
        I: Into<BrowserTabSelector<'a>>,
        T: Into<BrowserTabTypeInput<'a>>,
    {
        self.browser_backed_ref("get_tab")?
            .get_tab(id_or_num, title, url, tab_type, as_id)
    }

    pub fn get_tabs<'a, T>(
        &self,
        title: Option<&str>,
        url: Option<&str>,
        tab_type: Option<T>,
        as_id: bool,
    ) -> OpenPageResult<Vec<BrowserTabReference>>
    where
        T: Into<BrowserTabTypeInput<'a>>,
    {
        self.browser_backed_ref("get_tabs")?
            .get_tabs(title, url, tab_type, as_id)
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser_backed_ref("latest_tab")?.latest_tab()
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> OpenPageResult<Page> {
        self.browser_backed_ref("new_tab")?
            .new_tab(url, new_window, background, new_context)
    }

    pub fn activate_tab<'a, T>(&self, target: T) -> OpenPageResult<()>
    where
        T: Into<BrowserTabSelector<'a>>,
    {
        self.browser_backed_ref("activate_tab")?
            .activate_tab(target)
    }

    pub fn close_tabs<'a, T>(&self, targets: T, others: bool) -> OpenPageResult<usize>
    where
        T: Into<BrowserTabTargetsInput<'a>>,
    {
        self.browser_backed_ref("close_tabs")?
            .close_tabs(targets, others)
    }

    pub fn close_with_options(&self, others: bool, _session: bool) -> OpenPageResult<()> {
        if others {
            self.close_tabs(self, true)?;
        } else {
            self.close_tabs(self, false)?;
        }
        Ok(())
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match &self.browser {
            Some(browser) => browser.page_download_path(&self.target_id()),
            None => Ok(None),
        }
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "download_file_exists_mode()",
            ))
        })?;
        browser.page_download_file_exists_mode(&self.target_id())
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.download_with_cookie_scope(url, Some(scope_url.as_str()))
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.download_to_with_cookie_scope(url, path, Some(scope_url.as_str()))
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.browser_pid
    }

    pub fn process_id(&self) -> Option<u32> {
        self.browser_pid()
    }

    pub fn browser_version(&self) -> OpenPageResult<String> {
        self.browser_backed_ref("browser_version")?.version()
    }

    pub fn address(&self) -> OpenPageResult<String> {
        Ok(self.browser_backed_ref("address")?.address())
    }

    pub fn quit(&self) -> OpenPageResult<()> {
        self.browser_backed_ref("quit")?.close()
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        if wait_ms > 0 {
            sleep(Duration::from_millis(wait_ms));
        }
        let browser = self.browser_backed_ref("reconnect")?.reconnect()?;
        browser.get_page(&self.target_id())
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedPage> {
        let target_id = self.target_id();
        let browser = self.browser_backed_ref("disconnect")?.clone();
        Ok(DisconnectedPage { browser, target_id })
    }

    pub fn html(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.content(), "read html").await
        })
    }

    pub fn evaluate(&self, expression: &str) -> OpenPageResult<Value> {
        let timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(run_with_timeout(
            async {
                let result = self
                    .inner
                    .evaluate(expression)
                    .await
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
                result
                    .into_value::<Value>()
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))
            },
            timeout_ms,
            javascript_execution_timed_out_message(),
        ))
    }

    fn evaluate_with_options(
        &self,
        expression: &str,
        timeout_ms: Option<u64>,
        await_promise: bool,
    ) -> OpenPageResult<Value> {
        let timeout_ms = resolve_javascript_timeout_ms(timeout_ms, self.javascript_timeout_ms()?);
        let params = EvaluateParams::builder()
            .expression(expression)
            .await_promise(await_promise)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        self.evaluate_params_with_timeout(params, timeout_ms)
    }

    fn evaluate_params_with_timeout(
        &self,
        params: EvaluateParams,
        timeout_ms: u64,
    ) -> OpenPageResult<Value> {
        self.runtime.block_on(run_with_timeout(
            async {
                let result = self
                    .inner
                    .evaluate(params)
                    .await
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
                let fallback = result.value().cloned();
                match result.into_value::<Value>() {
                    Ok(value) => Ok(value),
                    Err(_) => Ok(fallback.unwrap_or(Value::Null)),
                }
            },
            timeout_ms,
            javascript_execution_timed_out_message(),
        ))
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let javascript_timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(async {
            let element = match locator.kind() {
                LocatorKind::Css => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_element(locator.query().to_string()),
                        "find element",
                    )
                    .await?
                }
                LocatorKind::XPath => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_xpath(locator.query().to_string()),
                        "find element by xpath",
                    )
                    .await?
                }
            };
            Ok(Element::new(
                Arc::clone(&self.runtime),
                self.inner.clone(),
                self.browser.clone(),
                Some(self.uploader.clone()),
                element,
                javascript_timeout_ms,
                Arc::clone(&self.none_element_config),
            ))
        })
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let javascript_timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(async {
            let elements = match locator.kind() {
                LocatorKind::Css => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_elements(locator.query().to_string()),
                        "find elements",
                    )
                    .await?
                }
                LocatorKind::XPath => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_xpaths(locator.query().to_string()),
                        "find elements by xpath",
                    )
                    .await?
                }
            };
            Ok(elements
                .into_iter()
                .map(|element| {
                    Element::new(
                        Arc::clone(&self.runtime),
                        self.inner.clone(),
                        self.browser.clone(),
                        Some(self.uploader.clone()),
                        element,
                        javascript_timeout_ms,
                        Arc::clone(&self.none_element_config),
                    )
                })
                .collect())
        })
    }

    pub fn wait_for<'a, L>(&self, locator: L, timeout_ms: u64) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.wait_for_raw(locator.raw(), timeout_ms)
    }

    fn wait_for_raw(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<Element> {
        let start = Instant::now();
        loop {
            match self.find(locator) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if start.elapsed() >= Duration::from_millis(timeout_ms) {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            locator,
                            &err.to_string(),
                        )));
                    }
                    sleep(Duration::from_millis(100));
                }
            }
        }
    }

    pub fn wait_for_elements_loaded<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            let mut matched = 0usize;
            for locator in &locators {
                if !self.find_all(locator)?.is_empty() {
                    matched += 1;
                }
            }
            if (!any_one && matched == locators.len()) || (any_one && matched > 0) {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_elements_loaded()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_ele_displayed<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_displayed(remaining)
        })
    }

    pub fn wait_for_ele_hidden<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_hidden(remaining)
        })
    }

    pub fn wait_for_ele_enabled<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_enabled(remaining)
        })
    }

    pub fn wait_for_ele_deleted<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_deleted(remaining)
        })
    }

    pub fn wait_for_ele_clickable<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_clickable(remaining)
        })
    }

    fn wait_for_ele_state<'a, L, F>(
        &self,
        target: L,
        timeout_ms: u64,
        wait_fn: F,
    ) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
        F: FnOnce(&Element, u64) -> OpenPageResult<bool>,
    {
        match target.into() {
            PageElementTarget::Locator(locator) => {
                let locator = Locator::from_input(locator)?;
                self.wait_for_ele_state_raw(locator.raw(), timeout_ms, wait_fn)
            }
            target => wait_fn(
                resolve_page_element_target(self, target)?.element(),
                timeout_ms,
            ),
        }
    }

    fn wait_for_ele_state_raw<F>(
        &self,
        locator: &str,
        timeout_ms: u64,
        wait_fn: F,
    ) -> OpenPageResult<bool>
    where
        F: FnOnce(&Element, u64) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        let element = loop {
            match self.find(locator) {
                Ok(ele) => break ele,
                Err(_) => {
                    sleep(Duration::from_millis(50));
                    if Instant::now() >= deadline {
                        return wait_timeout_result("Page::wait_for_ele_state()", timeout_ms);
                    }
                }
            }
        };
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        wait_fn(&element, remaining.max(1))
    }

    pub fn click<'a, L>(&self, locator: L) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .click()
    }

    pub fn fill<'a, L>(&self, locator: L, text: &str) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .input(text)
    }

    pub fn text<'a, L>(&self, locator: L) -> OpenPageResult<Option<String>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .text()
    }

    pub fn attr<'a, L>(&self, locator: L, name: &str) -> OpenPageResult<Option<String>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .attr(name)
    }

    pub fn active_element(&self) -> OpenPageResult<Option<Element>> {
        let marker = next_page_marker();
        let script = format!(
            "(() => {{ \
                const active = document.activeElement; \
                if (!active) return null; \
                active.setAttribute({attr}, {marker}); \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(&marker)?,
        );
        let result = self.run_js(&script)?;
        if result.is_null() {
            return Ok(None);
        }
        let selector = marker_selector(&marker);
        let element = self.find(&selector)?;
        self.clear_page_markers(&[marker.as_str()])?;
        Ok(Some(element))
    }

    pub fn remove_element<'a, L>(&self, target: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        match target.into() {
            PageElementTarget::Locator(locator) => {
                match self.find(Locator::from_input(locator)?.raw()) {
                    Ok(element) => {
                        element.run_js("this.remove(); return true;")?;
                        Ok(true)
                    }
                    Err(OpenPageError::ElementNotFound(_)) => Ok(false),
                    Err(err) => Err(err),
                }
            }
            target => {
                resolve_page_element_target(self, target)?
                    .element()
                    .run_js("this.remove(); return true;")?;
                Ok(true)
            }
        }
    }

    pub fn remove_ele<'a, L>(&self, target: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.remove_element(target)
    }

    pub fn add_element_html<'a, 'b, I, B>(
        &self,
        html: &str,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        let insert_to = insert_to
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let before = before
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let inserted_marker = next_page_marker();
        let parent_marker = insert_to.as_ref().map(|_| next_page_marker());
        let before_marker = before.as_ref().map(|_| next_page_marker());

        if let (Some(target), Some(marker)) = (insert_to.as_ref(), parent_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }
        if let (Some(target), Some(marker)) = (before.as_ref(), before_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }

        let script = format!(
            "(() => {{ \
                const markerAttr = {attr}; \
                const insertMarker = {insert_marker}; \
                const parent = {parent}; \
                const before = {before}; \
                const template = document.createElement('template'); \
                template.innerHTML = {html}; \
                const element = template.content.firstElementChild; \
                if (!element) return null; \
                element.setAttribute(markerAttr, insertMarker); \
                if (before && before.parentNode) {{ \
                    before.parentNode.insertBefore(element, before); \
                }} else {{ \
                    (parent || document.body || document.documentElement).appendChild(element); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            insert_marker = json_string(&inserted_marker)?,
            parent = page_marker_lookup_expression(parent_marker.as_deref())?,
            before = page_marker_lookup_expression(before_marker.as_deref())?,
            html = json_string(html)?,
        );
        self.run_js(&script)?;

        let selector = marker_selector(&inserted_marker);
        let element = self.find(&selector)?;
        self.clear_page_markers(&[inserted_marker.as_str()])?;
        if let Some(target) = insert_to.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        if let Some(target) = before.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        Ok(element)
    }

    pub fn add_element<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match content.into() {
            PageElementContent::Html(html) => {
                self.add_element_html(html.as_ref(), insert_to, before)
            }
            PageElementContent::Info(info) => self.add_element_info(info, insert_to, before),
        }
    }

    pub fn add_ele<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        self.add_element(content, insert_to, before)
    }

    pub fn add_element_info<'a, 'b, I, B, H>(
        &self,
        info: H,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
        H: Into<PageElementInfo>,
    {
        let info = info.into();
        let insert_to = insert_to
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let before = before
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let inserted_marker = next_page_marker();
        let parent_marker = insert_to.as_ref().map(|_| next_page_marker());
        let before_marker = before.as_ref().map(|_| next_page_marker());
        let detached_after_lookup = insert_to.is_none() && before.is_none();

        if let (Some(target), Some(marker)) = (insert_to.as_ref(), parent_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }
        if let (Some(target), Some(marker)) = (before.as_ref(), before_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }

        let script = format!(
            "(() => {{ \
                const markerAttr = {attr}; \
                const insertMarker = {insert_marker}; \
                const parent = {parent}; \
                const before = {before}; \
                const data = {data}; \
                const element = document.createElement({tag}); \
                for (const [name, value] of Object.entries(data)) {{ \
                    if (value === null || value === undefined) continue; \
                    if (name === 'innerHTML' || name === 'innerText' || name === 'textContent') {{ \
                        element[name] = String(value); \
                        continue; \
                    }} \
                    if (name in element) {{ \
                        try {{ element[name] = value; }} catch (_) {{}} \
                    }} \
                    try {{ element.setAttribute(name, String(value)); }} catch (_) {{}} \
                }} \
                element.setAttribute(markerAttr, insertMarker); \
                if (before && before.parentNode) {{ \
                    before.parentNode.insertBefore(element, before); \
                }} else if (parent) {{ \
                    parent.appendChild(element); \
                }} else {{ \
                    const root = document.body || document.documentElement; \
                    if (!root) return null; \
                    root.appendChild(element); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            insert_marker = json_string(&inserted_marker)?,
            parent = page_marker_lookup_expression(parent_marker.as_deref())?,
            before = page_marker_lookup_expression(before_marker.as_deref())?,
            data = page_element_info_properties_json(&info)?,
            tag = json_string(info.tag())?,
        );
        self.run_js(&script)?;

        let selector = marker_selector(&inserted_marker);
        let element = self.find(&selector)?;
        if detached_after_lookup {
            element.run_js("this.remove(); return true;")?;
        }
        element.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        if let Some(target) = insert_to.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        if let Some(target) = before.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        Ok(element)
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        match &target {
            PageFrameTarget::Frame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok((*frame).clone());
            }
            PageFrameTarget::WebFrame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok(frame.frame().clone());
            }
            PageFrameTarget::OwnedFrame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok(frame.clone());
            }
            PageFrameTarget::OwnedWebFrame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok(frame.frame().clone());
            }
            _ => {}
        }
        self.frame_from_element(self.get_frame_ele(target)?)
    }

    pub fn get_frame_with_timeout<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame(target.clone()) {
                Ok(frame) => return Ok(frame),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.get_frame(index)
    }

    pub fn get_frame_by_index_with_timeout(
        &self,
        index: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<Frame> {
        self.get_frame_with_timeout(index, timeout_ms)
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        resolve_page_frame_target(self, target.into())
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_ele(target.clone()) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame element",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_ele_by_index(&self, index: usize) -> OpenPageResult<Element> {
        self.get_frame_ele(index)
    }

    pub fn get_frame_ele_by_index_with_timeout(
        &self,
        index: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<Element> {
        self.get_frame_ele_with_timeout(index, timeout_ms)
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frame_eles(locator)?
            .into_iter()
            .map(|element| self.frame_from_element(element))
            .collect()
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frames(Some(locator.as_str())) {
                Ok(frames) if !frames.is_empty() => return Ok(frames),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frames",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(optional_frame_locator_input(locator)?.as_str())?;
        let batch = next_page_marker();
        let script = frame_find_all_script(&locator, &batch)?;
        let markers = value_as_string_vec(self.run_js(&script)?, "page get_frame_eles() result")?;
        let mut elements = Vec::with_capacity(markers.len());
        for marker in markers {
            let element = self.find(&marker_xpath(&marker))?;
            let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
            elements.push(element);
        }
        Ok(elements)
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_eles(Some(locator.as_str())) {
                Ok(elements) if !elements.is_empty() => return Ok(elements),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frame elements",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.get_frame(target)
    }

    pub fn get_frame_context_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.get_frame_by_index(index)
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frames(locator)
    }

    pub fn set_blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        let patterns = actions_input_values(patterns.into());
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NetworkEnableParams::default(),
            "Page::set_blocked_urls()",
        )?;
        let params = SetBlockedUrLsParams::builder()
            .url_patterns(
                patterns
                    .iter()
                    .cloned()
                    .map(|pattern| BlockPattern::new(pattern, true)),
            )
            .build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_blocked_urls()",
        )?;
        Ok(())
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        let params = ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .full_page(full_page)
            .build();

        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.save_screenshot(params, path),
                "save screenshot",
            )
            .await?;
            Ok(())
        })
    }

    pub fn screenshot_bytes(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<Vec<u8>> {
        let params = page_screenshot_params(full_page, left_top, right_bottom)?;
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.screenshot(params), "capture screenshot")
                .await
        })
    }

    pub fn screenshot_base64(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<String> {
        Ok(BASE64_STANDARD.encode(self.screenshot_bytes(full_page, left_top, right_bottom)?))
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<PathBuf> {
        let title = self.title()?;
        let target = resolve_page_screenshot_target_path(path, name, Some(title.as_str()))?;
        let bytes = self.screenshot_bytes(full_page, left_top, right_bottom)?;
        fs::write(&target, bytes)?;
        Ok(target)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
    ) -> OpenPageResult<PageSaveContent> {
        self.save_with_options(path, name, as_pdf, None)
    }

    pub fn save_with_options(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
        pdf_options: Option<PrintToPdfParams>,
    ) -> OpenPageResult<PageSaveContent> {
        let save_target = match (path, name) {
            (None, None) => None,
            _ => Some(resolve_page_save_target_path(
                path,
                name,
                resolve_page_save_title(self, path, name)?.as_deref(),
                if as_pdf { "pdf" } else { "mhtml" },
            )?),
        };

        let content = if as_pdf {
            let pdf = self.runtime.block_on(async {
                run_page_future_with_cdp_timeout(
                    self.inner.pdf(pdf_options.unwrap_or_default()),
                    "print pdf",
                )
                .await
            })?;
            PageSaveContent::Pdf(pdf)
        } else {
            let mhtml = self.runtime.block_on(async {
                execute_page_command_async(
                    &self.inner,
                    CaptureSnapshotParams::builder()
                        .format(CaptureSnapshotFormat::Mhtml)
                        .build(),
                    "Page::save_with_options()",
                )
                .await
                .map(|result| result.data.clone())
            })?;
            PageSaveContent::Mhtml(mhtml)
        };

        if let Some(target) = save_target {
            match &content {
                PageSaveContent::Mhtml(mhtml) => fs::write(&target, mhtml.as_bytes())?,
                PageSaveContent::Pdf(pdf) => fs::write(&target, pdf)?,
            }
        }

        Ok(content)
    }

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.save_pdf(PrintToPdfParams::default(), path),
                "save pdf",
            )
            .await?;
            Ok(())
        })
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        let params = ReloadParams::builder().ignore_cache(ignore_cache).build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::refresh()",
        )?;
        Ok(())
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(-(steps as isize))
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(steps as isize)
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        let script = load_javascript_source(script)?;
        match script {
            Cow::Borrowed(script) => self.evaluate(script),
            Cow::Owned(script) => self.run_js_with_options(&script, &[], false, None),
        }
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_with_options(script, args, as_expr, None)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.evaluate_with_options(&expression, timeout_ms, true)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        self.run_js_loaded_with_args(script, &[], false)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_loaded_with_options(script, args, as_expr, None)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let _ = self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?);
        self.run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        self.run_async_js_with_args(script, &[], false)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        self.run_async_js_with_options(script, args, as_expr, None)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.evaluate_with_options(&expression, timeout_ms, false)
            .map(|_| ())
    }

    pub fn scroll(&self) -> PageScroller<'_> {
        PageScroller { page: self }
    }

    pub fn set(&self) -> PageSetter<'_> {
        PageSetter { page: self }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, 0), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight / 2), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollWidth, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(0, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollTo({x}, {y}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {}), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {pixels}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({}, 0), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({pixels}, 0), true)"
        ))
        .map(|_| ())
    }

    pub fn execute_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            command,
            "Page::execute_cdp()",
        )
    }

    pub fn run_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.execute_cdp(command)
    }

    pub fn execute_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?)?;
        self.execute_cdp(command)
    }

    pub fn run_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.execute_cdp_loaded(command)
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        #[cfg(target_os = "macos")]
        if let Some(browser_pid) = self.browser_pid {
            set_app_visibility(browser_pid, true)?;
        }
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.bring_to_front(), "bring to front").await?;
            Ok::<(), OpenPageError>(())
        })?;
        #[cfg(target_os = "macos")]
        if let Some(browser_pid) = self.browser_pid {
            activate_app(browser_pid)?;
        }
        Ok(())
    }

    pub fn window_hide(&self) -> OpenPageResult<()> {
        let Some(browser_pid) = self.browser_pid else {
            return Err(OpenPageError::UnsupportedOperation(
                launched_browser_only_message("window hide()"),
            ));
        };
        set_app_visibility(browser_pid, false)
    }

    pub fn window_show(&self) -> OpenPageResult<()> {
        let Some(browser_pid) = self.browser_pid else {
            return Err(OpenPageError::UnsupportedOperation(
                launched_browser_only_message("window show()"),
            ));
        };
        set_app_visibility(browser_pid, true)
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.uploader.set_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
    }

    pub fn set_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_path()",
            ))
        })?;
        browser.set_page_download_path(&self.target_id(), path)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.set_tab_download_path(path)
    }

    pub fn set_tab_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_file_exists_mode()",
            ))
        })?;
        browser.set_page_download_file_exists_mode(&self.target_id(), mode)
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.set_tab_download_file_exists_mode(mode)
    }

    pub fn set_tab_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_tab_download_file_exists_mode(DownloadFileExistsMode::parse(mode)?)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_tab_when_download_file_exists(mode)
    }

    pub fn set_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_filename()",
            ))
        })?;
        browser.set_page_download_filename(&self.target_id(), rename, suffix, suffix_specified)
    }

    pub fn set_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_download_filename(rename, suffix, suffix_specified)
    }

    pub fn click_to_download<'a, L>(
        &self,
        locator: L,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "click_to_download()",
            ))
        })?;
        let target_id = self.target_id();
        let previous_settings = browser.snapshot_page_download_settings(&target_id)?;
        let previous_browser_settings = browser.snapshot_browser_download_settings()?;
        let timeout_ms = timeout_ms.unwrap_or(browser.timeouts()?.implicit_wait);
        let download_started_before = browser.download_started_len()?;
        let action_result = (|| {
            if new_tab {
                let mut temp_settings = browser.snapshot_browser_download_settings()?;
                if let Some(path) = save_path {
                    temp_settings.path = Some(PathBuf::from(path));
                } else if let Some(path) = previous_settings
                    .as_ref()
                    .and_then(|settings| settings.path.clone())
                {
                    temp_settings.path = Some(path);
                } else if temp_settings.path.is_none() {
                    temp_settings.path = Some(PathBuf::from("."));
                }
                if let Some(mode) = previous_settings
                    .as_ref()
                    .and_then(|settings| settings.file_exists)
                {
                    temp_settings.file_exists = mode;
                }
                if rename.is_some() || suffix_specified {
                    temp_settings.rename = rename.map(str::to_string);
                    temp_settings.suffix = if suffix_specified {
                        Some(suffix.map(str::to_string))
                    } else {
                        None
                    };
                } else if let Some(settings) = previous_settings.as_ref() {
                    temp_settings.rename = settings.rename.clone();
                    temp_settings.suffix = settings.suffix.clone();
                }
                browser.apply_browser_download_settings(&temp_settings)?;
            } else {
                if let Some(path) = save_path {
                    self.set_tab_download_path(path)?;
                } else if self.download_path()?.is_none() {
                    self.set_tab_download_path(".")?;
                }
                if rename.is_some() || suffix_specified {
                    self.set_tab_download_filename(rename, suffix, suffix_specified)?;
                }
            }
            let element = self.wait_for(locator, timeout_ms)?;
            if !element.click_with_options(Some(by_js), Some(timeout_ms), true)? {
                return Ok(None);
            }
            if new_tab {
                browser.wait_for_download_begin_after(download_started_before, timeout_ms, false)
            } else {
                browser.wait_for_download_begin_after_in_frames(
                    download_started_before,
                    &self.download_scope_frame_ids()?,
                    timeout_ms,
                    false,
                )
            }
        })();
        let restore_result = browser.restore_page_download_settings(&target_id, previous_settings);
        let browser_restore_result =
            browser.restore_browser_download_settings(previous_browser_settings);
        match (action_result, restore_result, browser_restore_result) {
            (Ok(result), Ok(()), Ok(())) => Ok(result),
            (Err(err), _, _) => Err(err),
            (Ok(_), Err(err), _) => Err(err),
            (Ok(_), Ok(()), Err(err)) => Err(err),
        }
    }

    pub fn click_to_upload<'a, 'b, L, F>(
        &self,
        locator: L,
        files: F,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorInput<'a>>,
        F: Into<UploadFilesInput<'b>>,
    {
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        self.set_upload_files(files)?;
        let element = self.wait_for(locator, timeout_ms)?;
        if !element.click_with_options(Some(by_js), Some(timeout_ms), true)? {
            return Ok(false);
        }
        self.wait_for_upload_paths_inputted(timeout_ms)
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "click_for_new_tab()",
            ))
        })?;
        let timeout_ms = timeout_ms.unwrap_or(browser.timeouts()?.implicit_wait);
        let current_tab_id = browser.newest_tab_id()?.unwrap_or_else(|| self.target_id());
        browser.activate_tab(self.target_id().as_str())?;
        let element = self.wait_for(locator, timeout_ms)?;
        let _ = element.click_with_options(Some(by_js), Some(timeout_ms), true)?;
        let Some(target_id) = browser.wait_for_new_tab(Some(&current_tab_id), timeout_ms)? else {
            return Err(OpenPageError::PageOperation(no_new_tab_message()));
        };
        browser.wait_for_page(&target_id, timeout_ms).map(Some)
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        if get_tab && self.browser.is_none() {
            return Err(OpenPageError::UnsupportedOperation(
                browser_backed_page_only_message("click_middle(get_tab=True)"),
            ));
        }
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        let element = self.wait_for(locator, timeout_ms)?;
        let browser = self.browser.as_ref();
        let current_tab_id = match browser {
            Some(browser) => Some(browser.newest_tab_id()?.unwrap_or_else(|| self.target_id())),
            None => None,
        };
        if get_tab && let Some(browser) = browser {
            browser.activate_tab(self.target_id().as_str())?;
        }
        element.click_middle()?;
        let detect_timeout_ms = if get_tab {
            timeout_ms
        } else {
            timeout_ms.min(500)
        };
        if let Some(browser) = browser {
            if let Some(target_id) =
                browser.wait_for_new_tab(current_tab_id.as_deref(), detect_timeout_ms)?
            {
                if get_tab {
                    return browser
                        .wait_for_page(&target_id, detect_timeout_ms)
                        .map(Some);
                }
                return Ok(None);
            }
        }
        if get_tab {
            return Err(OpenPageError::PageOperation(no_new_tab_message()));
        }
        Ok(None)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.uploader.wait_until_inputted(timeout_ms)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        self.browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                    "retry_times()",
                ))
            })?
            .retry_times()
    }

    pub fn retry_interval(&self) -> OpenPageResult<f64> {
        self.browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                    "retry_interval()",
                ))
            })?
            .retry_interval_millis()
            .map(|millis| millis as f64 / 1000.0)
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message("set_retry()"))
        })?;
        browser.set_retry(
            retry_times,
            retry_interval_secs
                .map(runtime_timeout_seconds_to_millis)
                .transpose()?,
        )
    }

    pub fn timeouts(&self) -> OpenPageResult<HashMap<&'static str, f64>> {
        let timeouts = self
            .browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message("timeouts()"))
            })?
            .timeouts()?;
        Ok(HashMap::from([
            ("base", timeouts.implicit_wait as f64 / 1000.0),
            ("page_load", timeouts.page_load as f64 / 1000.0),
            ("script", timeouts.script as f64 / 1000.0),
        ]))
    }

    pub fn set_timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message("set_timeouts()"))
        })?;
        let mut timeouts = browser.timeouts()?;
        if let Some(base_secs) = base_secs {
            timeouts.implicit_wait = runtime_timeout_seconds_to_millis(base_secs)?;
        }
        if let Some(page_load_secs) = page_load_secs {
            timeouts.page_load = runtime_timeout_seconds_to_millis(page_load_secs)?;
        }
        if let Some(script_secs) = script_secs {
            timeouts.script = runtime_timeout_seconds_to_millis(script_secs)?;
        }
        browser.set_timeouts(timeouts)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self.load_mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page load mode",
                "页面加载模式",
            ))
        })? = mode;
        Ok(())
    }

    pub fn window_id(&self) -> OpenPageResult<i64> {
        Ok(*self.window_info()?.window_id.inner())
    }

    pub fn window_state(&self) -> OpenPageResult<String> {
        let info = self.window_info()?;
        Ok(info
            .bounds
            .window_state
            .map(|state| state.as_ref().to_string())
            .unwrap_or_else(|| "normal".to_string()))
    }

    pub fn window_size(&self) -> OpenPageResult<(i64, i64)> {
        let info = self.window_info()?;
        Ok((
            info.bounds.width.unwrap_or_default(),
            info.bounds.height.unwrap_or_default(),
        ))
    }

    pub fn window_location(&self) -> OpenPageResult<(i64, i64)> {
        let info = self.window_info()?;
        Ok((
            info.bounds.left.unwrap_or_default(),
            info.bounds.top.unwrap_or_default(),
        ))
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        value_as_f64_pair(
            self.run_js(
                "[document.scrollingElement ? document.scrollingElement.scrollLeft : 0, \
                  document.scrollingElement ? document.scrollingElement.scrollTop : 0]",
            )?,
            "page scroll position",
        )
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { \
                    const doc = document.documentElement; \
                    const width = Number(window.innerWidth ?? (doc ? doc.clientWidth : 0)); \
                    const height = Number(window.innerHeight ?? (doc ? doc.clientHeight : 0)); \
                    return [width, height]; \
                })()",
            )?,
            "page viewport size",
        )
    }

    pub fn window_max(&self) -> OpenPageResult<()> {
        let current = self.window_state()?;
        if current == "fullscreen" || current == "minimized" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Maximized)
                .build(),
        )
    }

    pub fn window_min(&self) -> OpenPageResult<()> {
        if self.window_state()? == "fullscreen" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Minimized)
                .build(),
        )
    }

    pub fn window_full(&self) -> OpenPageResult<()> {
        if self.window_state()? == "minimized" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Fullscreen)
                .build(),
        )
    }

    pub fn window_normal(&self) -> OpenPageResult<()> {
        self.set_window_bounds(Bounds::builder().window_state(WindowState::Normal).build())
    }

    pub fn window_size_set(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        if width.is_none() && height.is_none() {
            return Ok(());
        }
        if self.window_state()? != "normal" {
            self.window_normal()?;
        }
        let info = self.window_info()?;
        let bounds = Bounds::builder()
            .width(width.unwrap_or(info.bounds.width.unwrap_or_default()))
            .height(height.unwrap_or(info.bounds.height.unwrap_or_default()))
            .build();
        self.set_window_bounds(bounds)
    }

    pub fn window_location_set(&self, left: Option<i64>, top: Option<i64>) -> OpenPageResult<()> {
        if left.is_none() && top.is_none() {
            return Ok(());
        }
        if self.window_state()? != "normal" {
            self.window_normal()?;
        }
        let info = self.window_info()?;
        let bounds = Bounds::builder()
            .left(left.unwrap_or(info.bounds.left.unwrap_or_default()))
            .top(top.unwrap_or(info.bounds.top.unwrap_or_default()))
            .build();
        self.set_window_bounds(bounds)
    }

    pub fn zoom_factor(&self) -> OpenPageResult<f64> {
        if let Some(value) = self.managed_zoom_factor()? {
            return Ok(value);
        }
        let metrics = self.execute_cdp(GetLayoutMetricsParams::default())?;
        Ok(metrics
            .css_visual_viewport
            .zoom
            .unwrap_or(metrics.css_visual_viewport.scale))
    }

    pub fn set_zoom_factor(&self, factor: f64) -> OpenPageResult<()> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(OpenPageError::BrowserOperation(
                zoom_factor_must_be_positive_message(factor),
            ));
        }
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root) return null; \
                if (root.getAttribute('{managed}') !== '1') {{ \
                    root.setAttribute('{managed}', '1'); \
                    root.setAttribute('{original}', root.style.zoom || ''); \
                }} \
                root.style.zoom = String({factor}); \
                const value = Number.parseFloat(getComputedStyle(root).zoom || root.style.zoom || '1'); \
                return Number.isFinite(value) && value > 0 ? value : 1; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
            original = PAGE_ZOOM_ORIGINAL_ATTRIBUTE,
            factor = factor,
        );
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    pub fn reset_zoom_factor(&self) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root) return null; \
                if (root.getAttribute('{managed}') === '1') {{ \
                    const original = root.getAttribute('{original}') || ''; \
                    if (original === '') {{ \
                        root.style.removeProperty('zoom'); \
                    }} else {{ \
                        root.style.zoom = original; \
                    }} \
                    root.removeAttribute('{managed}'); \
                    root.removeAttribute('{original}'); \
                }} \
                return true; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
            original = PAGE_ZOOM_ORIGINAL_ATTRIBUTE,
        );
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    fn managed_zoom_factor(&self) -> OpenPageResult<Option<f64>> {
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root || root.getAttribute('{managed}') !== '1') return null; \
                const raw = getComputedStyle(root).zoom || root.style.zoom || '1'; \
                const value = Number.parseFloat(raw); \
                return Number.isFinite(value) && value > 0 ? value : 1; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
        );
        match self.run_js_with_options(&script, &[], true, None)? {
            Value::Null => Ok(None),
            Value::Number(value) => value
                .as_f64()
                .ok_or_else(|| {
                    OpenPageError::JavaScript(value_did_not_return_message(
                        "managed page zoom",
                        "a numeric value",
                        "数值",
                        &value.to_string(),
                    ))
                })
                .map(Some),
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "managed page zoom",
                "a number or null",
                "数字或 null",
                &other.to_string(),
            ))),
        }
    }

    fn ensure_clipboard_api_available(&self, method_name: &str) -> OpenPageResult<()> {
        let available = self.run_js_with_options(
            "Boolean(window.isSecureContext && navigator.clipboard)",
            &[],
            true,
            None,
        )?;
        if available.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(OpenPageError::UnsupportedOperation(
                clipboard_secure_context_required_message(method_name),
            ))
        }
    }

    pub fn listener(&self) -> Listener {
        Listener::new(Arc::clone(&self.runtime), self.inner.clone())
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn interceptor(&self) -> Interceptor {
        self.interceptor.clone()
    }

    pub fn intercept(&self) -> Interceptor {
        self.interceptor()
    }

    pub fn console(&self) -> Console {
        self.console.clone()
    }

    pub fn screencast(&self) -> Screencast {
        self.screencast.clone()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.alerts.has_alert()
    }

    pub fn alert_text(&self) -> OpenPageResult<Option<String>> {
        self.alerts.alert_text()
    }

    pub fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.alerts.handle_alert(accept, prompt_text, timeout_ms)
    }

    pub fn set_next_alert_action(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.alerts.set_next_alert_action(accept, prompt_text)
    }

    pub fn set_auto_alert_action(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.alerts.set_auto_alert_action(accept, prompt_text)
    }

    pub fn wait_for_alert_closed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.alerts.wait_for_alert_closed(timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "wait_for_download_begin()",
            ))
        })?;
        browser.wait_for_download_begin_in_frames(
            &self.download_scope_frame_ids()?,
            timeout_ms,
            cancel_it,
        )
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "wait_for_downloads_done()",
            ))
        })?;
        browser.wait_for_downloads_done_in_frames(
            &self.download_scope_frame_ids()?,
            timeout_ms,
            cancel_if_timeout,
        )
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find(&self.html()?, locator.raw())
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find_all(&self.html()?, locator.raw())
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        snapshot_query_xpath(&self.html()?, expression)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<Element>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        snapshot_root(&self.html()?)
    }

    pub fn user_agent(&self) -> OpenPageResult<String> {
        match self.evaluate("navigator.userAgent")? {
            Value::String(value) => Ok(value),
            value => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "navigator.userAgent",
                "a string",
                "字符串",
                &value.to_string(),
            ))),
        }
    }

    pub fn set_user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        let mut params = SetUserAgentOverrideParams::new(user_agent.to_string());
        if let Some(platform) = platform {
            params.platform = Some(platform.to_string());
        }
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_user_agent()",
        )?;
        Ok(())
    }

    pub fn set_headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        let headers = parse_headers_input(headers)?;
        let header_map = headers
            .into_iter()
            .map(|(name, value)| (name, serde_json::Value::String(value)))
            .collect::<serde_json::Map<_, _>>();
        let params =
            SetExtraHttpHeadersParams::new(Headers::new(serde_json::Value::Object(header_map)));
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NetworkEnableParams::default(),
            "Page::set_headers()",
        )?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_headers()",
        )?;
        Ok(())
    }

    pub fn set_session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        let script = match value {
            Some(value) => format!(
                "(() => {{ sessionStorage.setItem({item}, {value}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
                value = serde_json::to_string(value)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
            None => format!(
                "(() => {{ sessionStorage.removeItem({item}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
        };
        self.run_js(&script)?;
        Ok(())
    }

    pub fn session_storage(&self, item: Option<&str>) -> OpenPageResult<Value> {
        self.run_js(&storage_lookup_script("sessionStorage", item)?)
    }

    pub fn set_local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        let script = match value {
            Some(value) => format!(
                "(() => {{ localStorage.setItem({item}, {value}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
                value = serde_json::to_string(value)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
            None => format!(
                "(() => {{ localStorage.removeItem({item}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
        };
        self.run_js(&script)?;
        Ok(())
    }

    pub fn local_storage(&self, item: Option<&str>) -> OpenPageResult<Value> {
        self.run_js(&storage_lookup_script("localStorage", item)?)
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        let params = AddScriptToEvaluateOnNewDocumentParams::new(script.to_string());
        let identifier: String = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::add_init_js()",
        )?
        .identifier
        .into();
        self.init_scripts
            .lock()
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "page init scripts",
                    "页面初始化脚本",
                ))
            })?
            .push(identifier.clone());
        Ok(identifier)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        let script_ids = match script_id {
            Some(script_id) => vec![script_id.to_string()],
            None => self
                .init_scripts
                .lock()
                .map_err(|_| {
                    OpenPageError::PageOperation(component_state_lock_poisoned_message(
                        "page init scripts",
                        "页面初始化脚本",
                    ))
                })?
                .clone(),
        };
        if script_ids.is_empty() {
            return Ok(());
        }
        for script_id in &script_ids {
            let params = RemoveScriptToEvaluateOnNewDocumentParams::new(script_id.clone());
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                params,
                "Page::remove_init_js()",
            )?;
        }
        let mut stored = self.init_scripts.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "page init scripts",
                "页面初始化脚本",
            ))
        })?;
        if let Some(script_id) = script_id {
            stored.retain(|existing| existing != script_id);
        } else {
            stored.clear();
        }
        Ok(())
    }

    pub fn clear_cache(
        &self,
        session_storage: bool,
        local_storage: bool,
        cache: bool,
        cookies: bool,
    ) -> OpenPageResult<()> {
        if session_storage {
            self.run_js("(() => { sessionStorage.clear(); return true; })()")?;
        }
        if local_storage {
            self.run_js("(() => { localStorage.clear(); return true; })()")?;
        }
        if cache {
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                ClearBrowserCacheParams::default(),
                "Page::clear_cache()",
            )?;
        }
        if cookies {
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                ClearBrowserCookiesParams::default(),
                "Page::clear_cache()",
            )?;
        }
        Ok(())
    }

    pub fn ready_state(&self) -> OpenPageResult<String> {
        match self.evaluate("document.readyState")? {
            Value::String(value) => Ok(value),
            value => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "document.readyState",
                "a string",
                "字符串",
                &value.to_string(),
            ))),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(self.ready_state()? != "complete")
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.runtime.block_on(async {
            Ok(run_with_timeout(
                async {
                    self.inner
                        .url()
                        .await
                        .map_err(|err| page_operation_error("Page::is_alive()", err))
                },
                timeout_duration_millis(cdp_timeout_duration()),
                "Page::is_alive()",
            )
            .await
            .is_ok())
        })
    }

    pub fn wait_for_url_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.url()?;
            Ok(if exclude {
                !value.contains(text)
            } else {
                value.contains(text)
            })
        })
    }

    pub fn wait_for_title_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.title()?;
            Ok(if exclude {
                !value.contains(text)
            } else {
                value.contains(text)
            })
        })
    }

    pub fn wait_for_load_start(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state != "complete" => return Ok(true),
                Ok(_) => {}
                Err(_) => return Ok(true),
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_load_start()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state == "complete" => return Ok(true),
                Ok(_) => {}
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_doc_loaded()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            StopLoadingParams::default(),
            "Page::stop_loading()",
        )?;
        Ok(())
    }

    pub fn cookie_header(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            let cookies =
                run_page_future_with_cdp_timeout(self.inner.get_cookies(), "read cookies").await?;
            if cookies.is_empty() {
                return Ok(None);
            }

            Ok(Some(
                cookies
                    .into_iter()
                    .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                    .collect::<Vec<_>>()
                    .join("; "),
            ))
        })
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        let url = self.url()?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(Vec::new());
        }
        let Some(cookie_header) = self.cookie_header()? else {
            return Ok(Vec::new());
        };
        cookies_from_header(&url, &cookie_header)
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = Url::parse(url).map_err(|err| {
            OpenPageError::PageOperation(invalid_url_message(url, Some(&err.to_string())))
        })?;
        let cookies = cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.set_cookies(cookies), "set cookie header")
                .await?;
            Ok(())
        })
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        let current_url = current_cookie_scope_url(self.url()?);
        let current_url = current_url
            .as_deref()
            .map(Url::parse)
            .transpose()
            .map_err(|err| page_operation_error("parse cookie scope url", err))?;
        let cookies = cookie_input_to_params_allow_missing_scope(cookies.into())
            .map_err(|err| page_operation_error("parse cookies", err))?;
        if cookies.is_empty() {
            return Ok(());
        }
        self.runtime.block_on(async {
            for cookie in &cookies {
                set_page_cookie_with_scope_fallback(&self.inner, cookie, current_url.as_ref())
                    .await?;
            }
            Ok(())
        })
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let cookie = cookie_param(name, value, url, domain, path);
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.set_cookie(cookie), "set cookie").await?;
            Ok(())
        })
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let params = delete_cookie_params(name, url, domain, path);
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.delete_cookie(params), "delete cookie")
                .await?;
            Ok(())
        })
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            ClearBrowserCookiesParams::default(),
            "Page::clear_cookies()",
        )?;
        Ok(())
    }

    pub fn set_permission(
        &self,
        name: &str,
        setting: &str,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
    ) -> OpenPageResult<String> {
        let browser = self.browser_backed_ref("set_permission")?;
        let origin = resolve_permission_origin(origin, &self.url()?)?;
        let embedded_origin = embedded_origin
            .map(permission_origin_from_input)
            .transpose()?;
        let setting = setting.parse::<PermissionSetting>().map_err(|_| {
            OpenPageError::BrowserOperation(permission_setting_invalid_message(setting))
        })?;
        let context_id = browser.browser_context_id(&self.target_id())?;
        browser.set_permission(
            PermissionDescriptor::new(name),
            setting,
            Some(&origin),
            embedded_origin.as_deref(),
            context_id.as_deref(),
        )?;
        Ok(origin)
    }

    pub fn reset_permissions(&self) -> OpenPageResult<()> {
        let browser = self.browser_backed_ref("reset_permissions")?;
        let context_id = browser.browser_context_id(&self.target_id())?;
        browser.reset_permissions(context_id.as_deref())
    }

    pub fn clipboard_read_text(&self) -> OpenPageResult<String> {
        self.ensure_clipboard_api_available("clipboard_read_text")?;
        match self.run_js_with_options("navigator.clipboard.readText()", &[], true, None)? {
            Value::String(value) => Ok(value),
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "clipboard read",
                "text",
                "文本",
                &other.to_string(),
            ))),
        }
    }

    pub fn clipboard_write_text(&self, text: &str) -> OpenPageResult<()> {
        self.ensure_clipboard_api_available("clipboard_write_text")?;
        let text = serde_json::to_string(text)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let script = format!("navigator.clipboard.writeText({text}).then(() => true)");
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    pub fn main_frame_id(&self) -> OpenPageResult<String> {
        Ok(execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetFrameTreeParams::default(),
            "Page::main_frame_id()",
        )?
        .frame_tree
        .frame
        .id
        .as_ref()
        .to_string())
    }

    pub(crate) fn download_scope_frame_ids(&self) -> OpenPageResult<Vec<String>> {
        let frame_tree = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetFrameTreeParams::default(),
            "Page::download_scope_frame_ids()",
        )?;
        let mut frame_ids = Vec::new();
        collect_frame_ids(&frame_tree.frame_tree, &mut frame_ids);
        Ok(frame_ids)
    }

    fn window_info(&self) -> OpenPageResult<GetWindowForTargetReturns> {
        let params = GetWindowForTargetParams::builder()
            .target_id(TargetId::new(self.target_id()))
            .build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::window_info()",
        )
    }

    fn set_window_bounds(&self, bounds: Bounds) -> OpenPageResult<()> {
        let info = self.window_info()?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            SetWindowBoundsParams::new(info.window_id, bounds),
            "Page::set_window_bounds()",
        )?;
        Ok(())
    }

    pub fn close(self) -> OpenPageResult<()> {
        let target_id = self.target_id();
        if let Some(browser) = &self.browser {
            return browser.close_target(&target_id);
        }
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.close(), "close page").await?;
            Ok::<(), OpenPageError>(())
        })?;
        Ok(())
    }

    fn download_with_cookie_scope(
        &self,
        url: &str,
        cookie_scope_url: Option<&str>,
    ) -> OpenPageResult<String> {
        self.build_download_session(cookie_scope_url)?.download(url)
    }

    fn download_to_with_cookie_scope(
        &self,
        url: &str,
        path: impl AsRef<Path>,
        cookie_scope_url: Option<&str>,
    ) -> OpenPageResult<String> {
        self.build_download_session(cookie_scope_url)?
            .download_to(url, path)
    }

    fn build_download_session(
        &self,
        cookie_scope_url: Option<&str>,
    ) -> OpenPageResult<SessionPage> {
        let mut options = SessionOptions {
            user_agent: Some(self.user_agent()?),
            ..SessionOptions::default()
        };
        if let Some(download_path) = self.download_path()? {
            options.download_path = PathBuf::from(download_path);
        }

        let session = SessionPage::new(options)?;
        if let Some(scope_url) = cookie_scope_url {
            if (scope_url.starts_with("http://") || scope_url.starts_with("https://"))
                && let Some(cookie_header) = self.cookie_header()?
            {
                session.set_cookie_header(scope_url, &cookie_header)?;
            }
        }
        Ok(session)
    }

    fn wait_for_change<F>(&self, timeout_ms: u64, mut predicate: F) -> OpenPageResult<bool>
    where
        F: FnMut(&Self) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if predicate(self)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_change()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.load_mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page load mode",
                "页面加载模式",
            ))
        })
    }

    fn navigate_via_cdp(&self, url: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.goto(url.to_string()), "navigate").await?;
            Ok::<(), OpenPageError>(())
        })
    }

    fn navigate_via_script(&self, url: &str) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ window.location.href = {url}; return true; }})()",
            url = serde_json::to_string(url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    fn navigate_history(&self, offset: isize) -> OpenPageResult<bool> {
        if offset == 0 {
            return Ok(true);
        }
        let history = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetNavigationHistoryParams::default(),
            "Page::navigate_history()",
        )?;
        let Some(target_index) = history_entry_index(
            history.current_index as usize,
            history.entries.len(),
            offset,
        ) else {
            return Ok(false);
        };
        let entry_id = history
            .entries
            .get(target_index)
            .ok_or_else(|| {
                OpenPageError::PageOperation(navigation_history_index_out_of_bounds_message(
                    target_index,
                ))
            })?
            .id;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NavigateToHistoryEntryParams::new(entry_id),
            "Page::navigate_history()",
        )?;
        Ok(true)
    }

    fn wait_for_ready_state_change(
        &self,
        timeout_ms: u64,
        include_interactive: bool,
    ) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state == "complete" => return Ok(true),
                Ok(state) if include_interactive && state == "interactive" => return Ok(true),
                Ok(_) => {}
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn wait_for_dom_ready(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if self.html().is_ok() {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub(crate) fn frame_from_element(&self, element: Element) -> OpenPageResult<Frame> {
        let backend_node_id = element.backend_node_id();
        let frame_id = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.inner,
                DescribeNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
                "Page::frame_from_element()",
            )
            .await?;
            response
                .node
                .frame_id
                .map(|frame_id| frame_id.as_ref().to_string())
                .ok_or_else(|| {
                    OpenPageError::PageOperation(frame_element_missing_frame_id_message())
                })
        });
        let frame_id = match frame_id {
            Ok(frame_id) => frame_id,
            Err(describe_err) => {
                let marker = next_page_marker();
                element.set_attr(PAGE_MARKER_ATTRIBUTE, &marker)?;
                let detected = (|| -> OpenPageResult<Option<String>> {
                    let main_frame_id = self.main_frame_id()?;
                    for candidate_frame_id in self.download_scope_frame_ids()? {
                        if candidate_frame_id == main_frame_id {
                            continue;
                        }
                        let owner_element = self.frame_owner_element_by_id(&candidate_frame_id)?;
                        if owner_element.attr(PAGE_MARKER_ATTRIBUTE)?.as_deref() == Some(&marker) {
                            return Ok(Some(candidate_frame_id));
                        }
                    }
                    Ok(None)
                })();
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                match detected {
                    Ok(Some(frame_id)) => frame_id,
                    Ok(None) => return Err(describe_err),
                    Err(err) => return Err(err),
                }
            }
        };
        let none_element_config = self.frame_none_element_config(&frame_id)?;
        Ok(Frame::new(
            self.clone(),
            frame_id,
            element,
            none_element_config,
        ))
    }

    fn frame_owner_element_by_id(&self, frame_id: &str) -> OpenPageResult<Element> {
        let (node_id, backend_node_id) = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.inner,
                GetFrameOwnerParams::new(FrameId::new(frame_id.to_string())),
                "Page::frame_owner_element_by_id()",
            )
            .await?;
            Ok::<
                (
                    Option<chromiumoxide::cdp::browser_protocol::dom::NodeId>,
                    BackendNodeId,
                ),
                OpenPageError,
            >((response.node_id, response.backend_node_id))
        })?;
        if let Some(node_id) = node_id {
            match self
                .resolve_dom_node_id(node_id, "frame owner could not be resolved to an element")
            {
                Ok(element) => Ok(element),
                Err(OpenPageError::PageOperation(message))
                    if message.contains("Could not find node with given id") =>
                {
                    self.resolve_dom_backend_node_id(backend_node_id)
                }
                Err(err) => Err(err),
            }
        } else {
            self.resolve_dom_backend_node_id(backend_node_id)
        }
    }

    pub(crate) fn resolve_dom_backend_node_id(
        &self,
        backend_node_id: BackendNodeId,
    ) -> OpenPageResult<Element> {
        let node_id = self.runtime.block_on(async {
            let resolved = execute_page_command_async(
                &self.inner,
                ResolveNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
                "Page::resolve_dom_backend_node_id()",
            )
            .await?;
            let object_id = resolved.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation(resolved_frame_owner_missing_object_id_message())
            })?;
            let requested = execute_page_command_async(
                &self.inner,
                RequestNodeParams::new(object_id),
                "Page::resolve_dom_backend_node_id()",
            )
            .await?;
            Ok::<chromiumoxide::cdp::browser_protocol::dom::NodeId, OpenPageError>(
                requested.node_id,
            )
        })?;
        self.resolve_dom_node_id(node_id, "frame owner could not be resolved to an element")
    }

    fn resolve_dom_node_id(
        &self,
        node_id: chromiumoxide::cdp::browser_protocol::dom::NodeId,
        error_message: &str,
    ) -> OpenPageResult<Element> {
        let marker = next_page_marker();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            SetAttributeValueParams::new(node_id, PAGE_MARKER_ATTRIBUTE, marker.clone()),
            "Page::resolve_dom_node_id()",
        )?;

        let element = self.find(marker_selector(&marker).as_str());
        let cleanup = self.runtime.block_on(async {
            let _ = execute_page_command_async(
                &self.inner,
                RemoveAttributeParams::new(node_id, PAGE_MARKER_ATTRIBUTE),
                "Page::resolve_dom_node_id()",
            )
            .await;
            Ok::<(), OpenPageError>(())
        });

        match (element, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(OpenPageError::Timeout(message)), _) => Err(OpenPageError::Timeout(message)),
            (Err(_), Ok(())) => Err(OpenPageError::ElementNotFound(error_message.to_string())),
            (Err(err), Err(_)) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn frame_name_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_name(FrameId::new(frame_id.to_string())),
                "read frame name",
            )
            .await
        })
    }

    fn frame_url_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_url(FrameId::new(frame_id.to_string())),
                "read frame url",
            )
            .await
        })
    }

    pub(crate) fn frame_parent_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_parent(FrameId::new(frame_id.to_string())),
                "read frame parent",
            )
            .await
            .map(|value| value.map(|frame_id| frame_id.as_ref().to_string()))
        })
    }

    fn frame_context_id(&self, frame_id: &str) -> OpenPageResult<ExecutionContextId> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner
                    .frame_execution_context(FrameId::new(frame_id.to_string())),
                "read frame execution context",
            )
            .await?
            .ok_or_else(|| {
                OpenPageError::PageOperation(frame_execution_context_unavailable_message(frame_id))
            })
        })
    }

    fn evaluate_in_frame(&self, frame_id: &str, expression: &str) -> OpenPageResult<Value> {
        self.evaluate_in_frame_with_options(frame_id, expression, None, false)
    }

    fn evaluate_in_frame_with_options(
        &self,
        frame_id: &str,
        expression: &str,
        timeout_ms: Option<u64>,
        await_promise: bool,
    ) -> OpenPageResult<Value> {
        let context_id = self.frame_context_id(frame_id)?;
        let params = EvaluateParams::builder()
            .expression(expression)
            .context_id(context_id)
            .await_promise(await_promise)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        let timeout_ms = resolve_javascript_timeout_ms(timeout_ms, self.javascript_timeout_ms()?);
        self.evaluate_params_with_timeout(params, timeout_ms)
    }

    fn clear_page_markers(&self, markers: &[&str]) -> OpenPageResult<()> {
        if markers.is_empty() {
            return Ok(());
        }
        let markers = serde_json::to_string(markers)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let script = format!(
            "(() => {{ \
                const attr = {attr}; \
                const markers = {markers}; \
                for (const marker of markers) {{ \
                    const element = document.querySelector(`[${{attr}}=\"${{marker}}\"]`); \
                    if (element) element.removeAttribute(attr); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            markers = markers,
        );
        self.run_js(&script)?;
        Ok(())
    }
}

fn normalize_navigation_target(target: &str) -> OpenPageResult<String> {
    let Some(path) = resolve_navigation_local_file_path(target)? else {
        return Ok(target.to_string());
    };

    let file_path = path.canonicalize().unwrap_or(path);
    Url::from_file_path(&file_path)
        .map(|url| url.to_string())
        .map_err(|_| {
            OpenPageError::PageOperation(build_file_url_failed_message(
                &file_path.display().to_string(),
            ))
        })
}

fn resolve_navigation_local_file_path(target: &str) -> OpenPageResult<Option<PathBuf>> {
    if target.starts_with("file://") {
        let url = Url::parse(target).map_err(|err| {
            OpenPageError::PageOperation(invalid_file_url_message(target, Some(&err.to_string())))
        })?;
        match url.host_str() {
            None | Some("localhost") => {}
            Some(_) => {
                return Err(OpenPageError::PageOperation(invalid_file_url_message(
                    target, None,
                )));
            }
        }
        return url
            .to_file_path()
            .map(Some)
            .map_err(|_| OpenPageError::PageOperation(invalid_file_url_message(target, None)));
    }

    let path = Path::new(target);
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

fn history_entry_index(current_index: usize, entry_count: usize, offset: isize) -> Option<usize> {
    let target = current_index.checked_add_signed(offset)?;
    (target < entry_count).then_some(target)
}

fn page_element_info_properties_json(info: &PageElementInfo) -> OpenPageResult<String> {
    let mut properties = serde_json::Map::new();
    for (name, value) in &info.properties {
        properties.insert(name.clone(), value.clone());
    }
    serde_json::to_string(&Value::Object(properties))
        .map_err(|err| OpenPageError::Serialization(err.to_string()))
}

fn resolve_page_element_target<'a>(
    page: &Page,
    target: PageElementTarget<'a>,
) -> OpenPageResult<ResolvedPageElementTarget<'a>> {
    match target {
        PageElementTarget::Locator(locator) => Ok(ResolvedPageElementTarget::Owned(
            page.find(Locator::from_input(locator)?.raw())?,
        )),
        PageElementTarget::Element(element) => Ok(ResolvedPageElementTarget::Borrowed(element)),
        PageElementTarget::OwnedElement(element) => Ok(ResolvedPageElementTarget::Owned(element)),
        PageElementTarget::SessionElement(_) => Err(OpenPageError::UnsupportedOperation(
            session_backed_element_driver_target_message(
                "SessionElement",
                "page element",
                "页面元素定位",
            ),
        )),
        PageElementTarget::OwnedSessionElement(_) => Err(OpenPageError::UnsupportedOperation(
            session_backed_element_driver_target_message(
                "SessionElement",
                "page element",
                "页面元素定位",
            ),
        )),
        PageElementTarget::WebElement(element) => match element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                Ok(ResolvedPageElementTarget::Borrowed(element))
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                session_backed_element_driver_target_message(
                    "WebElement",
                    "page element",
                    "页面元素定位",
                ),
            )),
        },
        PageElementTarget::OwnedWebElement(element) => match element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                Ok(ResolvedPageElementTarget::Owned(element))
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                session_backed_element_driver_target_message(
                    "WebElement",
                    "page element",
                    "页面元素定位",
                ),
            )),
        },
    }
}

fn resolve_page_frame_target<'a>(
    page: &Page,
    target: PageFrameTarget<'a>,
) -> OpenPageResult<Element> {
    match target {
        PageFrameTarget::Locator(locator) => {
            let locator = frame_locator_input(locator)?;
            page.find(locator.as_str())
        }
        PageFrameTarget::Index(index) => page_frame_element_by_index(page, index),
        PageFrameTarget::Element(element) => find_frame_element_from_object(page, element),
        PageFrameTarget::WebElement(element) => match element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                find_frame_element_from_object(page, element)
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                session_backed_element_driver_target_message(
                    "WebElement",
                    "page frame",
                    "页面 frame 定位",
                ),
            )),
        },
        PageFrameTarget::Frame(frame) => {
            find_frame_element_from_object(page, frame.frame_element())
        }
        PageFrameTarget::WebFrame(frame) => {
            find_frame_element_from_object(page, frame.frame_element())
        }
        PageFrameTarget::OwnedFrame(frame) => {
            find_frame_element_from_object(page, frame.frame_element())
        }
        PageFrameTarget::OwnedWebFrame(frame) => {
            find_frame_element_from_object(page, frame.frame_element())
        }
    }
}

fn page_frame_element_by_index(page: &Page, index: isize) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            frame_index_must_start_message(),
        ));
    }
    let frames = page.get_frame_eles(None::<&str>)?;
    let resolved_index = if index > 0 {
        (index as usize).checked_sub(1)
    } else {
        frames.len().checked_sub(index.unsigned_abs())
    };
    resolved_index
        .and_then(|resolved_index| frames.into_iter().nth(resolved_index))
        .ok_or_else(|| OpenPageError::ElementNotFound(frame_index_out_of_range_message(index)))
}

fn find_frame_element_from_object(page: &Page, element: &Element) -> OpenPageResult<Element> {
    let marker = next_page_marker();
    element.set_attr(PAGE_MARKER_ATTRIBUTE, &marker)?;
    let selector = marker_selector(&marker);
    let result = match page.find(selector.as_str()) {
        Ok(element) => Ok(element),
        Err(err @ OpenPageError::ElementNotFound(_)) => {
            let main_frame_id = page.main_frame_id()?;
            let mut found = None;
            for frame_id in page.download_scope_frame_ids()? {
                if frame_id == main_frame_id {
                    continue;
                }
                let owner_element = page.frame_owner_element_by_id(&frame_id)?;
                let frame = page.frame_from_element(owner_element)?;
                match frame.find(selector.as_str()) {
                    Ok(element) => {
                        found = Some(element);
                        break;
                    }
                    Err(OpenPageError::ElementNotFound(_)) => {}
                    Err(err) => return Err(err),
                }
            }
            found.ok_or(err)
        }
        Err(err) => Err(err),
    };
    let cleanup = element.remove_attr(PAGE_MARKER_ATTRIBUTE);

    match (result, cleanup) {
        (Ok(element), Ok(())) => Ok(element),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Err(err), Err(_)) => Err(err),
    }
}

fn resolve_actions_target_point<'a>(
    page: &Page,
    target: ActionsTarget<'a>,
    offset_x: Option<f64>,
    offset_y: Option<f64>,
) -> OpenPageResult<(f64, f64)> {
    match target {
        ActionsTarget::Locator(locator) => {
            let resolved = resolve_page_element_target(page, PageElementTarget::Locator(locator))?;
            match resolved {
                ResolvedPageElementTarget::Owned(element) => {
                    action_point_from_element(page, &element, offset_x, offset_y)
                }
                ResolvedPageElementTarget::Borrowed(element) => {
                    action_point_from_element(page, element, offset_x, offset_y)
                }
            }
        }
        ActionsTarget::Element(element) => {
            action_point_from_element(page, element, offset_x, offset_y)
        }
        ActionsTarget::OwnedElement(element) => {
            action_point_from_element(page, &element, offset_x, offset_y)
        }
        ActionsTarget::WebElement(element) => match element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                action_point_from_element(page, element, offset_x, offset_y)
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                session_backed_web_element_driver_actions_message(),
            )),
        },
        ActionsTarget::OwnedWebElement(element) => match element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                action_point_from_element(page, &element, offset_x, offset_y)
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                session_backed_web_element_driver_actions_message(),
            )),
        },
        ActionsTarget::Coordinates(x, y) => action_point_from_page_coordinates(
            page,
            x + offset_x.unwrap_or(0.0),
            y + offset_y.unwrap_or(0.0),
        ),
    }
}

fn action_point_from_element(
    page: &Page,
    element: &Element,
    offset_x: Option<f64>,
    offset_y: Option<f64>,
) -> OpenPageResult<(f64, f64)> {
    element.scroll_to_see(Some(false))?;
    let (page_x, page_y) = if offset_x.is_none() && offset_y.is_none() {
        element.rect_midpoint()?.ok_or_else(|| {
            OpenPageError::ElementNotFound(action_element_missing_clickable_rect_message())
        })?
    } else {
        let (left, top) = element.rect_location()?.ok_or_else(|| {
            OpenPageError::ElementNotFound(action_element_missing_rect_location_message())
        })?;
        (
            left + offset_x.unwrap_or(0.0),
            top + offset_y.unwrap_or(0.0),
        )
    };
    action_point_from_page_coordinates(page, page_x, page_y)
}

fn action_point_from_page_coordinates(
    page: &Page,
    page_x: f64,
    page_y: f64,
) -> OpenPageResult<(f64, f64)> {
    let (scroll_x, scroll_y) = action_page_scroll_position(page)?;
    let (viewport_width, viewport_height) = action_page_viewport_size(page)?;
    let in_viewport = page_x >= scroll_x
        && page_x <= scroll_x + viewport_width
        && page_y >= scroll_y
        && page_y <= scroll_y + viewport_height;
    if !in_viewport {
        let target_x = (page_x - viewport_width / 2.0).max(0.0);
        let target_y = (page_y - viewport_height / 2.0).max(0.0);
        page.run_js(&format!(
            "(() => {{ window.scrollTo({target_x}, {target_y}); return true; }})()"
        ))?;
    }
    let (scroll_x, scroll_y) = action_page_scroll_position(page)?;
    Ok((page_x - scroll_x, page_y - scroll_y))
}

fn action_page_scroll_position(page: &Page) -> OpenPageResult<(f64, f64)> {
    value_as_f64_pair(
        page.run_js("(() => [window.scrollX, window.scrollY])()")?,
        "actions page scroll position",
    )
}

fn action_page_viewport_size(page: &Page) -> OpenPageResult<(f64, f64)> {
    value_as_f64_pair(
        page.run_js("(() => [window.innerWidth, window.innerHeight])()")?,
        "actions page viewport size",
    )
}

fn action_mouse_buttons(button: &MouseButton) -> i64 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
        _ => 0,
    }
}

fn action_modifier_bit(value: &str) -> Option<i64> {
    match value.to_ascii_lowercase().as_str() {
        "alt" => Some(ACTION_MODIFIER_ALT),
        "control" | "ctrl" => Some(ACTION_MODIFIER_CTRL),
        "meta" | "command" | "cmd" => Some(ACTION_MODIFIER_META),
        "shift" => Some(ACTION_MODIFIER_SHIFT),
        _ => None,
    }
}

fn action_build_key_event(
    definition: &keys::KeyDefinition,
    modifiers: i64,
    key_up: bool,
) -> DispatchKeyEventParams {
    let mut builder = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::RawKeyDown)
        .modifiers(modifiers)
        .key(definition.key)
        .code(definition.code)
        .windows_virtual_key_code(definition.key_code)
        .native_virtual_key_code(definition.key_code);

    let mut has_text = false;
    if let Some(text) = definition.text {
        builder = builder.unmodified_text(text);
        if modifiers & !ACTION_MODIFIER_SHIFT == 0 {
            builder = builder.text(text);
            has_text = !text.is_empty();
        } else {
            builder = builder.text("");
        }
    } else if definition.key.len() == 1 {
        builder = builder.unmodified_text(definition.key);
        if modifiers & !ACTION_MODIFIER_SHIFT == 0 {
            builder = builder.text(definition.key);
            has_text = true;
        } else {
            builder = builder.text("");
        }
    }

    if cfg!(target_os = "macos") && (modifiers & ACTION_MODIFIER_META) != 0 && !key_up {
        if let Some(commands) = action_mac_meta_commands(definition.key) {
            builder = builder.commands(commands.iter().copied());
        }
    }

    let event_type = if key_up {
        DispatchKeyEventType::KeyUp
    } else if has_text {
        DispatchKeyEventType::KeyDown
    } else {
        DispatchKeyEventType::RawKeyDown
    };

    builder
        .r#type(event_type)
        .build()
        .expect("DispatchKeyEventParams should build with required type")
}

fn action_move_path(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    duration_secs: f64,
) -> Vec<Point> {
    if duration_secs <= 0.0 {
        return vec![Point::new(end_x, end_y)];
    }
    let steps = ((duration_secs * 60.0).ceil() as usize).max(1);
    (1..=steps)
        .map(|index| {
            let ratio = index as f64 / steps as f64;
            Point::new(
                start_x + (end_x - start_x) * ratio,
                start_y + (end_y - start_y) * ratio,
            )
        })
        .collect()
}

fn action_move_pause(duration_secs: f64, step_count: usize) -> Option<Duration> {
    if duration_secs <= 0.0 || step_count <= 1 {
        return None;
    }
    Some(Duration::from_secs_f64(duration_secs / step_count as f64))
}

fn action_effective_key_value(value: &str, modifiers: i64) -> Cow<'_, str> {
    if modifiers & ACTION_MODIFIER_SHIFT == 0 || value.chars().count() != 1 {
        return Cow::Borrowed(value);
    }

    let shifted = value
        .chars()
        .next()
        .and_then(action_shifted_char)
        .map(|ch| ch.to_string());

    shifted
        .map(Cow::Owned)
        .unwrap_or_else(|| Cow::Borrowed(value))
}

fn action_shifted_char(ch: char) -> Option<char> {
    match ch {
        'a'..='z' => Some(ch.to_ascii_uppercase()),
        '1' => Some('!'),
        '2' => Some('@'),
        '3' => Some('#'),
        '4' => Some('$'),
        '5' => Some('%'),
        '6' => Some('^'),
        '7' => Some('&'),
        '8' => Some('*'),
        '9' => Some('('),
        '0' => Some(')'),
        '-' => Some('_'),
        '=' => Some('+'),
        '[' => Some('{'),
        ']' => Some('}'),
        '\\' => Some('|'),
        ';' => Some(':'),
        '\'' => Some('"'),
        ',' => Some('<'),
        '.' => Some('>'),
        '/' => Some('?'),
        '`' => Some('~'),
        other => Some(other),
    }
}

fn action_mac_meta_commands(key: &str) -> Option<&'static [&'static str]> {
    match key.to_ascii_lowercase().as_str() {
        "a" => Some(&["selectAll"]),
        "c" => Some(&["copy"]),
        "x" => Some(&["cut"]),
        "v" => Some(&["paste"]),
        "z" => Some(&["undo"]),
        "y" => Some(&["redo"]),
        _ => None,
    }
}

fn action_sleep_interval(interval_secs: Option<f64>) {
    let Some(interval_secs) = interval_secs else {
        return;
    };
    if interval_secs > 0.0 {
        sleep(Duration::from_secs_f64(interval_secs));
    }
}

fn action_wait_duration_secs(start: f64, end: f64) -> f64 {
    let lower = start.min(end);
    let upper = start.max(end);
    if (upper - lower).abs() < f64::EPSILON {
        return lower;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as f64 / 1_000_000_000.0)
        .unwrap_or(0.5);
    lower + (upper - lower) * nanos
}

fn actions_input_values(input: ActionsInput<'_>) -> Vec<String> {
    match input {
        ActionsInput::Single(value) => vec![value.into_owned()],
        ActionsInput::Many(values) => values.into_iter().map(Cow::into_owned).collect(),
    }
}

fn action_drag_payload(data: ActionsDragData<'_>) -> OpenPageResult<ActionsDragPayload> {
    match data {
        ActionsDragData::Files(files) => {
            let files = action_drag_files(files)?;
            if files.is_empty() {
                return Err(OpenPageError::PageOperation(
                    drag_in_requires_file_path_message(),
                ));
            }
            Ok(ActionsDragPayload {
                items: files
                    .iter()
                    .map(|path| ActionsDragItem {
                        mime_type: "text/plain".to_string(),
                        data: path.clone(),
                        title: None,
                        base_url: None,
                    })
                    .collect(),
                drag_operations_mask: 16,
                files: Some(files),
            })
        }
        ActionsDragData::Text {
            text,
            title,
            base_url,
        } => {
            let (mime_type, title, base_url) = if let Some(title) = title {
                ("text/uri-list".to_string(), Some(title.into_owned()), None)
            } else if let Some(base_url) = base_url {
                (
                    "text/uri-list".to_string(),
                    None,
                    Some(base_url.into_owned()),
                )
            } else {
                ("text/plain".to_string(), None, None)
            };
            Ok(ActionsDragPayload {
                items: vec![ActionsDragItem {
                    mime_type,
                    data: text.into_owned(),
                    title,
                    base_url,
                }],
                drag_operations_mask: 1,
                files: None,
            })
        }
    }
}

fn action_drag_files(files: ActionsInput<'_>) -> OpenPageResult<Vec<String>> {
    actions_input_values(files)
        .into_iter()
        .map(|path| {
            let path = path.trim();
            if path.is_empty() {
                return Err(OpenPageError::PageOperation(
                    drag_in_file_path_empty_message(),
                ));
            }
            absolutize_path(PathBuf::from(path)).map(|path| path.to_string_lossy().into_owned())
        })
        .collect()
}

fn next_page_marker() -> String {
    format!(
        "openpage-page-{}",
        NEXT_PAGE_MARKER.fetch_add(1, Ordering::Relaxed)
    )
}

fn json_string(value: &str) -> OpenPageResult<String> {
    serde_json::to_string(value).map_err(|err| OpenPageError::Serialization(err.to_string()))
}

fn marker_selector(marker: &str) -> String {
    format!("css:[{PAGE_MARKER_ATTRIBUTE}=\"{marker}\"]")
}

fn default_frame_locator() -> String {
    r#"xpath://*[name()="iframe" or name()="frame"]"#.to_string()
}

fn is_explicit_locator(value: &str) -> bool {
    value.starts_with("css:")
        || value.starts_with("xpath:")
        || value.starts_with("tag:")
        || value.starts_with("t:")
        || value.starts_with('@')
}

fn frame_locator(locator: &str) -> String {
    let locator = locator.trim();
    if is_explicit_locator(locator) {
        locator.to_string()
    } else {
        format!(
            r#"xpath://*[(name()="iframe" or name()="frame") and (@name="{locator}" or @id="{locator}")]"#
        )
    }
}

pub(crate) fn frame_locator_input<'a, L>(locator: L) -> OpenPageResult<String>
where
    L: Into<LocatorInput<'a>>,
{
    match locator.into() {
        LocatorInput::Raw(raw) => Ok(frame_locator(raw)),
        LocatorInput::By(by, value) => Ok(Locator::from_by(by, value)?.raw().to_string()),
    }
}

fn optional_frame_locator_input<'a, L>(locator: Option<L>) -> OpenPageResult<String>
where
    L: Into<LocatorInput<'a>>,
{
    match locator {
        Some(locator) => frame_locator_input(locator),
        None => Ok(default_frame_locator()),
    }
}

fn marker_xpath(marker: &str) -> String {
    format!(r#"xpath://*[@{PAGE_MARKER_ATTRIBUTE}="{marker}"]"#)
}

fn compose_frame_html(tag: &str, outer_html: &str, inner_html: &str) -> String {
    match outer_html.find('>') {
        Some(index) => format!("{}{inner_html}</{tag}>", &outer_html[..=index]),
        None => format!("<{tag}>{inner_html}</{tag}>"),
    }
}

fn frame_find_script(locator: &Locator, marker: &str) -> OpenPageResult<String> {
    match locator.kind() {
        LocatorKind::Css => Ok(format!(
            "(() => {{ \
                const element = document.querySelector({query}); \
                if (!element) return null; \
                element.setAttribute({attr}, {marker}); \
                return {marker}; \
            }})()",
            query = json_string(locator.query())?,
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(marker)?,
        )),
        LocatorKind::XPath => Ok(format!(
            "(() => {{ \
                const result = document.evaluate({query}, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null); \
                const element = result.singleNodeValue; \
                if (!(element instanceof Element)) return null; \
                element.setAttribute({attr}, {marker}); \
                return {marker}; \
            }})()",
            query = json_string(locator.query())?,
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(marker)?,
        )),
    }
}

fn frame_find_all_script(locator: &Locator, batch: &str) -> OpenPageResult<String> {
    match locator.kind() {
        LocatorKind::Css => Ok(format!(
            "(() => {{ \
                const attr = {attr}; \
                const batch = {batch}; \
                let index = 0; \
                return Array.from(document.querySelectorAll({query})).map((element) => {{ \
                    const marker = `${{batch}}-${{index++}}`; \
                    element.setAttribute(attr, marker); \
                    return marker; \
                }}); \
            }})()",
            query = json_string(locator.query())?,
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            batch = json_string(batch)?,
        )),
        LocatorKind::XPath => Ok(format!(
            "(() => {{ \
                const attr = {attr}; \
                const batch = {batch}; \
                const result = []; \
                const snapshot = document.evaluate({query}, document, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null); \
                let index = 0; \
                for (let i = 0; i < snapshot.snapshotLength; i++) {{ \
                    const node = snapshot.snapshotItem(i); \
                    if (!(node instanceof Element)) continue; \
                    const marker = `${{batch}}-${{index++}}`; \
                    node.setAttribute(attr, marker); \
                    result.push(marker); \
                }} \
                return result; \
            }})()",
            query = json_string(locator.query())?,
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            batch = json_string(batch)?,
        )),
    }
}

fn page_marker_lookup_expression(marker: Option<&str>) -> OpenPageResult<String> {
    match marker {
        Some(marker) => Ok(format!(
            "document.querySelector(`[${{markerAttr}}={marker}]`)",
            marker = json_string(marker)?,
        )),
        None => Ok("null".to_string()),
    }
}

fn storage_lookup_script(storage: &str, item: Option<&str>) -> OpenPageResult<String> {
    match item {
        Some(item) => Ok(format!(
            "(() => {{ return {storage}.getItem({item}); }})()",
            item = serde_json::to_string(item)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        )),
        None => Ok(format!(
            "(() => {{ \
                const result = {{}}; \
                for (let i = 0; i < {storage}.length; i += 1) {{ \
                    const key = {storage}.key(i); \
                    result[key] = {storage}.getItem(key); \
                }} \
                return result; \
            }})()"
        )),
    }
}

fn value_as_string(value: Value, name: &str) -> OpenPageResult<String> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(OpenPageError::JavaScript(value_did_not_return_message(
            name,
            "a string",
            "字符串",
            &other.to_string(),
        ))),
    }
}

fn value_as_optional_string(value: Value, name: &str) -> OpenPageResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        other => Err(OpenPageError::JavaScript(value_did_not_return_message(
            name,
            "a string or null",
            "字符串或 null",
            &other.to_string(),
        ))),
    }
}

fn value_as_string_vec(value: Value, name: &str) -> OpenPageResult<Vec<String>> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::String(value) => Ok(value),
                other => Err(OpenPageError::JavaScript(
                    value_returned_non_string_entry_message(name, &other.to_string()),
                )),
            })
            .collect(),
        other => Err(OpenPageError::JavaScript(value_did_not_return_message(
            name,
            "an array",
            "数组",
            &other.to_string(),
        ))),
    }
}

fn value_as_f64_pair(value: Value, name: &str) -> OpenPageResult<(f64, f64)> {
    match value {
        Value::Array(values) if values.len() == 2 => Ok((
            values[0].as_f64().ok_or_else(|| {
                OpenPageError::JavaScript(value_pair_entry_not_number_message(name, "first"))
            })?,
            values[1].as_f64().ok_or_else(|| {
                OpenPageError::JavaScript(value_pair_entry_not_number_message(name, "second"))
            })?,
        )),
        other => Err(OpenPageError::JavaScript(value_did_not_return_message(
            name,
            "a number pair",
            "数字对",
            &other.to_string(),
        ))),
    }
}

fn value_as_optional_f64_pair(value: Value, name: &str) -> OpenPageResult<Option<(f64, f64)>> {
    match value {
        Value::Null => Ok(None),
        other => value_as_f64_pair(other, name).map(Some),
    }
}

fn page_screenshot_params(
    full_page: bool,
    left_top: Option<(f64, f64)>,
    right_bottom: Option<(f64, f64)>,
) -> OpenPageResult<ScreenshotParams> {
    let mut builder = ScreenshotParams::builder().format(CaptureScreenshotFormat::Png);
    if full_page {
        builder = builder.full_page(true);
    } else if let Some(clip) = screenshot_clip(left_top, right_bottom)? {
        builder = builder.clip(clip).capture_beyond_viewport(true);
    }
    Ok(builder.build())
}

fn screenshot_clip(
    left_top: Option<(f64, f64)>,
    right_bottom: Option<(f64, f64)>,
) -> OpenPageResult<Option<ClipViewport>> {
    match (left_top, right_bottom) {
        (None, None) => Ok(None),
        (Some((x, y)), Some((right, bottom))) => {
            let width = right - x;
            let height = bottom - y;
            if width <= 0.0 || height <= 0.0 {
                return Err(OpenPageError::PageOperation(screenshot_clip_order_message()));
            }
            Ok(Some(
                ClipViewport::builder()
                    .x(x)
                    .y(y)
                    .width(width)
                    .height(height)
                    .scale(1.0)
                    .build()
                    .map_err(|err| page_operation_error("build screenshot clip", err))?,
            ))
        }
        _ => Err(OpenPageError::PageOperation(
            screenshot_clip_complete_message(),
        )),
    }
}

fn resolve_page_screenshot_target_path(
    path: Option<&Path>,
    name: Option<&str>,
    title: Option<&str>,
) -> OpenPageResult<PathBuf> {
    let default_name = title
        .map(sanitize_file_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "page".to_string());
    let mut target = match (path, name) {
        (Some(path), Some(name)) => path.join(sanitize_file_name(name)),
        (Some(path), None) if path.extension().is_some() => path.to_path_buf(),
        (Some(path), None) => path.join(format!("{default_name}.png")),
        (None, Some(name)) => PathBuf::from(sanitize_file_name(name)),
        (None, None) => PathBuf::from(format!("{default_name}.png")),
    };
    if target.extension().is_none() {
        target.set_extension("png");
    }
    let target = absolutize_path(target)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(target)
}

fn resolve_page_save_title(
    page: &Page,
    path: Option<&Path>,
    name: Option<&str>,
) -> OpenPageResult<Option<String>> {
    let needs_title = match (path, name) {
        (None, None) => true,
        (Some(path), None) => path.extension().is_none(),
        _ => false,
    };
    if needs_title {
        Ok(Some(page.title()?))
    } else {
        Ok(None)
    }
}

fn resolve_page_save_target_path(
    path: Option<&Path>,
    name: Option<&str>,
    title: Option<&str>,
    extension: &str,
) -> OpenPageResult<PathBuf> {
    let default_name = title
        .map(sanitize_file_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "page".to_string());
    let mut target = match (path, name) {
        (Some(path), Some(name)) => path.join(sanitize_file_name(name)),
        (Some(path), None) if path.extension().is_some() => path.to_path_buf(),
        (Some(path), None) => path.join(format!("{default_name}.{extension}")),
        (None, Some(name)) => PathBuf::from(sanitize_file_name(name)),
        (None, None) => PathBuf::from(format!("{default_name}.{extension}")),
    };
    if target.extension().is_none() {
        target.set_extension(extension);
    }
    let target = absolutize_path(target)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(target)
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = name
        .trim()
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            _ => ch,
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() {
        "page".to_string()
    } else {
        sanitized
    }
}

fn absolutize_path(path: PathBuf) -> OpenPageResult<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn cookie_header_to_params(url: &Url, cookie_header: &str) -> Vec<CookieParam> {
    cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let (name, value) = item.split_once('=')?;
            let mut cookie = CookieParam::new(name.trim(), value.trim());
            cookie.url = Some(url.to_string());
            Some(cookie)
        })
        .collect()
}

fn build_page_js_expression(script: &str, args: &[Value], as_expr: bool) -> OpenPageResult<String> {
    let args_json =
        serde_json::to_string(args).map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    if as_expr {
        Ok(format!(
            "(() => {{ const __args = {args_json}; return ((...args) => ({script}))(...__args); }})()"
        ))
    } else {
        Ok(format!(
            "(() => {{ const __args = {args_json}; return (function(...args) {{ {script} }}).apply(globalThis, __args); }})()"
        ))
    }
}

fn current_cookie_scope_url(url: String) -> Option<String> {
    let url = url.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

fn load_public_suffix_list() -> Option<PublicSuffixList> {
    let bytes = fs::read(suffixes_list_path()).ok()?;
    PublicSuffixList::from_bytes(&bytes).ok()
}

fn fallback_registrable_domain(host: &str) -> String {
    let labels = host
        .split('.')
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if labels.len() <= 2 {
        return host.to_string();
    }
    let last = labels[labels.len() - 1];
    let second_last = labels[labels.len() - 2];
    if last.len() == 2 && second_last.len() <= 3 && labels.len() >= 3 {
        labels[labels.len() - 3..].join(".")
    } else {
        labels[labels.len() - 2..].join(".")
    }
}

fn registrable_domain_for_host(host: &str) -> String {
    load_public_suffix_list()
        .and_then(|list| list.domain(host.as_bytes()))
        .and_then(|domain| String::from_utf8(domain.as_bytes().to_vec()).ok())
        .unwrap_or_else(|| fallback_registrable_domain(host))
}

fn cookie_domain_candidates_for_url(url: &Url) -> Vec<String> {
    let Some(host) = url.host_str() else {
        return Vec::new();
    };
    let host = host.trim().trim_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Vec::new();
    }
    if host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.') {
        return vec![host];
    }

    let registrable = registrable_domain_for_host(&host);
    if registrable == host {
        return vec![host];
    }

    let Some(prefix) = host.strip_suffix(&format!(".{registrable}")) else {
        return vec![host];
    };
    if prefix.is_empty() {
        return vec![host];
    }

    let mut labels = prefix
        .split('.')
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    labels.push(registrable);

    let mut segments = vec![labels[0].clone()];
    for label in labels.iter().skip(1) {
        segments.push(".".to_string());
        segments.push(label.clone());
    }

    let mut candidates = Vec::new();
    for index in 0..segments.len() {
        let candidate = segments[index..].concat();
        if !candidate.is_empty() && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    if candidates.is_empty() {
        vec![host]
    } else {
        candidates
    }
}

fn permission_origin_from_input(value: &str) -> OpenPageResult<String> {
    let value = value.trim();
    let parsed = Url::parse(value).map_err(|err| {
        OpenPageError::BrowserOperation(invalid_url_message(value, Some(&err.to_string())))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(OpenPageError::BrowserOperation(
            permission_origin_scheme_message(value),
        ));
    }
    Ok(parsed.origin().ascii_serialization())
}

fn resolve_permission_origin(origin: Option<&str>, current_url: &str) -> OpenPageResult<String> {
    if let Some(origin) = origin {
        return permission_origin_from_input(origin);
    }
    permission_origin_from_input(current_url)
        .map_err(|_| OpenPageError::BrowserOperation(permission_origin_required_message()))
}

fn cookie_param(
    name: &str,
    value: &str,
    url: Option<&str>,
    domain: Option<&str>,
    path: Option<&str>,
) -> CookieParam {
    let mut cookie = CookieParam::new(name.trim(), value.trim());
    cookie.url = url
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie.domain = domain
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie.path = path
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie
}

fn browser_cookie_param_from_session_cookie(
    cookie: &SessionCookieParam,
) -> OpenPageResult<CookieParam> {
    let mut param = cookie_param(
        &cookie.name,
        &cookie.value,
        cookie.url.as_deref(),
        cookie.domain.as_deref(),
        cookie.path.as_deref(),
    );
    if cookie.secure {
        param.secure = Some(true);
    }
    if cookie.http_only {
        param.http_only = Some(true);
    }
    if let Some(same_site) = cookie
        .same_site
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        param.same_site = Some(same_site.parse::<CookieSameSite>().map_err(|_| {
            OpenPageError::PageOperation(invalid_cookie_same_site_message(same_site, &cookie.name))
        })?);
    }
    Ok(param)
}

fn page_cookie_matches_scope(
    current_domain: &str,
    expected_domain: Option<&str>,
    expected_url: Option<&str>,
    current_url: Option<&Url>,
) -> bool {
    let current_domain = current_domain.trim().trim_start_matches('.');
    if let Some(domain) = expected_domain {
        return current_domain.eq_ignore_ascii_case(domain.trim().trim_start_matches('.'));
    }
    if let Some(url) = expected_url
        && let Ok(parsed_url) = Url::parse(url)
        && let Some(host) = parsed_url.host_str()
    {
        return current_domain.eq_ignore_ascii_case(host.trim().trim_start_matches('.'));
    }
    if let Some(url) = current_url
        && let Some(host) = url.host_str()
    {
        return current_domain.eq_ignore_ascii_case(host.trim().trim_start_matches('.'));
    }
    true
}

async fn set_cookie_param_exact(page: &OxPage, cookie: CookieParam) -> OpenPageResult<()> {
    execute_page_command_async(
        page,
        DeleteCookiesParams::from_cookie(&cookie),
        "Page::set_cookie_param_exact()",
    )
    .await?;
    execute_page_command_async(
        page,
        SetCookiesParams::new(vec![cookie]),
        "Page::set_cookie_param_exact()",
    )
    .await?;
    Ok(())
}

async fn page_has_cookie(
    page: &OxPage,
    cookie: &SessionCookieParam,
    current_url: Option<&Url>,
) -> OpenPageResult<bool> {
    let cookies = run_page_future_with_cdp_timeout(page.get_cookies(), "read cookies").await?;
    Ok(cookies.into_iter().any(|current| {
        current.name == cookie.name
            && current.value == cookie.value
            && page_cookie_matches_scope(
                &current.domain,
                cookie.domain.as_deref(),
                cookie.url.as_deref(),
                current_url,
            )
    }))
}

async fn set_page_cookie_with_scope_fallback(
    page: &OxPage,
    cookie: &SessionCookieParam,
    current_url: Option<&Url>,
) -> OpenPageResult<()> {
    if cookie.url.is_some() || cookie.domain.is_some() {
        return set_cookie_param_exact(page, browser_cookie_param_from_session_cookie(cookie)?)
            .await;
    }

    let Some(current_url) = current_url else {
        return Err(OpenPageError::PageOperation(
            crate::settings::cookie_requires_url_or_domain_message(&cookie.name),
        ));
    };

    if cookie.name.starts_with("__Host-") {
        let mut scoped = cookie.clone();
        scoped.url = Some(current_url.as_str().to_string());
        scoped.secure = true;
        if scoped.path.is_none() {
            scoped.path = Some("/".to_string());
        }
        return set_cookie_param_exact(page, browser_cookie_param_from_session_cookie(&scoped)?)
            .await;
    }

    for domain in cookie_domain_candidates_for_url(current_url) {
        let mut scoped = cookie.clone();
        scoped.domain = Some(domain);
        if cookie.name.starts_with("__Secure-") {
            scoped.secure = true;
        }
        set_cookie_param_exact(page, browser_cookie_param_from_session_cookie(&scoped)?).await?;
        if page_has_cookie(page, &scoped, Some(current_url)).await? {
            return Ok(());
        }
    }

    let mut scoped = cookie.clone();
    scoped.url = Some(current_url.as_str().to_string());
    if cookie.name.starts_with("__Secure-") {
        scoped.secure = true;
    }
    set_cookie_param_exact(page, browser_cookie_param_from_session_cookie(&scoped)?).await
}

fn collect_frame_ids(frame_tree: &FrameTree, frame_ids: &mut Vec<String>) {
    frame_ids.push(frame_tree.frame.id.as_ref().to_string());
    if let Some(children) = &frame_tree.child_frames {
        for child in children {
            collect_frame_ids(child, frame_ids);
        }
    }
}

fn delete_cookie_params(
    name: &str,
    url: Option<&str>,
    domain: Option<&str>,
    path: Option<&str>,
) -> DeleteCookiesParams {
    let mut params = DeleteCookiesParams::new(name.trim());
    params.url = url
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params.domain = domain
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params.path = path
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params
}

fn runtime_timeout_seconds_to_millis(seconds: f64) -> OpenPageResult<u64> {
    if !seconds.is_finite() || seconds.is_sign_negative() {
        return Err(OpenPageError::PageOperation(
            timeout_must_be_non_negative_message(seconds),
        ));
    }
    Ok((seconds * 1000.0).round() as u64)
}

fn remaining_timeout_ms(deadline: Instant) -> u64 {
    deadline
        .checked_duration_since(Instant::now())
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn resolve_implicit_wait_timeout_ms(configured_timeout_ms: Option<u64>) -> u64 {
    configured_timeout_ms.unwrap_or(10_000)
}

async fn run_with_timeout<T, F>(
    future: F,
    timeout_ms: u64,
    timeout_message: impl Into<String>,
) -> OpenPageResult<T>
where
    F: Future<Output = OpenPageResult<T>>,
{
    let timeout_message = timeout_message.into();
    tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), future)
        .await
        .map_err(|_| OpenPageError::Timeout(timeout_message))?
}

pub(crate) async fn execute_page_command_async<T>(
    page: &OxPage,
    command: T,
    operation: &str,
) -> OpenPageResult<T::Response>
where
    T: Command,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, page.execute(command))
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map(|response| response.result)
        .map_err(|err| page_operation_error(operation, err))
}

pub(crate) fn execute_page_command_blocking<T>(
    runtime: &Runtime,
    page: &OxPage,
    command: T,
    operation: &str,
) -> OpenPageResult<T::Response>
where
    T: Command,
{
    runtime.block_on(execute_page_command_async(page, command, operation))
}

#[cfg(test)]
mod tests {
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
    use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, ExecutionContextId};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use tokio::runtime::Runtime;
    use url::Url;

    use super::{
        Page, PageElementContent, PageElementInfo, PageSaveContent, action_drag_payload,
        browser_cookie_param_from_session_cookie, compose_frame_html,
        cookie_domain_candidates_for_url, cookie_param, default_frame_locator,
        delete_cookie_params, frame_locator, frame_locator_input, history_entry_index,
        is_explicit_locator, marker_xpath, optional_frame_locator_input,
        page_element_info_properties_json, page_operation_error, permission_origin_from_input,
        register_navigation_listener_with_cdp_timeout, remaining_timeout_ms,
        resolve_implicit_wait_timeout_ms, resolve_navigation_local_file_path,
        resolve_page_save_target_path, resolve_page_screenshot_target_path,
        resolve_permission_origin, run_page_future_with_cdp_timeout,
        run_page_lookup_future_with_cdp_timeout, run_with_timeout,
        runtime_timeout_seconds_to_millis, screenshot_clip, storage_lookup_script,
        value_as_f64_pair, value_as_optional_string, value_as_string, value_as_string_vec,
    };
    use crate::element_list::ElementsListExt;
    use crate::error::OpenPageError;
    use crate::session::SessionCookieParam;
    use crate::settings::{
        cdp_timeout_duration, javascript_execution_timed_out_message, scoped_test_settings,
        timeout_duration_millis,
    };
    use crate::{
        Browser, BrowserTabReference, BrowserTabSelector, By, DisconnectedFrame, DisconnectedPage,
        Frame, Keys, LaunchOptions, OpenPageResult, Settings, WebElement, wait_until,
    };

    fn runtime_test_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("openpage-{name}-{}-{unique}", std::process::id()))
    }

    fn poison_mutex<T: Send + 'static>(mutex: Arc<std::sync::Mutex<T>>) {
        let join = thread::spawn(move || {
            let _guard = mutex.lock().expect("lock poisoned test mutex");
            panic!("poison mutex");
        })
        .join();
        assert!(join.is_err(), "poison helper thread should panic");
    }

    fn launch_headless_test_browser_with_args(
        name: &str,
        extra_args: &[&str],
    ) -> crate::OpenPageResult<(Browser, PathBuf)> {
        let temp_dir = runtime_test_temp_dir(name);
        fs::create_dir_all(&temp_dir).expect("create runtime test temp dir");

        let mut options = LaunchOptions::default();
        options.headless(true);
        options.auto_port(true);
        options.new_env(true);
        options.set_tmp_path(&temp_dir);
        options.set_timeouts(Some(1.0), Some(5.0), Some(1.0));
        for arg in extra_args {
            options.set_argument(*arg);
        }

        Browser::launch(options).map(|browser| (browser, temp_dir))
    }

    fn launch_headless_test_browser(name: &str) -> crate::OpenPageResult<(Browser, PathBuf)> {
        launch_headless_test_browser_with_args(name, &[])
    }

    fn pair_from_value(value: Value, label: &str) -> crate::OpenPageResult<(f64, f64)> {
        let values = match value {
            Value::Array(values) => values,
            other => {
                return Err(OpenPageError::PageOperation(format!(
                    "{label} did not return an array: {other}"
                )));
            }
        };
        if values.len() != 2 {
            return Err(OpenPageError::PageOperation(format!(
                "{label} did not return exactly two values"
            )));
        }
        let x = values[0].as_f64().ok_or_else(|| {
            OpenPageError::PageOperation(format!("{label} x was not numeric: {}", values[0]))
        })?;
        let y = values[1].as_f64().ok_or_else(|| {
            OpenPageError::PageOperation(format!("{label} y was not numeric: {}", values[1]))
        })?;
        Ok((x, y))
    }

    fn expected_dp_viewport_screen_origin(
        page: &super::Page,
    ) -> crate::OpenPageResult<(f64, f64, f64)> {
        let window_state = page.window_state()?;
        let (window_left, window_top) = page.window_location()?;
        let (window_width, window_height) = page.window_size()?;
        let (viewport_width, viewport_height) = pair_from_value(
            page.run_js("[window.innerWidth, window.innerHeight]")?,
            "top window viewport size with scrollbar",
        )?;
        let device_pixel_ratio = page
            .run_js("window.devicePixelRatio || 1")?
            .as_f64()
            .ok_or_else(|| {
                OpenPageError::PageOperation("devicePixelRatio was not numeric".to_string())
            })?;

        let (window_left, window_top) =
            if matches!(window_state.as_str(), "maximized" | "fullscreen") {
                (0.0, 0.0)
            } else {
                (window_left as f64 + 7.0, window_top as f64)
            };

        let (window_width, window_height) = match window_state.as_str() {
            "fullscreen" => (window_width as f64, window_height as f64),
            "maximized" => (window_width as f64 - 16.0, window_height as f64 - 16.0),
            _ => (window_width as f64 - 16.0, window_height as f64 - 7.0),
        };

        Ok((
            window_left + window_width - viewport_width,
            window_top + window_height - viewport_height,
            device_pixel_ratio,
        ))
    }

    fn assert_pair_close(actual: (f64, f64), expected: (f64, f64), label: &str) {
        assert!(
            (actual.0 - expected.0).abs() < 1.0,
            "{label} x mismatch: actual={:?}, expected={:?}",
            actual,
            expected
        );
        assert!(
            (actual.1 - expected.1).abs() < 1.0,
            "{label} y mismatch: actual={:?}, expected={:?}",
            actual,
            expected
        );
    }

    fn spawn_download_site() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind download server");
        listener
            .set_nonblocking(true)
            .expect("set download server nonblocking");
        let address = format!(
            "http://{}",
            listener.local_addr().expect("download server addr")
        );
        let handle = thread::spawn(move || {
            let html = r#"<!doctype html>
<html>
<body>
  <a id="download" href="/download">Download</a>
</body>
</html>
"#;
            let payload = b"openpage-download";
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut served_download = false;
            while Instant::now() < deadline && !served_download {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    "/download" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"openpage.txt\"\r\nConnection: close\r\n\r\n",
                            payload.len()
                        );
                        let _ = stream.write_all(payload);
                        served_download = true;
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });
        (address, handle)
    }

    fn spawn_cookie_site() -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind cookie server");
        listener
            .set_nonblocking(true)
            .expect("set cookie server nonblocking");
        let port = listener.local_addr().expect("cookie server addr").port();
        let handle = thread::spawn(move || {
            let html = "<!doctype html><html><body id=\"root\">cookie</body></html>";
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut served_html = false;
            while Instant::now() < deadline && !served_html {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                        served_html = true;
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });
        (port, handle)
    }

    fn spawn_delayed_load_site(delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind delayed load server");
        listener
            .set_nonblocking(true)
            .expect("set delayed load server nonblocking");
        let address = format!(
            "http://{}",
            listener.local_addr().expect("delayed load server addr")
        );
        let handle = thread::spawn(move || {
            let html = r#"<!doctype html>
<html>
<head>
  <script defer src="/slow.js"></script>
</head>
<body data-ready="pending">
  <div id="status">pending</div>
</body>
</html>
"#;
            let script = "document.body.dataset.ready = 'loaded'; document.getElementById('status').textContent = 'loaded'; window.__delayedScriptLoaded = true;";
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut served_html = false;
            let mut served_script = false;
            while Instant::now() < deadline && !(served_html && served_script) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                        served_html = true;
                    }
                    "/slow.js" => {
                        thread::sleep(delay);
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/javascript; charset=utf-8\r\nConnection: close\r\n\r\n{script}",
                            script.len()
                        );
                        served_script = true;
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });
        (address, handle)
    }

    fn spawn_cross_origin_iframe_site() -> (String, thread::JoinHandle<()>, thread::JoinHandle<()>)
    {
        let child_listener = TcpListener::bind("127.0.0.1:0").expect("bind child iframe server");
        child_listener
            .set_nonblocking(true)
            .expect("set child iframe server nonblocking");
        let child_address = format!(
            "http://{}",
            child_listener
                .local_addr()
                .expect("child iframe server addr")
        );

        let parent_listener = TcpListener::bind("127.0.0.1:0").expect("bind parent iframe server");
        parent_listener
            .set_nonblocking(true)
            .expect("set parent iframe server nonblocking");
        let parent_address = format!(
            "http://{}",
            parent_listener
                .local_addr()
                .expect("parent iframe server addr")
        );

        let child_url = format!("{child_address}/child");
        let child_handle = thread::spawn(move || {
            let html = r#"<!doctype html>
<html>
<head><title>Cross Origin Child</title></head>
<body style="margin:0;height:1600px;">
  <div
    id="inner-box"
    style="position:absolute;left:56px;top:88px;width:96px;height:58px;border:3px solid #111;padding:5px;background:#eee;"
  ></div>
</body>
</html>
"#;
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let (mut stream, _) = match child_listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/child" | "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        let parent_handle = thread::spawn(move || {
            let html = format!(
                r#"<!doctype html>
<html>
<body style="margin:0;">
  <iframe
    id="cross-frame"
    style="position:absolute;left:170px;top:110px;width:430px;height:280px;border:0;"
    src="{child_url}"
  ></iframe>
</body>
</html>
"#
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let (mut stream, _) = match parent_listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/parent" | "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        (
            format!("{parent_address}/parent"),
            parent_handle,
            child_handle,
        )
    }

    fn spawn_nested_cross_origin_iframe_site() -> (
        String,
        thread::JoinHandle<()>,
        thread::JoinHandle<()>,
        thread::JoinHandle<()>,
    ) {
        let grandchild_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind grandchild iframe server");
        grandchild_listener
            .set_nonblocking(true)
            .expect("set grandchild iframe server nonblocking");
        let grandchild_address = format!(
            "http://{}",
            grandchild_listener
                .local_addr()
                .expect("grandchild iframe server addr")
        );
        let grandchild_url = format!("{grandchild_address}/grandchild");

        let child_listener = TcpListener::bind("127.0.0.1:0").expect("bind nested child server");
        child_listener
            .set_nonblocking(true)
            .expect("set nested child server nonblocking");
        let child_address = format!(
            "http://{}",
            child_listener
                .local_addr()
                .expect("nested child server addr")
        );
        let child_url = format!("{child_address}/child");

        let parent_listener = TcpListener::bind("127.0.0.1:0").expect("bind nested parent server");
        parent_listener
            .set_nonblocking(true)
            .expect("set nested parent server nonblocking");
        let parent_address = format!(
            "http://{}",
            parent_listener
                .local_addr()
                .expect("nested parent server addr")
        );

        let grandchild_handle = thread::spawn(move || {
            let html = r#"<!doctype html>
<html>
<head><title>Nested Cross Origin Grandchild</title></head>
<body style="margin:0;height:1400px;">
  <div
    id="deep-box"
    style="position:absolute;left:44px;top:70px;width:88px;height:52px;border:3px solid #111;padding:5px;background:#eee;"
  ></div>
</body>
</html>
"#;
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let (mut stream, _) = match grandchild_listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/grandchild" | "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        let child_handle = thread::spawn(move || {
            let html = format!(
                r#"<!doctype html>
<html>
<head><title>Nested Cross Origin Child</title></head>
<body style="margin:0;">
  <iframe
    id="inner-frame"
    style="position:absolute;left:90px;top:60px;width:240px;height:180px;border:0;"
    src="{grandchild_url}"
  ></iframe>
</body>
</html>
"#
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let (mut stream, _) = match child_listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/child" | "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        let parent_handle = thread::spawn(move || {
            let html = format!(
                r#"<!doctype html>
<html>
<body style="margin:0;">
  <iframe
    id="outer-frame"
    style="position:absolute;left:170px;top:110px;width:430px;height:280px;border:0;"
    src="{child_url}"
  ></iframe>
</body>
</html>
"#
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let (mut stream, _) = match parent_listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/parent" | "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        (
            format!("{parent_address}/parent"),
            parent_handle,
            child_handle,
            grandchild_handle,
        )
    }

    #[test]
    fn page_operation_errors_follow_settings_language() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english = page_operation_error("read title", "boom").to_string();
        assert_eq!(
            english,
            "page operation failed: page operation read title failed: boom"
        );

        Settings::set_language("cn");

        let chinese = page_operation_error("read title", "boom").to_string();
        assert_eq!(chinese, "页面操作失败: 页面操作 read title 失败: boom");
    }

    #[test]
    fn history_entry_index_moves_backward() {
        assert_eq!(history_entry_index(3, 5, -2), Some(1));
    }

    #[test]
    fn history_entry_index_returns_none_when_offset_leaves_bounds() {
        assert_eq!(history_entry_index(0, 5, -1), None);
        assert_eq!(history_entry_index(4, 5, 1), None);
    }

    #[test]
    fn storage_lookup_script_returns_item_lookup() {
        let script = storage_lookup_script("sessionStorage", Some("token")).expect("script");
        assert!(script.contains("sessionStorage.getItem"));
        assert!(script.contains(&json!("token").to_string()));
    }

    #[test]
    fn storage_lookup_script_returns_full_dump() {
        let script = storage_lookup_script("localStorage", None).expect("script");
        assert!(script.contains("localStorage.length"));
        assert!(script.contains("return result"));
    }

    #[test]
    fn frame_locator_uses_name_or_id_lookup_for_plain_strings() {
        assert_eq!(
            frame_locator("demo-frame"),
            r#"xpath://*[(name()="iframe" or name()="frame") and (@name="demo-frame" or @id="demo-frame")]"#
        );
    }

    #[test]
    fn frame_locator_keeps_explicit_locators() {
        assert!(is_explicit_locator("css:iframe.demo"));
        assert_eq!(frame_locator("css:iframe.demo"), "css:iframe.demo");
        assert_eq!(
            default_frame_locator(),
            r#"xpath://*[name()="iframe" or name()="frame"]"#
        );
    }

    #[test]
    fn frame_locator_input_accepts_by_tuples() {
        assert_eq!(
            frame_locator_input((By::ID, "demo-frame")).expect("by id frame locator"),
            "@id=demo-frame"
        );
        assert_eq!(
            optional_frame_locator_input(Some((By::TAG_NAME, "iframe")))
                .expect("by tag frame locator"),
            "tag:iframe"
        );
        assert_eq!(
            optional_frame_locator_input(None::<&str>).expect("default frame locator"),
            default_frame_locator()
        );
    }

    #[test]
    fn marker_xpath_targets_global_marker_attribute() {
        assert_eq!(
            marker_xpath("openpage-page-1"),
            r#"xpath://*[@data-openpage-page-marker="openpage-page-1"]"#
        );
    }

    #[test]
    fn page_element_info_accepts_pairs_and_maps() {
        let pair_items = [
            ("innerText", "DrissionPage"),
            ("href", "https://drissionpage.cn"),
        ];
        let string_items = vec![
            ("innerText".to_string(), "OpenPage".to_string()),
            ("target".to_string(), "_blank".to_string()),
        ];
        let mut map_items = HashMap::new();
        map_items.insert("innerText".to_string(), "Detached".to_string());
        map_items.insert("href".to_string(), "https://example.test".to_string());

        let from_pairs = PageElementInfo::from(("a", &pair_items));
        let from_strings = PageElementInfo::from(("a", &string_items));
        let from_map = PageElementInfo::from(("a", &map_items));

        assert_eq!(from_pairs.tag(), "a");
        assert_eq!(from_pairs.properties.len(), 2);
        assert_eq!(from_strings.properties.len(), 2);
        assert_eq!(from_map.tag(), "a");
        assert_eq!(from_map.properties.len(), 2);
    }

    #[test]
    fn page_element_info_accepts_json_value_maps() {
        let pair_items = [("tabIndex", json!(3)), ("hidden", json!(true))];
        let value_items = vec![
            ("innerText".to_string(), json!("OpenPage")),
            ("draggable".to_string(), json!(false)),
        ];
        let mut map_items = HashMap::new();
        map_items.insert("value".to_string(), json!(12));
        map_items.insert("disabled".to_string(), Value::Bool(true));

        let from_pairs = PageElementInfo::from(("button", &pair_items));
        let from_values = PageElementInfo::from(("button", &value_items));
        let from_map = PageElementInfo::from(("button", &map_items));

        assert_eq!(from_pairs.properties.len(), 2);
        assert!(
            from_pairs
                .properties
                .iter()
                .any(|(name, value)| { name == "tabIndex" && value == &json!(3) })
        );
        assert!(
            from_values
                .properties
                .iter()
                .any(|(name, value)| { name == "draggable" && value == &json!(false) })
        );
        assert!(
            from_map
                .properties
                .iter()
                .any(|(name, value)| { name == "disabled" && value == &Value::Bool(true) })
        );
    }

    #[test]
    fn page_element_info_properties_json_serializes_json_scalars() {
        let info = PageElementInfo::from((
            "button",
            [
                ("innerText", json!("DrissionPage")),
                ("tabIndex", json!(3)),
                ("disabled", Value::Bool(false)),
            ],
        ));
        let properties = page_element_info_properties_json(&info).expect("properties json");

        assert!(properties.contains(r#""innerText":"DrissionPage""#));
        assert!(properties.contains(r#""tabIndex":3"#));
        assert!(properties.contains(r#""disabled":false"#));
    }

    #[test]
    fn page_element_content_accepts_html_and_info_inputs() {
        let html = PageElementContent::from("<button>demo</button>");
        let info = PageElementContent::from(("button", [("disabled", json!(true))]));

        match html {
            PageElementContent::Html(value) => {
                assert_eq!(value.as_ref(), "<button>demo</button>");
            }
            other => panic!("expected html content, got {other:?}"),
        }

        match info {
            PageElementContent::Info(info) => {
                assert_eq!(info.tag(), "button");
                assert_eq!(info.properties.len(), 1);
            }
            other => panic!("expected info content, got {other:?}"),
        }
    }

    #[test]
    fn compose_frame_html_reuses_opening_tag() {
        assert_eq!(
            compose_frame_html(
                "iframe",
                r#"<iframe id="demo"></iframe>"#,
                "<html>inner</html>"
            ),
            r#"<iframe id="demo"><html>inner</html></iframe>"#
        );
    }

    #[test]
    fn remaining_timeout_ms_clamps_elapsed_deadlines() {
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(10))
            .expect("expired instant");
        let future = Instant::now() + Duration::from_millis(50);

        assert_eq!(remaining_timeout_ms(expired), 0);
        assert!(remaining_timeout_ms(future) <= 50);
    }

    #[test]
    fn resolve_implicit_wait_timeout_ms_prefers_configured_value() {
        assert_eq!(resolve_implicit_wait_timeout_ms(Some(2500)), 2500);
        assert_eq!(resolve_implicit_wait_timeout_ms(Some(0)), 0);
        assert_eq!(resolve_implicit_wait_timeout_ms(None), 10_000);
    }

    #[test]
    fn run_with_timeout_times_out_slow_future() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let error = runtime
            .block_on(run_with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok::<_, OpenPageError>(())
                },
                1,
                "javascript execution timed out",
            ))
            .expect_err("future should time out");

        assert!(error.to_string().contains("javascript execution timed out"));
    }

    #[test]
    fn run_with_timeout_accepts_localized_timeout_message() {
        let _settings = scoped_test_settings();
        Settings::set_language("cn");

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let error = runtime
            .block_on(run_with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok::<_, OpenPageError>(())
                },
                1,
                javascript_execution_timed_out_message(),
            ))
            .expect_err("future should time out");

        assert!(error.to_string().contains("JavaScript 执行超时"));
    }

    #[test]
    fn screenshot_clip_requires_complete_bounds() {
        assert!(screenshot_clip(None, None).expect("no clip").is_none());
        assert!(screenshot_clip(Some((0.0, 0.0)), None).is_err());
        assert!(
            screenshot_clip(Some((0.0, 0.0)), Some((10.0, 10.0)))
                .expect("clip")
                .is_some()
        );
    }

    #[test]
    fn resolve_page_screenshot_target_path_defaults_to_title() {
        let path =
            resolve_page_screenshot_target_path(None, None, Some("Open:Page")).expect("path");
        assert!(path.is_absolute());
        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("Open_Page.png")
        );
    }

    #[test]
    fn resolve_page_save_target_path_defaults_to_title_and_extension() {
        let path = resolve_page_save_target_path(None, None, Some("Open:Page"), "mhtml")
            .expect("save path");
        assert!(path.is_absolute());
        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("Open_Page.mhtml")
        );
    }

    #[test]
    fn cookie_param_keeps_optional_scope_fields() {
        let cookie = cookie_param(
            "foo",
            "bar",
            Some("https://example.com/demo"),
            Some("example.com"),
            Some("/demo"),
        );
        assert_eq!(cookie.name, "foo");
        assert_eq!(cookie.value, "bar");
        assert_eq!(cookie.url.as_deref(), Some("https://example.com/demo"));
        assert_eq!(cookie.domain.as_deref(), Some("example.com"));
        assert_eq!(cookie.path.as_deref(), Some("/demo"));
    }

    #[test]
    fn delete_cookie_params_skip_blank_scope_fields() {
        let params = delete_cookie_params("foo", Some(" "), Some("example.com"), Some(""));
        assert_eq!(params.name, "foo");
        assert!(params.url.is_none());
        assert_eq!(params.domain.as_deref(), Some("example.com"));
        assert!(params.path.is_none());
    }

    #[test]
    fn cookie_domain_candidates_follow_configured_suffix_list() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let temp_dir = runtime_test_temp_dir("suffixes-list");
        fs::create_dir_all(&temp_dir).expect("create suffixes temp dir");
        let suffix_path = temp_dir.join("suffixes.dat");
        fs::write(&suffix_path, "// BEGIN ICANN DOMAINS\nwild.test\nco.uk\n")
            .expect("write custom suffix list");
        Settings::set_suffixes_list(&suffix_path);

        let url =
            Url::parse("https://www.example.wild.test/path").expect("parse custom suffix list url");
        assert_eq!(
            cookie_domain_candidates_for_url(&url),
            vec![
                "www.example.wild.test".to_string(),
                ".example.wild.test".to_string(),
                "example.wild.test".to_string(),
            ]
        );

        let uk_url =
            Url::parse("https://shop.service.example.co.uk/path").expect("parse co.uk url");
        assert_eq!(
            cookie_domain_candidates_for_url(&uk_url),
            vec![
                "shop.service.example.co.uk".to_string(),
                ".service.example.co.uk".to_string(),
                "service.example.co.uk".to_string(),
                ".example.co.uk".to_string(),
                "example.co.uk".to_string(),
            ]
        );
    }

    #[test]
    fn cookie_same_site_validation_follows_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let cookie = SessionCookieParam {
            name: "sid".to_string(),
            value: "1".to_string(),
            url: Some("https://example.test/".to_string()),
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: Some("Broken".to_string()),
        };

        let english = browser_cookie_param_from_session_cookie(&cookie)
            .expect_err("english same_site validation should fail");
        assert!(matches!(
            english,
            OpenPageError::PageOperation(ref message)
                if message.contains("invalid cookie same_site `Broken` for `sid`")
        ));

        Settings::set_language("cn");

        let chinese = browser_cookie_param_from_session_cookie(&cookie)
            .expect_err("chinese same_site validation should fail");
        assert!(matches!(
            chinese,
            OpenPageError::PageOperation(ref message)
                if message.contains("cookie `sid` 的 same_site `Broken` 无效")
        ));
    }

    #[test]
    fn page_navigation_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_file = resolve_navigation_local_file_path("file://example.com/path")
            .expect_err("english file url validation should fail");
        assert!(matches!(
            english_file,
            OpenPageError::PageOperation(ref message)
                if message.contains("invalid file url: file://example.com/path")
        ));

        let english_timeout = runtime_timeout_seconds_to_millis(f64::NAN)
            .expect_err("english timeout validation should fail");
        assert!(matches!(
            english_timeout,
            OpenPageError::PageOperation(ref message)
                if message.contains("timeout must be a finite non-negative number")
        ));

        Settings::set_language("cn");

        let chinese_file = resolve_navigation_local_file_path("file://example.com/path")
            .expect_err("chinese file url validation should fail");
        assert!(matches!(
            chinese_file,
            OpenPageError::PageOperation(ref message)
                if message.contains("无效的 file url: file://example.com/path")
        ));

        let chinese_timeout = runtime_timeout_seconds_to_millis(f64::NAN)
            .expect_err("chinese timeout validation should fail");
        assert!(matches!(
            chinese_timeout,
            OpenPageError::PageOperation(ref message)
                if message.contains("timeout 必须是有限且非负的数字")
        ));
    }

    #[test]
    fn page_host_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_drag = action_drag_payload(crate::ActionsDragData::files(Vec::<String>::new()))
            .expect_err("empty drag file list should fail");
        assert!(matches!(
            english_drag,
            OpenPageError::PageOperation(ref message)
                if message.contains("drag_in() requires at least one file path")
        ));

        let english_clip = screenshot_clip(Some((10.0, 10.0)), Some((5.0, 20.0)))
            .expect_err("invalid screenshot clip order should fail");
        assert!(matches!(
            english_clip,
            OpenPageError::PageOperation(ref message)
                if message.contains(
                    "screenshot clip requires right_bottom to be greater than left_top"
                )
        ));

        let english_origin = permission_origin_from_input("ftp://example.test")
            .expect_err("permission origin scheme should fail");
        assert!(matches!(
            english_origin,
            OpenPageError::BrowserOperation(ref message)
                if message.contains("permission origin must use http or https")
        ));

        Settings::set_language("cn");

        let chinese_drag = action_drag_payload(crate::ActionsDragData::files(vec![""]))
            .expect_err("empty drag file path should fail");
        assert!(matches!(
            chinese_drag,
            OpenPageError::PageOperation(ref message)
                if message.contains("drag_in() 文件路径不能为空")
        ));

        let chinese_clip = screenshot_clip(Some((10.0, 10.0)), None)
            .expect_err("partial screenshot clip should fail");
        assert!(matches!(
            chinese_clip,
            OpenPageError::PageOperation(ref message)
                if message.contains("截图裁剪需要同时提供 left_top 和 right_bottom")
        ));

        let chinese_origin = permission_origin_from_input("ftp://example.test")
            .expect_err("permission origin scheme should localize");
        assert!(matches!(
            chinese_origin,
            OpenPageError::BrowserOperation(ref message)
                if message.contains("permission origin 必须使用 http 或 https")
        ));
    }

    #[test]
    fn page_value_type_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_string =
            value_as_string(Value::Null, "demo").expect_err("string value conversion should fail");
        assert!(matches!(
            english_string,
            OpenPageError::JavaScript(ref message)
                if message.contains("demo did not return a string: null")
        ));

        let english_entry = value_as_string_vec(json!(["ok", 1]), "demo")
            .expect_err("string vector entry conversion should fail");
        assert!(matches!(
            english_entry,
            OpenPageError::JavaScript(ref message)
                if message.contains("demo returned a non-string entry: 1")
        ));

        Settings::set_language("cn");

        let chinese_optional = value_as_optional_string(json!(1), "demo")
            .expect_err("optional string value conversion should localize");
        assert!(matches!(
            chinese_optional,
            OpenPageError::JavaScript(ref message)
                if message.contains("demo 未返回字符串或 null: 1")
        ));

        let chinese_pair =
            value_as_f64_pair(json!([1, "x"]), "demo").expect_err("pair conversion should fail");
        assert!(matches!(
            chinese_pair,
            OpenPageError::JavaScript(ref message)
                if message.contains("demo second 条目不是数字")
        ));
    }

    #[test]
    fn browser_backed_page_method_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        assert_eq!(
            super::browser_backed_page_method_message("tabs_count"),
            "tabs_count() is only available on browser-backed pages"
        );

        Settings::set_language("cn");

        assert_eq!(
            super::browser_backed_page_method_message("tabs_count"),
            "tabs_count() 仅适用于 browser-backed 页面"
        );
    }

    #[test]
    fn page_browser_backed_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("page-browser-backed-l10n")
            .expect("launch headless browser");

        let result = (|| -> OpenPageResult<()> {
            let page = browser.new_page(None)?;
            let english_zoom = page
                .set_zoom_factor(0.0)
                .expect_err("invalid zoom factor should fail");
            assert!(matches!(
                english_zoom,
                OpenPageError::BrowserOperation(ref message)
                    if message.contains("zoom factor must be a positive finite number")
            ));
            let mut english_actions = page.new_actions();
            let english_action = match english_actions.wait(-0.1, None) {
                Err(error) => error,
                Ok(_) => panic!("negative action wait should fail"),
            };
            assert!(matches!(
                english_action,
                OpenPageError::PageOperation(ref message)
                    if message.contains("wait() seconds must be >= 0")
            ));

            let detached = Page {
                browser: None,
                browser_pid: None,
                ..page.clone()
            };

            let english = detached
                .download_file_exists_mode()
                .expect_err("download_file_exists_mode() should require browser backing");
            assert!(matches!(
                english,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains(
                        "download_file_exists_mode() is only available on browser-backed pages"
                )
            ));
            let english_window = detached
                .window_hide()
                .expect_err("window_hide should require launched browser pid");
            assert!(matches!(
                english_window,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains(
                        "window hide() is only available for launched browser instances"
                    )
            ));

            Settings::set_language("cn");

            let chinese_permission = page
                .set_permission(
                    "clipboard-read",
                    "maybe",
                    Some("https://example.test"),
                    None,
                )
                .expect_err("invalid permission setting should fail");
            assert!(matches!(
                chinese_permission,
                OpenPageError::BrowserOperation(ref message)
                    if message.contains("permission setting 必须是 granted/denied/prompt 之一")
            ));
            let mut chinese_actions = page.new_actions();
            let chinese_type = match chinese_actions.type_with_interval("x", -0.1) {
                Err(error) => error,
                Ok(_) => panic!("negative action type interval should fail"),
            };
            assert!(matches!(
                chinese_type,
                OpenPageError::PageOperation(ref message)
                    if message.contains("type_with_interval() 秒数必须 >= 0")
            ));
            let chinese_click = match chinese_actions.m_click(None::<&str>, 0) {
                Err(error) => error,
                Ok(_) => panic!("zero action click count should fail"),
            };
            assert!(matches!(
                chinese_click,
                OpenPageError::PageOperation(ref message)
                    if message.contains("click() 次数必须 >= 1")
            ));
            let chinese_clipboard = page
                .clipboard_read_text()
                .expect_err("about:blank clipboard should require secure context");
            assert!(matches!(
                chinese_clipboard,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains(
                        "clipboard_read_text() 需要 secure-context 页面并支持 navigator.clipboard"
                    )
            ));
            let chinese_origin = resolve_permission_origin(None, "about:blank")
                .expect_err("permission origin should require http(s)");
            assert!(matches!(
                chinese_origin,
                OpenPageError::BrowserOperation(ref message)
                    if message.contains("permission override 需要 http(s) 页面或显式 --origin")
            ));

            let chinese_retry = detached
                .retry_times()
                .expect_err("retry_times() should require browser backing");
            assert!(matches!(
                chinese_retry,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains("retry_times() 仅适用于 browser-backed 页面")
            ));
            let chinese_timeout = detached
                .set_timeouts(Some(1.0), None, None)
                .expect_err("set_timeouts() should require browser backing");
            assert!(matches!(
                chinese_timeout,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains("set_timeouts() 仅适用于 browser-backed 页面")
            ));
            let chinese_wait = detached
                .wait_for_downloads_done(10, true)
                .expect_err("wait_for_downloads_done() should require browser backing");
            assert!(matches!(
                chinese_wait,
                OpenPageError::UnsupportedOperation(ref message)
                    if message.contains("wait_for_downloads_done() 仅适用于 browser-backed 页面")
            ));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page browser-backed errors should localize");
    }

    #[test]
    fn page_lock_poisoned_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("page-lock-poisoned-l10n")
            .expect("launch headless browser");

        let result = (|| -> OpenPageResult<()> {
            let page = browser.new_page(None)?;

            poison_mutex(Arc::clone(&page.none_element_config));
            let english = page
                .set_raise_when_ele_not_found(true)
                .expect_err("set_raise_when_ele_not_found() should surface poisoned config")
                .to_string();
            assert!(english.contains("none element runtime config lock poisoned"));

            Settings::set_language("cn");

            poison_mutex(Arc::clone(&page.init_scripts));
            let chinese = page
                .remove_init_js(None)
                .expect_err("remove_init_js(None) should localize poisoned init script state")
                .to_string();
            assert!(chinese.contains("页面初始化脚本锁已损坏"));

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page lock poisoned localization regression");
    }

    #[test]
    fn page_set_cookies_accepts_scope_free_cookie_on_http_page() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (port, server) = spawn_cookie_site();
        let (browser, temp_dir) =
            launch_headless_test_browser("page-cookie-scope").expect("launch headless browser");

        let result = (|| -> OpenPageResult<()> {
            let url = format!("http://localhost:{port}/");
            let page = browser.new_page(Some(url.as_str()))?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            page.set_cookies("sid=abc")?;

            let cookie_header = page.cookie_header()?.unwrap_or_default();
            assert!(
                cookie_header.contains("sid=abc"),
                "cookie header should include sid=abc, got {cookie_header}"
            );
            let cookies = page.cookies()?;
            assert!(
                cookies
                    .iter()
                    .any(|cookie| cookie.name == "sid" && cookie.value == "abc"),
                "cookie list should include sid=abc, got {cookies:?}"
            );
            Ok(())
        })();

        if let Err(err) = browser.close() {
            panic!("close headless browser: {err}");
        }
        server.join().expect("join cookie server");
        let _ = fs::remove_dir_all(&temp_dir);

        result.expect("page set_cookies scope fallback regression");
    }

    #[test]
    fn page_add_ele_info_returns_detached_element_without_dom_residue() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-add-ele-detached").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js("(() => { document.body.innerHTML = ''; return true; })()")?;

            let marker = format!(
                "detached-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time")
                    .as_nanos()
            );
            let info = [
                ("innerText", json!("Detached link")),
                ("title", json!("detached-title")),
                ("data-openpage-detached-test", json!(marker.clone())),
            ];

            let element = page.add_ele(("a", &info), None::<&str>, None::<&str>)?;

            assert_eq!(
                element.attr("data-openpage-detached-test")?,
                Some(marker.clone())
            );
            assert_eq!(
                element.run_js("return this.isConnected;")?,
                Value::Bool(false)
            );
            let selector = format!("css:[data-openpage-detached-test=\"{marker}\"]");
            assert_eq!(page.find_all(&selector)?.len(), 0);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("detached add_ele(info) runtime regression");
    }

    #[test]
    fn page_and_frame_js_helper_wrappers_support_args_async_and_init_scripts() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-frame-js-helpers").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let page_script_path = temp_dir.join("page-run-js.js");
            let page_args_script_path = temp_dir.join("page-run-js-args.js");
            fs::write(&page_script_path, "return 40 + 2;")
                .map_err(|err| OpenPageError::Io(format!("write page js file: {err}")))?;
            fs::write(
                &page_args_script_path,
                "return arguments[0] + arguments[1];",
            )
            .map_err(|err| OpenPageError::Io(format!("write page args js file: {err}")))?;

            assert_eq!(
                page.run_js(page_script_path.to_str().ok_or_else(|| {
                    OpenPageError::PageOperation("page script path was not valid utf-8".to_string())
                })?,)?,
                Value::from(42)
            );

            assert_eq!(
                page.run_js_with_args(
                    page_args_script_path.to_str().ok_or_else(|| {
                        OpenPageError::PageOperation(
                            "page args script path was not valid utf-8".to_string(),
                        )
                    })?,
                    &[Value::from(1), Value::from(2)],
                    false,
                )?,
                Value::from(3)
            );
            assert_eq!(
                page.run_js_with_options(
                    "2 + 3",
                    &[Value::from(2), Value::from(3)],
                    true,
                    Some(1_000),
                )?,
                Value::from(5)
            );
            assert_eq!(page.run_js_loaded("return 20 + 1;")?, Value::from(21));

            page.run_async_js("setTimeout(() => { window.__pageAsync = 'done'; }, 0);")?;
            wait_until(Duration::from_millis(1_500), || {
                match page.run_js("window.__pageAsync || null").ok()? {
                    Value::String(value) if value == "done" => Some(()),
                    _ => None,
                }
            })?;

            let set_iframe = |text: &str| -> crate::OpenPageResult<()> {
                let srcdoc = serde_json::to_string(&format!(
                    "<html><body><button id='inner'>{text}</button></body></html>"
                ))
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
                page.run_js(&format!(
                    "(() => {{ \
                        document.body.innerHTML = '<iframe id=\"demo-frame\"></iframe>'; \
                        document.getElementById('demo-frame').srcdoc = {srcdoc}; \
                        return true; \
                    }})()"
                ))?;
                Ok(())
            };

            let page_init_id =
                page.add_init_js("window.__pageFrameInit = (window.__pageFrameInit || 0) + 1;")?;

            set_iframe("first")?;
            let frame = wait_until(Duration::from_millis(1_500), || {
                page.get_frame_context(1usize).ok()
            })?;

            assert_eq!(
                frame.run_js_with_args(
                    page_args_script_path.to_str().ok_or_else(|| {
                        OpenPageError::PageOperation(
                            "page args script path was not valid utf-8".to_string(),
                        )
                    })?,
                    &[Value::from(4), Value::from(5)],
                    false,
                )?,
                Value::from(9)
            );
            assert_eq!(
                frame.run_js_with_options(
                    "6 + 7",
                    &[Value::from(6), Value::from(7)],
                    true,
                    Some(1_000),
                )?,
                Value::from(13)
            );
            assert_eq!(frame.run_js_loaded("return 8 + 1;")?, Value::from(9));
            frame.run_async_js("setTimeout(() => { window.__frameAsync = 'done'; }, 0);")?;
            wait_until(Duration::from_millis(1_500), || {
                match frame.run_js("window.__frameAsync || null").ok()? {
                    Value::String(value) if value == "done" => Some(()),
                    _ => None,
                }
            })?;
            assert_eq!(frame.run_js("window.__pageFrameInit || 0")?, Value::from(1));

            let frame_init_id = frame
                .add_init_js("window.__frameWrapperInit = (window.__frameWrapperInit || 0) + 1;")?;

            set_iframe("second")?;
            let second_frame = wait_until(Duration::from_millis(1_500), || {
                page.get_frame_context(1usize).ok()
            })?;
            assert_eq!(
                second_frame.run_js("window.__pageFrameInit || 0")?,
                Value::from(1)
            );
            assert_eq!(
                second_frame.run_js("window.__frameWrapperInit || 0")?,
                Value::from(1)
            );

            frame.remove_init_js(Some(&frame_init_id))?;
            set_iframe("third")?;
            let third_frame = wait_until(Duration::from_millis(1_500), || {
                page.get_frame_context(1usize).ok()
            })?;
            assert_eq!(
                third_frame.run_js("window.__pageFrameInit || 0")?,
                Value::from(1)
            );
            assert_eq!(
                third_frame.run_js("window.__frameWrapperInit || 0")?,
                Value::from(0)
            );

            page.remove_init_js(Some(&page_init_id))?;
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page/frame js helper runtime regression");
    }

    #[test]
    fn page_run_js_loaded_waits_for_loaded_document_before_evaluating() {
        let (load_url, load_server) = spawn_delayed_load_site(Duration::from_millis(250));
        let (browser, temp_dir) =
            launch_headless_test_browser("page-run-js-loaded").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let load_url_json = serde_json::to_string(&load_url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            page.run_js(&format!("window.location.href = {load_url_json};"))?;
            assert!(page.wait_for_load_start(1_000)?);

            wait_until(Duration::from_millis(1_500), || {
                match page
                    .run_js("document.body ? document.body.dataset.ready : null")
                    .ok()?
                {
                    Value::String(value) if value == "pending" => Some(()),
                    _ => None,
                }
            })?;

            assert_eq!(
                page.run_js_loaded_with_options(
                    "document.body.dataset.ready",
                    &[],
                    true,
                    Some(1_000)
                )?,
                Value::from("loaded")
            );
            assert_eq!(
                page.run_js_loaded_with_args(
                    "return document.getElementById('status').textContent + arguments[0];",
                    &[Value::from("-ok")],
                    false,
                )?,
                Value::from("loaded-ok")
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        let server_result = load_server.join();

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        if let Err(err) = server_result {
            panic!("join delayed load server: {err:?}");
        }
        result.expect("run_js_loaded should wait for delayed document load");
    }

    #[test]
    fn page_retry_and_timeouts_runtime_settings_update_browser_backed_page() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-runtime-settings").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert_eq!(page.retry_times()?, 3);
            assert_eq!(page.retry_interval()?, 2.0);
            assert_eq!(page.timeouts()?.get("base").copied(), Some(1.0));
            assert_eq!(page.timeouts()?.get("page_load").copied(), Some(5.0));
            assert_eq!(page.timeouts()?.get("script").copied(), Some(1.0));

            page.set_retry(Some(5), Some(0.25))?;
            page.set_timeouts(Some(1.5), Some(6.0), Some(0.75))?;

            assert_eq!(page.retry_times()?, 5);
            assert_eq!(page.retry_interval()?, 0.25);
            let timeouts = page.timeouts()?;
            assert_eq!(timeouts.get("base").copied(), Some(1.5));
            assert_eq!(timeouts.get("page_load").copied(), Some(6.0));
            assert_eq!(timeouts.get("script").copied(), Some(0.75));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page runtime retry/timeout settings regression");
    }

    #[test]
    fn page_set_wrapper_updates_storage_and_runtime_settings() {
        let (page_url, page_server) = spawn_delayed_load_site(Duration::from_millis(0));
        let (browser, temp_dir) =
            launch_headless_test_browser("page-set-wrapper").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            page.goto(&page_url)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            page.set()
                .session_storage("set-wrapper-session", Some("one"))?;
            page.set().local_storage("set-wrapper-local", Some("two"))?;
            assert_eq!(
                page.session_storage(Some("set-wrapper-session"))?,
                Value::from("one")
            );
            assert_eq!(
                page.local_storage(Some("set-wrapper-local"))?,
                Value::from("two")
            );

            page.set().load_mode().eager()?;
            assert_eq!(page.load_mode()?, "eager");
            page.set().retry_times(4)?;
            page.set().retry_interval(0.5)?;
            page.set().timeouts(Some(2.0), Some(6.0), Some(1.5))?;

            assert_eq!(page.retry_times()?, 4);
            assert_eq!(page.retry_interval()?, 0.5);
            let timeouts = page.timeouts()?;
            assert_eq!(timeouts.get("base").copied(), Some(2.0));
            assert_eq!(timeouts.get("page_load").copied(), Some(6.0));
            assert_eq!(timeouts.get("script").copied(), Some(1.5));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        let server_result = page_server.join();

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        if let Err(err) = server_result {
            panic!("join page set wrapper server: {err:?}");
        }
        result.expect("page set wrapper regression");
    }

    #[test]
    fn page_scroll_wrapper_controls_page_scroll_position() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-scroll-wrapper").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div style="height:4000px;width:4000px"></div>';
                    document.documentElement.scrollTop = 0;
                    document.documentElement.scrollLeft = 0;
                    return true;
                })()"#,
            )?;

            page.scroll().down(120.0)?;
            page.scroll().right(80.0)?;
            assert_eq!(
                page.run_js(
                    "[document.scrollingElement.scrollLeft, document.scrollingElement.scrollTop]"
                )?,
                Value::Array(vec![Value::from(80), Value::from(120)])
            );

            page.scroll().to_location(25.0, 35.0)?;
            assert_eq!(
                page.run_js(
                    "[document.scrollingElement.scrollLeft, document.scrollingElement.scrollTop]"
                )?,
                Value::Array(vec![Value::from(25), Value::from(35)])
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page scroll wrapper regression");
    }

    #[test]
    fn page_wait_for_ele_methods_accept_element_targets_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-wait-ele-targets").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="ready">Ready</button>
                        <div id="hidden" style="display:none">Hidden</div>
                        <button id="delete-me">Delete me</button>
                    `;
                    return true;
                })()"#,
            )?;

            let ready = page.find("#ready")?;
            let hidden = page.find("#hidden")?;
            let delete_me = page.find("#delete-me")?;

            assert!(page.wait_for_ele_displayed(&ready, 1_000)?);
            assert!(page.wait_for_ele_enabled(&ready, 1_000)?);
            assert!(page.wait_for_ele_clickable(&ready, 1_000)?);
            assert!(page.wait_for_ele_hidden(&hidden, 1_000)?);

            page.run_js(
                r#"(() => {
                    setTimeout(() => document.getElementById('delete-me')?.remove(), 50);
                    return true;
                })()"#,
            )?;
            assert!(page.wait_for_ele_deleted(&delete_me, 2_000)?);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element target wait regression");
    }

    #[test]
    fn page_get_frame_methods_accept_element_and_frame_targets_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("page-get-frame-targets")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div id="host">
                            <iframe id="demo-frame" name="demo-frame"
                                srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                            </iframe>
                        </div>
                    `;
                    return true;
                })()"#,
            )?;

            let frame_element = page.get_frame_ele("css:#demo-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("locator get_frame_ele: {err}"))
            })?;
            let frame = page
                .get_frame("css:#demo-frame")
                .map_err(|err| OpenPageError::PageOperation(format!("locator get_frame: {err}")))?;
            let frame_by_index = page
                .get_frame(1usize)
                .map_err(|err| OpenPageError::PageOperation(format!("index get_frame: {err}")))?;
            let frame_element_by_index = page.get_frame_ele(1usize).map_err(|err| {
                OpenPageError::PageOperation(format!("index get_frame_ele: {err}"))
            })?;
            let frame_context_from_locator =
                page.get_frame_context("css:#demo-frame").map_err(|err| {
                    OpenPageError::PageOperation(format!("locator get_frame_context: {err}"))
                })?;
            let frame_context_by_index = page.get_frame_context(-1isize).map_err(|err| {
                OpenPageError::PageOperation(format!("index get_frame_context: {err}"))
            })?;

            assert_eq!(
                page.get_frame(&frame_element)
                    .and_then(|frame| frame.attr("id"))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame(&Element): {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                page.get_frame(&frame)
                    .and_then(|frame| frame.attr("name"))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame(&Frame): {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_by_index
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame(1) attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_element_by_index
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame_ele(1) attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );

            let frame_from_element = page
                .get_frame_context(&frame_element)
                .map_err(|err| OpenPageError::PageOperation(format!("from element: {err}")))?;
            let frame_from_frame = page
                .get_frame_context(&frame)
                .map_err(|err| OpenPageError::PageOperation(format!("from frame: {err}")))?;
            let host = page
                .find("css:#host")
                .map_err(|err| OpenPageError::PageOperation(format!("host find: {err}")))?;
            let host_frame = host.get_frame("css:#demo-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("host get_frame(locator): {err}"))
            })?;
            let host_frame_by_index = host
                .get_frame(1usize)
                .map_err(|err| OpenPageError::PageOperation(format!("host get_frame(1): {err}")))?;
            let host_frame_from_frame = host.get_frame(&frame).map_err(|err| {
                OpenPageError::PageOperation(format!("host get_frame(&Frame): {err}"))
            })?;
            let web_host =
                WebElement::Browser(page.find("css:#host").map_err(|err| {
                    OpenPageError::PageOperation(format!("web host find: {err}"))
                })?);
            let web_host_frame = web_host.get_frame("css:#demo-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("web host get_frame(locator): {err}"))
            })?;
            let web_host_frame_by_index = web_host.get_frame(1usize).map_err(|err| {
                OpenPageError::PageOperation(format!("web host get_frame(1): {err}"))
            })?;

            assert_eq!(
                frame_from_element
                    .frame_element()
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "frame_from_element attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_from_element
                    .name()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "frame_from_element name: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_from_frame
                    .name()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "frame_from_frame name: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_context_by_index
                    .name()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "frame_context_by_index name: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_context_from_locator
                    .name()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "frame_context_from_locator name: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                frame_from_frame.frame_element().attr("id").map_err(|err| {
                    OpenPageError::PageOperation(format!("frame_from_frame attr: {err}"))
                })?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                page.get_frame_ele(&frame)
                    .and_then(|element| element.attr("id"))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame_ele(&Frame): {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                host_frame
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "host_frame attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                host_frame_by_index
                    .attr("name")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "host_frame_by_index attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                host_frame_from_frame
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "host_frame_from_frame attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                web_host_frame
                    .attr("id")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "web_host_frame attr: {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                web_host_frame_by_index.attr("name").map_err(
                    |err| OpenPageError::PageOperation(format!(
                        "web_host_frame_by_index attr: {err}"
                    ))
                )?,
                Some("demo-frame".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("frame target lookup regression");
    }

    #[test]
    fn page_get_frame_with_timeout_waits_for_delayed_iframe() {
        let (browser, temp_dir) = launch_headless_test_browser("page-get-frame-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(page.get_frame("css:#delayed-frame").is_err());

            let frame = page.get_frame_with_timeout("css:#delayed-frame", 2_000)?;
            assert!(frame.wait_for_doc_loaded(2_000)?);
            assert_eq!(frame.attr("name")?, Some("delayed-frame".to_string()));
            assert_eq!(
                frame.find("css:#inside")?.text()?,
                Some("inside".to_string())
            );

            let frame_by_index = page.get_frame_by_index_with_timeout(1, 500)?;
            assert_eq!(
                frame_by_index.attr("id")?,
                Some("delayed-frame".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("frame timeout lookup regression");
    }

    #[test]
    fn get_frames_with_timeout_waits_for_delayed_iframes() {
        let (browser, temp_dir) = launch_headless_test_browser("page-get-frames-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><div id='outer-host'></div></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(page.get_frames(Some("css:#delayed-frame"))?.is_empty());
            let frames = page.get_frames_with_timeout(Some("css:#delayed-frame"), 2_000)?;
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].attr("name")?, Some("delayed-frame".to_string()));

            let outer = frames.into_iter().next().expect("frame exists");
            assert!(outer.wait_for_doc_loaded(2_000)?);
            outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'nested-frame';
                        frame.name = 'nested-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(outer.get_frames(Some("css:#nested-frame"))?.is_empty());
            let nested = outer.get_frames_with_timeout(Some("css:#nested-frame"), 2_000)?;
            assert_eq!(nested.len(), 1);
            assert_eq!(nested[0].attr("id")?, Some("nested-frame".to_string()));
            assert_eq!(
                nested[0].find("css:#inside")?.text()?,
                Some("inside".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("frame batch timeout lookup regression");
    }

    #[test]
    fn get_frame_eles_with_timeout_waits_for_delayed_iframe_elements() {
        let (browser, temp_dir) = launch_headless_test_browser("page-get-frame-eles-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><div id='outer-host'></div></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(page.get_frame_eles(Some("css:#delayed-frame"))?.is_empty());
            let frame_ele = page.get_frame_ele_with_timeout("css:#delayed-frame", 2_000)?;
            assert_eq!(frame_ele.attr("name")?, Some("delayed-frame".to_string()));
            let frame_eles = page.get_frame_eles_with_timeout(Some("css:#delayed-frame"), 500)?;
            assert_eq!(frame_eles.len(), 1);

            let outer = page.get_frame(&frame_ele)?;
            assert!(outer.wait_for_doc_loaded(2_000)?);
            outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'nested-frame';
                        frame.name = 'nested-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(outer.get_frame_eles(Some("css:#nested-frame"))?.is_empty());
            let nested_ele = outer.get_frame_ele_with_timeout("css:#nested-frame", 2_000)?;
            assert_eq!(nested_ele.attr("id")?, Some("nested-frame".to_string()));
            let nested_eles = outer.get_frame_eles_with_timeout(Some("css:#nested-frame"), 500)?;
            assert_eq!(nested_eles.len(), 1);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("frame element timeout lookup regression");
    }

    #[test]
    fn frame_get_frame_finds_nested_iframe_in_frame_context() {
        let (browser, temp_dir) = launch_headless_test_browser("frame-get-nested-frame")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let outer = page.get_frame("css:#outer-frame")?;
            assert!(outer.wait_for_doc_loaded(2_000)?);
            outer.run_js(
                r#"(() => {
                    const frame = document.createElement('iframe');
                    frame.id = 'inner-frame';
                    frame.name = 'inner-frame';
                    frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                    document.getElementById('outer-host').appendChild(frame);
                    return true;
                })()"#,
            )?;

            let inner = outer.get_frame("css:#inner-frame")?;
            let inner_by_index = outer.get_frame_by_index(1)?;
            let inner_ele = outer.get_frame_ele("css:#inner-frame")?;
            let nested_frames = outer.get_frames(Some((By::TAG_NAME, "iframe")))?;

            assert!(inner.wait_for_doc_loaded(2_000)?);
            assert_eq!(inner.attr("name")?, Some("inner-frame".to_string()));
            assert_eq!(inner_by_index.attr("id")?, Some("inner-frame".to_string()));
            assert_eq!(inner_ele.attr("id")?, Some("inner-frame".to_string()));
            assert_eq!(nested_frames.len(), 1);
            assert_eq!(
                inner.find("css:#inside")?.text()?,
                Some("inside".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("nested frame lookup regression");
    }

    #[test]
    fn frame_get_frame_with_timeout_waits_for_delayed_nested_iframe() {
        let (browser, temp_dir) = launch_headless_test_browser("frame-get-nested-frame-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let outer = page.get_frame("css:#outer-frame")?;
            assert!(outer.wait_for_doc_loaded(2_000)?);
            outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'inner-frame';
                        frame.name = 'inner-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            assert!(outer.get_frame("css:#inner-frame").is_err());

            let inner = outer.get_frame_with_timeout("css:#inner-frame", 2_000)?;
            assert!(inner.wait_for_doc_loaded(2_000)?);
            assert_eq!(inner.attr("name")?, Some("inner-frame".to_string()));
            assert_eq!(
                inner.find("css:#inside")?.text()?,
                Some("inside".to_string())
            );

            let inner_by_index = outer.get_frame_by_index_with_timeout(1, 500)?;
            assert_eq!(inner_by_index.attr("id")?, Some("inner-frame".to_string()));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("nested frame timeout lookup regression");
    }

    #[test]
    fn element_get_frame_with_timeout_waits_for_delayed_iframe_child() {
        let (browser, temp_dir) = launch_headless_test_browser("element-get-frame-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'child-frame';
                        frame.name = 'child-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

            let host = page.find("css:#host")?;
            assert!(host.get_frame("css:#child-frame").is_err());

            let frame = host.get_frame_with_timeout("css:#child-frame", 2_000)?;
            assert!(frame.wait_for_doc_loaded(2_000)?);
            assert_eq!(frame.attr("name")?, Some("child-frame".to_string()));
            assert_eq!(
                frame.find("css:#inside")?.text()?,
                Some("inside".to_string())
            );

            let frame_by_index = host.get_frame_by_index_with_timeout(1, 500)?;
            assert_eq!(frame_by_index.attr("id")?, Some("child-frame".to_string()));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element frame timeout lookup regression");
    }

    #[test]
    fn page_save_returns_mhtml_and_pdf_content_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-save").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.title = "Save Capability";
                    document.body.innerHTML = `
                        <main id="content">
                            <h1>save target</h1>
                            <p>Rust page.save runtime coverage.</p>
                        </main>
                    `;
                    return true;
                })()"#,
            )?;

            let mhtml = page.save(None, None, false)?;
            match mhtml {
                PageSaveContent::Mhtml(data) => {
                    assert!(data.contains("save target"));
                    assert!(data.contains("Content-Location:"));
                }
                other => panic!("expected mhtml save content, got {other:?}"),
            }

            let mhtml_dir = temp_dir.join("page-save-files");
            let mhtml = page.save(Some(&mhtml_dir), Some("saved-page"), false)?;
            let mhtml_path = mhtml_dir.join("saved-page.mhtml");
            assert!(mhtml_path.exists());
            let saved_mhtml = fs::read_to_string(&mhtml_path).expect("read saved mhtml");
            assert!(saved_mhtml.contains("save target"));
            match mhtml {
                PageSaveContent::Mhtml(data) => {
                    assert_eq!(data, saved_mhtml);
                }
                other => panic!("expected saved mhtml content, got {other:?}"),
            }

            let pdf = page.save_with_options(
                Some(&temp_dir),
                Some("saved-page"),
                true,
                Some(PrintToPdfParams::builder().landscape(true).build()),
            )?;
            let pdf_path = temp_dir.join("saved-page.pdf");
            assert!(pdf_path.exists());
            match pdf {
                PageSaveContent::Pdf(bytes) => {
                    assert!(bytes.starts_with(b"%PDF"));
                    assert_eq!(bytes, fs::read(&pdf_path).expect("read saved pdf"));
                }
                other => panic!("expected pdf save content, got {other:?}"),
            }

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page save runtime regression");
    }

    #[test]
    fn page_actions_support_mouse_keyboard_and_scroll_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-actions").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="kw" />
                        <button id="btn">Go</button>
                        <div style="height: 2000px"></div>
                        <button id="far-btn">Far</button>
                    `;
                    window.__clicked = 0;
                    window.__farClicked = 0;
                    window.__mouse = [];
                    window.__moveButtons = [];
                    window.__moveShift = [];
                    window.__wheel = [];
                    window.__keys = [];
                    document.getElementById('btn').addEventListener('click', () => window.__clicked += 1);
                    document.getElementById('far-btn').addEventListener('click', () => window.__farClicked += 1);
                    document.addEventListener('mousedown', (event) => window.__mouse.push(`down:${event.button}`));
                    document.addEventListener('mouseup', (event) => window.__mouse.push(`up:${event.button}`));
                    document.addEventListener('mousemove', (event) => {
                        window.__moveButtons.push(event.buttons);
                        window.__moveShift.push(event.shiftKey);
                    });
                    document.addEventListener('wheel', (event) => window.__wheel.push([event.deltaY, event.deltaX]));
                    document.addEventListener('keydown', (event) => window.__keys.push(`down:${event.key}`));
                    document.addEventListener('keyup', (event) => window.__keys.push(`up:${event.key}`));
                    return true;
                })()"#,
            )?;

            let mut actions = page.actions()?;
            actions
                .move_to("css:#btn", None, None, 0.0)?
                .click(None::<&str>, 1)?
                .move_to((20, 20), None, None, 0.0)?
                .hold(None::<&str>)?
                .r#move(25.0, 0.0, 0.0)?
                .release(None::<&str>)?
                .r_hold(None::<&str>)?
                .r_release(None::<&str>)?
                .m_hold(None::<&str>)?
                .m_release(None::<&str>)?
                .move_to("css:#kw", None, None, 0.0)?
                .click(None::<&str>, 1)?
                .input("Drission")?
                .r#type(["Page"])?
                .scroll(120.0, 10.0, None::<&str>)?
                .key_down("Shift")?
                .r#move(15.0, 5.0, 0.0)?
                .key_up("Shift")?;

            assert_eq!(page.run_js("window.__clicked")?, Value::from(1));
            assert_eq!(
                page.run_js("document.getElementById('kw').value")?,
                Value::from("DrissionPage")
            );
            let mouse = page.run_js("window.__mouse.join(',')")?;
            match mouse {
                Value::String(mouse) => {
                    assert!(mouse.contains("down:0"));
                    assert!(mouse.contains("up:0"));
                    assert!(mouse.contains("down:1"));
                    assert!(mouse.contains("up:1"));
                    assert!(mouse.contains("down:2"));
                    assert!(mouse.contains("up:2"));
                }
                other => panic!("unexpected mouse event payload: {other}"),
            }
            assert_eq!(
                page.run_js("[window.__wheel.length, window.__wheel[0][0], window.__wheel[0][1]]")?,
                json!([1, 120, 10])
            );
            assert_eq!(
                page.run_js("window.__moveButtons.includes(1)")?,
                Value::Bool(true)
            );
            assert_eq!(
                page.run_js("window.__moveShift.includes(true)")?,
                Value::Bool(true)
            );
            let keys = page.run_js("window.__keys.join(',')")?;
            match keys {
                Value::String(keys) => {
                    assert!(keys.contains("down:Shift"));
                    assert!(keys.contains("up:Shift"));
                }
                other => panic!("unexpected key event payload: {other}"),
            }
            assert!(actions.curr_x() > 0);
            assert!(actions.curr_y() >= 0);

            let mut no_wait_actions = page.new_actions();
            no_wait_actions.move_to((20, 30), None, None, 0.0)?;
            assert_eq!(
                (no_wait_actions.curr_x(), no_wait_actions.curr_y()),
                (20, 30)
            );
            let mut absolute_actions = page.new_actions();
            absolute_actions.move_to((30, 900), None, None, 0.0)?;
            let absolute_scroll_y = page
                .run_js("window.scrollY")?
                .as_f64()
                .expect("window.scrollY as f64");
            assert!(absolute_scroll_y > 0.0);
            assert!(
                ((absolute_actions.curr_y() as f64) - (900.0 - absolute_scroll_y)).abs() <= 1.0
            );

            let mut far_element_actions = page.new_actions();
            far_element_actions
                .move_to("css:#far-btn", None, None, 0.0)?
                .click(None::<&str>, 1)?;
            let far_scroll_y = page
                .run_js("window.scrollY")?
                .as_f64()
                .expect("window.scrollY as f64");
            assert!(far_scroll_y > 0.0);
            assert_eq!(page.run_js("window.__farClicked")?, Value::from(1));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("actions runtime regression");
    }

    #[test]
    fn page_actions_type_supports_modifier_events_and_interval_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-actions-type").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `<input id="kw" value="">`;
                    window.__keys = [];
                    window.__letterTimes = [];
                    window.__keyStates = [];
                    const kw = document.getElementById('kw');
                    kw.addEventListener('keydown', event => {
                        window.__keys.push(`down:${event.key}`);
                        window.__keyStates.push([event.key, event.ctrlKey, event.metaKey, event.shiftKey]);
                        if (event.key.length === 1) {
                            window.__letterTimes.push(performance.now());
                        }
                    });
                    kw.addEventListener('keyup', event => window.__keys.push(`up:${event.key}`));
                    return true;
                })()"#,
            )?;

            let input = page.find("css:#kw")?;
            let mut actions = page.actions()?;
            actions.click(Some("css:#kw"), 1)?;
            let start = Instant::now();
            actions.type_with_interval("abc", 0.12)?;
            assert!(start.elapsed() >= Duration::from_millis(300));
            assert_eq!(input.value()?, Some("abc".to_string()));
            assert_eq!(
                page.run_js("window.__letterTimes.length === 3 && (window.__letterTimes[2] - window.__letterTimes[0]) >= 200")?,
                Value::from(true)
            );

            page.run_js(
                r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = '';
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
            )?;

            actions.type_keys_with_interval(["Shift", "a"], 0.01)?;
            assert_eq!(input.value()?, Some("A".to_string()));
            let keys = page
                .run_js("window.__keys.join(',')")
                .map_err(|err| OpenPageError::PageOperation(format!("post combo keys: {err}")))?;
            match keys {
                Value::String(keys) => {
                    assert!(keys.contains("down:Shift"));
                    assert!(keys.contains("up:Shift"));
                    assert!(keys.contains("down:a") || keys.contains("down:A"));
                    assert!(keys.contains("up:a") || keys.contains("up:A"));
                }
                other => panic!("unexpected actions type key payload: {other}"),
            }
            assert_eq!(
                page.run_js("window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && item[3] === true)")
                    .map_err(|err| OpenPageError::PageOperation(format!("post combo key states: {err}")))?,
                Value::from(true)
            );

            page.run_js(
                r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = '';
                    kw.focus();
                    kw.setSelectionRange(0, 0);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
            )?;

            actions.type_keys_with_interval(["Shift", "1"], 0.01)?;
            assert_eq!(input.value()?, Some("!".to_string()));
            assert_eq!(
                page.run_js("window.__keyStates.some(item => item[0] === '!' && item[3] === true)")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "post symbol key states: {err}"
                    )))?,
                Value::from(true)
            );

            page.run_js(
                r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = 'abcdef';
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
            )?;

            actions.type_keys(Keys::CTRL_A)?;
            let selection_start = input
                .property("selectionStart")?
                .and_then(|value| value.as_u64())
                .expect("selectionStart as u64");
            let selection_end = input
                .property("selectionEnd")?
                .and_then(|value| value.as_u64())
                .expect("selectionEnd as u64");
            let selection_len = input
                .value()?
                .map(|value| value.len() as u64)
                .expect("input value length");
            assert_eq!(
                Value::from(selection_start == 0 && selection_end == selection_len),
                Value::from(true)
            );
            assert_eq!(
                page.run_js(
                    "window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && (item[1] === true || item[2] === true))"
                )
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "post shortcut modifier states: {err}"
                )))?,
                Value::from(true)
            );

            actions.type_keys("q")?;
            assert_eq!(input.value()?, Some("q".to_string()));

            actions.type_keys(Keys::CTRL_Z)?;
            assert_eq!(input.value()?, Some("abcdef".to_string()));

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("actions type interval runtime regression");
    }

    #[test]
    fn page_actions_shortcuts_support_cut_redo_and_held_modifiers_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("page-actions-shortcuts")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="kw" value="abcdef">
                        <input id="clone" value="">
                    `;
                    window.__keyStates = [];
                    const kw = document.getElementById('kw');
                    kw.addEventListener('keydown', event => {
                        window.__keyStates.push([event.key, event.ctrlKey, event.metaKey, event.shiftKey]);
                    });
                    return true;
                })()"#,
            )?;

            let input = page.find("css:#kw")?;
            let mut actions = page.actions()?;
            actions.click(Some("css:#kw"), 1)?;

            actions.type_keys(Keys::CTRL_A)?;
            actions.type_keys(Keys::CTRL_X)?;
            assert_eq!(input.value()?, Some(String::new()));

            actions.type_keys(Keys::CTRL_Z)?;
            assert_eq!(input.value()?, Some("abcdef".to_string()));

            actions.type_keys(Keys::CTRL_Y)?;
            assert_eq!(input.value()?, Some(String::new()));

            input.set().value("abcdef")?;
            let clone = page.find("css:#clone")?;
            clone.set().value("")?;
            page.run_js(
                r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keyStates = [];
                    return true;
                })()"#,
            )?;

            actions
                .key_down(Keys::CTRL_COMM)?
                .type_keys("a")?
                .key_up(Keys::CTRL_COMM)?;

            let selection_start = input
                .property("selectionStart")?
                .and_then(|value| value.as_u64())
                .expect("selectionStart as u64");
            let selection_end = input
                .property("selectionEnd")?
                .and_then(|value| value.as_u64())
                .expect("selectionEnd as u64");
            let selection_len = input
                .value()?
                .map(|value| value.len() as u64)
                .expect("input value length");
            assert_eq!((selection_start, selection_end), (0, selection_len));
            assert_eq!(
                page.run_js(
                    "window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && (item[1] === true || item[2] === true))"
                )
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "held shortcut modifier states: {err}"
                )))?,
                Value::from(true)
            );

            actions.type_keys(Keys::CTRL_C)?;
            actions.click(Some("css:#clone"), 1)?;
            actions.type_keys(Keys::CTRL_V)?;
            assert_eq!(clone.value()?, Some("abcdef".to_string()));

            clone.set().value("")?;
            actions.click(Some("css:#kw"), 1)?;
            actions
                .key_down(Keys::CTRL_COMM)?
                .type_keys("a")?
                .type_keys("x")?
                .key_up(Keys::CTRL_COMM)?;
            assert_eq!(input.value()?, Some(String::new()));

            input.set().value("abcdef")?;
            actions
                .key_down(Keys::CTRL_COMM)?
                .type_keys("a")?
                .type_keys("c")?
                .key_up(Keys::CTRL_COMM)?;
            actions
                .click(Some("css:#clone"), 1)?
                .key_down(Keys::CTRL_COMM)?
                .type_keys("v")?
                .key_up(Keys::CTRL_COMM)?;
            assert_eq!(clone.value()?, Some("abcdef".to_string()));

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("actions shortcut runtime regression");
    }

    #[test]
    fn page_actions_drag_in_supports_files_and_text_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-actions-drag").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div id="drop" style="width: 240px; height: 120px; border: 1px solid #333;">
                            Drop here
                        </div>
                    `;
                    window.__dragEvents = [];
                    const target = document.getElementById('drop');
                    const capture = (type, event) => {
                        event.preventDefault();
                        const files = event.dataTransfer ? Array.from(event.dataTransfer.files || []).map(file => file.name) : [];
                        const itemTypes = event.dataTransfer ? Array.from(event.dataTransfer.items || []).map(item => item.type) : [];
                        let text = '';
                        let uri = '';
                        let html = '';
                        try {
                            if (event.dataTransfer) {
                                text = event.dataTransfer.getData('text/plain') || '';
                                uri = event.dataTransfer.getData('text/uri-list') || '';
                                html = event.dataTransfer.getData('text/html') || '';
                                if (!text) {
                                    text = uri || html || '';
                                }
                            }
                        } catch (error) {
                            text = `error:${error && error.message ? error.message : error}`;
                        }
                        window.__dragEvents.push({ type, files, itemTypes, text, uri, html });
                    };
                    target.addEventListener('dragenter', event => capture('dragenter', event));
                    target.addEventListener('dragover', event => capture('dragover', event));
                    target.addEventListener('drop', event => capture('drop', event));
                    return true;
                })()"#,
            )?;

            let file_path = temp_dir.join("drag-file.txt");
            fs::write(&file_path, "drag payload")?;
            let file_path = file_path.to_string_lossy().into_owned();

            let mut actions = page.actions()?;
            actions.drag_in(
                "css:#drop",
                crate::ActionsDragData::files(vec![file_path.clone()]),
            )?;

            assert_eq!(
                page.run_js("window.__dragEvents.some(event => event.type === 'dragenter')")?,
                Value::from(true)
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).type")?,
                Value::from("drop")
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).files[0]")?,
                Value::from("drag-file.txt")
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).itemTypes[0]")?,
                Value::from("text/plain")
            );

            page.run_js("window.__dragEvents = [];")?;
            actions.drag_in("css:#drop", crate::ActionsDragData::text("Dragged text"))?;

            assert_eq!(
                page.run_js("window.__dragEvents.some(event => event.type === 'dragenter')")?,
                Value::from(true)
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).type")?,
                Value::from("drop")
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).text")?,
                Value::from("Dragged text")
            );
            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).itemTypes[0]")?,
                Value::from("text/plain")
            );

            page.run_js("window.__dragEvents = [];")?;
            actions.drag_in(
                "css:#drop",
                crate::ActionsDragData::link("https://example.test/path", "Example title"),
            )?;

            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).type")?,
                Value::from("drop")
            );
            assert_eq!(
                page.run_js("window.__dragEvents.length > 0")?,
                Value::from(true)
            );

            page.run_js("window.__dragEvents = [];")?;
            actions.drag_in(
                "css:#drop",
                crate::ActionsDragData::html(
                    "<strong>Dragged html</strong>",
                    "https://example.test/base/",
                ),
            )?;

            assert_eq!(
                page.run_js("window.__dragEvents.at(-1).type")?,
                Value::from("drop")
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("actions drag_in runtime regression");
    }

    #[test]
    fn actions_drag_payload_preserves_link_and_html_metadata() {
        let link_payload = action_drag_payload(crate::ActionsDragData::link(
            "https://example.test/path",
            "Example title",
        ))
        .expect("link payload");
        assert_eq!(link_payload.items.len(), 1);
        assert_eq!(link_payload.items[0].mime_type, "text/uri-list");
        assert_eq!(link_payload.items[0].data, "https://example.test/path");
        assert_eq!(
            link_payload.items[0].title.as_deref(),
            Some("Example title")
        );
        assert_eq!(link_payload.items[0].base_url, None);
        assert_eq!(link_payload.files, None);

        let html_payload = action_drag_payload(crate::ActionsDragData::html(
            "<strong>Dragged html</strong>",
            "https://example.test/base/",
        ))
        .expect("html payload");
        assert_eq!(html_payload.items.len(), 1);
        assert_eq!(html_payload.items[0].mime_type, "text/uri-list");
        assert_eq!(html_payload.items[0].data, "<strong>Dragged html</strong>");
        assert_eq!(html_payload.items[0].title, None);
        assert_eq!(
            html_payload.items[0].base_url.as_deref(),
            Some("https://example.test/base/")
        );
        assert_eq!(html_payload.files, None);
    }

    #[test]
    fn page_element_list_getter_returns_attrs_links_and_texts_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-list-getter").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <a class="item" href="https://example.test/one">One</a>
                        <a class="item">Two</a>
                        <img class="item" src="https://example.test/three.png">
                    `;
                    return true;
                })()"#,
            )?;

            let items = page.find_all(".item")?;
            assert_eq!(
                items.get().attrs("href")?,
                vec![Some("https://example.test/one".to_string()), None, None,]
            );
            assert_eq!(
                items.get().links()?,
                vec![
                    Some("https://example.test/one".to_string()),
                    None,
                    Some("https://example.test/three.png".to_string()),
                ]
            );
            assert_eq!(
                items.get().texts()?,
                vec![Some("One".to_string()), Some("Two".to_string()), None,]
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element list getter runtime regression");
    }

    #[test]
    fn page_and_web_element_lists_support_filter_and_filter_one_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-list-filter").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" data-kind="keep">Alpha keep</button>
                        <button class="item" data-kind="drop" style="display:none">Hidden keep</button>
                        <button class="item" data-kind="drop" disabled>Disabled keep</button>
                        <button class="item" data-kind="keep">Gamma keep</button>
                    `;
                    return true;
                })()"#,
            )?;

            let items = page.find_all(".item")?;
            let active_keep = items
                .filter()
                .displayed(true)?
                .enabled(true)?
                .attr("data-kind", "keep", true)?
                .text("keep", true, true)?;
            assert_eq!(active_keep.len(), 2);
            assert_eq!(
                active_keep.get().texts()?,
                vec![
                    Some("Alpha keep".to_string()),
                    Some("Gamma keep".to_string()),
                ]
            );
            assert_eq!(
                active_keep
                    .into_iter()
                    .map(|element| element.text())
                    .collect::<crate::OpenPageResult<Vec<_>>>()?,
                vec![
                    Some("Alpha keep".to_string()),
                    Some("Gamma keep".to_string()),
                ]
            );

            let second_displayed = items
                .filter_one_at(2)
                .displayed(true)?
                .expect("second displayed element");
            assert_eq!(second_displayed.text()?, Some("Disabled keep".to_string()));

            let disabled = items
                .filter_one()
                .enabled(false)?
                .expect("disabled element");
            assert_eq!(disabled.text()?, Some("Disabled keep".to_string()));

            let web_items = page
                .find_all(".item")?
                .into_iter()
                .map(WebElement::Browser)
                .collect::<Vec<_>>();
            assert_eq!(web_items.filter().displayed(true)?.len(), 3);
            let second_keep = web_items
                .filter_one_at(2)
                .attr("data-kind", "keep", true)?
                .expect("second keep web element");
            assert_eq!(second_keep.text()?, Some("Gamma keep".to_string()));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element list filter runtime regression");
    }

    #[test]
    fn page_and_web_element_lists_support_extended_filters_and_search_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-list-search").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input class="item" id="checked-input" type="checkbox" checked />
                        <button class="item" id="primary-btn" style="display:block">Primary</button>
                        <button class="item" id="disabled-btn" disabled>Disabled</button>
                        <select>
                            <option class="item" id="plain-option">Plain</option>
                            <option class="item" id="selected-option" selected>Selected</option>
                        </select>
                        <span class="item" id="zero-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</span>
                        <div class="item" id="hidden-box" style="display:none">Hidden</div>
                    `;
                    return true;
                })()"#,
            )?;

            let items = page.find_all(".item")?;

            assert_eq!(
                items.filter().checked(true)?.get().attrs("id")?,
                vec![Some("checked-input".to_string())]
            );
            assert_eq!(
                items.filter().selected(true)?.get().attrs("id")?,
                vec![Some("selected-option".to_string())]
            );
            assert_eq!(
                items.filter().clickable(true)?.get().attrs("id")?,
                vec![
                    Some("checked-input".to_string()),
                    Some("primary-btn".to_string()),
                ]
            );
            assert_eq!(
                items.filter().have_rect(false)?.get().attrs("id")?,
                vec![
                    Some("plain-option".to_string()),
                    Some("selected-option".to_string()),
                    Some("zero-rect".to_string()),
                    Some("hidden-box".to_string()),
                ]
            );
            assert_eq!(items.filter().tag("option", true)?.len(), 2);
            assert_eq!(
                items
                    .filter()
                    .style("overflow", "hidden", true)?
                    .get()
                    .attrs("id")?,
                vec![Some("zero-rect".to_string())]
            );
            assert_eq!(items.filter().property("id", "primary-btn", true)?.len(), 1);

            let selected = items.filter_one().selected(true)?;
            assert_eq!(selected.attr("id")?, Some("selected-option".to_string()));
            assert_eq!(selected.is_selected()?, Some(true));

            let primary_button = items
                .filter()
                .tag("button", true)?
                .clickable(true)?
                .first()
                .expect("clickable primary");
            assert_eq!(primary_button.attr("id")?, Some("primary-btn".to_string()));

            let search = crate::ElementsSearch::new()
                .checked(true)
                .selected(true)
                .tag("button");
            let searched = items.search(&search)?;
            assert_eq!(searched.len(), 4);
            assert_eq!(
                searched.get().attrs("id")?,
                vec![
                    Some("checked-input".to_string()),
                    Some("primary-btn".to_string()),
                    Some("disabled-btn".to_string()),
                    Some("selected-option".to_string()),
                ]
            );

            let second_search_match = items.search_one_at(2, &search)?;
            assert_eq!(
                second_search_match.attr("id")?,
                Some("primary-btn".to_string())
            );
            assert_eq!(second_search_match.is_displayed()?, Some(true));

            let filtered_search = items
                .filter()
                .enabled(true)?
                .search(&crate::ElementsSearch::new().tag("button").selected(true))?;
            assert_eq!(
                filtered_search.get().attrs("id")?,
                vec![
                    Some("primary-btn".to_string()),
                    Some("selected-option".to_string()),
                ]
            );

            let web_items = page
                .find_all(".item")?
                .into_iter()
                .map(WebElement::Browser)
                .collect::<Vec<_>>();
            assert_eq!(web_items.filter().checked(true)?.len(), 1);
            assert_eq!(web_items.filter().selected(true)?.len(), 1);
            let checked_web = web_items
                .filter_one()
                .property("id", "checked-input", true)?
                .expect("checked web element");
            assert!(checked_web.is_checked()?);
            let disabled_web = web_items
                .filter_one()
                .property("id", "disabled-btn", true)?
                .expect("disabled web element");
            assert!(!disabled_web.is_enabled()?);
            assert_eq!(
                web_items
                    .search_one(&crate::ElementsSearch::new().tag("button"))?
                    .attr("id")?,
                Some("primary-btn".to_string())
            );
            assert_eq!(
                web_items
                    .filter_one()
                    .property("id", "selected-option", true)?
                    .text()?,
                Some("Selected".to_string())
            );
            let missing = web_items.filter_one().property("id", "missing", true)?;
            assert!(missing.is_none());
            assert_eq!(missing.text()?, None);
            assert_eq!(missing.is_enabled()?, None);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("extended element list filter/search runtime regression");
    }

    #[test]
    fn elements_one_supports_common_interactions_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("elements-one-interactions")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" data-role="cta">Click me</button>
                        <input class="item" id="name" value="">
                        <div class="item" id="text-block">alpha <span>beta</span> <em>gamma</em></div>
                        <link class="item" id="asset-link" rel="prefetch" href="data:text/plain;base64,aGVsbG8=">
                        <select id="single-picker">
                            <option class="item" id="single-a" value="a" selected>Single A</option>
                            <option class="item" id="single-b" value="b">Single B</option>
                        </select>
                        <select id="multi-picker" multiple>
                            <option class="item" id="multi-a" value="a">Multi A</option>
                            <option class="item" id="multi-b" value="b">Multi B</option>
                        </select>
                    `;
                    window.__oneClicks = 0;
                    window.__oneHover = 0;
                    document.querySelector('[data-role="cta"]').addEventListener('click', () => {
                        window.__oneClicks += 1;
                    });
                    document.querySelector('[data-role="cta"]').addEventListener('mouseenter', () => {
                        window.__oneHover += 1;
                    });
                    return true;
                })()"#,
            )?;

            let page_items = page.find_all(".item")?;
            let button_one = page_items.filter_one().attr("data-role", "cta", true)?;
            assert!(button_one.click()?);
            assert!(button_one.clicker().left()?);
            assert!(button_one.hover()?);
            assert_eq!(page.run_js("window.__oneClicks")?, Value::from(2));
            assert_eq!(page.run_js("window.__oneHover")?, Value::from(1));

            let input_one = page_items.filter_one().tag("input", true)?;
            assert!(input_one.focus()?);
            assert_eq!(
                page.run_js("document.activeElement && document.activeElement.id")?,
                Value::from("name")
            );
            assert!(input_one.input("Gamma")?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("Gamma")
            );
            assert!(input_one.clear()?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("")
            );

            let text_one = page_items.filter_one().attr("id", "text-block", true)?;
            assert_eq!(text_one.texts(true)?, Some(vec!["alpha".to_string()]));
            assert_eq!(
                text_one.texts(false)?,
                Some(vec![
                    "alpha".to_string(),
                    "beta".to_string(),
                    "gamma".to_string(),
                ])
            );
            let text_size = text_one.size()?.expect("text block size");
            assert!(text_size.0 > 0.0);
            assert!(text_size.1 > 0.0);

            let asset_one = page_items.filter_one().attr("id", "asset-link", true)?;
            assert_eq!(
                asset_one.src(500, true)?,
                Some(crate::ElementResource::Bytes(b"hello".to_vec()))
            );

            let single_option_one = page_items.filter_one().attr("id", "single-b", true)?;
            assert!(single_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('single-picker').value")?,
                Value::from("b")
            );
            let multi_option_one = page_items.filter_one().attr("id", "multi-a", true)?;
            assert!(multi_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('multi-a').selected")?,
                Value::from(true)
            );
            assert!(multi_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('multi-a').selected")?,
                Value::from(false)
            );

            let missing_page = page_items.filter_one().attr("data-role", "missing", true)?;
            assert!(!missing_page.click()?);
            assert!(!missing_page.clicker().left()?);
            assert!(!missing_page.input("noop")?);
            assert!(!missing_page.set().value("noop")?);
            assert!(!missing_page.scroll().to_top()?);
            assert!(!missing_page.select().by_text("noop")?);
            assert_eq!(missing_page.select().is_multi()?, None);
            assert!(!missing_page.clear()?);
            assert!(!missing_page.focus()?);
            assert!(!missing_page.hover()?);
            assert_eq!(missing_page.texts(false)?, None);
            assert_eq!(missing_page.size()?, None);
            assert_eq!(missing_page.src(500, true)?, None);

            let web_items = vec![
                WebElement::Browser(page.wait_for("css:[data-role='cta']", 1_000)?),
                WebElement::Browser(page.wait_for("css:#name", 1_000)?),
                WebElement::Browser(page.wait_for("css:#text-block", 1_000)?),
                WebElement::Browser(page.wait_for("css:#asset-link", 1_000)?),
                WebElement::Browser(page.wait_for("css:#single-b", 1_000)?),
                WebElement::Browser(page.wait_for("css:#multi-b", 1_000)?),
            ];
            let web_button_one = web_items.filter_one().attr("data-role", "cta", true)?;
            assert!(web_button_one.click()?);
            assert!(web_button_one.clicker().left()?);
            assert!(web_button_one.hover()?);
            assert_eq!(page.run_js("window.__oneClicks")?, Value::from(4));

            let web_input_one = web_items.filter_one().tag("input", true)?;
            assert!(web_input_one.focus()?);
            assert!(web_input_one.input("Delta")?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("Delta")
            );
            assert!(web_input_one.clear()?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("")
            );

            let web_text_one = web_items.filter_one().attr("id", "text-block", true)?;
            assert_eq!(web_text_one.texts(true)?, Some(vec!["alpha".to_string()]));
            assert_eq!(
                web_text_one.texts(false)?,
                Some(vec![
                    "alpha".to_string(),
                    "beta".to_string(),
                    "gamma".to_string(),
                ])
            );
            let web_text_size = web_text_one.size()?.expect("web text block size");
            assert!(web_text_size.0 > 0.0);
            assert!(web_text_size.1 > 0.0);

            let web_asset_one = web_items.filter_one().attr("id", "asset-link", true)?;
            assert_eq!(
                web_asset_one.src(500, true)?,
                Some(crate::ElementResource::Bytes(b"hello".to_vec()))
            );

            let web_single_option_one = web_items.filter_one().attr("id", "single-b", true)?;
            assert!(web_single_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('single-picker').value")?,
                Value::from("b")
            );
            let web_multi_option_one = web_items.filter_one().attr("id", "multi-b", true)?;
            assert!(web_multi_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('multi-b').selected")?,
                Value::from(true)
            );
            assert!(web_multi_option_one.clicker().left()?);
            assert_eq!(
                page.run_js("document.getElementById('multi-b').selected")?,
                Value::from(false)
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one interaction runtime regression");
    }

    #[test]
    fn elements_one_runtime_config_supports_none_value_and_raise_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("elements-one-none-config")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
            )?;

            page.set_none_element_value(Some("missing"), true)?;
            let page_items = page.find_all(".item")?;
            let missing_page = page_items.filter_one().attr("data-role", "missing", true)?;
            assert_eq!(missing_page.text()?, Some("missing".to_string()));
            assert_eq!(missing_page.attr("id")?, Some("missing".to_string()));
            assert_eq!(
                missing_page.texts(false)?,
                Some(vec!["missing".to_string()])
            );
            assert_eq!(missing_page.property("id")?, Some(Value::from("missing")));
            assert_eq!(missing_page.comments()?, Some(vec!["missing".to_string()]));
            assert_eq!(missing_page.child_count()?, None);

            let web_items = vec![
                WebElement::Browser(page.wait_for("css:[data-role='keep']", 1_000)?),
                WebElement::Browser(page.wait_for("css:[data-role='other']", 1_000)?),
            ];
            let missing_web = web_items.filter_one().attr("data-role", "missing", true)?;
            assert_eq!(missing_web.text()?, Some("missing".to_string()));
            assert_eq!(
                missing_web.src(100, true)?,
                Some(crate::ElementResource::Text("missing".to_string()))
            );

            page.set_raise_when_ele_not_found(true)?;
            let error = page_items
                .filter_one()
                .attr("data-role", "missing", true)
                .expect_err("page items missing filter should raise");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected page filter error: {error}"
            );

            let error = web_items
                .filter_one()
                .attr("data-role", "missing", true)
                .expect_err("web items missing filter should raise");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected web filter error: {error}"
            );

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one runtime config regression");
    }

    #[test]
    fn page_inherits_global_raise_when_ele_not_found_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_raise_when_ele_not_found(true);

        let (browser, temp_dir) = launch_headless_test_browser("page-global-none-config")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
            )?;

            let error = page
                .find_all(".item")?
                .filter_one()
                .attr("data-role", "missing", true)
                .expect_err("missing filter should use global raise setting");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected page filter error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page global missing-element setting regression");
    }

    #[test]
    fn singleton_tab_obj_reuses_runtime_page_state_when_enabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) = launch_headless_test_browser("page-singleton-enabled")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
            )?;

            page.set_raise_when_ele_not_found(true)?;
            let same_page = browser.get_page(&page.target_id())?;
            let error = same_page
                .find_all(".item")?
                .filter_one()
                .attr("data-role", "missing", true)
                .expect_err("singleton page should reuse missing-element runtime setting");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected singleton page error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("singleton page runtime-state regression");
    }

    #[test]
    fn singleton_tab_obj_returns_fresh_runtime_page_state_when_disabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(false);

        let (browser, temp_dir) = launch_headless_test_browser("page-singleton-disabled")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
            )?;

            page.set_raise_when_ele_not_found(true)?;
            let fresh_page = browser.get_page(&page.target_id())?;
            let items = fresh_page.find_all(".item")?;
            let missing = items.filter_one().attr("data-role", "missing", true)?;
            assert_eq!(missing.text()?, None);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("non-singleton page runtime-state regression");
    }

    #[test]
    fn singleton_tab_obj_reuses_runtime_frame_state_when_enabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) = launch_headless_test_browser("frame-singleton-enabled")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            frame.set_none_element_value(Some("missing"), true)?;

            let same_frame = page.get_frame_context("css:#demo-frame")?;
            assert_eq!(
                same_frame.ele(".does-not-exist")?.text()?,
                Some("missing".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("singleton frame runtime-state regression");
    }

    #[test]
    fn singleton_frame_runtime_cache_prunes_detached_frames() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) =
            launch_headless_test_browser("frame-cache-prune").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            let old_frame_id = frame.id().to_string();
            frame.set_none_element_value(Some("stale"), true)?;

            assert!(
                page.frame_none_element_configs
                    .lock()
                    .expect("frame runtime cache")
                    .contains_key(&old_frame_id)
            );

            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="replacement-frame"
                            srcdoc="<html><body><div id='inside'>fresh</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let replacement = page.get_frame_context("css:#replacement-frame")?;
            assert!(replacement.wait_for_doc_loaded(5_000)?);
            assert_ne!(replacement.id(), old_frame_id);

            let configs = page
                .frame_none_element_configs
                .lock()
                .expect("frame runtime cache");
            assert!(!configs.contains_key(&old_frame_id));
            assert!(configs.contains_key(replacement.id()));
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("detached frame runtime cache prune regression");
    }

    #[test]
    fn singleton_tab_obj_returns_fresh_runtime_frame_state_when_disabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(false);

        let (browser, temp_dir) = launch_headless_test_browser("frame-singleton-disabled")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            frame.set_none_element_value(Some("missing"), true)?;

            let same_handle = page.get_frame_context(&frame)?;
            assert_eq!(
                same_handle.ele(".does-not-exist")?.text()?,
                Some("missing".to_string())
            );
            let host = page.find("css:body")?;
            let same_handle_from_element = host.get_frame(&frame)?;
            assert_eq!(
                same_handle_from_element.ele(".does-not-exist")?.text()?,
                Some("missing".to_string())
            );

            let fresh_frame = page.get_frame_context("css:#demo-frame")?;
            assert_eq!(fresh_frame.ele(".does-not-exist")?.text()?, None);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("non-singleton frame runtime-state regression");
    }

    #[test]
    fn frame_initial_runtime_config_inherits_current_page_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("frame-runtime-config-inherit")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            page.set_none_element_value(Some("page-default"), true)?;
            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            assert_eq!(
                frame.ele(".does-not-exist")?.text()?,
                Some("page-default".to_string())
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("frame runtime-state inheritance regression");
    }

    #[test]
    fn latest_tab_returns_page_reference_when_singleton_enabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) = launch_headless_test_browser("page-latest-tab-singleton-enabled")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let _page = browser.new_page(None)?;
            let expected_id = browser
                .tab_ids()?
                .into_iter()
                .next()
                .expect("tab_ids should include latest tab");
            let latest = browser
                .latest_tab()?
                .expect("latest tab should exist after new page");
            match latest {
                BrowserTabReference::Page(latest_page) => {
                    assert_eq!(latest_page.target_id(), expected_id);
                }
                BrowserTabReference::WebPage(latest_page) => {
                    panic!(
                        "singleton latest_tab from Page should return page, got webpage {}",
                        latest_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("singleton latest_tab should return page, got id {id}");
                }
            }
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("singleton latest_tab return-type regression");
    }

    #[test]
    fn latest_tab_returns_id_when_singleton_disabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(false);

        let (browser, temp_dir) =
            launch_headless_test_browser("page-latest-tab-singleton-disabled")
                .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let _page = browser.new_page(None)?;
            let expected_id = browser
                .tab_ids()?
                .into_iter()
                .next()
                .expect("tab_ids should include latest tab");
            let latest = browser
                .latest_tab()?
                .expect("latest tab should exist after new page");
            match latest {
                BrowserTabReference::Id(id) => {
                    assert_eq!(id, expected_id);
                }
                BrowserTabReference::WebPage(latest_page) => {
                    panic!(
                        "non-singleton latest_tab from Page should return id, got webpage {}",
                        latest_page.target_id()
                    );
                }
                BrowserTabReference::Page(latest_page) => {
                    panic!(
                        "non-singleton latest_tab should return id, got page {}",
                        latest_page.target_id()
                    );
                }
            }
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("non-singleton latest_tab return-type regression");
    }

    #[test]
    fn page_chromium_page_tab_wrapper_signatures_accept_common_inputs() {
        fn assert_calls(page: &Page) {
            let tab_types = vec!["page".to_string(), "tab".to_string()];
            let target_ids = vec!["tab-1".to_string(), "tab-2".to_string()];
            let indices = vec![1usize, 2usize];
            let pages = vec![page];
            let selectors = [
                BrowserTabSelector::from("tab-1"),
                BrowserTabSelector::from(1usize),
            ];

            let _ = page.tabs_count();
            let _ = page.tab_ids();
            let _ = page.latest_tab();
            let _ = page.process_id();
            let _ = page.browser_version();
            let _ = page.address();
            let _ = page.reconnect(0);
            let _ = page.get_tab(Some("target-id"), None, None, None::<&str>, false);
            let _ = page.get_tab(Some(1usize), Some("Docs"), None, Some("page"), true);
            let _ = page.get_tab(
                Some(-1isize),
                None,
                Some("example"),
                Some(&tab_types),
                false,
            );
            let _ = page.get_tabs(None, None, Some("page"), false);
            let _ = page.get_tabs(Some("Docs"), Some("example"), Some(&tab_types), true);
            let _ = page.new_tab(None, false, true, false);
            let _ = page.new_tab(None, false, true, true);
            let _ = page.activate_tab("tab-1");
            let _ = page.activate_tab(1usize);
            let _ = page.activate_tab(page);
            let _ = page.activate_tab(page.clone());
            let _ = page.close_tabs("tab-1", false);
            let _ = page.close_tabs(1usize, false);
            let _ = page.close_tabs(page, false);
            let _ = page.close_tabs(page.clone(), false);
            let _ = page.close_tabs(&target_ids, false);
            let _ = page.close_tabs(&indices, false);
            let _ = page.close_tabs(&pages, false);
            let _ = page.close_tabs(&selectors[..], false);
            let _ = page.close_with_options(false, false);
            let _ = page.close_with_options(true, false);
            let _ = page.close_with_options(false, true);
            let _ = page.quit();
        }

        let _ = assert_calls as fn(&Page);
    }

    #[test]
    fn page_listener_interceptor_alias_signatures_accept_calls() {
        fn assert_calls(page: &Page) {
            let _ = page.listener();
            let _ = page.listen();
            let _ = page.interceptor();
            let _ = page.intercept();
        }

        let _ = assert_calls as fn(&Page);
    }

    #[test]
    fn frame_reconnect_signature_accepts_wait_argument() {
        fn assert_calls(frame: &Frame) {
            let _ = frame.reconnect(0);
        }

        let _ = assert_calls as fn(&Frame);
    }

    #[test]
    fn page_and_frame_disconnect_signatures_accept_roundtrip_calls() {
        let _ = Page::disconnect as fn(Page) -> OpenPageResult<DisconnectedPage>;
        let _ = Frame::disconnect as fn(Frame) -> OpenPageResult<DisconnectedFrame>;
        let _ = DisconnectedPage::reconnect as fn(&DisconnectedPage, u64) -> OpenPageResult<Page>;
        let _ =
            DisconnectedFrame::reconnect as fn(&DisconnectedFrame, u64) -> OpenPageResult<Frame>;
    }

    #[test]
    fn page_exposes_chromium_page_tab_wrappers_at_runtime() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) = launch_headless_test_browser("page-chromium-tab-wrappers")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert_eq!(page.tabs_count()?, browser.tabs_count()?);
            assert_eq!(page.tab_ids()?, browser.tab_ids()?);
            assert_eq!(page.address()?, browser.address());
            assert_eq!(page.browser_version()?, browser.version()?);
            assert_eq!(page.process_id(), browser.browser_pid());

            let current = page
                .get_tab(Some(&page), None, None, None::<&str>, false)?
                .expect("current tab should resolve");
            match current {
                BrowserTabReference::Page(current_page) => {
                    assert_eq!(current_page.target_id(), page.target_id());
                }
                BrowserTabReference::WebPage(current_page) => {
                    panic!(
                        "page.get_tab() should return page, got webpage {}",
                        current_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("current tab wrapper should return page, got id {id}");
                }
            }

            let expected_latest_id = page
                .tab_ids()?
                .into_iter()
                .next()
                .expect("tab_ids should include latest tab");
            let latest = page
                .latest_tab()?
                .expect("latest tab should exist after new page");
            match latest {
                BrowserTabReference::Page(latest_page) => {
                    assert_eq!(latest_page.target_id(), expected_latest_id);
                }
                BrowserTabReference::WebPage(latest_page) => {
                    panic!(
                        "singleton page.latest_tab() should return page, got webpage {}",
                        latest_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("singleton page.latest_tab() should return page, got id {id}");
                }
            }

            let new_tab = page.new_tab(Some("about:blank"), false, true, false)?;
            assert!(new_tab.wait_for_doc_loaded(5_000)?);
            page.activate_tab(&new_tab)?;

            let tab_types = ["page", "tab"];
            let tab_ids = page
                .get_tabs(None, None, Some(&tab_types[..]), true)?
                .into_iter()
                .map(|reference| match reference {
                    BrowserTabReference::Id(id) => id,
                    BrowserTabReference::WebPage(tab_page) => tab_page.target_id(),
                    BrowserTabReference::Page(tab_page) => tab_page.target_id(),
                })
                .collect::<Vec<_>>();
            assert!(tab_ids.contains(&new_tab.target_id()));

            let closed_tab_id = new_tab.target_id();
            let closed = page.close_tabs(&new_tab, false)?;
            assert_eq!(closed, 1);
            wait_until(Duration::from_millis(5_000), || match page.tab_ids() {
                Ok(ids) if !ids.contains(&closed_tab_id) => Some(()),
                _ => None,
            })?;
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("chromium page tab wrapper regression");
    }

    #[test]
    fn page_close_with_options_controls_current_and_other_tabs() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (browser, temp_dir) =
            launch_headless_test_browser("page-close-options").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            let current_id = page.target_id();

            let other = page.new_tab(Some("about:blank"), false, true, false)?;
            assert!(other.wait_for_doc_loaded(5_000)?);
            let other_id = other.target_id();
            assert!(page.tab_ids()?.contains(&other_id));

            page.close_with_options(true, false)?;
            wait_until(Duration::from_millis(5_000), || match page.tab_ids() {
                Ok(ids) if ids.contains(&current_id) && !ids.contains(&other_id) => Some(()),
                _ => None,
            })?;

            let closing = page.new_tab(Some("about:blank"), false, true, false)?;
            assert!(closing.wait_for_doc_loaded(5_000)?);
            let closing_id = closing.target_id();
            assert!(page.tab_ids()?.contains(&closing_id));

            closing.close_with_options(false, true)?;
            wait_until(Duration::from_millis(5_000), || match browser.tab_ids() {
                Ok(ids) if !ids.contains(&closing_id) => Some(()),
                _ => None,
            })?;
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page close_with_options runtime regression");
    }

    #[test]
    fn browser_page_and_frame_reconnect_rebuild_fresh_connections() {
        let (browser, temp_dir) = launch_headless_test_browser("browser-page-frame-reconnect")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<Page> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<iframe id="demo-frame" srcdoc="<html><body><div id=&quot;msg&quot;>frame reconnect</div></body></html>"></iframe><div id="msg">page reconnect</div>';
                    return true;
                })()"#,
            )?;

            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);

            let reconnected_browser = browser
                .reconnect()
                .map_err(|err| OpenPageError::PageOperation(format!("browser reconnect: {err}")))?;
            assert_eq!(reconnected_browser.address(), browser.address());
            assert_eq!(reconnected_browser.process_id(), browser.process_id());
            let browser_page = reconnected_browser
                .get_page(&page.target_id())
                .map_err(|err| OpenPageError::PageOperation(format!("browser get_page: {err}")))?;
            assert_eq!(browser_page.target_id(), page.target_id());
            assert_eq!(
                browser_page
                    .run_js("document.querySelector('#msg').textContent")
                    .map_err(|err| {
                        OpenPageError::PageOperation(format!("browser page run_js: {err}"))
                    })?,
                Value::from("page reconnect")
            );

            let reconnected_page = page
                .reconnect(0)
                .map_err(|err| OpenPageError::PageOperation(format!("page reconnect: {err}")))?;
            assert_eq!(reconnected_page.target_id(), page.target_id());
            assert_eq!(reconnected_page.address()?, page.address()?);
            assert_eq!(reconnected_page.process_id(), page.process_id());
            assert_eq!(
                reconnected_page
                    .run_js("document.querySelector('#msg').textContent")
                    .map_err(|err| OpenPageError::PageOperation(format!("page run_js: {err}")))?,
                Value::from("page reconnect")
            );

            let reconnected_frame = frame
                .reconnect(0)
                .map_err(|err| OpenPageError::PageOperation(format!("frame reconnect: {err}")))?;
            assert_eq!(
                reconnected_frame
                    .run_js("document.querySelector('#msg').textContent")
                    .map_err(|err| OpenPageError::PageOperation(format!("frame run_js: {err}")))?,
                Value::from("frame reconnect")
            );

            let disconnected_page = reconnected_page.clone().disconnect()?;
            let roundtrip_page = disconnected_page.reconnect(0)?;
            assert_eq!(roundtrip_page.target_id(), page.target_id());
            assert_eq!(
                roundtrip_page.run_js("document.querySelector('#msg').textContent")?,
                Value::from("page reconnect")
            );

            let disconnected_frame = reconnected_frame.disconnect()?;
            let roundtrip_frame = disconnected_frame.reconnect(0)?;
            assert_eq!(
                roundtrip_frame.run_js("document.querySelector('#msg').textContent")?,
                Value::from("frame reconnect")
            );

            Ok(roundtrip_page)
        })();

        let reconnected_page = match result {
            Ok(page) => page,
            Err(err) => {
                let _ = browser.close();
                let _ = fs::remove_dir_all(&temp_dir);
                panic!("reconnect regression failed before cleanup: {err}");
            }
        };

        let close_result = reconnected_page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser after reconnect: {err}");
        }
    }

    #[test]
    fn page_new_tab_with_new_context_creates_and_closes_isolated_tab() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("page-new-tab-new-context")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let new_tab = page.new_tab(Some("about:blank"), false, true, true)?;
            assert!(new_tab.wait_for_doc_loaded(5_000)?);

            let target_id = new_tab.target_id();
            new_tab.close()?;

            wait_until(Duration::from_secs(5), || {
                let tab_ids = browser.tab_ids().ok()?;
                if tab_ids.iter().all(|tab_id| tab_id != &target_id) {
                    Some(())
                } else {
                    None
                }
            })?;
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("new_context page tab regression");
    }

    #[test]
    fn page_wait_failures_raise_timeout_when_global_setting_enabled() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_raise_when_wait_failed(true);
        Settings::set_language("cn");

        let (load_url, load_server) = spawn_delayed_load_site(Duration::from_millis(250));
        let (browser, temp_dir) = launch_headless_test_browser("page-global-wait-failed")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let load_url_json = serde_json::to_string(&load_url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            page.run_js(&format!("window.location.href = {load_url_json};"))?;
            assert!(page.wait_for_load_start(1_000)?);

            let error = page
                .wait_for_doc_loaded(50)
                .expect_err("wait_for_doc_loaded should raise timeout");
            assert!(
                matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::wait_for_doc_loaded()") && message.contains("等待超时")),
                "unexpected wait error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = load_server.join();

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page global wait-failed setting regression");
    }

    #[test]
    fn page_execute_cdp_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("page-global-cdp-timeout")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            Settings::set_cdp_timeout(0.01);

            let params = EvaluateParams::builder()
                .expression("new Promise(resolve => setTimeout(() => resolve('ok'), 150))")
                .await_promise(true)
                .build()
                .map_err(OpenPageError::PageOperation)?;
            let error = page
                .execute_cdp(params)
                .expect_err("execute_cdp should respect global timeout");
            assert!(
                matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::execute_cdp()")),
                "unexpected cdp timeout error: {error}"
            );
            Ok(())
        })();

        Settings::reset();
        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page global cdp-timeout setting regression");
    }

    #[test]
    fn page_navigation_listener_registration_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");
        let result = runtime.block_on(async {
            register_navigation_listener_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "register navigation lifecycle listener",
            )
            .await
        });

        Settings::reset();

        let error = result.expect_err("navigation listener registration should time out");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("register navigation lifecycle listener")),
            "unexpected navigation registration timeout error: {error}"
        );
    }

    #[test]
    fn page_is_alive_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");
        let result = runtime.block_on(async {
            run_with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), OpenPageError>(())
                },
                timeout_duration_millis(cdp_timeout_duration()),
                "Page::is_alive()",
            )
            .await
        });

        Settings::reset();

        let error = result.expect_err("page is_alive should time out");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::is_alive()")),
            "unexpected page is_alive timeout error: {error}"
        );
    }

    #[test]
    fn page_cookie_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");
        let result = runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "set cookie",
            )
            .await
        });

        Settings::reset();

        let error = result.expect_err("page cookie operation should time out");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("set cookie")),
            "unexpected page cookie timeout error: {error}"
        );
    }

    #[test]
    fn page_url_and_title_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let url_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Option<String>, &'static str>(Some("https://example.com".to_string()))
                },
                "read url",
            ))
            .expect_err("page url operation should time out");
        assert!(
            matches!(url_error, OpenPageError::Timeout(ref message) if message.contains("read url")),
            "unexpected page url timeout error: {url_error}"
        );

        let title_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Option<String>, &'static str>(Some("example".to_string()))
                },
                "read title",
            ))
            .expect_err("page title operation should time out");

        Settings::reset();

        assert!(
            matches!(title_error, OpenPageError::Timeout(ref message) if message.contains("read title")),
            "unexpected page title timeout error: {title_error}"
        );
    }

    #[test]
    fn page_content_and_visual_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let html_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<String, &'static str>("<html></html>".to_string())
                },
                "read html",
            ))
            .expect_err("page html operation should time out");
        assert!(
            matches!(html_error, OpenPageError::Timeout(ref message) if message.contains("read html")),
            "unexpected page html timeout error: {html_error}"
        );

        let screenshot_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<u8>, &'static str>(vec![1, 2, 3])
                },
                "capture screenshot",
            ))
            .expect_err("page screenshot operation should time out");

        Settings::reset();

        assert!(
            matches!(screenshot_error, OpenPageError::Timeout(ref message) if message.contains("capture screenshot")),
            "unexpected page screenshot timeout error: {screenshot_error}"
        );
    }

    #[test]
    fn page_lookup_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let lookup_error = runtime
            .block_on(run_page_lookup_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "find element",
            ))
            .expect_err("page lookup should time out");
        assert!(
            matches!(lookup_error, OpenPageError::Timeout(ref message) if message.contains("find element")),
            "unexpected page lookup timeout error: {lookup_error}"
        );

        Settings::reset();

        let lookup_error = runtime
            .block_on(run_page_lookup_future_with_cdp_timeout(
                async { Err::<(), &'static str>("missing") },
                "find element",
            ))
            .expect_err("page lookup failure should remain ElementNotFound");
        assert!(
            matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "page operation find element failed: missing"),
            "unexpected page lookup error: {lookup_error}"
        );

        Settings::set_language("cn");

        let lookup_error = runtime
            .block_on(run_page_lookup_future_with_cdp_timeout(
                async { Err::<(), &'static str>("missing") },
                "find element",
            ))
            .expect_err("page lookup failure should localize");
        assert!(
            matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "页面操作 find element 失败: missing"),
            "unexpected localized page lookup error: {lookup_error}"
        );
    }

    #[test]
    fn page_cookie_pdf_and_close_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let cookie_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<chromiumoxide::cdp::browser_protocol::network::Cookie>, &'static str>(
                        Vec::new(),
                    )
                },
                "read cookies",
            ))
            .expect_err("page cookie read should time out");
        assert!(
            matches!(cookie_error, OpenPageError::Timeout(ref message) if message.contains("read cookies")),
            "unexpected page cookie read timeout error: {cookie_error}"
        );

        let pdf_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "save pdf",
            ))
            .expect_err("page save_pdf should time out");

        Settings::reset();

        assert!(
            matches!(pdf_error, OpenPageError::Timeout(ref message) if message.contains("save pdf")),
            "unexpected page save_pdf timeout error: {pdf_error}"
        );
    }

    #[test]
    fn page_frame_metadata_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let frame_name_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Option<String>, &'static str>(Some("frame-a".to_string()))
                },
                "read frame name",
            ))
            .expect_err("page frame name read should time out");
        assert!(
            matches!(frame_name_error, OpenPageError::Timeout(ref message) if message.contains("read frame name")),
            "unexpected page frame name timeout error: {frame_name_error}"
        );

        let frame_context_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Option<ExecutionContextId>, &'static str>(None)
                },
                "read frame execution context",
            ))
            .expect_err("page frame execution context read should time out");

        Settings::reset();

        assert!(
            matches!(frame_context_error, OpenPageError::Timeout(ref message) if message.contains("read frame execution context")),
            "unexpected page frame execution context timeout error: {frame_context_error}"
        );
    }

    #[test]
    fn page_navigation_operations_respect_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let navigate_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "navigate",
            ))
            .expect_err("page navigation should time out");
        assert!(
            matches!(navigate_error, OpenPageError::Timeout(ref message) if message.contains("navigate")),
            "unexpected page navigation timeout error: {navigate_error}"
        );

        let cookie_helper_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<chromiumoxide::cdp::browser_protocol::network::Cookie>, &'static str>(
                        Vec::new(),
                    )
                },
                "read cookies",
            ))
            .expect_err("page cookie helper read should time out");

        Settings::reset();

        assert!(
            matches!(cookie_helper_error, OpenPageError::Timeout(ref message) if message.contains("read cookies")),
            "unexpected page cookie helper timeout error: {cookie_helper_error}"
        );
    }

    #[test]
    fn page_pdf_generation_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");

        let pdf_error = runtime
            .block_on(run_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<u8>, &'static str>(vec![1, 2, 3])
                },
                "print pdf",
            ))
            .expect_err("page pdf generation should time out");

        Settings::reset();

        assert!(
            matches!(pdf_error, OpenPageError::Timeout(ref message) if message.contains("print pdf")),
            "unexpected page pdf timeout error: {pdf_error}"
        );
    }

    #[test]
    fn page_and_element_frame_index_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("page-frame-index-localization")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `<div id="host"></div>`;
                    return true;
                })()"#,
            )?;

            let host = page.find("css:#host")?;
            let assert_error = |label: &str, err: OpenPageError, expected: &str| {
                assert!(
                    matches!(err, OpenPageError::ElementNotFound(ref message) if message == expected),
                    "unexpected {label} error: {err}"
                );
            };

            assert_error(
                "page.get_frame(0)",
                page.get_frame(0isize)
                    .err()
                    .expect("page.get_frame(0) should fail"),
                "frame index must start from 1 or use negative indices from -1",
            );
            assert_error(
                "host.get_frame(0)",
                host.get_frame(0isize)
                    .err()
                    .expect("host.get_frame(0) should fail"),
                "frame index must start from 1 or use negative indices from -1",
            );
            assert_error(
                "page.get_frame(1)",
                page.get_frame(1isize)
                    .err()
                    .expect("page.get_frame(1) should fail without any frame"),
                "frame index out of range: 1",
            );
            assert_error(
                "host.get_frame(1)",
                host.get_frame(1isize)
                    .err()
                    .expect("host.get_frame(1) should fail without any frame"),
                "frame index out of range: 1",
            );

            Settings::set_language("cn");

            assert_error(
                "page.get_frame(0) localized",
                page.get_frame(0isize)
                    .err()
                    .expect("page.get_frame(0) should localize"),
                "frame 序号必须从 1 开始，或使用从 -1 开始的负序号",
            );
            assert_error(
                "host.get_frame(0) localized",
                host.get_frame(0isize)
                    .err()
                    .expect("host.get_frame(0) should localize"),
                "frame 序号必须从 1 开始，或使用从 -1 开始的负序号",
            );
            assert_error(
                "page.get_frame(1) localized",
                page.get_frame(1isize)
                    .err()
                    .expect("page.get_frame(1) should localize"),
                "frame 序号超出范围: 1",
            );
            assert_error(
                "host.get_frame(1) localized",
                host.get_frame(1isize)
                    .err()
                    .expect("host.get_frame(1) should localize"),
                "frame 序号超出范围: 1",
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page frame index localization regression");
    }

    #[test]
    fn page_ele_runtime_config_supports_none_value_and_nested_queries() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-ele-none-config").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <section id="card">
                            <span class="name">Alpha</span>
                            <span class="phone">10086</span>
                        </section>
                        <section id="tail">Omega</section>
                    `;
                    return true;
                })()"#,
            )?;

            assert_eq!(page.eles(".missing")?.len(), 0);

            let card = page.ele("#card")?;
            assert!(card.is_some());
            assert_eq!(card.ele(".name")?.text()?, Some("Alpha".to_string()));

            let missing_default = page.ele(".missing")?;
            assert!(missing_default.is_none());
            assert_eq!(missing_default.text()?, None);
            assert!(!missing_default.click()?);

            page.set_none_element_value(Some("missing"), true)?;

            let missing = page.ele(".missing")?;
            assert_eq!(missing.text()?, Some("missing".to_string()));
            assert_eq!(missing.attr("id")?, Some("missing".to_string()));
            assert_eq!(missing.ele(".child")?.text()?, Some("missing".to_string()));
            assert_eq!(missing.child()?.text()?, Some("missing".to_string()));
            assert_eq!(missing.parent()?.text()?, Some("missing".to_string()));
            assert_eq!(missing.next()?.text()?, Some("missing".to_string()));
            assert_eq!(missing.before()?.text()?, Some("missing".to_string()));
            assert_eq!(missing.after()?.text()?, Some("missing".to_string()));
            assert_eq!(missing.over()?.text()?, Some("missing".to_string()));
            assert_eq!(
                missing
                    .offset::<&str>(None, Some(0.0), Some(0.0), 50)?
                    .text()?,
                Some("missing".to_string())
            );
            assert_eq!(
                missing.east(None::<&str>, None, 1)?.text()?,
                Some("missing".to_string())
            );
            assert_eq!(
                page.ele("#card")?.ele(".phone")?.text()?,
                Some("10086".to_string())
            );
            assert!(missing.wait().deleted(100)?);

            page.set_raise_when_ele_not_found(true)?;
            let error = page.ele(".missing").expect_err("page ele should raise");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected page ele error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page ele runtime config regression");
    }

    #[test]
    fn elements_one_owned_shadow_root_supports_existing_and_missing_elements() {
        let (browser, temp_dir) = launch_headless_test_browser("elements-one-shadow-root")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div id="host"></div>
                        <div id="plain">Plain</div>
                    `;
                    const host = document.getElementById('host');
                    const root = host.attachShadow({mode: 'open'});
                    root.innerHTML = `
                        <span class="inside">Shadow Text</span>
                        <span class="inside">Shadow Extra</span>
                    `;
                    return true;
                })()"#,
            )?;

            let host = page.ele("#host")?;
            let shadow_root = host.shadow_root()?.expect("host shadow root");
            assert!(shadow_root.inner_html()?.contains("Shadow Text"));
            let inside = shadow_root.find(".inside").expect("shadow root css find");
            assert_eq!(inside.text()?, Some("Shadow Text".to_string()));
            let inside_by_xpath = shadow_root
                .find("xpath:.//*[@class='inside']")
                .expect("shadow root xpath find");
            assert_eq!(inside_by_xpath.text()?, Some("Shadow Text".to_string()));
            let inside_list = shadow_root
                .find_all(".inside")
                .expect("shadow root css find_all");
            assert_eq!(inside_list.len(), 2);
            assert_eq!(inside_list[0].text()?, Some("Shadow Text".to_string()));
            assert_eq!(inside_list[1].text()?, Some("Shadow Extra".to_string()));
            let inside_xpath_list = shadow_root
                .find_all("xpath:.//*[@class='inside']")
                .expect("shadow root xpath find_all");
            assert_eq!(inside_xpath_list.len(), 2);
            assert_eq!(
                inside_xpath_list[0].text()?,
                Some("Shadow Text".to_string())
            );
            assert_eq!(
                inside_xpath_list[1].text()?,
                Some("Shadow Extra".to_string())
            );
            let direct_child = shadow_root
                .child_with(Some("xpath:./span[@class='inside']"), 2)
                .expect("shadow root xpath child");
            assert_eq!(direct_child.text()?, Some("Shadow Extra".to_string()));
            let direct_children = shadow_root
                .children_with(Some("xpath:./span[@class='inside']"))
                .expect("shadow root xpath children");
            assert_eq!(direct_children.len(), 2);
            let shadow_root_alias = host.sr()?.expect("host sr alias");
            assert!(shadow_root_alias.inner_html()?.contains("Shadow Text"));

            let plain = page.ele("#plain")?;
            assert!(plain.shadow_root()?.is_none());

            let web_host = page.ele("#host")?.map(WebElement::Browser);
            let web_shadow_root = web_host.shadow_root()?.expect("web host shadow root");
            assert!(web_shadow_root.inner_html()?.contains("Shadow Text"));
            let direct_web = WebElement::Browser(page.wait_for("css:#host", 1_000)?);
            let direct_web_shadow = direct_web.sr()?.expect("direct web sr alias");
            assert!(direct_web_shadow.inner_html()?.contains("Shadow Text"));

            page.set_none_element_value(Some("missing"), true)?;
            let missing = page.ele(".missing")?;
            assert!(missing.shadow_root()?.is_none());
            assert!(missing.sr()?.is_none());

            let missing_web = page.ele(".missing")?.map(WebElement::Browser);
            assert!(missing_web.shadow_root()?.is_none());
            assert!(missing_web.sr()?.is_none());

            page.set_raise_when_ele_not_found(true)?;
            let error = missing
                .shadow_root()
                .expect_err("missing shadow_root should raise");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected missing shadow_root error: {error}"
            );
            let error = missing_web
                .shadow_root()
                .expect_err("missing web shadow_root should raise");
            assert!(
                matches!(error, OpenPageError::ElementNotFound(_)),
                "unexpected missing web shadow_root error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one owned shadow root regression");
    }

    #[test]
    fn elements_one_supports_set_scroll_and_select_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("elements-one-objects").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input class="item" id="name" value="">
                        <input class="item" id="agree" type="checkbox">
                        <div class="item" id="content">Old</div>
                        <div class="item" id="scrollbox" style="width:120px;height:80px;overflow:auto;">
                            <div style="width:600px;height:400px;"></div>
                        </div>
                        <select class="item" id="picker" multiple>
                            <option value="one">One</option>
                            <option value="two">Two</option>
                            <option value="three">Three</option>
                        </select>
                        <select class="item" id="single-picker">
                            <option value="solo">Solo</option>
                            <option value="duo">Duo</option>
                        </select>
                    `;
                    return true;
                })()"#,
            )?;

            let page_items = page.find_all(".item")?;
            let input_one = page_items.filter_one().tag("input", true)?;
            assert!(input_one.set().value("Omega")?);
            assert!(input_one.set().attr("data-role", "primary")?);
            assert!(input_one.set().property("tabIndex", &Value::from(3))?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("Omega")
            );
            assert_eq!(
                page.run_js("document.getElementById('name').getAttribute('data-role')")?,
                Value::from("primary")
            );
            assert_eq!(
                page.run_js("document.getElementById('name').tabIndex")?,
                Value::from(3)
            );
            assert!(input_one.remove_attr("data-role")?);
            assert_eq!(
                page.run_js("document.getElementById('name').getAttribute('data-role') === null")?,
                Value::from(true)
            );

            let content_one = page_items.filter_one().attr("id", "content", true)?;
            assert!(content_one.set().inner_html("<span>Changed</span>")?);
            assert!(content_one.set().style("display", "block")?);
            assert_eq!(
                page.run_js("document.getElementById('content').textContent")?,
                Value::from("Changed")
            );
            let content_one_select_err = content_one
                .select()
                .is_multi()
                .expect_err("div ElementsOne select().is_multi() should error");
            assert!(matches!(
                content_one_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let content_one_direct_select_err = content_one
                .select_by_text("noop")
                .expect_err("div ElementsOne select_by_text() should error");
            assert!(matches!(
                content_one_direct_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));

            let scroll_one = page_items.filter_one().attr("id", "scrollbox", true)?;
            assert!(scroll_one.scroll().to_location(30.0, 40.0)?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollTop")?,
                Value::from(40)
            );
            assert!(scroll_one.scroll().to_top()?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollTop")?,
                Value::from(0)
            );
            assert!(scroll_one.scroll().to_half()?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollTop > 0")?,
                Value::from(true)
            );
            assert!(scroll_one.scroll().to_rightmost()?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollLeft > 0")?,
                Value::from(true)
            );
            assert!(scroll_one.scroll().to_leftmost()?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollLeft")?,
                Value::from(0)
            );
            assert!(scroll_one.scroll_to_bottom()?);
            assert!(scroll_one.scroll().up(15.0)?);
            assert!(scroll_one.scroll().down(15.0)?);
            assert!(scroll_one.scroll().left(10.0)?);
            assert!(scroll_one.scroll().right(10.0)?);
            assert!(scroll_one.scroll().to_see(Some(true))?);
            assert!(scroll_one.scroll().to_center()?);

            let checkbox_one = page_items.filter_one().attr("id", "agree", true)?;
            assert!(checkbox_one.check(false, true)?);
            assert_eq!(
                page.run_js("document.getElementById('agree').checked")?,
                Value::from(true)
            );
            assert!(checkbox_one.uncheck(true)?);
            assert_eq!(
                page.run_js("document.getElementById('agree').checked")?,
                Value::from(false)
            );

            let select_one = page_items
                .filter_one()
                .attr("id", "picker", true)
                .map_err(|err| OpenPageError::PageOperation(format!("select_one picker: {err}")))?;
            assert_eq!(select_one.select_is_multi()?, Some(true));
            assert_eq!(select_one.select().is_multi()?, Some(true));
            assert_eq!(select_one.select_options()?.unwrap().len(), 3);
            assert_eq!(select_one.select().options()?.unwrap().len(), 3);
            assert!(select_one.select_by_value(["one", "three"])?);
            assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("one,three")
            );
            assert_eq!(select_one.select_selected_options()?.unwrap().len(), 2);
            assert_eq!(select_one.select().selected_options()?.unwrap().len(), 2);
            assert_eq!(
                select_one
                    .select_selected_option()?
                    .and_then(|option| option.value().ok())
                    .flatten(),
                Some("one".to_string())
            );
            assert!(select_one.select_clear()?);
            assert!(select_one.select().clear()?);
            assert!(select_one.select_by_locator("css:option[value='two']")?);
            assert!(select_one.select().by_index(2)?);
            assert!(select_one.select_by_index(2)?);
            assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("two")
            );
            assert!(
                select_one
                    .select()
                    .cancel_by_locator("css:option[value='two']")?
            );
            assert!(select_one.select().by_indices(&[1, 3])?);
            let select_options = select_one.select_options()?.unwrap();
            let select_option_refs = [&select_options[0], &select_options[2]];
            assert!(select_one.cancel_by_options(&select_option_refs)?);
            assert!(select_one.select().all()?);
            assert!(select_one.select_invert()?);
            assert_eq!(
                page.run_js("document.getElementById('picker').selectedOptions.length")?,
                Value::from(0)
            );

            let single_select_one = page_items
                .filter_one()
                .attr("id", "single-picker", true)
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("single_select_one filter: {err}"))
                })?;
            assert_eq!(
                single_select_one
                    .select_is_multi()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "single_select_one.select_is_multi(): {err}"
                    )))?,
                Some(false)
            );
            assert_eq!(
                single_select_one.select().is_multi().map_err(|err| {
                    OpenPageError::PageOperation(format!(
                        "single_select_one.select().is_multi(): {err}"
                    ))
                })?,
                Some(false)
            );
            let single_select_all_err = single_select_one
                .select_all()
                .expect_err("single ElementsOne select_all() should error");
            assert!(matches!(
                single_select_all_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let single_select_clear_err = single_select_one
                .select()
                .clear()
                .expect_err("single ElementsOne select().clear() should error");
            assert!(matches!(
                single_select_clear_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let single_select_invert_err = single_select_one
                .select_invert()
                .expect_err("single ElementsOne select_invert() should error");
            assert!(matches!(
                single_select_invert_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            assert_eq!(
                page.run_js("document.getElementById('single-picker').value")?,
                Value::from("solo")
            );

            let web_items = vec![
                WebElement::Browser(page.wait_for("css:#name", 1_000)?),
                WebElement::Browser(page.wait_for("css:#content", 1_000)?),
                WebElement::Browser(page.wait_for("css:#scrollbox", 1_000)?),
                WebElement::Browser(page.wait_for("css:#picker", 1_000)?),
            ];
            let web_input_one = web_items.filter_one().tag("input", true)?;
            assert!(web_input_one.set().value("Sigma")?);
            assert_eq!(
                page.run_js("document.getElementById('name').value")?,
                Value::from("Sigma")
            );
            let web_input_one_select_err = web_input_one
                .select()
                .is_multi()
                .expect_err("input ElementsOne<WebElement> select().is_multi() should error");
            assert!(matches!(
                web_input_one_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            assert!(web_input_one.set().attr("data-extra", "demo")?);
            assert!(web_input_one.set().property("tabIndex", &Value::from(5))?);
            assert!(web_input_one.remove_attr("data-extra")?);
            assert_eq!(
                page.run_js("document.getElementById('name').tabIndex")?,
                Value::from(5)
            );

            let web_scroll_one = web_items.filter_one().attr("id", "scrollbox", true)?;
            assert!(web_scroll_one.scroll().to_location(10.0, 20.0)?);
            assert!(web_scroll_one.scroll().to_rightmost()?);
            assert!(web_scroll_one.scroll().to_leftmost()?);
            assert!(web_scroll_one.scroll().up(5.0)?);
            assert!(web_scroll_one.scroll().down(5.0)?);
            assert_eq!(
                page.run_js("document.getElementById('scrollbox').scrollTop")?,
                Value::from(20)
            );

            let web_select_one =
                web_items
                    .filter_one()
                    .attr("id", "picker", true)
                    .map_err(|err| {
                        OpenPageError::PageOperation(format!("web_select_one picker: {err}"))
                    })?;
            assert_eq!(web_select_one.select_is_multi()?, Some(true));
            assert_eq!(web_select_one.select().is_multi()?, Some(true));
            assert!(web_select_one.select_by_text(["One", "Two"])?);
            assert_eq!(
                web_select_one.select().selected_options()?.unwrap().len(),
                2
            );
            assert_eq!(
                page.run_js("document.getElementById('picker').selectedOptions.length")?,
                Value::from(2)
            );
            assert!(web_select_one.select().cancel_by_value(["one", "two"])?);
            assert!(web_select_one.select_by_locator("css:option[value='three']")?);
            assert!(web_select_one.select().clear()?);
            assert!(web_select_one.select_clear()?);
            assert_eq!(
                page.run_js("document.getElementById('picker').selectedOptions.length")?,
                Value::from(0)
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one object-ops runtime regression");
    }

    #[test]
    fn elements_one_object_wrappers_support_clicker_and_select_waiting_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("elements-one-clicker-select")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <a class="item" id="open-tab" href="about:blank#elements-one-open" target="_blank">Open tab</a>
                        <a class="item" id="middle-tab" href="about:blank#elements-one-middle">Middle tab</a>
                        <button class="item" id="click-target">Click target</button>
                        <select class="item" id="picker" multiple></select>
                    `;
                    window.__clicks = 0;
                    window.__rightClicks = 0;
                    document.getElementById('click-target').addEventListener('click', () => {
                        window.__clicks += 1;
                    });
                    document.getElementById('click-target').addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__rightClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

            let page_items = page.find_all(".item")?;
            let button_one = page_items.filter_one().attr("id", "click-target", true)?;
            assert!(button_one.clicker().multi(2)?);
            assert!(button_one.clicker().at(Some(5.0), Some(5.0), "left", 1)?);
            assert_eq!(page.run_js("window.__clicks")?, Value::from(3));

            let select_one = page_items.filter_one().attr("id", "picker", true)?;
            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const one = document.createElement('option');
                        one.value = 'late-one';
                        one.text = 'Late One';
                        picker.appendChild(one);
                        const two = document.createElement('option');
                        two.value = 'late-two';
                        two.text = 'Late Two';
                        picker.appendChild(two);
                    }, 150);
                    return true;
                })()"#,
            )?;
            assert!(
                select_one
                    .select()
                    .by_value_with_timeout(["late-one", "late-two"], Some(1_000))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "elements one select by_value_with_timeout: {err}"
                    )))?
            );
            assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("late-one,late-two")
            );

            let missing_page = page_items.filter_one().attr("id", "missing", true)?;
            assert!(!missing_page.clicker().left()?);
            assert!(
                !missing_page
                    .select()
                    .by_text_with_timeout("noop", Some(100))?
            );
            assert_eq!(missing_page.select().is_multi()?, None);

            let web_items = vec![
                WebElement::Browser(page.wait_for("css:#click-target", 1_000)?),
                WebElement::Browser(page.wait_for("css:#open-tab", 1_000)?),
                WebElement::Browser(page.wait_for("css:#picker", 1_000)?),
            ];
            let web_button_one = web_items.filter_one().attr("id", "click-target", true)?;
            assert!(web_button_one.clicker().right()?);
            assert_eq!(page.run_js("window.__rightClicks")?, Value::from(1));

            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'web-late';
                        option.text = 'Web Late';
                        option.dataset.kind = 'late';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
            )?;
            let web_select_one = web_items.filter_one().attr("id", "picker", true)?;
            assert!(
                web_select_one
                    .select()
                    .by_locator_with_timeout("css:option[data-kind='late']", Some(1_000))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "web elements one select by_locator_with_timeout: {err}"
                    )))?
            );
            assert_eq!(
                page.run_js("document.getElementById('picker').selectedOptions[0].value")?,
                Value::from("web-late")
            );

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one object wrapper runtime regression");
    }

    #[test]
    fn elements_one_supports_states_rect_and_wait_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("elements-one-state-rect-wait")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" id="show-me" style="display:none">Show</button>
                        <button class="item" id="hide-me">Hide</button>
                        <button class="item" id="enable-me" disabled>Enable</button>
                        <button class="item" id="disabled-now" disabled>Disabled</button>
                        <button class="item" id="delete-me">Delete</button>
                        <div class="item" id="cover-wrap" style="position:relative;width:140px;height:40px;">
                            <button class="item" id="covered-btn" style="position:absolute;left:0;top:0;width:140px;height:40px;">Covered</button>
                            <div id="overlay" style="position:absolute;left:0;top:0;width:140px;height:40px;background:rgba(0,0,0,0.2);"></div>
                        </div>
                        <div class="item" id="no-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</div>
                        <div class="item" id="static-box" style="display:block;width:120px;height:80px;">Static</div>
                        <div class="item" id="scroll-box" style="display:block;width:80px;height:40px;overflow:auto;">
                            <div style="width:200px;height:120px;"></div>
                        </div>
                        <div style="height:1200px;"></div>
                    `;
                    setTimeout(() => {
                        document.getElementById('show-me').style.display = 'block';
                        document.getElementById('hide-me').style.display = 'none';
                        document.getElementById('enable-me').disabled = false;
                        document.getElementById('delete-me')?.remove();
                        const zero = document.getElementById('no-rect');
                        zero.style.width = '40px';
                        zero.style.height = '20px';
                    }, 200);
                    setTimeout(() => document.getElementById('overlay')?.remove(), 260);
                    return true;
                })()"#,
            )?;

            let page_items = page.find_all(".item")?;
            page.run_js(
                "(() => { \
                    window.scrollTo(0, 60); \
                    document.getElementById('scroll-box')?.scrollTo(25, 35); \
                    return true; \
                })()",
            )?;
            let scroll_y = page
                .run_js("(() => window.scrollY)()")?
                .as_f64()
                .expect("window.scrollY as f64");
            let static_one = page_items.filter_one().attr("id", "static-box", true)?;
            assert_eq!(static_one.states().is_alive()?, Some(true));
            assert_eq!(static_one.states().is_displayed()?, Some(true));
            assert_eq!(static_one.states().has_rect()?, Some(true));
            assert_eq!(static_one.states().is_in_viewport()?, Some(true));
            assert_eq!(
                static_one
                    .rect()
                    .size()?
                    .map(|(width, height)| (width.round() as i64, height.round() as i64)),
                Some((120, 80))
            );
            assert_eq!(
                static_one.rect().corners()?.map(|corners| corners.len()),
                Some(4)
            );
            assert_eq!(
                static_one
                    .rect()
                    .viewport_corners()?
                    .map(|corners| corners.len()),
                Some(4)
            );
            let static_location = static_one.rect().location()?.expect("static box location");
            let static_viewport_location = static_one
                .rect()
                .viewport_location()?
                .expect("static box viewport location");
            assert!((static_location.1 - (static_viewport_location.1 + scroll_y)).abs() < 1.0);
            let static_midpoint = static_one.rect().midpoint()?.expect("static box midpoint");
            let static_viewport_midpoint = static_one
                .rect()
                .viewport_midpoint()?
                .expect("static box viewport midpoint");
            assert!((static_midpoint.1 - (static_viewport_midpoint.1 + scroll_y)).abs() < 1.0);
            let static_click_point = static_one
                .rect()
                .click_point()?
                .expect("static box click point");
            let static_viewport_click_point = static_one
                .rect()
                .viewport_click_point()?
                .expect("static box viewport click point");
            assert!(
                (static_click_point.1 - (static_viewport_click_point.1 + scroll_y)).abs() < 1.0
            );
            assert!(static_one.rect().screen_location()?.is_some());
            assert!(static_one.rect().screen_midpoint()?.is_some());
            assert!(static_one.rect().screen_click_point()?.is_some());
            assert_eq!(static_one.rect().scroll_position()?, Some((0.0, 0.0)));
            assert!(static_one.wait().stop_moving(500)?);
            let scroll_box_one = page_items.filter_one().attr("id", "scroll-box", true)?;
            assert_eq!(
                scroll_box_one
                    .rect()
                    .scroll_position()?
                    .map(|(x, y)| (x.round() as i64, y.round() as i64)),
                Some((25, 35))
            );

            let covered_one = page_items.filter_one().attr("id", "covered-btn", true)?;
            assert_eq!(covered_one.states().is_covered()?, Some(true));
            assert!(covered_one.wait().covered(500)?);
            assert!(covered_one.wait().not_covered(1_500)?);
            assert_eq!(covered_one.states().is_covered()?, Some(false));

            let show_one = page_items.filter_one().attr("id", "show-me", true)?;
            assert!(show_one.wait().displayed(1_500)?);
            let hide_one = page_items.filter_one().attr("id", "hide-me", true)?;
            assert!(hide_one.wait().hidden(1_500)?);
            let enable_one = page_items.filter_one().attr("id", "enable-me", true)?;
            assert!(enable_one.wait().enabled(1_500)?);
            assert_eq!(enable_one.states().is_clickable()?, Some(true));
            let disabled_one = page_items.filter_one().attr("id", "disabled-now", true)?;
            assert!(disabled_one.wait().disabled(100)?);
            assert!(disabled_one.wait().disabled_or_deleted(100)?);
            let no_rect_one = page_items.filter_one().attr("id", "no-rect", true)?;
            assert!(no_rect_one.wait().has_rect(1_500)?);
            assert_eq!(no_rect_one.states().has_rect()?, Some(true));
            let delete_one = page_items.filter_one().attr("id", "delete-me", true)?;
            assert!(delete_one.wait().deleted(1_500)?);
            assert_eq!(delete_one.states().is_alive()?, Some(false));

            let missing_one = page_items.filter_one().attr("id", "missing", true)?;
            assert_eq!(missing_one.states().is_alive()?, None);
            assert_eq!(missing_one.rect().size()?, None);
            assert_eq!(missing_one.rect().click_point()?, None);
            assert_eq!(missing_one.rect().scroll_position()?, None);
            assert!(!missing_one.wait().displayed(100)?);
            assert!(missing_one.wait().deleted(100)?);
            assert!(missing_one.wait().disabled_or_deleted(100)?);

            let web_items = page
                .find_all(".item")?
                .into_iter()
                .map(WebElement::Browser)
                .collect::<Vec<_>>();
            let web_static_one = web_items.filter_one().attr("id", "static-box", true)?;
            assert_eq!(web_static_one.states().is_alive()?, Some(true));
            assert_eq!(
                web_static_one
                    .rect()
                    .size()?
                    .map(|(width, height)| (width.round() as i64, height.round() as i64)),
                Some((120, 80))
            );
            assert_eq!(
                web_static_one
                    .rect()
                    .viewport_corners()?
                    .map(|corners| corners.len()),
                Some(4)
            );
            let web_static_location = web_static_one
                .rect()
                .location()?
                .expect("web static box location");
            let web_static_viewport_location = web_static_one
                .rect()
                .viewport_location()?
                .expect("web static box viewport location");
            assert!(web_static_location.1 >= web_static_viewport_location.1);
            let web_static_midpoint = web_static_one
                .rect()
                .midpoint()?
                .expect("web static box midpoint");
            let web_static_viewport_midpoint = web_static_one
                .rect()
                .viewport_midpoint()?
                .expect("web static box viewport midpoint");
            assert!(web_static_midpoint.1 >= web_static_viewport_midpoint.1);
            let web_static_click_point = web_static_one
                .rect()
                .click_point()?
                .expect("web static box click point");
            let web_static_viewport_click_point = web_static_one
                .rect()
                .viewport_click_point()?
                .expect("web static box viewport click point");
            assert!(web_static_click_point.1 >= web_static_viewport_click_point.1);
            assert!(web_static_one.rect().screen_location()?.is_some());
            assert!(web_static_one.rect().screen_midpoint()?.is_some());
            assert!(web_static_one.rect().screen_click_point()?.is_some());
            assert_eq!(web_static_one.rect().scroll_position()?, Some((0.0, 0.0)));
            let web_scroll_box_one = web_items.filter_one().attr("id", "scroll-box", true)?;
            assert_eq!(
                web_scroll_box_one
                    .rect()
                    .scroll_position()?
                    .map(|(x, y)| (x.round() as i64, y.round() as i64)),
                Some((25, 35))
            );
            let web_show_one = web_items.filter_one().attr("id", "show-me", true)?;
            assert!(web_show_one.wait().displayed(100)?);
            let web_enable_one = web_items.filter_one().attr("id", "enable-me", true)?;
            assert!(web_enable_one.wait().clickable(100)?);
            let web_no_rect_one = web_items.filter_one().attr("id", "no-rect", true)?;
            assert!(web_no_rect_one.wait().has_rect(100)?);
            let web_delete_one = web_items.filter_one().attr("id", "delete-me", true)?;
            assert!(web_delete_one.wait().deleted(100)?);
            assert_eq!(web_delete_one.states().is_alive()?, None);
            let missing_web_one = web_items.filter_one().attr("id", "missing", true)?;
            assert_eq!(missing_web_one.states().is_alive()?, None);
            assert_eq!(missing_web_one.rect().size()?, None);
            assert_eq!(missing_web_one.rect().click_point()?, None);
            assert_eq!(missing_web_one.rect().scroll_position()?, None);
            assert!(!missing_web_one.wait().clickable(100)?);
            assert!(missing_web_one.wait().deleted(100)?);
            assert!(missing_web_one.wait().disabled_or_deleted(100)?);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("elements one state/rect/wait runtime regression");
    }

    #[test]
    fn element_and_webelement_object_wrappers_work_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("element-object-wrappers")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="name" value="" />
                        <div id="content">Old</div>
                        <div id="scrollbox" style="width:120px;height:80px;overflow:auto;">
                            <div style="width:600px;height:400px;"></div>
                        </div>
                        <select id="single-picker">
                            <option value="solo">Solo</option>
                            <option value="duo">Duo</option>
                        </select>
                        <select id="picker" multiple>
                            <option value="one" data-kind="primary">One</option>
                            <option value="two" data-kind="secondary">Two</option>
                            <option value="three" data-kind="secondary">Three</option>
                        </select>
                    `;
                    return true;
                })()"#,
            )?;

            let input = page.wait_for("css:#name", 1_000)?;
            input.set().value("Alpha")?;
            input.set().attr("data-role", "primary")?;
            assert_eq!(input.value()?, Some("Alpha".to_string()));
            assert_eq!(input.attr("data-role")?, Some("primary".to_string()));

            let content = page.wait_for("css:#content", 1_000)?;
            content
                .set()
                .inner_html("<span class=\"inner\">Changed</span>")?;
            assert_eq!(content.text()?, Some("Changed".to_string()));
            let content_select_err = content
                .select()
                .is_multi()
                .expect_err("div select().is_multi() should error");
            assert!(matches!(
                content_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let input_select_err = input
                .select()
                .by_text("noop")
                .expect_err("input select().by_text() should error");
            assert!(matches!(
                input_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));

            let scrollbox = page.wait_for("css:#scrollbox", 1_000)?;
            scrollbox.scroll().to_location(40.0, 60.0)?;
            assert_eq!(
                scrollbox.run_js("return this.scrollLeft === 40 && this.scrollTop === 60;")?,
                Value::from(true)
            );
            scrollbox.scroll().to_top()?;
            assert_eq!(
                scrollbox.run_js("return this.scrollTop === 0;")?,
                Value::from(true)
            );

            let select = page.wait_for("css:#picker", 1_000)?;
            assert!(select.select().is_multi()?);
            let options = select.select().options()?;
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].text()?, Some("One".to_string()));
            assert!(select.select().by_value("two")?);
            let selected_option = select
                .select()
                .selected_option()?
                .expect("selected option element");
            assert_eq!(selected_option.text()?, Some("Two".to_string()));
            assert_eq!(
                select.run_js("return this.options[1].selected && !this.options[0].selected;")?,
                Value::from(true)
            );
            assert!(
                select
                    .select()
                    .by_locator("css:option[data-kind='secondary']")?
            );
            assert_eq!(select.select().selected_options()?.len(), 2);
            assert!(select.select().cancel_by_text("Two")?);
            assert_eq!(
                select.run_js("return this.options[1].selected || this.options[2].selected;")?,
                Value::from(true)
            );
            assert!(select.select().cancel_by_value("three")?);
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            assert!(select.select().by_text(["One", "Three"])?);
            assert_eq!(select.select().selected_options()?.len(), 2);
            assert!(select.select().cancel_by_value(["one", "three"])?);
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            assert!(select.select().by_index([1, 3])?);
            assert_eq!(select.select().selected_options()?.len(), 2);
            assert!(select.select().cancel_by_index([1, 3])?);
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            assert!(select.select().by_option(&options[0])?);
            assert!(select.select().by_index(2)?);
            assert!(select.select().cancel_by_index(1)?);
            assert_eq!(
                select.run_js("return this.options[1].selected && !this.options[0].selected;")?,
                Value::from(true)
            );
            assert!(select.select().by_option([&options[0], &options[2]])?);
            assert_eq!(select.select().selected_options()?.len(), 3);
            assert!(
                select
                    .select()
                    .cancel_by_option([&options[0], &options[2]])?
            );
            assert_eq!(
                select.run_js("return this.options[1].selected && !this.options[0].selected && !this.options[2].selected;")?,
                Value::from(true)
            );
            assert!(select.select().by_indices(&[1, 3])?);
            assert_eq!(select.select().selected_options()?.len(), 3);
            assert!(select.select().cancel_by_indices(&[1, 2, 3])?);
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            let option_refs = [&options[0], &options[2]];
            assert!(select.select().by_options(&option_refs)?);
            assert_eq!(select.select().selected_options()?.len(), 2);
            assert!(select.select().cancel_by_options(&option_refs)?);
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            select.select().all()?;
            assert_eq!(select.select().selected_options()?.len(), 3);
            select.select().invert()?;
            assert_eq!(
                select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );

            let single_select = page.wait_for("css:#single-picker", 1_000)?;
            assert!(!single_select.select().is_multi()?);
            let single_all_err = single_select
                .select()
                .all()
                .expect_err("single select all() should error");
            assert!(matches!(
                single_all_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let single_clear_err = single_select
                .select()
                .clear()
                .expect_err("single select clear() should error");
            assert!(matches!(
                single_clear_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let single_invert_err = single_select
                .select()
                .invert()
                .expect_err("single select invert() should error");
            assert!(matches!(
                single_invert_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            assert_eq!(
                single_select.run_js("return this.value;")?,
                Value::from("solo")
            );

            let web_input = WebElement::Browser(page.wait_for("css:#name", 1_000)?);
            web_input.set().value("Beta")?;
            assert_eq!(web_input.value()?, Some("Beta".to_string()));
            assert!(web_input.select().selected_options()?.is_empty());

            let web_scrollbox = WebElement::Browser(page.wait_for("css:#scrollbox", 1_000)?);
            web_scrollbox.scroll().down(30.0)?;
            assert_eq!(
                web_scrollbox.run_js("return this.scrollTop === 30;")?,
                Value::from(true)
            );

            let web_select = WebElement::Browser(page.wait_for("css:#picker", 1_000)?);
            assert!(web_select.select().is_multi()?);
            let web_options = web_select.select().options()?;
            assert_eq!(web_options.len(), 3);
            assert_eq!(web_options[2].text()?, Some("Three".to_string()));
            web_select.select().clear()?;
            assert!(web_select.select().by_index(1)?);
            assert_eq!(
                web_select
                    .run_js("return this.options[0].selected && !this.options[1].selected;")?,
                Value::from(true)
            );
            assert!(web_select.select().by_option(&web_options[1])?);
            assert!(
                web_select
                    .select()
                    .cancel_by_locator("css:option[data-kind='secondary']")?
            );
            assert_eq!(
                web_select
                    .run_js("return this.options[1].selected || this.options[2].selected;")?,
                Value::from(false)
            );
            assert!(web_select.select().by_value(["one", "three"])?);
            assert_eq!(web_select.select().selected_options()?.len(), 2);
            assert!(web_select.select().cancel_by_text(["One", "Three"])?);
            assert_eq!(
                web_select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            assert!(web_select.select().by_index([1, 3])?);
            assert_eq!(web_select.select().selected_options()?.len(), 2);
            assert!(web_select.select().cancel_by_index([1, 3])?);
            assert_eq!(
                web_select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            let all_locators = vec![
                "css:option[data-kind='primary']".to_string(),
                "css:option[data-kind='secondary']".to_string(),
            ];
            assert!(web_select.select().by_locator(&all_locators)?);
            assert_eq!(web_select.select().selected_options()?.len(), 3);
            assert!(web_select.select().cancel_by_indices(&[1, 2, 3])?);
            assert_eq!(
                web_select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );
            let web_option_refs = [&web_options[0], &web_options[2]];
            assert!(
                web_select
                    .select()
                    .by_option([&web_options[0], &web_options[2]])?
            );
            assert!(
                web_select
                    .select()
                    .cancel_by_option([&web_options[0], &web_options[2]])?
            );
            assert_eq!(
                web_select
                    .run_js("return this.options[0].selected || this.options[2].selected;")?,
                Value::from(false)
            );
            assert!(web_select.select().by_options(&web_option_refs)?);
            assert!(web_select.select().cancel_by_options(&web_option_refs)?);
            assert_eq!(
                web_select
                    .run_js("return this.options[0].selected || this.options[2].selected;")?,
                Value::from(false)
            );
            web_select.select().all()?;
            assert_eq!(web_select.select().selected_options()?.len(), 3);
            web_select.select().invert()?;
            web_select.select().clear()?;
            assert_eq!(
                web_select
                    .run_js("return Array.from(this.options).every(option => !option.selected);")?,
                Value::from(true)
            );

            let web_single_select =
                WebElement::Browser(page.wait_for("css:#single-picker", 1_000)?);
            assert!(!web_single_select.select().is_multi()?);
            let web_single_all_err = web_single_select
                .select()
                .all()
                .expect_err("web single select all() should error");
            assert!(matches!(
                web_single_all_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let web_single_clear_err = web_single_select
                .select()
                .clear()
                .expect_err("web single select clear() should error");
            assert!(matches!(
                web_single_clear_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            let web_single_invert_err = web_single_select
                .select()
                .invert()
                .expect_err("web single select invert() should error");
            assert!(matches!(
                web_single_invert_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));
            assert_eq!(
                web_single_select.run_js("return this.value;")?,
                Value::from("solo")
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element object wrapper runtime regression");
    }

    #[test]
    fn element_and_webelement_states_rect_and_wait_wrappers_work_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("element-state-rect-wait")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="show-me" style="display:none">Show</button>
                        <button id="hide-me">Hide</button>
                        <button id="enable-me" disabled>Enable</button>
                        <button id="disabled-now" disabled>Disabled</button>
                        <button id="delete-me">Delete</button>
                        <div id="cover-wrap" style="position:relative;width:140px;height:40px;">
                            <button id="covered-btn" style="position:absolute;left:0;top:0;width:140px;height:40px;">Covered</button>
                            <div id="overlay" style="position:absolute;left:0;top:0;width:140px;height:40px;background:rgba(0,0,0,0.2);"></div>
                        </div>
                        <div id="no-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</div>
                        <div id="static-box" style="display:block;width:120px;height:80px;">Static</div>
                        <div id="scroll-box" style="display:block;width:80px;height:40px;overflow:auto;">
                            <div style="width:200px;height:120px;"></div>
                        </div>
                        <div style="height:1200px;"></div>
                    `;
                    setTimeout(() => {
                        document.getElementById('show-me').style.display = 'block';
                        document.getElementById('hide-me').style.display = 'none';
                        document.getElementById('enable-me').disabled = false;
                        document.getElementById('delete-me')?.remove();
                        const zero = document.getElementById('no-rect');
                        zero.style.width = '40px';
                        zero.style.height = '20px';
                    }, 200);
                    setTimeout(() => document.getElementById('overlay')?.remove(), 260);
                    return true;
                })()"#,
            )?;

            let show_me = page.wait_for("css:#show-me", 1_000)?;
            let web_show_me = WebElement::Browser(page.wait_for("css:#show-me", 1_000)?);
            let hide_me = page.wait_for("css:#hide-me", 1_000)?;
            let web_hide_me = WebElement::Browser(page.wait_for("css:#hide-me", 1_000)?);
            let enable_me = page.wait_for("css:#enable-me", 1_000)?;
            let web_enable_me = WebElement::Browser(page.wait_for("css:#enable-me", 1_000)?);
            let disabled_now = page.wait_for("css:#disabled-now", 1_000)?;
            let web_disabled_now = WebElement::Browser(page.wait_for("css:#disabled-now", 1_000)?);
            let delete_me = page.wait_for("css:#delete-me", 1_000)?;
            let web_delete_me = WebElement::Browser(page.wait_for("css:#delete-me", 1_000)?);
            let covered_btn = page.wait_for("css:#covered-btn", 1_000)?;
            let web_covered_btn = WebElement::Browser(page.wait_for("css:#covered-btn", 1_000)?);
            let no_rect = page.wait_for("css:#no-rect", 1_000)?;
            let web_no_rect = WebElement::Browser(page.wait_for("css:#no-rect", 1_000)?);
            let static_box = page.wait_for("css:#static-box", 1_000)?;
            let web_static_box = WebElement::Browser(page.wait_for("css:#static-box", 1_000)?);
            let scroll_box = page.wait_for("css:#scroll-box", 1_000)?;
            let web_scroll_box = WebElement::Browser(page.wait_for("css:#scroll-box", 1_000)?);
            page.run_js(
                "(() => { \
                    window.scrollTo(0, 60); \
                    document.getElementById('scroll-box')?.scrollTo(25, 35); \
                    return true; \
                })()",
            )?;
            let scroll_y = page
                .run_js("(() => window.scrollY)()")?
                .as_f64()
                .expect("window.scrollY as f64");

            assert!(static_box.states().is_alive()?);
            assert!(static_box.states().is_displayed()?);
            assert!(static_box.states().is_enabled()?);
            assert!(static_box.states().has_rect()?);
            assert!(static_box.states().is_in_viewport()?);
            assert!(static_box.states().is_whole_in_viewport()?);
            assert!(!static_box.states().is_covered()?);
            assert_eq!(
                static_box
                    .rect()
                    .size()?
                    .map(|(width, height)| (width.round() as i64, height.round() as i64)),
                Some((120, 80))
            );
            assert_eq!(
                static_box.rect().corners()?.map(|corners| corners.len()),
                Some(4)
            );
            assert_eq!(
                static_box
                    .rect()
                    .viewport_corners()?
                    .map(|corners| corners.len()),
                Some(4)
            );
            let static_location = static_box.rect().location()?.expect("static box location");
            let static_viewport_location = static_box
                .rect()
                .viewport_location()?
                .expect("static box viewport location");
            assert!((static_location.1 - (static_viewport_location.1 + scroll_y)).abs() < 1.0);
            let static_midpoint = static_box.rect().midpoint()?.expect("static box midpoint");
            let static_viewport_midpoint = static_box
                .rect()
                .viewport_midpoint()?
                .expect("static box viewport midpoint");
            assert!((static_midpoint.1 - (static_viewport_midpoint.1 + scroll_y)).abs() < 1.0);
            let static_click_point = static_box
                .rect()
                .click_point()?
                .expect("static box click point");
            let static_viewport_click_point = static_box
                .rect()
                .viewport_click_point()?
                .expect("static box viewport click point");
            assert!(
                (static_click_point.1 - (static_viewport_click_point.1 + scroll_y)).abs() < 1.0
            );
            assert!(static_box.rect().screen_location()?.is_some());
            assert!(static_box.rect().screen_midpoint()?.is_some());
            assert!(static_box.rect().screen_click_point()?.is_some());
            assert_eq!(static_box.rect().scroll_position()?, Some((0.0, 0.0)));
            assert_eq!(
                scroll_box
                    .rect()
                    .scroll_position()?
                    .map(|(x, y)| (x.round() as i64, y.round() as i64)),
                Some((25, 35))
            );
            assert!(static_box.wait().stop_moving(500)?);

            assert!(web_static_box.states().is_alive()?);
            assert!(web_static_box.states().is_displayed()?);
            assert!(web_static_box.states().is_enabled()?);
            assert!(web_static_box.states().has_rect()?);
            assert!(web_static_box.states().is_in_viewport()?);
            assert!(web_static_box.states().is_whole_in_viewport()?);
            assert_eq!(
                web_static_box
                    .rect()
                    .size()?
                    .map(|(width, height)| (width.round() as i64, height.round() as i64)),
                Some((120, 80))
            );
            assert_eq!(
                web_static_box
                    .rect()
                    .corners()?
                    .map(|corners| corners.len()),
                Some(4)
            );
            assert_eq!(
                web_static_box
                    .rect()
                    .viewport_corners()?
                    .map(|corners| corners.len()),
                Some(4)
            );
            let web_static_location = web_static_box
                .rect()
                .location()?
                .expect("web static box location");
            let web_static_viewport_location = web_static_box
                .rect()
                .viewport_location()?
                .expect("web static box viewport location");
            assert!(web_static_location.1 >= web_static_viewport_location.1);
            let web_static_midpoint = web_static_box
                .rect()
                .midpoint()?
                .expect("web static box midpoint");
            let web_static_viewport_midpoint = web_static_box
                .rect()
                .viewport_midpoint()?
                .expect("web static box viewport midpoint");
            assert!(web_static_midpoint.1 >= web_static_viewport_midpoint.1);
            let web_static_click_point = web_static_box
                .rect()
                .click_point()?
                .expect("web static box click point");
            let web_static_viewport_click_point = web_static_box
                .rect()
                .viewport_click_point()?
                .expect("web static box viewport click point");
            assert!(web_static_click_point.1 >= web_static_viewport_click_point.1);
            assert!(web_static_box.rect().screen_location()?.is_some());
            assert!(web_static_box.rect().screen_midpoint()?.is_some());
            assert!(web_static_box.rect().screen_click_point()?.is_some());
            assert_eq!(web_static_box.rect().scroll_position()?, Some((0.0, 0.0)));
            assert_eq!(
                web_scroll_box
                    .rect()
                    .scroll_position()?
                    .map(|(x, y)| (x.round() as i64, y.round() as i64)),
                Some((25, 35))
            );

            assert!(covered_btn.states().is_covered()?);
            assert!(covered_btn.wait().covered(500)?);
            assert!(web_covered_btn.wait().not_covered(1_500)?);
            assert!(!web_covered_btn.states().is_covered()?);

            assert!(show_me.wait().displayed(1_500)?);
            assert!(web_show_me.wait().displayed(100)?);
            assert!(hide_me.wait().hidden(1_500)?);
            assert!(web_hide_me.wait().hidden(100)?);

            assert!(enable_me.wait().enabled(1_500)?);
            assert!(web_enable_me.wait().clickable(1_500)?);
            assert!(web_enable_me.states().is_clickable()?);

            assert!(disabled_now.wait().disabled(100)?);
            assert!(disabled_now.wait().disabled_or_deleted(100)?);
            assert!(web_disabled_now.wait().disabled(100)?);

            assert!(no_rect.wait().has_rect(1_500)?);
            assert!(web_no_rect.wait().has_rect(100)?);
            assert!(web_no_rect.states().has_rect()?);

            assert!(delete_me.wait().deleted(1_500)?);
            assert!(!web_delete_me.states().is_alive()?);
            assert!(web_delete_me.wait().deleted(100)?);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element state/rect/wait wrapper runtime regression");
    }

    #[test]
    fn element_screen_points_follow_dp_device_pixel_ratio_formula() {
        let (browser, temp_dir) =
            launch_headless_test_browser("element-screen-points").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div
                            id="box"
                            style="position:absolute;left:80px;top:120px;width:100px;height:60px;border:4px solid #111;padding:6px;background:#eee;"
                        ></div>
                    `;
                    return true;
                })()"#,
            )?;

            let element = page.wait_for("css:#box", 1_000)?;
            let web_element = WebElement::Browser(page.wait_for("css:#box", 1_000)?);
            let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
                expected_dp_viewport_screen_origin(&page)?;
            assert!(
                (device_pixel_ratio - 2.0).abs() < 0.01,
                "devicePixelRatio override did not apply: {device_pixel_ratio}"
            );

            let viewport_location = element
                .rect_viewport_location()?
                .expect("element viewport location");
            let screen_location = element
                .rect_screen_location()?
                .expect("element screen location");
            assert_pair_close(
                screen_location,
                (
                    (viewport_screen_x + viewport_location.0) * device_pixel_ratio,
                    (viewport_screen_y + viewport_location.1) * device_pixel_ratio,
                ),
                "element screen_location",
            );

            let viewport_midpoint = element
                .rect_viewport_midpoint()?
                .expect("element viewport midpoint");
            let screen_midpoint = element
                .rect_screen_midpoint()?
                .expect("element screen midpoint");
            assert_pair_close(
                screen_midpoint,
                (
                    (viewport_screen_x + viewport_midpoint.0) * device_pixel_ratio,
                    (viewport_screen_y + viewport_midpoint.1) * device_pixel_ratio,
                ),
                "element screen_midpoint",
            );

            let viewport_click_point = element
                .rect_viewport_click_point()?
                .expect("element viewport click point");
            let screen_click_point = element
                .rect_screen_click_point()?
                .expect("element screen click point");
            assert_pair_close(
                screen_click_point,
                (
                    (viewport_screen_x + viewport_click_point.0) * device_pixel_ratio,
                    (viewport_screen_y + viewport_click_point.1) * device_pixel_ratio,
                ),
                "element screen_click_point",
            );

            assert_pair_close(
                web_element
                    .rect_screen_location()?
                    .expect("web element screen location"),
                screen_location,
                "web element screen_location",
            );
            assert_pair_close(
                web_element
                    .rect_screen_midpoint()?
                    .expect("web element screen midpoint"),
                screen_midpoint,
                "web element screen_midpoint",
            );
            assert_pair_close(
                web_element
                    .rect_screen_click_point()?
                    .expect("web element screen click point"),
                screen_click_point,
                "web element screen_click_point",
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element screen point formula regression");
    }

    #[test]
    fn iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
        let (browser, temp_dir) = launch_headless_test_browser("iframe-element-screen-points")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe
                            id="demo-frame"
                            style="position:absolute;left:160px;top:90px;width:420px;height:260px;border:0;"
                            srcdoc="<html><head><title>Inside Frame</title></head><body style='margin:0;height:1600px'><div id='inner-box' style='position:absolute;left:48px;top:72px;width:90px;height:54px;border:3px solid #111;padding:5px;background:#eee;'></div></body></html>"
                        ></iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let frame = page.get_frame_context("css:#demo-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            assert_eq!(frame.title()?, Some("Inside Frame".to_string()));
            assert!(frame.inner_html()?.contains("inner-box"));
            frame.run_js("(window.scrollTo(0, 23), true)")?;
            let frame_scroll_position = frame.scroll_position()?;
            assert_eq!(
                (
                    frame_scroll_position.0.round() as i64,
                    frame_scroll_position.1.round() as i64,
                ),
                (0, 23)
            );
            let element = frame.find("css:#inner-box")?;
            let web_element = WebElement::Browser(frame.find("css:#inner-box")?);

            let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
                expected_dp_viewport_screen_origin(&page)?;
            let frame_viewport_location = frame
                .frame_element()
                .rect_viewport_location()?
                .expect("frame viewport location");

            let viewport_location = element
                .rect_viewport_location()?
                .expect("iframe element viewport location");
            let screen_location = element
                .rect_screen_location()?
                .expect("iframe element screen location");
            assert_pair_close(
                screen_location,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_location.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_location.1)
                        * device_pixel_ratio,
                ),
                "iframe element screen_location",
            );

            let viewport_midpoint = element
                .rect_viewport_midpoint()?
                .expect("iframe element viewport midpoint");
            let screen_midpoint = element
                .rect_screen_midpoint()?
                .expect("iframe element screen midpoint");
            assert_pair_close(
                screen_midpoint,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_midpoint.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_midpoint.1)
                        * device_pixel_ratio,
                ),
                "iframe element screen_midpoint",
            );

            let viewport_click_point = element
                .rect_viewport_click_point()?
                .expect("iframe element viewport click point");
            let screen_click_point = element
                .rect_screen_click_point()?
                .expect("iframe element screen click point");
            assert_pair_close(
                screen_click_point,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_click_point.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_click_point.1)
                        * device_pixel_ratio,
                ),
                "iframe element screen_click_point",
            );

            assert_pair_close(
                web_element
                    .rect_screen_location()?
                    .expect("iframe web element screen location"),
                screen_location,
                "iframe web element screen_location",
            );
            assert_pair_close(
                web_element
                    .rect_screen_midpoint()?
                    .expect("iframe web element screen midpoint"),
                screen_midpoint,
                "iframe web element screen_midpoint",
            );
            assert_pair_close(
                web_element
                    .rect_screen_click_point()?
                    .expect("iframe web element screen click point"),
                screen_click_point,
                "iframe web element screen_click_point",
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("iframe element screen point formula regression");
    }

    #[test]
    fn cross_origin_iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
        let (browser, temp_dir) = launch_headless_test_browser("xorigin-iframe-screen-points")
            .expect("launch headless browser");
        let (parent_url, parent_server, child_server) = spawn_cross_origin_iframe_site();

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
            page.goto(&parent_url)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let frame = page.get_frame_context("css:#cross-frame")?;
            assert!(frame.wait_for_doc_loaded(5_000)?);
            assert_eq!(frame.title()?, Some("Cross Origin Child".to_string()));

            let element = frame.find("css:#inner-box")?;
            let web_element = WebElement::Browser(frame.find("css:#inner-box")?);
            let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
                expected_dp_viewport_screen_origin(&page)?;
            let frame_viewport_location = frame
                .frame_element()
                .rect_viewport_location()?
                .expect("cross-origin frame viewport location");

            let viewport_location = element
                .rect_viewport_location()?
                .expect("cross-origin iframe element viewport location");
            let screen_location = element
                .rect_screen_location()?
                .expect("cross-origin iframe element screen location");
            assert_pair_close(
                screen_location,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_location.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_location.1)
                        * device_pixel_ratio,
                ),
                "cross-origin iframe element screen_location",
            );

            let viewport_midpoint = element
                .rect_viewport_midpoint()?
                .expect("cross-origin iframe element viewport midpoint");
            let screen_midpoint = element
                .rect_screen_midpoint()?
                .expect("cross-origin iframe element screen midpoint");
            assert_pair_close(
                screen_midpoint,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_midpoint.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_midpoint.1)
                        * device_pixel_ratio,
                ),
                "cross-origin iframe element screen_midpoint",
            );

            let viewport_click_point = element
                .rect_viewport_click_point()?
                .expect("cross-origin iframe element viewport click point");
            let screen_click_point = element
                .rect_screen_click_point()?
                .expect("cross-origin iframe element screen click point");
            assert_pair_close(
                screen_click_point,
                (
                    (viewport_screen_x + frame_viewport_location.0 + viewport_click_point.0)
                        * device_pixel_ratio,
                    (viewport_screen_y + frame_viewport_location.1 + viewport_click_point.1)
                        * device_pixel_ratio,
                ),
                "cross-origin iframe element screen_click_point",
            );

            assert_pair_close(
                web_element
                    .rect_screen_location()?
                    .expect("cross-origin web element screen location"),
                screen_location,
                "cross-origin web element screen_location",
            );
            assert_pair_close(
                web_element
                    .rect_screen_midpoint()?
                    .expect("cross-origin web element screen midpoint"),
                screen_midpoint,
                "cross-origin web element screen_midpoint",
            );
            assert_pair_close(
                web_element
                    .rect_screen_click_point()?
                    .expect("cross-origin web element screen click point"),
                screen_click_point,
                "cross-origin web element screen_click_point",
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        parent_server.join().expect("join parent iframe server");
        child_server.join().expect("join child iframe server");

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("cross-origin iframe element screen point formula regression");
    }

    #[test]
    fn nested_cross_origin_iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
        let (browser, temp_dir) =
            launch_headless_test_browser("nested-xorigin-iframe-screen-points")
                .expect("launch headless browser");
        let (parent_url, parent_server, child_server, grandchild_server) =
            spawn_nested_cross_origin_iframe_site();

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
            page.goto(&parent_url)?;
            assert!(page.wait_for_doc_loaded(5_000)?);

            let outer_frame = page.get_frame_context("css:#outer-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("outer frame context: {err}"))
            })?;
            assert!(outer_frame.wait_for_doc_loaded(5_000).map_err(|err| {
                OpenPageError::PageOperation(format!("outer frame wait_for_doc_loaded: {err}"))
            })?);
            let inner_frame_element = outer_frame.find("css:#inner-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("outer frame find inner frame: {err}"))
            })?;
            let inner_frame = page
                .get_frame_context(&inner_frame_element)
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("inner frame context from element: {err}"))
                })?;
            assert!(inner_frame.wait_for_doc_loaded(5_000).map_err(|err| {
                OpenPageError::PageOperation(format!("inner frame wait_for_doc_loaded: {err}"))
            })?);
            assert_eq!(
                inner_frame
                    .title()
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "inner frame title: {err}"
                    )))?,
                Some("Nested Cross Origin Grandchild".to_string())
            );

            let element = inner_frame.find("css:#deep-box").map_err(|err| {
                OpenPageError::PageOperation(format!("inner frame find deep-box: {err}"))
            })?;
            let web_element =
                WebElement::Browser(inner_frame.find("css:#deep-box").map_err(|err| {
                    OpenPageError::PageOperation(format!(
                        "inner frame find deep-box for web element: {err}"
                    ))
                })?);
            let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
                expected_dp_viewport_screen_origin(&page)?;
            let outer_frame_viewport_location = outer_frame
                .frame_element()
                .rect_viewport_location()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!(
                        "outer frame rect_viewport_location: {err}"
                    ))
                })?
                .expect("outer frame viewport location");
            let inner_frame_viewport_location = inner_frame
                .frame_element()
                .rect_viewport_location()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!(
                        "inner frame rect_viewport_location: {err}"
                    ))
                })?
                .expect("inner frame viewport location");

            let viewport_location = element
                .rect_viewport_location()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("deep-box rect_viewport_location: {err}"))
                })?
                .expect("nested cross-origin iframe element viewport location");
            let screen_location = element
                .rect_screen_location()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("deep-box rect_screen_location: {err}"))
                })?
                .expect("nested cross-origin iframe element screen location");
            assert_pair_close(
                screen_location,
                (
                    (viewport_screen_x
                        + outer_frame_viewport_location.0
                        + inner_frame_viewport_location.0
                        + viewport_location.0)
                        * device_pixel_ratio,
                    (viewport_screen_y
                        + outer_frame_viewport_location.1
                        + inner_frame_viewport_location.1
                        + viewport_location.1)
                        * device_pixel_ratio,
                ),
                "nested cross-origin iframe element screen_location",
            );

            let viewport_midpoint = element
                .rect_viewport_midpoint()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("deep-box rect_viewport_midpoint: {err}"))
                })?
                .expect("nested cross-origin iframe element viewport midpoint");
            let screen_midpoint = element
                .rect_screen_midpoint()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("deep-box rect_screen_midpoint: {err}"))
                })?
                .expect("nested cross-origin iframe element screen midpoint");
            assert_pair_close(
                screen_midpoint,
                (
                    (viewport_screen_x
                        + outer_frame_viewport_location.0
                        + inner_frame_viewport_location.0
                        + viewport_midpoint.0)
                        * device_pixel_ratio,
                    (viewport_screen_y
                        + outer_frame_viewport_location.1
                        + inner_frame_viewport_location.1
                        + viewport_midpoint.1)
                        * device_pixel_ratio,
                ),
                "nested cross-origin iframe element screen_midpoint",
            );

            let viewport_click_point = element
                .rect_viewport_click_point()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!(
                        "deep-box rect_viewport_click_point: {err}"
                    ))
                })?
                .expect("nested cross-origin iframe element viewport click point");
            let screen_click_point = element
                .rect_screen_click_point()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("deep-box rect_screen_click_point: {err}"))
                })?
                .expect("nested cross-origin iframe element screen click point");
            assert_pair_close(
                screen_click_point,
                (
                    (viewport_screen_x
                        + outer_frame_viewport_location.0
                        + inner_frame_viewport_location.0
                        + viewport_click_point.0)
                        * device_pixel_ratio,
                    (viewport_screen_y
                        + outer_frame_viewport_location.1
                        + inner_frame_viewport_location.1
                        + viewport_click_point.1)
                        * device_pixel_ratio,
                ),
                "nested cross-origin iframe element screen_click_point",
            );

            assert_pair_close(
                web_element
                    .rect_screen_location()?
                    .expect("nested cross-origin web element screen location"),
                screen_location,
                "nested cross-origin web element screen_location",
            );
            assert_pair_close(
                web_element
                    .rect_screen_midpoint()?
                    .expect("nested cross-origin web element screen midpoint"),
                screen_midpoint,
                "nested cross-origin web element screen_midpoint",
            );
            assert_pair_close(
                web_element
                    .rect_screen_click_point()?
                    .expect("nested cross-origin web element screen click point"),
                screen_click_point,
                "nested cross-origin web element screen_click_point",
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        parent_server
            .join()
            .expect("join nested parent iframe server");
        child_server
            .join()
            .expect("join nested child iframe server");
        grandchild_server
            .join()
            .expect("join nested grandchild iframe server");

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("nested cross-origin iframe element screen point formula regression");
    }

    #[test]
    fn select_waits_for_delayed_options_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("select-delayed-options")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `<select id="picker" multiple></select>`;
                    return true;
                })()"#,
            )?;

            let select = page.wait_for("css:#picker", 1_000)?;

            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-text';
                        option.text = 'Late Text';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
            )?;
            let start = Instant::now();
            assert!(select.select().by_text("Late Text")?);
            assert!(start.elapsed() >= Duration::from_millis(100));
            assert_eq!(
                select.run_js("return this.value;")?,
                Value::from("late-text")
            );

            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-value';
                        option.text = 'Late Value';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
            )?;
            let web_select = WebElement::Browser(page.wait_for("css:#picker", 1_000)?);
            let start = Instant::now();
            assert!(
                web_select
                    .select()
                    .by_value_with_timeout("late-value", Some(1_000))?
            );
            assert!(start.elapsed() >= Duration::from_millis(100));
            assert_eq!(
                web_select.run_js("return this.value;")?,
                Value::from("late-value")
            );

            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        for (const [index, value] of ['one', 'two'].entries()) {
                            const option = document.createElement('option');
                            option.value = value;
                            option.text = `Option ${index + 1}`;
                            picker.appendChild(option);
                        }
                    }, 150);
                    return true;
                })()"#,
            )?;
            let page_selects = page.find_all("css:#picker")?;
            let select_one = page_selects.filter_one().tag("select", true)?;
            let start = Instant::now();
            assert!(select_one.select_by_index([1, 2])?);
            assert!(start.elapsed() >= Duration::from_millis(100));
            assert_eq!(
                page.run_js(
                    "Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')"
                )?,
                Value::from("one,two")
            );

            page.run_js(
                r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-locator';
                        option.text = 'Late Locator';
                        option.dataset.kind = 'locator';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
            )?;
            let web_selects = vec![WebElement::Browser(page.wait_for("css:#picker", 1_000)?)];
            let web_select_one = web_selects.filter_one().tag("select", true)?;
            let start = Instant::now();
            assert!(
                web_select_one.select_by_locator_with_timeout(
                    "css:option[data-kind='locator']",
                    Some(1_000)
                )?
            );
            assert!(start.elapsed() >= Duration::from_millis(100));
            assert_eq!(
                page.run_js("document.getElementById('picker').selectedOptions[0].value")?,
                Value::from("late-locator")
            );

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("select delayed options runtime regression");
    }

    #[test]
    fn element_and_webelement_clicker_work_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("element-clicker").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="click-target">Click target</button>
                        <select id="single-picker">
                            <option id="single-a" value="a" selected>Single A</option>
                            <option id="single-b" value="b">Single B</option>
                        </select>
                        <select id="multi-picker" multiple>
                            <option id="multi-a" value="a">Multi A</option>
                            <option id="multi-b" value="b">Multi B</option>
                        </select>
                    `;
                    window.__clicks = 0;
                    window.__rightClicks = 0;
                    document.getElementById('click-target').addEventListener('click', () => {
                        window.__clicks += 1;
                    });
                    document.getElementById('click-target').addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__rightClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

            let click_target = page.wait_for("css:#click-target", 1_000)?;
            click_target.clicker().multi(2)?;
            click_target.clicker().at(Some(5.0), Some(5.0), "left", 1)?;
            assert_eq!(page.run_js("window.__clicks")?, Value::from(3));

            page.wait_for("css:#single-b", 1_000)?
                .clicker()
                .left()
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("single-b clicker.left(): {err}"))
                })?;
            assert_eq!(
                page.run_js("document.getElementById('single-picker').value")?,
                Value::from("b")
            );
            let multi_a = page.wait_for("css:#multi-a", 1_000)?;
            multi_a.clicker().left().map_err(|err| {
                OpenPageError::PageOperation(format!("multi-a first clicker.left(): {err}"))
            })?;
            assert_eq!(
                page.run_js("document.getElementById('multi-a').selected")?,
                Value::from(true)
            );
            multi_a.clicker().left().map_err(|err| {
                OpenPageError::PageOperation(format!("multi-a second clicker.left(): {err}"))
            })?;
            assert_eq!(
                page.run_js("document.getElementById('multi-a').selected")?,
                Value::from(false)
            );

            let web_click_target = WebElement::Browser(page.wait_for("css:#click-target", 1_000)?);
            web_click_target.clicker().right()?;
            assert_eq!(page.run_js("window.__rightClicks")?, Value::from(1));
            let web_multi_b = WebElement::Browser(page.wait_for("css:#multi-b", 1_000)?);
            web_multi_b.clicker().left().map_err(|err| {
                OpenPageError::PageOperation(format!("web multi-b clicker.left(): {err}"))
            })?;
            assert_eq!(
                page.run_js("document.getElementById('multi-b').selected")?,
                Value::from(true)
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element clicker runtime regression");
    }

    #[test]
    fn element_clicker_left_with_options_supports_js_fallback_and_click_failed_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("element-clicker-options")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="hidden-click" style="visibility:hidden;width:120px;height:32px;">
                            Hidden click
                        </button>
                    `;
                    window.__hiddenClicks = 0;
                    document.getElementById('hidden-click').addEventListener('click', () => {
                        window.__hiddenClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

            let hidden = page.wait_for("css:#hidden-click", 1_000)?;
            hidden.click()?;
            assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(0));

            page.click("css:#hidden-click")?;
            assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(0));

            assert!(hidden.clicker().left_with_options(None, Some(100), false)?);
            assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(1));

            assert!(
                !hidden
                    .clicker()
                    .left_with_options(Some(false), Some(100), false)?
            );
            assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(1));

            let web_hidden = WebElement::Browser(page.wait_for("css:#hidden-click", 1_000)?);
            assert!(
                web_hidden
                    .clicker()
                    .left_with_options(Some(true), Some(100), false)?
            );
            assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(2));

            Settings::set_raise_when_click_failed(true);
            let direct_error = hidden
                .click()
                .expect_err("direct click should raise when global setting is enabled");
            assert!(
                matches!(direct_error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
                "unexpected direct click failure error: {direct_error}"
            );

            let page_error = page
                .click("css:#hidden-click")
                .expect_err("page.click() should raise when global setting is enabled");
            assert!(
                matches!(page_error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
                "unexpected page.click() failure error: {page_error}"
            );

            let error = hidden
                .clicker()
                .left_with_options(Some(false), Some(100), false)
                .expect_err("click failure should raise when global setting is enabled");
            assert!(
                matches!(error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
                "unexpected click failure error: {error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element clicker option runtime regression");
    }

    #[test]
    fn non_left_click_helpers_share_click_failed_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (browser, temp_dir) = launch_headless_test_browser("element-clicker-non-left-fail")
            .expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button
                            id="no-rect-click"
                            style="display:inline-block;width:0;height:0;overflow:hidden;padding:0;border:0;margin:0;"
                        >
                            No rect
                        </button>
                    `;
                    window.__noRectClicks = 0;
                    window.__noRectAuxClicks = 0;
                    window.__noRectContextMenus = 0;
                    const button = document.getElementById('no-rect-click');
                    button.addEventListener('click', () => {
                        window.__noRectClicks += 1;
                    });
                    button.addEventListener('auxclick', event => {
                        if (event.button === 1) {
                            window.__noRectAuxClicks += 1;
                        }
                    });
                    button.addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__noRectContextMenus += 1;
                    });
                    return true;
                })()"#,
            )?;

            let no_rect = page.wait_for("css:#no-rect-click", 1_000)?;
            let web_no_rect = WebElement::Browser(page.wait_for("css:#no-rect-click", 1_000)?);

            assert!(!no_rect.has_rect()?);
            assert!(!web_no_rect.has_rect()?);

            Settings::set_raise_when_click_failed(false);

            no_rect.click_right()?;
            no_rect.click_middle()?;
            no_rect.click_multi(2)?;
            no_rect.click_at(None, None, "left", 1)?;
            no_rect.clicker().right()?;
            assert!(no_rect.clicker().middle(false)?.is_none());
            no_rect.clicker().multi(2)?;
            no_rect.clicker().at(None, None, "left", 1)?;

            web_no_rect.click_right()?;
            web_no_rect.click_middle()?;
            web_no_rect.click_multi(2)?;
            web_no_rect.click_at(None, None, "left", 1)?;
            web_no_rect.clicker().right()?;
            assert!(web_no_rect.clicker().middle(false)?.is_none());
            web_no_rect.clicker().multi(2)?;
            web_no_rect.clicker().at(None, None, "left", 1)?;

            assert!(
                page.click_middle("css:#no-rect-click", Some(100), false)?
                    .is_none()
            );
            assert_eq!(
                page.run_js(
                    "[window.__noRectClicks, window.__noRectAuxClicks, window.__noRectContextMenus]"
                )?,
                Value::Array(vec![Value::from(0), Value::from(0), Value::from(0)])
            );

            let assert_visible_rect_error = |label: &str, err: OpenPageError| {
                assert!(
                    matches!(err, OpenPageError::PageOperation(ref message) if message.contains("visible rect")),
                    "unexpected {label} failure error: {err}"
                );
            };

            Settings::set_raise_when_click_failed(true);

            assert_visible_rect_error(
                "element.click_right()",
                no_rect
                    .click_right()
                    .expect_err("element.click_right() should raise"),
            );
            assert_visible_rect_error(
                "element.click_middle()",
                no_rect
                    .click_middle()
                    .expect_err("element.click_middle() should raise"),
            );
            assert_visible_rect_error(
                "element.click_multi()",
                no_rect
                    .click_multi(2)
                    .expect_err("element.click_multi() should raise"),
            );
            assert_visible_rect_error(
                "element.click_at()",
                no_rect
                    .click_at(None, None, "left", 1)
                    .expect_err("element.click_at() should raise"),
            );
            assert_visible_rect_error(
                "element.clicker().right()",
                no_rect
                    .clicker()
                    .right()
                    .expect_err("element.clicker().right() should raise"),
            );
            assert_visible_rect_error(
                "element.clicker().middle(false)",
                no_rect
                    .clicker()
                    .middle(false)
                    .expect_err("element.clicker().middle(false) should raise"),
            );
            assert_visible_rect_error(
                "element.clicker().multi()",
                no_rect
                    .clicker()
                    .multi(2)
                    .expect_err("element.clicker().multi() should raise"),
            );
            assert_visible_rect_error(
                "element.clicker().at()",
                no_rect
                    .clicker()
                    .at(None, None, "left", 1)
                    .expect_err("element.clicker().at() should raise"),
            );

            assert_visible_rect_error(
                "web_element.click_right()",
                web_no_rect
                    .click_right()
                    .expect_err("web_element.click_right() should raise"),
            );
            assert_visible_rect_error(
                "web_element.click_middle()",
                web_no_rect
                    .click_middle()
                    .expect_err("web_element.click_middle() should raise"),
            );
            assert_visible_rect_error(
                "web_element.click_multi()",
                web_no_rect
                    .click_multi(2)
                    .expect_err("web_element.click_multi() should raise"),
            );
            assert_visible_rect_error(
                "web_element.click_at()",
                web_no_rect
                    .click_at(None, None, "left", 1)
                    .expect_err("web_element.click_at() should raise"),
            );
            assert_visible_rect_error(
                "web_element.clicker().right()",
                web_no_rect
                    .clicker()
                    .right()
                    .expect_err("web_element.clicker().right() should raise"),
            );
            assert_visible_rect_error(
                "web_element.clicker().middle(false)",
                web_no_rect
                    .clicker()
                    .middle(false)
                    .expect_err("web_element.clicker().middle(false) should raise"),
            );
            assert_visible_rect_error(
                "web_element.clicker().multi()",
                web_no_rect
                    .clicker()
                    .multi(2)
                    .expect_err("web_element.clicker().multi() should raise"),
            );
            assert_visible_rect_error(
                "web_element.clicker().at()",
                web_no_rect
                    .clicker()
                    .at(None, None, "left", 1)
                    .expect_err("web_element.clicker().at() should raise"),
            );
            assert_visible_rect_error(
                "page.click_middle()",
                page.click_middle("css:#no-rect-click", Some(100), false)
                    .expect_err("page.click_middle() should raise"),
            );

            Settings::set_language("cn");
            let localized_error = no_rect
                .click_right()
                .expect_err("element.click_right() should raise localized message");
            assert!(
                matches!(localized_error, OpenPageError::PageOperation(ref message) if message.contains("可见位置及大小")),
                "unexpected localized click failure error: {localized_error}"
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("non-left click failure setting runtime regression");
    }

    #[test]
    fn element_and_webelement_clicker_tabs_work_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("element-clicker-tabs").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    const newTabUrl = 'about:blank#clicker-new-tab';
                    const middleUrl = 'about:blank#clicker-middle-tab';
                    document.body.innerHTML = `
                        <a id="open-tab" href="${newTabUrl}" target="_blank">Open tab</a>
                        <a id="middle-open-tab" href="${middleUrl}">Open by middle click</a>
                    `;
                    return true;
                })()"#,
            )?;

            let new_page = page
                .wait_for("css:#open-tab", 1_000)?
                .clicker()
                .for_new_tab(Some(5_000), false)
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("clicker.for_new_tab(): {err}"))
                })?
                .expect("clicker new tab");
            assert!(new_page.wait_for_doc_loaded(5_000).map_err(|err| {
                OpenPageError::PageOperation(format!("new_tab.wait_for_doc_loaded(): {err}"))
            })?);
            assert_eq!(new_page.url()?, "about:blank#clicker-new-tab".to_string());

            let middle_page = WebElement::Browser(page.wait_for("css:#middle-open-tab", 1_000)?)
                .clicker()
                .middle(true)
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("clicker.middle(true): {err}"))
                })?
                .expect("clicker middle tab");
            let BrowserTabReference::Page(middle_page) = middle_page else {
                panic!("browser-backed WebElement should return a Page tab reference");
            };
            assert!(middle_page.wait_for_doc_loaded(5_000).map_err(|err| {
                OpenPageError::PageOperation(format!("middle_tab.wait_for_doc_loaded(): {err}"))
            })?);
            assert_eq!(
                middle_page.url()?,
                "about:blank#clicker-middle-tab".to_string()
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("element clicker tab runtime regression");
    }

    #[test]
    fn page_and_element_tab_helpers_raise_when_no_new_tab_is_opened() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_language("cn");

        let (browser, temp_dir) =
            launch_headless_test_browser("page-no-new-tab-error").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            let latest_tab =
                browser.new_tab(Some("about:blank#existing-latest"), false, false, false)?;
            assert!(latest_tab.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="stay-put" type="button">Stay put</button>
                    `;
                    return true;
                })()"#,
            )?;

            let assert_no_new_tab_error = |label: &str, err: OpenPageError| {
                assert!(
                    matches!(err, OpenPageError::PageOperation(ref message) if message == "没有等到新标签页"),
                    "unexpected {label} error: {err}"
                );
            };

            assert_no_new_tab_error(
                "page.click_for_new_tab()",
                page.click_for_new_tab("css:#stay-put", Some(100), false)
                    .expect_err("page.click_for_new_tab() should raise"),
            );
            assert_no_new_tab_error(
                "page.click_middle(get_tab=true)",
                page.click_middle("css:#stay-put", Some(100), true)
                    .expect_err("page.click_middle(get_tab=true) should raise"),
            );

            let element = page.wait_for("css:#stay-put", 1_000)?;
            assert_no_new_tab_error(
                "element.clicker().for_new_tab()",
                element
                    .clicker()
                    .for_new_tab(Some(100), false)
                    .expect_err("element.clicker().for_new_tab() should raise"),
            );
            assert_no_new_tab_error(
                "element.clicker().middle(true)",
                element
                    .clicker()
                    .middle(true)
                    .expect_err("element.clicker().middle(true) should raise"),
            );

            let web_element = WebElement::Browser(page.wait_for("css:#stay-put", 1_000)?);
            assert_no_new_tab_error(
                "web_element.clicker().for_new_tab()",
                web_element
                    .clicker()
                    .for_new_tab(Some(100), false)
                    .expect_err("web_element.clicker().for_new_tab() should raise"),
            );
            assert_no_new_tab_error(
                "web_element.clicker().middle(true)",
                web_element
                    .clicker()
                    .middle(true)
                    .expect_err("web_element.clicker().middle(true) should raise"),
            );

            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page and element no-new-tab runtime regression");
    }

    #[test]
    fn element_and_webelement_clicker_upload_and_download_work_at_runtime() {
        let (browser, temp_dir) = launch_headless_test_browser("element-clicker-transfer")
            .expect("launch headless browser");
        let (download_url, download_server) = spawn_download_site();

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="picker" type="file" multiple
                            onchange='document.getElementById("out").textContent = Array.from(this.files).map(f => f.name).join(",")' />
                        <div id="out"></div>
                    `;
                    return true;
                })()"#,
            )?;

            let first = temp_dir.join("first.txt");
            let second = temp_dir.join("second.txt");
            fs::write(&first, "first")?;
            fs::write(&second, "second")?;
            let files = vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ];

            page.wait_for("css:#picker", 1_000)?
                .clicker()
                .to_upload(&files, Some(5_000), false)?;
            assert_eq!(
                page.run_js("document.getElementById('picker').files.length")?,
                Value::from(2)
            );
            assert_eq!(
                page.run_js("document.getElementById('out').textContent")?,
                Value::from("first.txt,second.txt")
            );

            page.goto(&download_url)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.set_download_path(temp_dir.to_string_lossy().as_ref())?;
            let mission = WebElement::Browser(page.wait_for("css:#download", 1_000)?)
                .clicker()
                .to_download(None, None, None, false, Some(5_000), false, false)?
                .expect("clicker download mission");
            assert_eq!(mission.suggested_filename()?, "openpage.txt".to_string());
            let final_path = mission
                .wait(false, Some(10_000), false)?
                .expect("download final path");
            assert!(PathBuf::from(&final_path).exists());
            assert!(final_path.ends_with("openpage.txt"));
            Ok(())
        })();

        let close_result = browser.close();
        let server_result = download_server.join();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        if let Err(err) = server_result {
            panic!("join download server: {err:?}");
        }
        result.expect("element clicker upload/download runtime regression");
    }

    #[test]
    fn page_zoom_css_fallback_roundtrips_at_runtime() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-zoom-css").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            page.run_js(
                "(() => { document.documentElement.style.zoom = '0.9'; return getComputedStyle(document.documentElement).zoom; })()",
            )?;

            page.set_zoom_factor(1.25)?;
            let zoom = page.zoom_factor()?;
            assert!(
                (zoom - 1.25).abs() < 0.01,
                "expected managed zoom near 1.25, got {zoom}"
            );
            assert_eq!(
                page.run_js("document.documentElement.getAttribute('data-openpage-zoom-managed')")?,
                Value::from("1")
            );
            assert_eq!(
                page.run_js("getComputedStyle(document.documentElement).zoom")?,
                Value::from("1.25")
            );

            page.reset_zoom_factor()?;
            assert_eq!(page.zoom_factor()?, 1.0);
            assert_eq!(
                page.run_js(
                    "(() => document.documentElement.hasAttribute('data-openpage-zoom-managed'))()",
                )?,
                Value::from(false)
            );
            assert_eq!(
                page.run_js("getComputedStyle(document.documentElement).zoom")?,
                Value::from("0.9")
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page zoom css fallback runtime regression");
    }

    #[test]
    fn page_clipboard_roundtrips_with_permission_override() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-clipboard").expect("launch headless browser");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind clipboard server");
        listener
            .set_nonblocking(true)
            .expect("set clipboard server nonblocking");
        let address = format!(
            "http://{}",
            listener.local_addr().expect("clipboard server addr")
        );
        let server = thread::spawn(move || {
            let html = r#"<!doctype html>
<html>
<body>
  <main id="app">clipboard test</main>
</body>
</html>
"#;
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut served = false;
            while Instant::now() < deadline && !served {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut buffer = [0_u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match path {
                    "/" => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                            html.len()
                        );
                        served = true;
                    }
                    _ => {
                        let body = "not found";
                        let _ = write!(
                            stream,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(Some(address.as_str()))?;
            assert!(page.wait_for_doc_loaded(5_000)?);
            assert_eq!(
                page.run_js(
                    "(() => ({ secure: window.isSecureContext, hasClipboard: !!navigator.clipboard }))()",
                )?,
                json!({"secure": true, "hasClipboard": true})
            );

            page.set_permission("clipboard-read", "granted", None, None)?;
            page.set_permission("clipboard-write", "granted", None, None)?;
            page.clipboard_write_text("openpage clipboard runtime")?;
            assert_eq!(
                page.clipboard_read_text()?,
                "openpage clipboard runtime".to_string()
            );
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);
        let server_result = server.join();

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        if let Err(err) = server_result {
            panic!("join clipboard server: {err:?}");
        }
        result.expect("page clipboard permission runtime regression");
    }

    #[test]
    fn page_window_id_distinguishes_same_and_new_window_tabs() {
        let (browser, temp_dir) =
            launch_headless_test_browser("page-window-id").expect("launch headless browser");

        let result = (|| -> crate::OpenPageResult<()> {
            let page = browser.new_page(None)?;
            let same_window_tab = browser.new_tab(None, false, false, false)?;
            let new_window_tab = browser.new_tab(None, true, false, false)?;

            assert_eq!(page.window_id()?, same_window_tab.window_id()?);
            assert_ne!(page.window_id()?, new_window_tab.window_id()?);
            Ok(())
        })();

        let close_result = browser.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless browser: {err}");
        }
        result.expect("page window id runtime regression");
    }
}
