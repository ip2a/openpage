use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::browser::{
    Bounds, GetWindowForTargetParams, GetWindowForTargetReturns, SetWindowBoundsParams, WindowState,
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
    BlockPattern, ClearBrowserCacheParams, ClearBrowserCookiesParams, CookieParam,
    DeleteCookiesParams, EnableParams as NetworkEnableParams, Headers, SetBlockedUrLsParams,
    SetExtraHttpHeadersParams,
};
use chromiumoxide::cdp::browser_protocol::page::GetFrameTreeParams;
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureSnapshotFormat,
    CaptureSnapshotParams, FrameId, FrameTree, GetNavigationHistoryParams,
    NavigateToHistoryEntryParams, PrintToPdfParams, ReloadParams,
    RemoveScriptToEvaluateOnNewDocumentParams, StopLoadingParams,
    Viewport as ClipViewport,
};
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, ExecutionContextId};
use chromiumoxide::keys;
use chromiumoxide::layout::Point;
use chromiumoxide::page::{Page as OxPage, ScreenshotParams};
use chromiumoxide::{Command, Method};
use serde::Serialize;
use serde_json::Value;
use tokio::runtime::Runtime;
use url::Url;

use crate::alert::AlertTracker;
use crate::browser::{Browser, DownloadFileExistsMode, LoadMode};
use crate::console::Console;
use crate::download::DownloadMission;
use crate::element::Element;
use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfig, ElementsOneRuntimeConfigHandle,
    elements_one_should_raise_when_missing,
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
    CookieEntry, SessionElement, SessionOptions, SessionPage, SessionXPathResult,
    cookies_from_header, snapshot_find, snapshot_find_all, snapshot_query_xpath, snapshot_root,
};
use crate::shadow_root::ShadowRoot;
use crate::upload::UploadTracker;
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

#[derive(Clone, Debug)]
pub struct Page {
    runtime: Arc<Runtime>,
    inner: OxPage,
    browser: Option<Browser>,
    interceptor: Interceptor,
    console: Console,
    screencast: Screencast,
    alerts: AlertTracker,
    uploader: UploadTracker,
    load_mode: Arc<std::sync::Mutex<LoadMode>>,
    init_scripts: Arc<std::sync::Mutex<Vec<String>>>,
    browser_pid: Option<u32>,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

pub struct Frame {
    page: Page,
    frame_id: String,
    frame_element: Element,
}

pub struct FrameScroller<'a> {
    frame: &'a Frame,
}

pub struct FrameSetter<'a> {
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
    WebElement(&'a WebElement),
}

