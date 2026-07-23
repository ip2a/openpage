mod actions;
mod cookies;
mod dialogs;
mod frame;
mod interaction;
mod lifecycle;
mod navigation;
mod operations;
mod screenshot;
mod settings;
mod tabs;

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
use crate::recorder::Recorder;
use crate::screencast::Screencast;
use crate::session::{
    CookieEntry, CookieInput, DocumentElement, HeadersInput, Session, SessionCookieParam,
    SessionOptions, SessionXPathResult, cookie_input_to_params_allow_missing_scope,
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

pub(crate) type FrameCacheHandle = Arc<std::sync::Mutex<HashMap<String, Frame>>>;
pub(crate) type FrameNoneElementConfigCacheHandle =
    Arc<std::sync::Mutex<HashMap<String, ElementsOneRuntimeConfigHandle>>>;

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
    recorder: Recorder,
    alerts: AlertTracker,
    uploader: UploadTracker,
    load_mode: Arc<std::sync::Mutex<LoadMode>>,
    init_scripts: Arc<std::sync::Mutex<Vec<String>>>,
    browser_pid: Option<u32>,
    none_element_config: ElementsOneRuntimeConfigHandle,
    frame_cache: FrameCacheHandle,
    frame_none_element_configs: FrameNoneElementConfigCacheHandle,
}

#[derive(Clone)]
pub struct Frame {
    page: Page,
    frame_id: String,
    frame_element: Arc<Element>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("frame_id", &self.frame_id)
            .field(
                "frame_element_backend_node_id",
                &self.frame_element.backend_node_id(),
            )
            .finish()
    }
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
    frame_id: String,
    frame_dom_id: Option<String>,
    frame_dom_name: Option<String>,
    frame_xpath: Option<String>,
    frame_css_path: Option<String>,
    frame_backend_node_id: BackendNodeId,
    none_element_config: ElementsOneRuntimeConfigHandle,
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
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Some(name) = self.frame_dom_name.as_deref()
            && !name.is_empty()
        {
            let locator = format!(r#"css:iframe[name="{name}"],frame[name="{name}"]"#);
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Some(xpath) = self.frame_xpath.as_deref()
            && !xpath.is_empty()
        {
            let locator = format!("xpath:{xpath}");
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Some(css_path) = self.frame_css_path.as_deref()
            && !css_path.is_empty()
        {
            let locator = format!("css:{css_path}");
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }

        if let Ok(frame_element) = page.frame_owner_element_by_id(&self.frame_id) {
            return page
                .frame_from_element_with_config_source(frame_element, &self.none_element_config);
        }

        let frame_element = page.resolve_dom_backend_node_id(self.frame_backend_node_id)?;
        page.frame_from_element_with_config_source(frame_element, &self.none_element_config)
    }
}

impl NavigationTracker {
    fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        let (snapshot, initial_error) = match initial_navigation_snapshot(runtime.as_ref(), &page) {
            Ok(snapshot) => (snapshot, None),
            Err(err) => (PageNavigationSnapshot::default(), Some(err.to_string())),
        };
        let shared = Arc::new(NavigationShared::new(snapshot));
        let tracker = Self {
            shared: Arc::clone(&shared),
        };

        if let Err(err) = execute_page_command_blocking(
            runtime.as_ref(),
            &page,
            SetLifecycleEventsEnabledParams::new(true),
            "Page::set_lifecycle_events_enabled()",
        ) {
            set_navigation_last_error(&shared, err.to_string());
        } else if let Some(error) = initial_error {
            set_navigation_last_error(&shared, error);
        }

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
        let state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page navigation state",
                "页面导航状态",
            ))
        })?;
        if let Some(error) = state.last_error.as_ref() {
            return Err(OpenPageError::PageOperation(error.clone()));
        }
        Ok(state.snapshot.clone())
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
    DocumentElement(&'a DocumentElement),
    WebElement(&'a WebElement),
    OwnedElement(Element),
    OwnedDocumentElement(DocumentElement),
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

pub trait FrameIndexInput {
    fn into_frame_index(self) -> isize;
}

impl FrameIndexInput for usize {
    fn into_frame_index(self) -> isize {
        self as isize
    }
}

impl FrameIndexInput for isize {
    fn into_frame_index(self) -> isize {
        self
    }
}

impl FrameIndexInput for i32 {
    fn into_frame_index(self) -> isize {
        self as isize
    }
}

impl FrameIndexInput for i64 {
    fn into_frame_index(self) -> isize {
        self as isize
    }
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

impl<'a> From<&'a DocumentElement> for PageElementTarget<'a> {
    fn from(value: &'a DocumentElement) -> Self {
        Self::DocumentElement(value)
    }
}

impl From<DocumentElement> for PageElementTarget<'_> {
    fn from(value: DocumentElement) -> Self {
        Self::OwnedDocumentElement(value)
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
        let recorder = Recorder::new(Arc::clone(&runtime), inner.clone());
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
            recorder,
            alerts,
            uploader,
            load_mode: Arc::new(std::sync::Mutex::new(load_mode)),
            init_scripts: Arc::new(std::sync::Mutex::new(Vec::new())),
            browser_pid: None,
            none_element_config: Arc::new(std::sync::Mutex::new(
                default_none_element_runtime_config(),
            )),
            frame_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
            frame_none_element_configs: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn with_browser_pid(mut self, browser_pid: Option<u32>) -> Self {
        self.browser_pid = browser_pid;
        self
    }

    pub(crate) fn with_browser(mut self, browser: Browser) -> Self {
        self.recorder = self.recorder.clone().with_browser(browser.clone());
        self.browser = Some(browser);
        self
    }

    pub(crate) fn with_frame_caches(
        mut self,
        frame_cache: FrameCacheHandle,
        frame_none_element_configs: FrameNoneElementConfigCacheHandle,
    ) -> Self {
        self.frame_cache = frame_cache;
        self.frame_none_element_configs = frame_none_element_configs;
        self
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
        PageElementTarget::DocumentElement(_) => Err(OpenPageError::UnsupportedOperation(
            session_backed_element_driver_target_message(
                "DocumentElement",
                "page element",
                "页面元素定位",
            ),
        )),
        PageElementTarget::OwnedDocumentElement(_) => Err(OpenPageError::UnsupportedOperation(
            session_backed_element_driver_target_message(
                "DocumentElement",
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
        LocatorInput::Raw(raw) => Ok(frame_locator(raw.as_ref())),
        LocatorInput::By(by, value) => Ok(Locator::from_by(by.as_ref(), value.as_ref())?
            .raw()
            .to_string()),
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

fn frame_execution_context_was_stale(message: &str) -> bool {
    message.contains("Cannot find context with specified id")
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
mod tests;