pub enum ActionsTarget<'a> {
    Locator(LocatorInput<'a>),
    Element(&'a Element),
    WebElement(&'a WebElement),
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

pub enum PageFrameTarget<'a> {
    Locator(LocatorInput<'a>),
    Element(&'a Element),
    WebElement(&'a WebElement),
    Frame(&'a Frame),
    WebFrame(&'a WebFrame),
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

impl<'a> From<&'a WebElement> for PageElementTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
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

impl<'a> From<&'a WebElement> for ActionsTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
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

impl<'a> From<&'a WebFrame> for PageFrameTarget<'a> {
    fn from(value: &'a WebFrame) -> Self {
        Self::WebFrame(value)
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
    pub(crate) fn new(page: Page, frame_id: String, frame_element: Element) -> Self {
        Self {
            page,
            frame_id,
            frame_element,
        }
    }

    pub fn id(&self) -> &str {
        &self.frame_id
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

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        self.frame_element.style(name, pseudo)
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
        let outer_html = self.frame_element.html()?.ok_or_else(|| {
            OpenPageError::ElementNotFound("frame html is unavailable".to_string())
        })?;
        let inner_html = self.inner_html()?;
        Ok(compose_frame_html(&tag, &outer_html, &inner_html))
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        self.page.evaluate_in_frame(&self.frame_id, expression)
    }

    pub fn refresh(&self) -> OpenPageResult<()> {
        self.run_js("this.location.reload();").map(|_| ())
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

    pub fn set_upload_files(&self, files: &[String]) -> OpenPageResult<()> {
        self.page.set_upload_files(files)
    }

    pub fn set_upload_paths(&self, files: &[String]) -> OpenPageResult<()> {
        self.set_upload_files(files)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.page.set_tab_download_path(path)
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

    pub fn click_to_download(
        &self,
        locator: &str,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
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

    pub fn click_to_upload(
        &self,
        locator: &str,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
        self.page.click_to_upload(locator, files, timeout_ms, by_js)
    }

    pub fn click_for_new_tab(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>> {
        self.page.click_for_new_tab(locator, timeout_ms, by_js)
    }

    pub fn click_middle(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<Page>> {
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
            other => Err(OpenPageError::JavaScript(format!(
                "frame active element returned unexpected value: {other}"
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
                Some(Arc::clone(&self.page.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.page.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.page.none_element_config,
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
            Value::Null => Err(OpenPageError::ElementNotFound(format!(
                "frame element not found: {}",
                locator.raw()
            ))),
            Value::String(_) => {
                let element = self.page.find(&marker_xpath(&marker))?;
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                Ok(element)
            }
            other => Err(OpenPageError::JavaScript(format!(
                "frame find() returned unexpected value: {other}"
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

    pub fn parent(&self) -> OpenPageResult<Element> {
        self.frame_element.parent()
    }

    pub fn prev(&self) -> OpenPageResult<Element> {
        self.frame_element.prev()
    }

    pub fn next(&self) -> OpenPageResult<Element> {
        self.frame_element.next()
    }

    pub fn before(&self) -> OpenPageResult<Element> {
        self.frame_element.before()
    }

    pub fn after(&self) -> OpenPageResult<Element> {
        self.frame_element.after()
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.prevs()
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.nexts()
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.befores()
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.afters()
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

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_js("(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, 0), true)")
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
        self.run_js("(document.scrollingElement.scrollTo(0, document.scrollingElement.scrollTop), true)")
            .map(|_| ())
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_js(&format!("(document.scrollingElement.scrollTo({x}, {y}), true)"))
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
        self.run_js(&format!("(document.scrollingElement.scrollBy(0, {pixels}), true)"))
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
        self.run_js(&format!("(document.scrollingElement.scrollBy({pixels}, 0), true)"))
            .map(|_| ())
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        value_as_f64_pair(
            self.run_js("[document.documentElement.scrollLeft, document.documentElement.scrollTop]")?,
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
        value_as_optional_string(
            self.run_js("document.readyState")?,
            "frame ready state",
        )
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

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        snapshot_find(&self.inner_html()?, locator)
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        snapshot_find_all(&self.inner_html()?, locator)
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

impl FrameSetter<'_> {
    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_attr(name, value)
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame.set_property(name, value)
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_style(name, value)
    }

    pub fn upload_files(&self, files: &[String]) -> OpenPageResult<()> {
        self.frame.set_upload_files(files)
    }

    pub fn upload_paths(&self, files: &[String]) -> OpenPageResult<()> {
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
            .ok_or_else(|| OpenPageError::PageOperation(format!("unsupported key: {key}")))?;
        let next_modifiers = self.modifiers | action_modifier_bit(definition.key).unwrap_or(0);
        self.dispatch_key_event(action_build_key_event(&definition, next_modifiers, false))?;
        self.modifiers = next_modifiers;
        Ok(self)
    }

    pub fn key_up(&mut self, key: &str) -> OpenPageResult<&mut Self> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(format!("unsupported key: {key}")))?;
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
                "wait() seconds must be >= 0".to_string(),
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
                    "type_with_interval() seconds must be >= 0".to_string(),
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
                "click() times must be >= 1".to_string(),
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
        self.page.runtime.block_on(async {
            self.page
                .inner
                .execute(event)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    fn dispatch_key_event(&self, event: DispatchKeyEventParams) -> OpenPageResult<()> {
        self.page.runtime.block_on(async {
            self.page
                .inner
                .execute(event)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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
        self.page.runtime.block_on(async {
            self.page
                .inner
                .execute(event)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    fn insert_text_value(&self, value: &str) -> OpenPageResult<()> {
        self.page.runtime.block_on(async {
            self.page
                .inner
                .execute(InsertTextParams::new(value.to_string()))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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
            .ok_or_else(|| OpenPageError::PageOperation(format!("unsupported key: {value}")))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, false))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, true))
    }
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
        let interceptor = Interceptor::new(Arc::clone(&runtime), inner.clone());
        let console = Console::new(Arc::clone(&runtime), inner.clone());
        let screencast = Screencast::new(Arc::clone(&runtime), inner.clone());
        let alerts = AlertTracker::new(Arc::clone(&runtime), inner.clone());
        let uploader = UploadTracker::new(Arc::clone(&runtime), inner.clone());
        Self {
            runtime,
            inner,
            browser: None,
            interceptor,
            console,
            screencast,
            alerts,
            uploader,
            load_mode: Arc::new(std::sync::Mutex::new(load_mode)),
            init_scripts: Arc::new(std::sync::Mutex::new(Vec::new())),
            browser_pid: None,
            none_element_config: Arc::new(std::sync::Mutex::new(
                ElementsOneRuntimeConfig::default(),
            )),
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

    pub fn browser(&self) -> Option<&Browser> {
        self.browser.as_ref()
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(
                    "none element runtime config lock poisoned".to_string(),
                )
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(
                    "none element runtime config lock poisoned".to_string(),
                )
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
        let url = url.to_string();
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
            .unwrap_or_else(|| OpenPageError::Timeout(format!("page connect timed out: {url}"))))
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
                    Err(OpenPageError::Timeout(format!(
                        "page connect timed out: {url}"
                    )))
                }
            }
            LoadMode::Eager if supports_script_navigation => {
                self.navigate_via_script(&url)?;
                if self.wait_for_ready_state_change(page_load_timeout_ms, true)? {
                    let _ = self.stop_loading();
                    if self.wait_for_dom_ready(remaining_timeout_ms(deadline))? {
                        Ok(())
                    } else {
                        Err(OpenPageError::Timeout(format!(
                            "page connect timed out: {url}"
                        )))
                    }
                } else {
                    Err(OpenPageError::Timeout(format!(
                        "page connect timed out: {url}"
                    )))
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
                    Err(OpenPageError::Timeout(format!(
                        "page connect timed out: {url}"
                    )))
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
            Ok(self
                .inner
                .url()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?
                .unwrap_or_default())
        })
    }

    pub fn title(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            Ok(self
                .inner
                .get_title()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?
                .unwrap_or_default())
        })
    }

    pub fn target_id(&self) -> String {
        self.inner.target_id().as_ref().to_string()
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match &self.browser {
            Some(browser) => browser.page_download_path(&self.target_id()),
            None => Ok(None),
        }
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(
                "download_file_exists_mode() is only available on browser-backed pages".to_string(),
            )
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

    pub fn html(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            self.inner
                .content()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
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
            "javascript execution timed out",
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
                LocatorKind::Css => self
                    .inner
                    .find_element(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?,
                LocatorKind::XPath => self
                    .inner
                    .find_xpath(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?,
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
                LocatorKind::Css => self
                    .inner
                    .find_elements(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?,
                LocatorKind::XPath => self
                    .inner
                    .find_xpaths(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?,
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
                        return Err(OpenPageError::Timeout(format!("{locator} ({err})")));
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
                return Ok(false);
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
                        return Ok(false);
                    }
                }
            }
        };
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        wait_fn(&element, remaining.max(1))
    }

    pub fn click(&self, locator: &str) -> OpenPageResult<()> {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .click()
    }

    pub fn fill(&self, locator: &str, text: &str) -> OpenPageResult<()> {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .input(text)
    }

    pub fn text(&self, locator: &str) -> OpenPageResult<Option<String>> {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .text()
    }

    pub fn attr(&self, locator: &str, name: &str) -> OpenPageResult<Option<String>> {
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

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        resolve_page_frame_target(self, target.into())
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<Element> {
        let frames = self.get_frames(None::<&str>)?;
        frames
            .into_iter()
            .nth(index.saturating_sub(1))
            .ok_or_else(|| {
                OpenPageError::ElementNotFound(format!("frame index out of range: {index}"))
            })
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        self.find_all(locator.as_str())
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.frame_from_element(resolve_page_frame_target(self, target.into())?)
    }

    pub fn get_frame_context_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.frame_from_element(self.get_frame_by_index(index)?)
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frames(locator)?
            .into_iter()
            .map(|element| self.frame_from_element(element))
            .collect()
    }

    pub fn set_blocked_urls(&self, patterns: &[String]) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .execute(NetworkEnableParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            let params = SetBlockedUrLsParams::builder()
                .url_patterns(
                    patterns
                        .iter()
                        .cloned()
                        .map(|pattern| BlockPattern::new(pattern, true)),
                )
                .build();
            self.inner
                .execute(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        let params = ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .full_page(full_page)
            .build();

        self.runtime.block_on(async {
            self.inner
                .save_screenshot(params, path)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
            self.inner
                .screenshot(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
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
                self.inner
                    .pdf(pdf_options.unwrap_or_default())
                    .await
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))
            })?;
            PageSaveContent::Pdf(pdf)
        } else {
            let mhtml = self.runtime.block_on(async {
                self.inner
                    .execute(
                        CaptureSnapshotParams::builder()
                            .format(CaptureSnapshotFormat::Mhtml)
                            .build(),
                    )
                    .await
                    .map(|result| result.data.clone())
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))
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
            self.inner
                .save_pdf(PrintToPdfParams::default(), path)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        let params = ReloadParams::builder().ignore_cache(ignore_cache).build();
        self.runtime.block_on(async {
            self.inner
                .execute(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(-(steps as isize))
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(steps as isize)
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        self.evaluate(script)
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_js("(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, 0), true)")
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
        self.run_js("(document.scrollingElement.scrollTo(0, document.scrollingElement.scrollTop), true)")
            .map(|_| ())
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_js(&format!("(document.scrollingElement.scrollTo({x}, {y}), true)"))
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
        self.run_js(&format!("(document.scrollingElement.scrollBy(0, {pixels}), true)"))
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
        self.run_js(&format!("(document.scrollingElement.scrollBy({pixels}, 0), true)"))
            .map(|_| ())
    }

    pub fn execute_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.runtime.block_on(async {
            self.inner
                .execute(command)
                .await
                .map(|response| response.result)
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub fn execute_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?)?;
        self.execute_cdp(command)
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        #[cfg(target_os = "macos")]
        if let Some(browser_pid) = self.browser_pid {
            set_app_visibility(browser_pid, true)?;
        }
        self.runtime.block_on(async {
            self.inner
                .bring_to_front()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
                "window hide() is only available for launched browser instances".to_string(),
            ));
        };
        set_app_visibility(browser_pid, false)
    }

    pub fn window_show(&self) -> OpenPageResult<()> {
        let Some(browser_pid) = self.browser_pid else {
            return Err(OpenPageError::UnsupportedOperation(
                "window show() is only available for launched browser instances".to_string(),
            ));
        };
        set_app_visibility(browser_pid, true)
    }

    pub fn set_upload_files(&self, files: &[String]) -> OpenPageResult<()> {
        self.uploader.set_files(files)
    }

    pub fn set_upload_paths(&self, files: &[String]) -> OpenPageResult<()> {
        self.set_upload_files(files)
    }

    pub fn set_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(
                "set_tab_download_path() is only available on browser-backed pages".to_string(),
            )
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
            OpenPageError::UnsupportedOperation(
                "set_tab_download_file_exists_mode() is only available on browser-backed pages"
                    .to_string(),
            )
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
            OpenPageError::UnsupportedOperation(
                "set_tab_download_filename() is only available on browser-backed pages".to_string(),
            )
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

    pub fn click_to_download(
        &self,
        locator: &str,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(
                "click_to_download() is only available on browser-backed pages".to_string(),
            )
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
            if by_js {
                element.run_js("this.click(); return true;")?;
            } else {
                element.click()?;
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

    pub fn click_to_upload(
        &self,
        locator: &str,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        self.set_upload_files(files)?;
        let element = self.wait_for(locator, timeout_ms)?;
        if by_js {
            element.run_js("this.click(); return true;")?;
        } else {
            element.click()?;
        }
        self.wait_for_upload_paths_inputted(timeout_ms)
    }

    pub fn click_for_new_tab(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(
                "click_for_new_tab() is only available on browser-backed pages".to_string(),
            )
        })?;
        let timeout_ms = timeout_ms.unwrap_or(browser.timeouts()?.implicit_wait);
        let current_tab_id = self.target_id();
        let element = self.wait_for(locator, timeout_ms)?;
        if by_js {
            element.run_js("this.click(); return true;")?;
        } else {
            element.click()?;
        }
        let Some(target_id) = browser.wait_for_new_tab(Some(&current_tab_id), timeout_ms)? else {
            return Ok(None);
        };
        browser.get_page(&target_id).map(Some)
    }

    pub fn click_middle(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<Page>> {
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        let element = self.wait_for(locator, timeout_ms)?;
        let middle_click_link = if element.tag()? == "a" {
            element.link()?.filter(|link| {
                let link = link.trim();
                !link.is_empty() && !link.starts_with("javascript:")
            })
        } else {
            None
        };
        let browser = self.browser.as_ref();
        let current_tab_id = browser.map(|_| self.target_id());
        if get_tab {
            if let (Some(browser), Some(link)) = (browser, middle_click_link.clone()) {
                return browser.new_tab(Some(&link), false, true).map(Some);
            }
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
                    return browser.get_page(&target_id).map(Some);
                }
                return Ok(None);
            }
            if let Some(link) = middle_click_link {
                let page = browser.new_tab(Some(&link), false, true)?;
                if get_tab {
                    return Ok(Some(page));
                }
                return Ok(None);
            }
        }
        if get_tab {
            return Err(OpenPageError::UnsupportedOperation(
                "click_middle(get_tab=True) is only available on browser-backed pages".to_string(),
            ));
        }
        Ok(None)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.uploader.wait_until_inputted(timeout_ms)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self.load_mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page load mode lock poisoned".to_string())
        })? = mode;
        Ok(())
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

    pub fn listener(&self) -> Listener {
        Listener::new(Arc::clone(&self.runtime), self.inner.clone())
    }

    pub fn interceptor(&self) -> Interceptor {
        self.interceptor.clone()
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
            OpenPageError::UnsupportedOperation(
                "wait_for_download_begin() is only available on browser-backed pages".to_string(),
            )
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
            OpenPageError::UnsupportedOperation(
                "wait_for_downloads_done() is only available on browser-backed pages".to_string(),
            )
        })?;
        browser.wait_for_downloads_done_in_frames(
            &self.download_scope_frame_ids()?,
            timeout_ms,
            cancel_if_timeout,
        )
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        snapshot_find(&self.html()?, locator)
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        snapshot_find_all(&self.html()?, locator)
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
            value => Err(OpenPageError::JavaScript(format!(
                "navigator.userAgent did not return a string: {value}"
            ))),
        }
    }

    pub fn set_user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        let mut params = SetUserAgentOverrideParams::new(user_agent.to_string());
        if let Some(platform) = platform {
            params.platform = Some(platform.to_string());
        }
        self.runtime.block_on(async {
            self.inner
                .execute(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn set_headers(&self, headers: &[(String, String)]) -> OpenPageResult<()> {
        let header_map = headers
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect::<serde_json::Map<_, _>>();
        let params =
            SetExtraHttpHeadersParams::new(Headers::new(serde_json::Value::Object(header_map)));
        self.runtime.block_on(async {
            self.inner
                .execute(NetworkEnableParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            self.inner
                .execute(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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
        let identifier = self.runtime.block_on(async {
            let response = self
                .inner
                .execute(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok::<String, OpenPageError>(response.result.identifier.into())
        })?;
        self.init_scripts
            .lock()
            .map_err(|_| {
                OpenPageError::PageOperation("page init scripts lock poisoned".to_string())
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
                    OpenPageError::PageOperation("page init scripts lock poisoned".to_string())
                })?
                .clone(),
        };
        if script_ids.is_empty() {
            return Ok(());
        }
        for script_id in &script_ids {
            let params = RemoveScriptToEvaluateOnNewDocumentParams::new(script_id.clone());
            self.runtime.block_on(async {
                self.inner
                    .execute(params)
                    .await
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
                Ok::<(), OpenPageError>(())
            })?;
        }
        let mut stored = self.init_scripts.lock().map_err(|_| {
            OpenPageError::PageOperation("page init scripts lock poisoned".to_string())
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
        self.runtime.block_on(async {
            if cache {
                self.inner
                    .execute(ClearBrowserCacheParams::default())
                    .await
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            }
            if cookies {
                self.inner
                    .execute(ClearBrowserCookiesParams::default())
                    .await
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            }
            Ok(())
        })
    }

    pub fn ready_state(&self) -> OpenPageResult<String> {
        match self.evaluate("document.readyState")? {
            Value::String(value) => Ok(value),
            value => Err(OpenPageError::JavaScript(format!(
                "document.readyState did not return a string: {value}"
            ))),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(self.ready_state()? != "complete")
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.runtime
            .block_on(async { Ok(self.inner.url().await.is_ok()) })
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
                return Ok(false);
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
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .execute(StopLoadingParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn cookie_header(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            let cookies = self
                .inner
                .get_cookies()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
        let url = Url::parse(url).map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
        let cookies = cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        self.runtime.block_on(async {
            self.inner
                .set_cookies(cookies)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
            self.inner
                .set_cookie(cookie)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
            self.inner
                .delete_cookie(params)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .execute(ClearBrowserCookiesParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn main_frame_id(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            let frame_tree = self
                .inner
                .execute(GetFrameTreeParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(frame_tree.result.frame_tree.frame.id.as_ref().to_string())
        })
    }

    pub(crate) fn download_scope_frame_ids(&self) -> OpenPageResult<Vec<String>> {
        self.runtime.block_on(async {
            let frame_tree = self
                .inner
                .execute(GetFrameTreeParams::default())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            let mut frame_ids = Vec::new();
            collect_frame_ids(&frame_tree.result.frame_tree, &mut frame_ids);
            Ok(frame_ids)
        })
    }

    fn window_info(&self) -> OpenPageResult<GetWindowForTargetReturns> {
        let params = GetWindowForTargetParams::builder()
            .target_id(TargetId::new(self.target_id()))
            .build();
        self.runtime.block_on(async {
            self.inner
                .execute(params)
                .await
                .map(|response| response.result)
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    fn set_window_bounds(&self, bounds: Bounds) -> OpenPageResult<()> {
        let info = self.window_info()?;
        self.runtime.block_on(async {
            self.inner
                .execute(SetWindowBoundsParams::new(info.window_id, bounds))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn close(self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .close()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.load_mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation("page load mode lock poisoned".to_string())
        })
    }

    fn navigate_via_cdp(&self, url: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .goto(url.to_string())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
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
        let history = self.runtime.block_on(async {
            self.inner
                .execute(GetNavigationHistoryParams::default())
                .await
                .map(|response| response.result)
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })?;
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
                OpenPageError::PageOperation(format!(
                    "navigation history index {target_index} out of bounds"
                ))
            })?
            .id;
        self.runtime.block_on(async {
            self.inner
                .execute(NavigateToHistoryEntryParams::new(entry_id))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok::<(), OpenPageError>(())
        })?;
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

    fn frame_from_element(&self, element: Element) -> OpenPageResult<Frame> {
        let backend_node_id = element.backend_node_id();
        let frame_id = self.runtime.block_on(async {
            let response = self
                .inner
                .execute(
                    DescribeNodeParams::builder()
                        .backend_node_id(backend_node_id)
                        .build(),
                )
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            response
                .result
                .node
                .frame_id
                .map(|frame_id| frame_id.as_ref().to_string())
                .ok_or_else(|| {
                    OpenPageError::PageOperation("frame element has no frame id".to_string())
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
        Ok(Frame::new(self.clone(), frame_id, element))
    }

    fn frame_owner_element_by_id(&self, frame_id: &str) -> OpenPageResult<Element> {
        let (node_id, backend_node_id) = self.runtime.block_on(async {
            let response = self
                .inner
                .execute(GetFrameOwnerParams::new(FrameId::new(frame_id.to_string())))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok::<
                (
                    Option<chromiumoxide::cdp::browser_protocol::dom::NodeId>,
                    BackendNodeId,
                ),
                OpenPageError,
            >((response.result.node_id, response.result.backend_node_id))
        })?;
        if let Some(node_id) = node_id {
            self.resolve_dom_node_id(node_id, "frame owner could not be resolved to an element")
        } else {
            self.resolve_dom_backend_node_id(backend_node_id)
        }
    }

    fn resolve_dom_backend_node_id(&self, backend_node_id: BackendNodeId) -> OpenPageResult<Element> {
        let node_id = self.runtime.block_on(async {
            let resolved = self
                .inner
                .execute(ResolveNodeParams::builder().backend_node_id(backend_node_id).build())
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            let object_id = resolved.result.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation("resolved frame owner has no object id".to_string())
            })?;
            let requested = self
                .inner
                .execute(RequestNodeParams::new(object_id))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok::<chromiumoxide::cdp::browser_protocol::dom::NodeId, OpenPageError>(
                requested.result.node_id,
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
        self.runtime.block_on(async {
            self.inner
                .execute(SetAttributeValueParams::new(
                    node_id,
                    PAGE_MARKER_ATTRIBUTE,
                    marker.clone(),
                ))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok::<(), OpenPageError>(())
        })?;

        let element = self.find(marker_selector(&marker).as_str());
        let cleanup = self.runtime.block_on(async {
            let _ = self
                .inner
                .execute(RemoveAttributeParams::new(node_id, PAGE_MARKER_ATTRIBUTE))
                .await;
            Ok::<(), OpenPageError>(())
        });

        match (element, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(_), Ok(())) => Err(OpenPageError::ElementNotFound(error_message.to_string())),
            (Err(err), Err(_)) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn frame_name_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .frame_name(FrameId::new(frame_id.to_string()))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    fn frame_url_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .frame_url(FrameId::new(frame_id.to_string()))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub(crate) fn frame_parent_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .frame_parent(FrameId::new(frame_id.to_string()))
                .await
                .map(|value| value.map(|frame_id| frame_id.as_ref().to_string()))
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    fn frame_context_id(&self, frame_id: &str) -> OpenPageResult<ExecutionContextId> {
        self.runtime.block_on(async {
            self.inner
                .frame_execution_context(FrameId::new(frame_id.to_string()))
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?
                .ok_or_else(|| {
                    OpenPageError::PageOperation(format!(
                        "frame execution context is unavailable: {frame_id}"
                    ))
                })
        })
    }

    fn evaluate_in_frame(&self, frame_id: &str, expression: &str) -> OpenPageResult<Value> {
        let context_id = self.frame_context_id(frame_id)?;
        let timeout_ms = self.javascript_timeout_ms()?;
        let params = EvaluateParams::builder()
            .expression(expression)
            .context_id(context_id)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        self.runtime.block_on(run_with_timeout(
            async {
                let result = self
                    .inner
                    .evaluate(params)
                    .await
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
                result
                    .into_value::<Value>()
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))
            },
            timeout_ms,
            "javascript execution timed out",
        ))
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
        PageElementTarget::WebElement(element) => match element {
            WebElement::Browser(element) => Ok(ResolvedPageElementTarget::Borrowed(element)),
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "session-backed WebElement is not supported for driver page element targeting"
                    .to_string(),
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
        PageFrameTarget::Element(element) => find_frame_element_from_object(page, element),
        PageFrameTarget::WebElement(element) => match element {
            WebElement::Browser(element) => find_frame_element_from_object(page, element),
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "session-backed WebElement is not supported for driver page frame targeting"
                    .to_string(),
            )),
        },
        PageFrameTarget::Frame(frame) => {
            find_frame_element_from_object(page, frame.frame_element())
        }
        PageFrameTarget::WebFrame(frame) => match frame {
            WebFrame::Browser(frame) => find_frame_element_from_object(page, frame.frame_element()),
        },
    }
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
        ActionsTarget::WebElement(element) => match element {
            WebElement::Browser(element) => {
                action_point_from_element(page, element, offset_x, offset_y)
            }
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "session-backed WebElement is not supported for driver actions".to_string(),
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
            OpenPageError::ElementNotFound("element has no clickable rect for actions".to_string())
        })?
    } else {
        let (left, top) = element.rect_location()?.ok_or_else(|| {
            OpenPageError::ElementNotFound("element has no rect location for actions".to_string())
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
                    "drag_in() requires at least one file path".to_string(),
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
                    "drag_in() file path must not be empty".to_string(),
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

fn frame_locator_input<'a, L>(locator: L) -> OpenPageResult<String>
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
                const iterator = document.evaluate({query}, document, null, XPathResult.ORDERED_NODE_ITERATOR_TYPE, null); \
                let index = 0; \
                for (let node = iterator.iterateNext(); node; node = iterator.iterateNext()) {{ \
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
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a string: {other}"
        ))),
    }
}

fn value_as_optional_string(value: Value, name: &str) -> OpenPageResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a string or null: {other}"
        ))),
    }
}

fn value_as_string_vec(value: Value, name: &str) -> OpenPageResult<Vec<String>> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::String(value) => Ok(value),
                other => Err(OpenPageError::JavaScript(format!(
                    "{name} returned a non-string entry: {other}"
                ))),
            })
            .collect(),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return an array: {other}"
        ))),
    }
}

fn value_as_f64_pair(value: Value, name: &str) -> OpenPageResult<(f64, f64)> {
    match value {
        Value::Array(values) if values.len() == 2 => Ok((
            values[0].as_f64().ok_or_else(|| {
                OpenPageError::JavaScript(format!("{name} first entry is not a number"))
            })?,
            values[1].as_f64().ok_or_else(|| {
                OpenPageError::JavaScript(format!("{name} second entry is not a number"))
            })?,
        )),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a number pair: {other}"
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
                return Err(OpenPageError::PageOperation(
                    "screenshot clip requires right_bottom to be greater than left_top".to_string(),
                ));
            }
            Ok(Some(
                ClipViewport::builder()
                    .x(x)
                    .y(y)
                    .width(width)
                    .height(height)
                    .scale(1.0)
                    .build()
                    .map_err(|err| OpenPageError::PageOperation(err.to_string()))?,
            ))
        }
        _ => Err(OpenPageError::PageOperation(
            "screenshot clip requires both left_top and right_bottom".to_string(),
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
    timeout_message: &'static str,
) -> OpenPageResult<T>
where
    F: Future<Output = OpenPageResult<T>>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), future)
        .await
        .map_err(|_| OpenPageError::Timeout(timeout_message.to_string()))?
}

#[cfg(test)]
mod tests {
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use super::{
        PageElementContent, PageElementInfo, PageSaveContent, action_drag_payload,
        compose_frame_html, cookie_param, default_frame_locator, delete_cookie_params,
        frame_locator, frame_locator_input, history_entry_index, is_explicit_locator,
        marker_xpath, optional_frame_locator_input, page_element_info_properties_json,
        remaining_timeout_ms, resolve_implicit_wait_timeout_ms, resolve_page_save_target_path,
        resolve_page_screenshot_target_path, run_with_timeout, screenshot_clip,
        storage_lookup_script,
    };
    use crate::element_list::ElementsListExt;
    use crate::error::OpenPageError;
    use crate::{Browser, By, Keys, LaunchOptions, WebElement};

    fn runtime_test_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("openpage-{name}-{}-{unique}", std::process::id()))
    }

    fn launch_headless_test_browser(name: &str) -> crate::OpenPageResult<(Browser, PathBuf)> {
        let temp_dir = runtime_test_temp_dir(name);
        fs::create_dir_all(&temp_dir).expect("create runtime test temp dir");

        let mut options = LaunchOptions::default();
        options.headless(true);
        options.auto_port(true);
        options.new_env(true);
        options.set_tmp_path(&temp_dir);
        options.set_timeouts(Some(1.0), Some(5.0), Some(1.0));

        Browser::launch(options).map(|browser| (browser, temp_dir))
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

        let (window_left, window_top) = if matches!(window_state.as_str(), "maximized" | "fullscreen")
        {
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

    fn spawn_cross_origin_iframe_site() -> (String, thread::JoinHandle<()>, thread::JoinHandle<()>) {
        let child_listener = TcpListener::bind("127.0.0.1:0").expect("bind child iframe server");
        child_listener
            .set_nonblocking(true)
            .expect("set child iframe server nonblocking");
        let child_address = format!(
            "http://{}",
            child_listener.local_addr().expect("child iframe server addr")
        );

        let parent_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind parent iframe server");
        parent_listener
            .set_nonblocking(true)
            .expect("set parent iframe server nonblocking");
        let parent_address = format!(
            "http://{}",
            parent_listener.local_addr().expect("parent iframe server addr")
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

        (format!("{parent_address}/parent"), parent_handle, child_handle)
    }

    fn spawn_nested_cross_origin_iframe_site(
    ) -> (
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
            child_listener.local_addr().expect("nested child server addr")
        );
        let child_url = format!("{child_address}/child");

        let parent_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind nested parent server");
        parent_listener
            .set_nonblocking(true)
            .expect("set nested parent server nonblocking");
        let parent_address = format!(
            "http://{}",
            parent_listener.local_addr().expect("nested parent server addr")
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
                        <iframe id="demo-frame" name="demo-frame"
                            srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

            let frame_element = page
                .get_frame("css:#demo-frame")
                .map_err(|err| OpenPageError::PageOperation(format!("locator get_frame: {err}")))?;
            let frame = page.get_frame_context("css:#demo-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("locator get_frame_context: {err}"))
            })?;

            assert_eq!(
                page.get_frame(&frame_element)
                    .and_then(|element| element.attr("id"))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame(&Element): {err}"
                    )))?,
                Some("demo-frame".to_string())
            );
            assert_eq!(
                page.get_frame(&frame)
                    .and_then(|element| element.attr("name"))
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "get_frame(&Frame): {err}"
                    )))?,
                Some("demo-frame".to_string())
            );

            let frame_from_element = page
                .get_frame_context(&frame_element)
                .map_err(|err| OpenPageError::PageOperation(format!("from element: {err}")))?;
            let frame_from_frame = page
                .get_frame_context(&frame)
                .map_err(|err| OpenPageError::PageOperation(format!("from frame: {err}")))?;

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
                frame_from_frame.frame_element().attr("id").map_err(|err| {
                    OpenPageError::PageOperation(format!("frame_from_frame attr: {err}"))
                })?,
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
                missing.east(None, None, 1)?.text()?,
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
            let inside_list = shadow_root.find_all(".inside").expect("shadow root css find_all");
            assert_eq!(inside_list.len(), 2);
            assert_eq!(inside_list[0].text()?, Some("Shadow Text".to_string()));
            assert_eq!(inside_list[1].text()?, Some("Shadow Extra".to_string()));
            let inside_xpath_list = shadow_root
                .find_all("xpath:.//*[@class='inside']")
                .expect("shadow root xpath find_all");
            assert_eq!(inside_xpath_list.len(), 2);
            assert_eq!(inside_xpath_list[0].text()?, Some("Shadow Text".to_string()));
            assert_eq!(
                inside_xpath_list[1].text()?,
                Some("Shadow Extra".to_string())
            );
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
            let web_input_select_err = web_input
                .select()
                .selected_options()
                .expect_err("input web select().selected_options() should error");
            assert!(matches!(
                web_input_select_err,
                crate::OpenPageError::UnsupportedOperation(_)
            ));

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

            let outer_frame = page
                .get_frame_context("css:#outer-frame")
                .map_err(|err| OpenPageError::PageOperation(format!("outer frame context: {err}")))?;
            assert!(
                outer_frame
                    .wait_for_doc_loaded(5_000)
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "outer frame wait_for_doc_loaded: {err}"
                    )))?
            );
            let inner_frame_element = outer_frame
                .find("css:#inner-frame")
                .map_err(|err| OpenPageError::PageOperation(format!("outer frame find inner frame: {err}")))?;
            let inner_frame = page
                .get_frame_context(&inner_frame_element)
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "inner frame context from element: {err}"
                )))?;
            assert!(
                inner_frame
                    .wait_for_doc_loaded(5_000)
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "inner frame wait_for_doc_loaded: {err}"
                    )))?
            );
            assert_eq!(
                inner_frame
                    .title()
                    .map_err(|err| OpenPageError::PageOperation(format!("inner frame title: {err}")))?,
                Some("Nested Cross Origin Grandchild".to_string())
            );

            let element = inner_frame
                .find("css:#deep-box")
                .map_err(|err| OpenPageError::PageOperation(format!("inner frame find deep-box: {err}")))?;
            let web_element = WebElement::Browser(
                inner_frame
                    .find("css:#deep-box")
                    .map_err(|err| OpenPageError::PageOperation(format!(
                        "inner frame find deep-box for web element: {err}"
                    )))?,
            );
            let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
                expected_dp_viewport_screen_origin(&page)?;
            let outer_frame_viewport_location = outer_frame
                .frame_element()
                .rect_viewport_location()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "outer frame rect_viewport_location: {err}"
                )))?
                .expect("outer frame viewport location");
            let inner_frame_viewport_location = inner_frame
                .frame_element()
                .rect_viewport_location()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "inner frame rect_viewport_location: {err}"
                )))?
                .expect("inner frame viewport location");

            let viewport_location = element
                .rect_viewport_location()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_viewport_location: {err}"
                )))?
                .expect("nested cross-origin iframe element viewport location");
            let screen_location = element
                .rect_screen_location()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_screen_location: {err}"
                )))?
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
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_viewport_midpoint: {err}"
                )))?
                .expect("nested cross-origin iframe element viewport midpoint");
            let screen_midpoint = element
                .rect_screen_midpoint()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_screen_midpoint: {err}"
                )))?
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
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_viewport_click_point: {err}"
                )))?
                .expect("nested cross-origin iframe element viewport click point");
            let screen_click_point = element
                .rect_screen_click_point()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "deep-box rect_screen_click_point: {err}"
                )))?
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
        parent_server.join().expect("join nested parent iframe server");
        child_server.join().expect("join nested child iframe server");
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
}
