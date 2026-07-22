mod html;
mod parsing;
mod request;
mod response;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::Command;
use serde_json::Value;

use crate::browser::{
    Browser, BrowserTabReference, BrowserTabSelector, BrowserTabTargetsInput, BrowserTabTypeInput,
    DownloadFileExistsMode, LaunchOptions, TabInfo,
};
use crate::console::Console;
use crate::download::DownloadMission;
use crate::element::{Element, ElementClicker, ElementResource, SelectIndexInput};
use crate::element_list::{ElementsOneOwned, ElementsOneRuntimeConfigHandle};
use crate::error::{OpenPageError, OpenPageResult};
use crate::intercept::Interceptor;
use crate::listener::Listener;
use crate::locator::{
    Locator, LocatorBatchInput, LocatorInput, LocatorMatch, parse_locator_batch_input,
};
use crate::page::{
    Actions, ActionsInput, DisconnectedFrame, Frame, FrameIndexInput, FrameRect, FrameScroller,
    FrameSetter, FrameStates, FrameWait, Page, PageElementContent, PageElementInfo,
    PageElementTarget, PageFrameTarget, PageSaveContent,
};
use crate::screencast::Screencast;
use crate::session::{
    CookieEntry, CookieInput, HeadersInput, SessionDownload, SessionElement, SessionEncodingInput,
    SessionOptions, SessionPage, SessionXPathResult,
};
use crate::settings::{
    component_state_lock_poisoned_message, driver_mode_only_message,
    timeout_must_be_non_negative_message, wait_for_locator_timed_out_message, wait_timeout_result,
    web_browser_backed_option_required_message, web_driver_element_required_message,
    web_mode_invalid_message, web_timeout_base_non_negative_message,
};
use crate::shadow_root::ShadowRoot;
use crate::upload::UploadFilesInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebMode {
    Driver,
    Session,
}

impl WebMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Driver => "d",
            Self::Session => "s",
        }
    }

    pub fn parse(mode: &str) -> OpenPageResult<Self> {
        match mode.to_ascii_lowercase().as_str() {
            "d" => Ok(Self::Driver),
            "s" => Ok(Self::Session),
            _ => Err(OpenPageError::BrowserOperation(web_mode_invalid_message(
                mode,
            ))),
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Driver => Self::Session,
            Self::Session => Self::Driver,
        }
    }
}

#[derive(Debug)]
pub enum WebElement {
    Browser(Element),
    Mix {
        element: Element,
        page: Box<WebPage>,
    },
    Session(SessionElement),
}

pub enum WebElementDragTarget<'a> {
    Element(&'a WebElement),
    OwnedElement(WebElement),
    Locator(LocatorInput<'a>),
    Coordinates(f64, f64),
}

impl<'a> From<&'a WebElement> for WebElementDragTarget<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::Element(value)
    }
}

impl From<WebElement> for WebElementDragTarget<'_> {
    fn from(value: WebElement) -> Self {
        Self::OwnedElement(value)
    }
}

impl<'a> From<&'a str> for WebElementDragTarget<'a> {
    fn from(value: &'a str) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<&'a String> for WebElementDragTarget<'a> {
    fn from(value: &'a String) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl<'a> From<(&'a str, &'a str)> for WebElementDragTarget<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::Locator(LocatorInput::from(value))
    }
}

impl From<(i32, i32)> for WebElementDragTarget<'_> {
    fn from(value: (i32, i32)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(u32, u32)> for WebElementDragTarget<'_> {
    fn from(value: (u32, u32)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(usize, usize)> for WebElementDragTarget<'_> {
    fn from(value: (usize, usize)) -> Self {
        Self::Coordinates(value.0 as f64, value.1 as f64)
    }
}

impl From<(f64, f64)> for WebElementDragTarget<'_> {
    fn from(value: (f64, f64)) -> Self {
        Self::Coordinates(value.0, value.1)
    }
}

#[derive(Clone)]
pub enum WebFrame {
    Browser(Frame),
    Mix { frame: Frame, page: Box<WebPage> },
}

#[derive(Clone, Debug)]
pub enum DisconnectedWebFrame {
    Browser(DisconnectedFrame),
    Mix {
        frame: DisconnectedFrame,
        page: Box<WebPage>,
    },
}

pub struct WebElementScroller<'a> {
    element: &'a WebElement,
}

pub struct WebElementClicker<'a> {
    element: &'a WebElement,
}

pub struct WebElementSetter<'a> {
    element: &'a WebElement,
}

pub struct WebElementSelector<'a> {
    element: &'a WebElement,
}

pub struct WebElementStates<'a> {
    element: &'a WebElement,
}

pub struct WebElementRect<'a> {
    element: &'a WebElement,
}

pub struct WebElementWait<'a> {
    element: &'a WebElement,
}

pub struct WebPageScroller<'a> {
    page: &'a WebPage,
}

pub struct WebPageSetter<'a> {
    page: &'a WebPage,
}

pub struct WebPageCookieSetter<'a> {
    page: &'a WebPage,
}

pub struct WebPageWindowSetter<'a> {
    page: &'a WebPage,
}

pub struct WebPageLoadModeSetter<'a> {
    page: &'a WebPage,
}

pub enum WebSelectOptionInput<'a> {
    Single(&'a WebElement),
    OwnedSingle(WebElement),
    Many(Vec<&'a WebElement>),
}

impl<'a> From<&'a WebElement> for WebSelectOptionInput<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::Single(value)
    }
}

impl From<WebElement> for WebSelectOptionInput<'_> {
    fn from(value: WebElement) -> Self {
        Self::OwnedSingle(value)
    }
}

impl<'a> From<&'a [&'a WebElement]> for WebSelectOptionInput<'a> {
    fn from(value: &'a [&'a WebElement]) -> Self {
        Self::Many(value.to_vec())
    }
}

impl<'a> From<&'a Vec<&'a WebElement>> for WebSelectOptionInput<'a> {
    fn from(value: &'a Vec<&'a WebElement>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<Vec<&'a WebElement>> for WebSelectOptionInput<'a> {
    fn from(value: Vec<&'a WebElement>) -> Self {
        Self::Many(value)
    }
}

impl<'a, const N: usize> From<[&'a WebElement; N]> for WebSelectOptionInput<'a> {
    fn from(value: [&'a WebElement; N]) -> Self {
        Self::Many(value.into_iter().collect())
    }
}

impl<'a, const N: usize> From<&'a [&'a WebElement; N]> for WebSelectOptionInput<'a> {
    fn from(value: &'a [&'a WebElement; N]) -> Self {
        Self::from(value.as_slice())
    }
}

#[derive(Clone, Debug)]
pub struct DisconnectedWebPage {
    browser: Browser,
    session: SessionPage,
    mode: Arc<Mutex<WebMode>>,
    target_id: String,
}

impl DisconnectedWebPage {
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<WebPage> {
        if wait_ms > 0 {
            sleep(Duration::from_millis(wait_ms));
        }
        let browser = self.browser.reconnect()?;
        let driver = browser.get_page(&self.target_id)?;
        Ok(WebPage {
            browser,
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        })
    }
}

impl DisconnectedWebFrame {
    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<WebFrame> {
        match self {
            Self::Browser(frame) => frame.reconnect(wait_ms).map(WebFrame::Browser),
            Self::Mix { frame, page } => frame
                .reconnect(wait_ms)
                .map(|frame| page.with_driver_frame(frame)),
        }
    }
}

impl WebFrame {
    pub(crate) fn frame(&self) -> &Frame {
        match self {
            Self::Browser(frame) | Self::Mix { frame, .. } => frame,
        }
    }

    fn wrap_frame(&self, frame: Frame) -> WebFrame {
        match self {
            Self::Browser(_) => WebFrame::Browser(frame),
            Self::Mix { page, .. } => page.with_driver_frame(frame),
        }
    }

    fn wrap_element(&self, element: Element) -> WebElement {
        match self {
            Self::Browser(_) => WebElement::Browser(element),
            Self::Mix { page, .. } => page.with_driver_element(element),
        }
    }

    fn wrap_page(&self, page: Page) -> BrowserTabReference {
        match self {
            Self::Browser(_) => BrowserTabReference::Page(page),
            Self::Mix { page: owner, .. } => {
                BrowserTabReference::WebPage(owner.with_driver_page(page))
            }
        }
    }

    pub fn scroll(&self) -> FrameScroller<'_> {
        self.frame().scroll()
    }

    pub fn set(&self) -> FrameSetter<'_> {
        self.frame().set()
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.frame().set_cookies(cookies)
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.frame().remove_cookie(name, url, domain, path)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.frame().clear_cookies()
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame().set_upload_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame().set_upload_paths(files)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.frame().set_download_path(path)
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.frame().set_download_file_exists_mode(mode)
    }

    pub fn set_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.frame().set_when_download_file_exists(mode)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.frame()
            .set_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.frame()
            .set_download_file_name(rename, suffix, suffix_specified)
    }

    pub fn states(&self) -> FrameStates<'_> {
        self.frame().states()
    }

    pub fn wait(&self) -> FrameWait<'_> {
        self.frame().wait()
    }

    pub fn rect(&self) -> FrameRect<'_> {
        self.frame().rect()
    }

    pub fn id(&self) -> &str {
        self.frame().id()
    }

    pub fn frame_id(&self) -> &str {
        self.frame().frame_id()
    }

    pub fn frame_element(&self) -> &Element {
        self.frame().frame_element()
    }

    pub fn frame_ele(&self) -> &Element {
        self.frame_element()
    }

    pub fn frame_element_reference(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .owner()
            .get_frame_ele(self.frame().frame_element())
            .map(|element| self.wrap_element(element))
    }

    pub fn frame_ele_reference(&self) -> OpenPageResult<WebElement> {
        self.frame_element_reference()
    }

    pub fn owner(&self) -> &crate::page::Page {
        self.frame().owner()
    }

    pub fn page(&self) -> &crate::page::Page {
        self.owner()
    }

    pub fn owner_reference(&self) -> BrowserTabReference {
        self.wrap_page(self.frame().owner().clone())
    }

    pub fn tab(&self) -> &crate::page::Page {
        self.frame().tab()
    }

    pub fn tab_reference(&self) -> BrowserTabReference {
        self.owner_reference()
    }

    pub fn tab_id(&self) -> String {
        self.frame().tab_id()
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.frame().set_none_element_value(value, on_off)
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.frame().set_raise_when_ele_not_found(on_off)
    }

    pub fn name(&self) -> OpenPageResult<Option<String>> {
        self.frame().name()
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.frame().tag()
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        self.frame().link()
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.frame().attrs()
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.frame().attr(name)
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.frame().property(name)
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.frame().text()
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.frame().raw_text()
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        self.frame().value()
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.frame().comments()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.frame().texts(text_node_only)
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        self.frame().src(timeout_ms, base64_to_bytes)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<std::path::PathBuf> {
        self.frame().save(path, name, timeout_ms, rename)
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        self.frame().style(name, pseudo)
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        self.frame().pseudo_before()
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        self.frame().pseudo_after()
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        self.frame().scroll_to_see(center)
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_center()
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.frame().css_path()
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.frame().xpath()
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        self.frame().child_count()
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame().sr()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame().shadow_root()
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        self.frame().url()
    }

    pub fn parent_id(&self) -> OpenPageResult<Option<String>> {
        self.frame().parent_id()
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        self.frame().title()
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.frame().download_path()
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        self.frame().download(url)
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        self.frame().download_to(url, path)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_for_upload_paths_inputted(timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.frame().wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.frame()
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
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
        self.frame().click_to_download(
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
        self.frame()
            .click_to_upload(locator, files, timeout_ms, by_js)
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .click_for_new_tab(locator, timeout_ms, by_js)
            .map(|page| page.map(|page| self.wrap_page(page)))
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .click_middle(locator, timeout_ms, get_tab)
            .map(|page| page.map(|page| self.wrap_page(page)))
    }

    pub fn html(&self) -> OpenPageResult<String> {
        self.frame().html()
    }

    pub fn inner_html(&self) -> OpenPageResult<String> {
        self.frame().inner_html()
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        self.frame().run_js(expression)
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.frame().run_js_with_args(script, args, as_expr)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        self.frame()
            .run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        self.frame().run_js_loaded(script)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.frame().run_js_loaded_with_args(script, args, as_expr)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        self.frame()
            .run_js_loaded_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        self.frame().run_async_js(script)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        self.frame().run_async_js_with_args(script, args, as_expr)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        self.frame()
            .run_async_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        self.frame().add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        self.frame().remove_init_js(script_id)
    }

    pub fn refresh(&self) -> OpenPageResult<()> {
        self.frame().refresh()
    }

    pub fn refresh_with_options(&self, ignore_cache: bool) -> OpenPageResult<()> {
        self.frame().refresh_with_options(ignore_cache)
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        self.frame().get(url)
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        self.frame().goto(url)
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        match self {
            Self::Browser(frame) => frame.reconnect(wait_ms).map(Self::Browser),
            Self::Mix { frame, page } => frame
                .reconnect(wait_ms)
                .map(|frame| page.with_driver_frame(frame)),
        }
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedWebFrame> {
        match self {
            Self::Browser(frame) => frame.disconnect().map(DisconnectedWebFrame::Browser),
            Self::Mix { frame, page } => Ok(DisconnectedWebFrame::Mix {
                frame: frame.disconnect()?,
                page,
            }),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        self.frame().remove_attr(name)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame().set_attr(name, value)
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame().set_property(name, value)
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame().set_style(name, value)
    }

    pub fn click(&self) -> OpenPageResult<()> {
        self.frame().click()
    }

    pub fn click_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame()
            .click_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame()
            .click_left_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        self.frame().click_at(offset_x, offset_y, button, count)
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        self.frame().click_multi(times)
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        self.frame().click_left()
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        self.frame().click_right()
    }

    pub fn input<'a, I>(&self, text: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame().input(text)
    }

    pub fn input_with_options<'a, I>(&self, text: I, clear: bool, by_js: bool) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame().input_with_options(text, clear, by_js)
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
        self.frame().input_keys_with_options(values, clear, by_js)
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        self.frame().press_key(key)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.frame().clear()
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame().clear_with_mode(by_js)
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        self.frame().submit()
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        self.frame().focus()
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        self.frame().hover()
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        self.frame().hover_with_offset(offset_x, offset_y)
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame().drag(offset_x, offset_y, duration_secs)
    }

    pub fn drag_to<'a, T>(&self, target: T, duration_secs: f64) -> OpenPageResult<()>
    where
        T: Into<WebElementDragTarget<'a>>,
    {
        let target = match target.into() {
            WebElementDragTarget::Element(target) => {
                return self.drag_to_browser_element(target, duration_secs);
            }
            WebElementDragTarget::OwnedElement(target) => {
                return self.drag_to_browser_element(&target, duration_secs);
            }
            WebElementDragTarget::Locator(locator) => self.find(locator)?,
            WebElementDragTarget::Coordinates(x, y) => {
                return self.frame().drag_to_point(x, y, duration_secs);
            }
        };
        self.drag_to_browser_element(&target, duration_secs)
    }

    fn drag_to_browser_element(
        &self,
        target: &WebElement,
        duration_secs: f64,
    ) -> OpenPageResult<()> {
        let Some(target) = target.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                web_driver_element_required_message("drag_to() target"),
            ));
        };
        self.frame().drag_to(target, duration_secs)
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame().drag_to_point(x, y, duration_secs)
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        self.frame().set_checked(checked)
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        self.frame().check(uncheck, by_js)
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame().uncheck(by_js)
    }

    pub fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        self.frame()
            .active_element()
            .map(|element| element.map(|element| self.wrap_element(element)))
    }

    pub fn active_ele(&self) -> OpenPageResult<Option<WebElement>> {
        self.active_element()
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.frame()
            .ele(locator.raw())
            .map(|element| element.map(|element| self.wrap_element(element)))
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .find(locator)
            .map(|element| self.wrap_element(element))
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().find_all(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.frame()
            .get_frame(target)
            .map(|frame| self.wrap_frame(frame))
    }

    pub fn get_frame_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.frame()
            .get_frame_with_timeout(target, timeout_ms)
            .map(|frame| self.wrap_frame(frame))
    }

    pub fn get_frame_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        self.frame()
            .get_frame_by_index(index)
            .map(|frame| self.wrap_frame(frame))
    }

    pub fn get_frame_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        self.frame()
            .get_frame_by_index_with_timeout(index, timeout_ms)
            .map(|frame| self.wrap_frame(frame))
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.frame()
            .get_frame_ele(target)
            .map(|element| self.wrap_element(element))
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.frame()
            .get_frame_ele_with_timeout(target, timeout_ms)
            .map(|element| self.wrap_element(element))
    }

    pub fn get_frame_ele_by_index<I>(&self, index: I) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        self.frame()
            .get_frame_ele_by_index(index)
            .map(|element| self.wrap_element(element))
    }

    pub fn get_frame_ele_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        self.frame()
            .get_frame_ele_by_index_with_timeout(index, timeout_ms)
            .map(|element| self.wrap_element(element))
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().get_frames(locator).map(|frames| {
            frames
                .into_iter()
                .map(|frame| self.wrap_frame(frame))
                .collect()
        })
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .get_frames_with_timeout(locator, timeout_ms)
            .map(|frames| {
                frames
                    .into_iter()
                    .map(|frame| self.wrap_frame(frame))
                    .collect()
            })
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().get_frame_eles(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .get_frame_eles_with_timeout(locator, timeout_ms)
            .map(|elements| {
                elements
                    .into_iter()
                    .map(|element| self.wrap_element(element))
                    .collect()
            })
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.get_frame(target)
    }

    pub fn get_frame_context_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        self.get_frame_by_index(index)
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frames(locator)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<WebElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.frame()
            .find_locators(locators, any_one, first_match_only)
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| LocatorMatch {
                        locator: item.locator,
                        elements: item
                            .elements
                            .into_iter()
                            .map(|element| self.wrap_element(element))
                            .collect(),
                    })
                    .collect()
            })
    }

    pub fn parent(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .parent()
            .map(|element| self.wrap_element(element))
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<WebElement> {
        self.frame()
            .parent_level(level)
            .map(|element| self.wrap_element(element))
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .parent_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn child(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .child()
            .map(|element| self.wrap_element(element))
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .child_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn children(&self) -> OpenPageResult<Vec<WebElement>> {
        self.frame().children().map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().children_with(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn prev(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .prev()
            .map(|element| self.wrap_element(element))
    }

    pub fn prev_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .prev_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn next(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .next()
            .map(|element| self.wrap_element(element))
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .next_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn before(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .before()
            .map(|element| self.wrap_element(element))
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .before_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn after(&self) -> OpenPageResult<WebElement> {
        self.frame()
            .after()
            .map(|element| self.wrap_element(element))
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .after_with(locator, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<WebElement>> {
        self.frame().prevs().map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().prevs_with(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<WebElement>> {
        self.frame().nexts().map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().nexts_with(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn befores(&self) -> OpenPageResult<Vec<WebElement>> {
        self.frame().befores().map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().befores_with(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn afters(&self) -> OpenPageResult<Vec<WebElement>> {
        self.frame().afters().map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().afters_with(locator).map(|elements| {
            elements
                .into_iter()
                .map(|element| self.wrap_element(element))
                .collect()
        })
    }

    pub fn over(&self) -> OpenPageResult<Option<WebElement>> {
        self.frame()
            .over()
            .map(|value| value.map(|element| self.wrap_element(element)))
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<WebElement>> {
        self.frame()
            .over_with_timeout(timeout_ms)
            .map(|value| value.map(|element| self.wrap_element(element)))
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .offset(locator, x, y, timeout_ms)
            .map(|element| self.wrap_element(element))
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .east(locator, pixels, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .south(locator, pixels, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .west(locator, pixels, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame()
            .north(locator, pixels, index)
            .map(|element| self.wrap_element(element))
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        self.frame().screenshot_bytes(scroll_to_center, timeout_ms)
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        self.frame().screenshot_base64(scroll_to_center, timeout_ms)
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<std::path::PathBuf> {
        self.frame()
            .get_screenshot(path, name, scroll_to_center, timeout_ms)
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.frame().save_screenshot(path)
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_top()
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_bottom()
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_half()
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_rightmost()
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        self.frame().scroll_to_leftmost()
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.frame().scroll_to_location(x, y)
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame().scroll_up(pixels)
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame().scroll_down(pixels)
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame().scroll_left(pixels)
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame().scroll_right(pixels)
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        self.frame().scroll_position()
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame().location()
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame().viewport_location()
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame().screen_location()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame().size()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame().viewport_size()
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame().corners()
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame().viewport_corners()
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        self.frame().ready_state()
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        self.frame().is_loading()
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.frame().is_alive()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.frame().is_displayed()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.frame().is_enabled()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.frame().has_rect()
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame().is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame().is_whole_in_viewport()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.frame().is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.frame().is_clickable()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.frame().has_alert()
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_for_doc_loaded(timeout_ms)
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_displayed(timeout_ms)
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_hidden(timeout_ms)
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_enabled(timeout_ms)
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_disabled(timeout_ms)
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_deleted(timeout_ms)
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_clickable(timeout_ms)
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_has_rect(timeout_ms)
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_covered(timeout_ms)
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_not_covered(timeout_ms)
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame().wait_until_stop_moving(timeout_ms)
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        self.frame().snapshot_root()
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame().snapshot_find(locator)
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
        self.frame().snapshot_find_all(locator)
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        self.frame().snapshot_find_by(by, value)
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        self.frame().snapshot_find_all_by(by, value)
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.frame().snapshot_query_xpath(expression)
    }

    pub fn listener(&self) -> Listener {
        self.frame().listener()
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn console(&self) -> Console {
        self.frame().console()
    }
}

impl WebElement {
    fn browser_element(&self) -> Option<&Element> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => Some(element),
            Self::Session(_) => None,
        }
    }

    fn wrap_browser_element(&self, element: Element) -> WebElement {
        match self {
            Self::Browser(_) => WebElement::Browser(element),
            Self::Mix { page, .. } => page.with_driver_element(element),
            Self::Session(_) => WebElement::Browser(element),
        }
    }

    fn wrap_browser_frame_result(&self, frame: Frame) -> OpenPageResult<WebFrame> {
        match self {
            Self::Browser(_) => Ok(WebFrame::Browser(frame)),
            Self::Mix { page, .. } => Ok(page.with_driver_frame(frame)),
            Self::Session(_) => Ok(WebFrame::Browser(frame)),
        }
    }

    fn wrap_page(&self, page: Page) -> BrowserTabReference {
        match self {
            Self::Browser(_) => BrowserTabReference::Page(page),
            Self::Mix { page: owner, .. } => {
                BrowserTabReference::WebPage(owner.with_driver_page(page))
            }
            Self::Session(_) => BrowserTabReference::Page(page),
        }
    }

    pub(crate) fn none_element_runtime_config_handle(
        &self,
    ) -> Option<&ElementsOneRuntimeConfigHandle> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                Some(element.none_element_runtime_config_handle())
            }
            Self::Session(element) => element.none_element_runtime_config_handle(),
        }
    }

    pub fn scroll(&self) -> WebElementScroller<'_> {
        WebElementScroller { element: self }
    }

    pub fn clicker(&self) -> WebElementClicker<'_> {
        WebElementClicker { element: self }
    }

    pub fn set(&self) -> WebElementSetter<'_> {
        WebElementSetter { element: self }
    }

    pub fn select(&self) -> WebElementSelector<'_> {
        WebElementSelector { element: self }
    }

    pub fn states(&self) -> WebElementStates<'_> {
        WebElementStates { element: self }
    }

    pub fn rect(&self) -> WebElementRect<'_> {
        WebElementRect { element: self }
    }

    pub fn wait(&self) -> WebElementWait<'_> {
        WebElementWait { element: self }
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.tag(),
            Self::Session(element) => element.tag(),
        }
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.text(),
            Self::Session(element) => element.text(),
        }
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.html(),
            Self::Session(element) => element.html(),
        }
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.inner_html(),
            Self::Session(element) => element.inner_html(),
        }
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.snapshot_root(),
            Self::Session(element) => Ok(element.clone()),
        }
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.snapshot_find(locator),
            Self::Session(element) => element.find(locator),
        }
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
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.snapshot_find_all(locator)
            }
            Self::Session(element) => element.find_all(locator),
        }
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.snapshot_find_by(by, value)
            }
            Self::Session(element) => element.find_by(by, value),
        }
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.snapshot_find_all_by(by, value)
            }
            Self::Session(element) => element.find_all_by(by, value),
        }
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.snapshot_query_xpath(expression)
            }
            Self::Session(element) => element.query_xpath(expression),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .ele(locator.raw())
                .map(|value| value.map(|element| self.wrap_browser_element(element))),
            Self::Session(element) => element
                .ele(locator.raw())
                .map(|value| value.map(Self::Session)),
        }
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .get_frame(target)
                .and_then(|frame| self.wrap_browser_frame_result(frame)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame()"),
            )),
        }
    }

    pub fn get_frame_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .get_frame_with_timeout(target, timeout_ms)
                .and_then(|frame| self.wrap_browser_frame_result(frame)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .get_frame_by_index(index)
                .and_then(|frame| self.wrap_browser_frame_result(frame)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index()"),
            )),
        }
    }

    pub fn get_frame_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .get_frame_by_index_with_timeout(index, timeout_ms)
                .and_then(|frame| self.wrap_browser_frame_result(frame)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index_with_timeout()"),
            )),
        }
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .find(locator)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.find(locator).map(Self::Session),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.find_all(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .find_all(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<WebElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item
                                .elements
                                .into_iter()
                                .map(|element| self.wrap_browser_element(element))
                                .collect(),
                        })
                        .collect()
                }),
            Self::Session(element) => element
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item.elements.into_iter().map(WebElement::Session).collect(),
                        })
                        .collect()
                }),
        }
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.attrs(),
            Self::Session(element) => element.attrs(),
        }
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.attr(name),
            Self::Session(element) => element.attr(name),
        }
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.property(name),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("property()"),
            )),
        }
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.raw_text(),
            Self::Session(element) => element.raw_text(),
        }
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.value(),
            Self::Session(element) => element.attr("value"),
        }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.link(),
            Self::Session(element) => {
                let href = element.attr("href")?;
                if href.as_deref().is_some_and(|value| !value.is_empty()) {
                    return Ok(href);
                }
                element.attr("src")
            }
        }
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.child_count(),
            Self::Session(element) => Ok(element.children()?.len()),
        }
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.css_path(),
            Self::Session(element) => element.css_path(),
        }
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.xpath(),
            Self::Session(element) => element.xpath(),
        }
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.comments(),
            Self::Session(element) => element.comments(),
        }
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.texts(text_node_only),
            Self::Session(element) => element.texts(text_node_only),
        }
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_displayed(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_displayed()"),
            )),
        }
    }

    pub fn is_checked(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_checked(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_checked()"),
            )),
        }
    }

    pub fn is_selected(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_selected()"),
            )),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_enabled(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_enabled()"),
            )),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_alive(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_alive()"),
            )),
        }
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_in_viewport(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_in_viewport()"),
            )),
        }
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_whole_in_viewport(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_whole_in_viewport()"),
            )),
        }
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_covered(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_covered()"),
            )),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_clickable(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_clickable()"),
            )),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.has_rect(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("has_rect()"),
            )),
        }
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.style(name, pseudo),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("style()"),
            )),
        }
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.pseudo_before(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("pseudo_before()"),
            )),
        }
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.pseudo_after(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("pseudo_after()"),
            )),
        }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_top(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_top()"),
            )),
        }
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_bottom(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_bottom()"),
            )),
        }
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_half(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_half()"),
            )),
        }
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_rightmost(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_rightmost()"),
            )),
        }
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_leftmost(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_leftmost()"),
            )),
        }
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_location(x, y),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_location()"),
            )),
        }
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_up(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_up()"),
            )),
        }
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_down(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_down()"),
            )),
        }
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_left(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_left()"),
            )),
        }
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_right(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_right()"),
            )),
        }
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_see(center),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_see()"),
            )),
        }
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.scroll_to_center(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_center()"),
            )),
        }
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.src(timeout_ms, base64_to_bytes)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("src()"),
            )),
        }
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<std::path::PathBuf> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.save(path, name, timeout_ms, rename)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save()"),
            )),
        }
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.shadow_root(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("shadow_root()"),
            )),
        }
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.shadow_root()
    }

    pub fn parent(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .parent()
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.parent().map(Self::Session),
        }
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .parent_level(level)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.parent_level(level).map(Self::Session),
        }
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .parent_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.parent_with(locator, index).map(Self::Session),
        }
    }

    pub fn child(&self) -> OpenPageResult<WebElement> {
        self.child_with(None::<&str>, 1)
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .child_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.child_with(locator, index).map(Self::Session),
        }
    }

    pub fn children(&self) -> OpenPageResult<Vec<WebElement>> {
        self.children_with(None::<&str>)
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.children_with(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .children_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn prev(&self) -> OpenPageResult<WebElement> {
        self.prev_with(None::<&str>, 1)
    }

    pub fn prev_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .prev_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.prev_with(locator, index).map(Self::Session),
        }
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<WebElement>> {
        self.prevs_with(None::<&str>)
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.prevs_with(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .prevs_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn next(&self) -> OpenPageResult<WebElement> {
        self.next_with(None::<&str>, 1)
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .next_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.next_with(locator, index).map(Self::Session),
        }
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<WebElement>> {
        self.nexts_with(None::<&str>)
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.nexts_with(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .nexts_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn before(&self) -> OpenPageResult<WebElement> {
        self.before_with(None::<&str>, 1)
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .before_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.before_with(locator, index).map(Self::Session),
        }
    }

    pub fn befores(&self) -> OpenPageResult<Vec<WebElement>> {
        self.befores_with(None::<&str>)
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.befores_with(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .befores_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn after(&self) -> OpenPageResult<WebElement> {
        self.after_with(None::<&str>, 1)
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .after_with(locator, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(element) => element.after_with(locator, index).map(Self::Session),
        }
    }

    pub fn afters(&self) -> OpenPageResult<Vec<WebElement>> {
        self.afters_with(None::<&str>)
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.afters_with(locator).map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.wrap_browser_element(element))
                        .collect()
                })
            }
            Self::Session(element) => element
                .afters_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn over(&self) -> OpenPageResult<Option<WebElement>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .over()
                .map(|value| value.map(|element| self.wrap_browser_element(element))),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("over()"),
            )),
        }
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<WebElement>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .over_with_timeout(timeout_ms)
                .map(|value| value.map(|element| self.wrap_browser_element(element))),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("over_with_timeout()"),
            )),
        }
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .offset(locator, x, y, timeout_ms)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("offset()"),
            )),
        }
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .east(locator, pixels, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("east()"),
            )),
        }
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .south(locator, pixels, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("south()"),
            )),
        }
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .west(locator, pixels, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("west()"),
            )),
        }
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element
                .north(locator, pixels, index)
                .map(|element| self.wrap_browser_element(element)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("north()"),
            )),
        }
    }

    pub fn click(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.click(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click()"),
            )),
        }
    }

    pub fn click_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.click_with_options(by_js, timeout_ms, wait_stop)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_with_options()"),
            )),
        }
    }

    pub fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.click_at(offset_x, offset_y, button, count)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_at()"),
            )),
        }
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.click_multi(times),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_multi()"),
            )),
        }
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.click_left(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_left()"),
            )),
        }
    }

    pub fn click_left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.click_left_with_options(by_js, timeout_ms, wait_stop)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_left_with_options()"),
            )),
        }
    }

    pub fn click_middle(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.click_middle(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_middle()"),
            )),
        }
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.click_right(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_right()"),
            )),
        }
    }

    pub fn input<'a, I>(&self, text: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.input(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input()"),
            )),
        }
    }

    pub fn input_with_options<'a, I>(&self, text: I, clear: bool, by_js: bool) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.input_with_options(text, clear, by_js)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input_with_options()"),
            )),
        }
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
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.input_keys_with_options(values, clear, by_js)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input_keys_with_options()"),
            )),
        }
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.clear(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear()"),
            )),
        }
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.submit(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("submit()"),
            )),
        }
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.clear_with_mode(by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear_with_mode()"),
            )),
        }
    }

    pub fn set_file_input_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.set_file_input_files(files)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_file_input_files()"),
            )),
        }
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.press_key(key),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("press_key()"),
            )),
        }
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.run_js(script),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js()"),
            )),
        }
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.run_js_with_args(script, args, as_expr)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_args()"),
            )),
        }
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.run_js_with_options(script, args, as_expr, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_options()"),
            )),
        }
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.run_async_js(script),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js()"),
            )),
        }
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.run_async_js_with_args(script, args, as_expr)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_args()"),
            )),
        }
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.run_async_js_with_options(script, args, as_expr, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_options()"),
            )),
        }
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.save_screenshot(path),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_screenshot()"),
            )),
        }
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.screenshot_bytes(scroll_to_center, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_bytes()"),
            )),
        }
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.screenshot_base64(scroll_to_center, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_base64()"),
            )),
        }
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<std::path::PathBuf> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.get_screenshot(path, name, scroll_to_center, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_screenshot()"),
            )),
        }
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.focus(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("focus()"),
            )),
        }
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.hover(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("hover()"),
            )),
        }
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.hover_with_offset(offset_x, offset_y)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("hover_with_offset()"),
            )),
        }
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.drag(offset_x, offset_y, duration_secs)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("drag()"),
            )),
        }
    }

    pub fn drag_to_element(&self, target: &WebElement, duration_secs: f64) -> OpenPageResult<()> {
        let Some(element) = self.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("drag_to_element()"),
            ));
        };
        let Some(target) = target.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                web_driver_element_required_message("drag_to_element() target"),
            ));
        };
        element.drag_to(target, duration_secs)
    }

    pub fn drag_to<'a, T>(&self, target: T, duration_secs: f64) -> OpenPageResult<()>
    where
        T: Into<WebElementDragTarget<'a>>,
    {
        let target = match target.into() {
            WebElementDragTarget::Element(target) => {
                return self.drag_to_browser_element(target, duration_secs);
            }
            WebElementDragTarget::OwnedElement(target) => {
                return self.drag_to_browser_element(&target, duration_secs);
            }
            WebElementDragTarget::Locator(locator) => self.find(locator)?,
            WebElementDragTarget::Coordinates(x, y) => {
                let Some(element) = self.browser_element() else {
                    return Err(OpenPageError::UnsupportedOperation(
                        driver_mode_only_message("drag_to()"),
                    ));
                };
                return element.drag_to_point(x, y, duration_secs);
            }
        };
        self.drag_to_browser_element(&target, duration_secs)
    }

    fn drag_to_browser_element(
        &self,
        target: &WebElement,
        duration_secs: f64,
    ) -> OpenPageResult<()> {
        let Some(element) = self.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("drag_to()"),
            ));
        };
        let Some(target) = target.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                web_driver_element_required_message("drag_to() target"),
            ));
        };
        element.drag_to(target, duration_secs)
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.drag_to_point(x, y, duration_secs)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("drag_to_point()"),
            )),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.remove_attr(name),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("remove_attr()"),
            )),
        }
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.set_attr(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_attr()"),
            )),
        }
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.set_property(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_property()"),
            )),
        }
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.set_style(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_style()"),
            )),
        }
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.set_checked(checked),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_checked()"),
            )),
        }
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.check(uncheck, by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("check()"),
            )),
        }
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.uncheck(by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("uncheck()"),
            )),
        }
    }

    pub fn is_multi_select(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_multi_select(),
            Self::Session(element) => Ok(element.attr("multiple")?.is_some()),
        }
    }

    pub fn option_texts(&self) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.option_texts(),
            Self::Session(element) => {
                let options = element.children_with(Some("css:option"))?;
                let mut texts = Vec::with_capacity(options.len());
                for option in options {
                    if let Some(text) = option.text()? {
                        texts.push(text);
                    }
                }
                Ok(texts)
            }
        }
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.selected_option(),
            Self::Session(element) => {
                let option = element
                    .children_with(Some("css:option[selected]"))?
                    .into_iter()
                    .next();
                option
                    .map(|item| item.text())
                    .transpose()
                    .map(|value| value.flatten())
            }
        }
    }

    pub fn selected_options(&self) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.selected_options(),
            Self::Session(element) => {
                let options = element.children_with(Some("css:option[selected]"))?;
                let mut texts = Vec::with_capacity(options.len());
                for option in options {
                    if let Some(text) = option.text()? {
                        texts.push(text);
                    }
                }
                Ok(texts)
            }
        }
    }

    pub fn option_elements(&self) -> OpenPageResult<Vec<WebElement>> {
        self.find_all("css:option")
    }

    pub fn selected_option_element(&self) -> OpenPageResult<Option<WebElement>> {
        Ok(self.selected_option_elements()?.into_iter().next())
    }

    pub fn selected_option_elements(&self) -> OpenPageResult<Vec<WebElement>> {
        match self {
            Self::Browser(_) | Self::Mix { .. } => self.find_all("css:option:checked"),
            Self::Session(_) => self.find_all("css:option[selected]"),
        }
    }

    pub fn select_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.select_by_text(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_text()"),
            )),
        }
    }

    pub fn select_by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_text_with_timeout(text, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_text_with_timeout()"),
            )),
        }
    }

    pub fn select_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.select_by_value(value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_value()"),
            )),
        }
    }

    pub fn select_by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_value_with_timeout(value, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_value_with_timeout()"),
            )),
        }
    }

    pub fn select_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_locator(locator)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_locator()"),
            )),
        }
    }

    pub fn select_by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_locator_with_timeout(locator, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_locator_with_timeout()"),
            )),
        }
    }

    pub fn select_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.select_by_index(index),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_index()"),
            )),
        }
    }

    pub fn select_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_index_with_timeout(index, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_index_with_timeout()"),
            )),
        }
    }

    pub fn select_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_indices(indices)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_indices()"),
            )),
        }
    }

    pub fn select_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.select_by_indices_with_timeout(indices, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_indices_with_timeout()"),
            )),
        }
    }

    pub fn select_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        match option.into() {
            WebSelectOptionInput::Single(option) => self.select_by_option_value(option),
            WebSelectOptionInput::OwnedSingle(option) => self.select_by_option_value(&option),
            WebSelectOptionInput::Many(options) => self.select_by_options(&options),
        }
    }

    fn select_by_option_value(&self, option: &WebElement) -> OpenPageResult<bool> {
        let Some(element) = self.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_by_option()"),
            ));
        };
        let Some(option) = option.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                web_browser_backed_option_required_message("select_by_option()"),
            ));
        };
        element.select_by_option(option)
    }

    pub fn select_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        let mut matched = false;
        for option in options {
            matched |= self.select_by_option_value(option)?;
        }
        Ok(matched)
    }

    pub fn cancel_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.cancel_by_text(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_text()"),
            )),
        }
    }

    pub fn cancel_by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_text_with_timeout(text, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_text_with_timeout()"),
            )),
        }
    }

    pub fn cancel_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.cancel_by_value(value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_value()"),
            )),
        }
    }

    pub fn cancel_by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_value_with_timeout(value, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_value_with_timeout()"),
            )),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.cancel_by_index(index),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_index()"),
            )),
        }
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_index_with_timeout(index, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_index_with_timeout()"),
            )),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_indices(indices)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_indices()"),
            )),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_indices_with_timeout(indices, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_indices_with_timeout()"),
            )),
        }
    }

    pub fn cancel_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_locator(locator)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_locator()"),
            )),
        }
    }

    pub fn cancel_by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.cancel_by_locator_with_timeout(locator, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_locator_with_timeout()"),
            )),
        }
    }

    pub fn cancel_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        match option.into() {
            WebSelectOptionInput::Single(option) => self.cancel_by_option_value(option),
            WebSelectOptionInput::OwnedSingle(option) => self.cancel_by_option_value(&option),
            WebSelectOptionInput::Many(options) => self.cancel_by_options(&options),
        }
    }

    fn cancel_by_option_value(&self, option: &WebElement) -> OpenPageResult<bool> {
        let Some(element) = self.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("cancel_by_option()"),
            ));
        };
        let Some(option) = option.browser_element() else {
            return Err(OpenPageError::UnsupportedOperation(
                web_browser_backed_option_required_message("cancel_by_option()"),
            ));
        };
        element.cancel_by_option(option)
    }

    pub fn cancel_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        let mut matched = false;
        for option in options {
            matched |= self.cancel_by_option_value(option)?;
        }
        Ok(matched)
    }

    pub fn select_all(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.select_all(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("select_all()"),
            )),
        }
    }

    pub fn invert_selected(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.invert_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("invert_selected()"),
            )),
        }
    }

    pub fn clear_selected(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.clear_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear_selected()"),
            )),
        }
    }

    pub fn rect_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_corners(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_corners()"),
            )),
        }
    }

    pub fn rect_viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_viewport_corners(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_viewport_corners()"),
            )),
        }
    }

    pub fn rect_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_location()"),
            )),
        }
    }

    pub fn rect_viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_viewport_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_viewport_location()"),
            )),
        }
    }

    pub fn rect_screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_screen_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_screen_location()"),
            )),
        }
    }

    pub fn rect_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_midpoint()"),
            )),
        }
    }

    pub fn rect_viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_viewport_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_viewport_midpoint()"),
            )),
        }
    }

    pub fn rect_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_click_point(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_click_point()"),
            )),
        }
    }

    pub fn rect_viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.rect_viewport_click_point()
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_viewport_click_point()"),
            )),
        }
    }

    pub fn rect_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_size(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_size()"),
            )),
        }
    }

    pub fn rect_screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_screen_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_screen_midpoint()"),
            )),
        }
    }

    pub fn rect_screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_screen_click_point(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_screen_click_point()"),
            )),
        }
    }

    pub fn rect_scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.rect_scroll_position(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("rect_scroll_position()"),
            )),
        }
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_displayed(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_displayed()"),
            )),
        }
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_hidden(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_hidden()"),
            )),
        }
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_enabled(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_enabled()"),
            )),
        }
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_disabled(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_disabled()"),
            )),
        }
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_deleted(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_deleted()"),
            )),
        }
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_clickable(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_clickable()"),
            )),
        }
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_has_rect(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_has_rect()"),
            )),
        }
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_covered(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_covered()"),
            )),
        }
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_not_covered(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_not_covered()"),
            )),
        }
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_disabled_or_deleted(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_disabled_or_deleted()"),
            )),
        }
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => {
                element.wait_until_stop_moving(timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("wait_until_stop_moving()"),
            )),
        }
    }
}

impl<'a> From<&'a WebPage> for BrowserTabSelector<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::Id(std::borrow::Cow::Owned(value.target_id()))
    }
}

impl From<WebPage> for BrowserTabSelector<'_> {
    fn from(value: WebPage) -> Self {
        Self::Id(std::borrow::Cow::Owned(value.target_id()))
    }
}

impl<'a> From<&'a WebPage> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<WebPage> for BrowserTabTargetsInput<'_> {
    fn from(value: WebPage) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl<'a> From<&'a [&'a WebPage]> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a [&'a WebPage]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| BrowserTabSelector::from(*item))
                .collect(),
        )
    }
}

impl<'a> From<&'a Vec<&'a WebPage>> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Vec<&'a WebPage>) -> Self {
        Self::from(value.as_slice())
    }
}

#[derive(Clone, Debug)]
pub struct WebPage {
    browser: Browser,
    driver: crate::page::Page,
    session: SessionPage,
    mode: Arc<Mutex<WebMode>>,
}

impl WebPage {
    fn with_driver_page(&self, driver: Page) -> Self {
        Self {
            browser: self.browser.clone(),
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        }
    }

    fn with_driver_frame(&self, frame: Frame) -> WebFrame {
        WebFrame::Mix {
            frame,
            page: Box::new(self.clone()),
        }
    }

    fn with_driver_element(&self, element: Element) -> WebElement {
        WebElement::Mix {
            element,
            page: Box::new(self.clone()),
        }
    }

    fn mix_tab_reference(&self, reference: BrowserTabReference) -> BrowserTabReference {
        match reference {
            BrowserTabReference::Page(page) => {
                BrowserTabReference::WebPage(self.with_driver_page(page))
            }
            other => other,
        }
    }

    pub fn new(
        mode: WebMode,
        launch_options: LaunchOptions,
        session_options: SessionOptions,
    ) -> OpenPageResult<Self> {
        let browser = Browser::launch(launch_options)?;
        let driver = browser.new_page(None)?;
        let session = SessionPage::new(session_options)?;
        Ok(Self {
            browser,
            driver,
            session,
            mode: Arc::new(Mutex::new(mode)),
        })
    }

    pub fn mode(&self) -> OpenPageResult<WebMode> {
        self.mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "webpage mode",
                "网页模式",
            ))
        })
    }

    pub fn navigation_snapshot(&self) -> OpenPageResult<crate::page::PageNavigationSnapshot> {
        match self.mode()? {
            WebMode::Driver => self.driver.navigation_snapshot(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("navigation_snapshot()"),
            )),
        }
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.driver.set_none_element_value(value, on_off)?;
        self.session.set_none_element_value(value, on_off)
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.driver.set_raise_when_ele_not_found(on_off)?;
        self.session.set_raise_when_ele_not_found(on_off)
    }

    pub fn actions(&self) -> OpenPageResult<Actions> {
        match self.mode()? {
            WebMode::Driver => self.driver.actions(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("actions()"),
            )),
        }
    }

    pub fn new_actions(&self) -> OpenPageResult<Actions> {
        match self.mode()? {
            WebMode::Driver => Ok(self.driver.new_actions()),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("new_actions()"),
            )),
        }
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        self.browser.tabs_count()
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        self.browser.tab_ids()
    }

    pub fn target_id(&self) -> String {
        self.driver.target_id()
    }

    pub fn tab_infos(&self) -> OpenPageResult<Vec<TabInfo>> {
        self.browser.tab_infos()
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
        self.browser
            .get_tabs(title, url, tab_type, as_id)
            .map(|references| {
                references
                    .into_iter()
                    .map(|reference| self.mix_tab_reference(reference))
                    .collect()
            })
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser
            .latest_tab()
            .map(|reference| reference.map(|reference| self.mix_tab_reference(reference)))
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
        new_context: bool,
    ) -> OpenPageResult<crate::page::Page> {
        self.browser
            .new_tab(url, new_window, background, new_context)
    }

    pub fn activate_tab<'a, T>(&self, target: T) -> OpenPageResult<()>
    where
        T: Into<BrowserTabSelector<'a>>,
    {
        self.browser.activate_tab(target)
    }

    pub fn close_tabs<'a, T>(&self, targets: T, others: bool) -> OpenPageResult<usize>
    where
        T: Into<BrowserTabTargetsInput<'a>>,
    {
        self.browser.close_tabs(targets, others)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.browser.set_download_path(path)?;
        self.session.set_download_path(path)
    }

    pub fn current_tab_download_path(&self) -> OpenPageResult<Option<String>> {
        self.browser.page_download_path(&self.driver.target_id())
    }

    pub fn set_current_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.browser
            .set_page_download_path(&self.driver.target_id(), path)
    }

    pub fn set_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.set_current_tab_download_path(path)
    }

    pub fn set_blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.driver.set_blocked_urls(patterns)
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.browser.download_file_exists_mode()
    }

    pub fn current_tab_download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.browser
            .page_download_file_exists_mode(&self.driver.target_id())
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.browser.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.browser.when_download_file_exists(mode)
    }

    pub fn set_current_tab_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.browser
            .set_page_download_file_exists_mode(&self.driver.target_id(), mode)
    }

    pub fn set_tab_download_file_exists_mode(
        &self,
        mode: crate::browser::DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_exists_mode(mode)
    }

    pub fn when_current_tab_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.browser
            .when_page_download_file_exists(&self.driver.target_id(), mode)
    }

    pub fn set_tab_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.when_current_tab_download_file_exists(mode)
    }

    pub fn set_current_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.browser.set_page_download_filename(
            &self.driver.target_id(),
            rename,
            suffix,
            suffix_specified,
        )
    }

    pub fn set_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_current_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_name(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_file_name(rename, suffix, suffix_specified)
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
        match self.mode()? {
            WebMode::Driver => self.driver.click_to_download(
                locator,
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            ),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_to_download()"),
            )),
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
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_to_upload(locator, files, timeout_ms, by_js),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_to_upload()"),
            )),
        }
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<WebPage>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_for_new_tab(locator, timeout_ms, by_js)
                .map(|page| page.map(|page| self.with_driver_page(page))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_for_new_tab()"),
            )),
        }
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<WebPage>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_middle(locator, timeout_ms, get_tab)
                .map(|page| page.map(|page| self.with_driver_page(page))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_middle()"),
            )),
        }
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.driver.set_upload_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        match self.mode()? {
            WebMode::Driver => self.driver.retry_times(),
            WebMode::Session => self.session.retry_times(),
        }
    }

    pub fn retry_interval(&self) -> OpenPageResult<f64> {
        match self.mode()? {
            WebMode::Driver => self.driver.retry_interval(),
            WebMode::Session => self
                .session
                .retry_interval_millis()
                .map(|millis| millis as f64 / 1000.0),
        }
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_retry(retry_times, retry_interval_secs),
            WebMode::Session => self.session.set_retry(
                retry_times,
                retry_interval_secs
                    .map(webpage_timeout_seconds_to_millis)
                    .transpose()?,
            ),
        }
    }

    pub fn timeouts(&self) -> OpenPageResult<HashMap<&'static str, f64>> {
        match self.mode()? {
            WebMode::Driver => self.driver.timeouts(),
            WebMode::Session => Ok(HashMap::from([(
                "base",
                self.session.timeout_secs()? as f64,
            )])),
        }
    }

    fn implicit_wait_timeout_ms(&self) -> OpenPageResult<u64> {
        Ok(self
            .timeouts()?
            .get("base")
            .map(|seconds| (seconds * 1000.0).round().max(0.0) as u64)
            .unwrap_or(10_000))
    }

    pub fn set_timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .set_timeouts(base_secs, page_load_secs, script_secs),
            WebMode::Session => {
                if page_load_secs.is_some() || script_secs.is_some() {
                    return Err(OpenPageError::UnsupportedOperation(
                        driver_mode_only_message("set_timeouts(page_load/script)"),
                    ));
                }
                if let Some(base_secs) = base_secs {
                    if !base_secs.is_finite() || base_secs.is_sign_negative() {
                        return Err(OpenPageError::UnsupportedOperation(
                            web_timeout_base_non_negative_message(base_secs),
                        ));
                    }
                    self.session.set_timeout(base_secs.round() as u64)?;
                }
                Ok(())
            }
        }
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        self.driver.load_mode()
    }

    pub fn set_load_mode(&self, mode: crate::browser::LoadMode) -> OpenPageResult<()> {
        self.driver.set_load_mode(mode)
    }

    pub fn window_state(&self) -> OpenPageResult<String> {
        self.driver.window_state()
    }

    pub fn window_id(&self) -> OpenPageResult<i64> {
        self.driver.window_id()
    }

    pub fn window_size(&self) -> OpenPageResult<(i64, i64)> {
        self.driver.window_size()
    }

    pub fn window_location(&self) -> OpenPageResult<(i64, i64)> {
        self.driver.window_location()
    }

    pub fn window_max(&self) -> OpenPageResult<()> {
        self.driver.window_max()
    }

    pub fn window_min(&self) -> OpenPageResult<()> {
        self.driver.window_min()
    }

    pub fn window_full(&self) -> OpenPageResult<()> {
        self.driver.window_full()
    }

    pub fn window_normal(&self) -> OpenPageResult<()> {
        self.driver.window_normal()
    }

    pub fn window_hide(&self) -> OpenPageResult<()> {
        self.driver.window_hide()
    }

    pub fn window_show(&self) -> OpenPageResult<()> {
        self.driver.window_show()
    }

    pub fn window_size_set(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        self.driver.window_size_set(width, height)
    }

    pub fn window_location_set(&self, left: Option<i64>, top: Option<i64>) -> OpenPageResult<()> {
        self.driver.window_location_set(left, top)
    }

    pub fn zoom_factor(&self) -> OpenPageResult<f64> {
        self.driver.zoom_factor()
    }

    pub fn set_zoom_factor(&self, factor: f64) -> OpenPageResult<()> {
        self.driver.set_zoom_factor(factor)
    }

    pub fn reset_zoom_factor(&self) -> OpenPageResult<()> {
        self.driver.reset_zoom_factor()
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        self.browser.wait_for_download(filename, timeout_ms)
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        self.browser.download_missions()
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        self.browser.last_download()
    }

    pub fn clear_finished_downloads(&self) -> OpenPageResult<usize> {
        self.browser.clear_finished_downloads()
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        self.browser.cancel_download(guid)
    }

    pub fn last_session_download(&self) -> OpenPageResult<Option<SessionDownload>> {
        self.session.last_download()
    }

    pub fn listener(&self) -> Listener {
        self.driver.listener()
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn interceptor(&self) -> Interceptor {
        self.driver.interceptor()
    }

    pub fn intercept(&self) -> Interceptor {
        self.interceptor()
    }

    pub fn console(&self) -> Console {
        self.driver.console()
    }

    pub fn screencast(&self) -> Screencast {
        self.driver.screencast()
    }

    pub fn recorder(&self) -> crate::recorder::Recorder {
        self.driver.recorder()
    }

    pub fn change_mode(
        &self,
        mode: Option<WebMode>,
        go: bool,
        copy_cookies: bool,
    ) -> OpenPageResult<()> {
        let current = self.mode()?;
        let target = mode.unwrap_or_else(|| current.toggled());
        if target == current {
            return Ok(());
        }

        match target {
            WebMode::Session => {
                if copy_cookies {
                    self.cookies_to_session(true)?;
                }
                if go {
                    let url = self.driver.url()?;
                    if !url.is_empty() {
                        self.session.get(&url)?;
                    }
                }
            }
            WebMode::Driver => {
                if copy_cookies {
                    self.cookies_to_browser()?;
                }
                if go {
                    if let Some(url) = self.session.url()? {
                        self.driver.goto(&url)?;
                    }
                }
            }
        }

        self.set_mode(target)
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => {
                self.driver.goto(url)?;
                Ok(true)
            }
            WebMode::Session => self.session.get(url),
        }
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.download(url)
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => {
                let url = self.driver.url()?;
                if url.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(url))
                }
            }
            WebMode::Session => self.session.url(),
        }
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => {
                let title = self.driver.title()?;
                if title.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(title))
                }
            }
            WebMode::Session => self.session.title(),
        }
    }

    pub fn browser(&self) -> Option<&Browser> {
        Some(&self.browser)
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.driver.browser_pid()
    }

    pub fn process_id(&self) -> Option<u32> {
        self.driver.process_id()
    }

    pub fn browser_version(&self) -> OpenPageResult<String> {
        self.driver.browser_version()
    }

    pub fn address(&self) -> OpenPageResult<String> {
        self.driver.address()
    }

    pub fn user_agent(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => Ok(Some(self.driver.user_agent()?)),
            WebMode::Session => self.session.user_agent(),
        }
    }

    pub fn evaluate(&self, expression: &str) -> OpenPageResult<Value> {
        match self.mode()? {
            WebMode::Driver => self.driver.evaluate(expression),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("evaluate()"),
            )),
        }
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_screenshot()"),
            ));
        }
        self.driver.save_screenshot(path, full_page)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
    ) -> OpenPageResult<PageSaveContent> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save()"),
            ));
        }
        self.driver.save(path, name, as_pdf)
    }

    pub fn save_with_options(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
        pdf_options: Option<chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams>,
    ) -> OpenPageResult<PageSaveContent> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_with_options()"),
            ));
        }
        self.driver
            .save_with_options(path, name, as_pdf, pdf_options)
    }

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_pdf()"),
            ));
        }
        self.driver.save_pdf(path)
    }

    pub fn screenshot_bytes(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<Vec<u8>> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_bytes()"),
            ));
        }
        self.driver
            .screenshot_bytes(full_page, left_top, right_bottom)
    }

    pub fn screenshot_base64(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_base64()"),
            ));
        }
        self.driver
            .screenshot_base64(full_page, left_top, right_bottom)
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<std::path::PathBuf> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_screenshot()"),
            ));
        }
        self.driver
            .get_screenshot(path, name, full_page, left_top, right_bottom)
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_position()"),
            ));
        }
        self.driver.scroll_position()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("viewport_size()"),
            ));
        }
        self.driver.viewport_size()
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("refresh()"),
            ));
        }
        self.driver.refresh(ignore_cache)
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("back()"),
            ));
        }
        self.driver.back(steps)
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("forward()"),
            ));
        }
        self.driver.forward(steps)
    }

    pub fn scroll(&self) -> WebPageScroller<'_> {
        WebPageScroller { page: self }
    }

    pub fn set(&self) -> WebPageSetter<'_> {
        WebPageSetter { page: self }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_top()"),
            ));
        }
        self.driver.scroll_to_top()
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_bottom()"),
            ));
        }
        self.driver.scroll_to_bottom()
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_half()"),
            ));
        }
        self.driver.scroll_to_half()
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_rightmost()"),
            ));
        }
        self.driver.scroll_to_rightmost()
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_leftmost()"),
            ));
        }
        self.driver.scroll_to_leftmost()
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_location()"),
            ));
        }
        self.driver.scroll_to_location(x, y)
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_up()"),
            ));
        }
        self.driver.scroll_up(pixels)
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_down()"),
            ));
        }
        self.driver.scroll_down(pixels)
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_left()"),
            ));
        }
        self.driver.scroll_left(pixels)
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_right()"),
            ));
        }
        self.driver.scroll_right(pixels)
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        match self.mode()? {
            WebMode::Driver => self.driver.cookies(),
            WebMode::Session => self.session.cookies(),
        }
    }

    pub fn cookie_header(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => self.driver.cookie_header(),
            WebMode::Session => {
                let Some(url) = self.session.url()? else {
                    return Ok(None);
                };
                self.session.cookie_header(&url)
            }
        }
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookies(cookies),
            WebMode::Session => self.session.set_cookies(cookies),
        }
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookie_header(url, cookie_header),
            WebMode::Session => self.session.set_cookie_header(url, cookie_header),
        }
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookie(name, value, url, domain, path),
            WebMode::Session => self.session.set_cookie(name, value, url, domain, path),
        }
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.remove_cookie(name, url, domain, path),
            WebMode::Session => self.session.remove_cookie(name, url),
        }
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.clear_cookies(),
            WebMode::Session => self.session.clear_cookies(),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.is_alive(),
            WebMode::Session => Ok(true),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.is_loading(),
            WebMode::Session => Ok(false),
        }
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => Ok(Some(self.driver.ready_state()?)),
            WebMode::Session => Ok(None),
        }
    }

    pub fn is_headless(&self) -> bool {
        self.browser.is_headless()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.driver.has_alert()
    }

    pub fn is_existed(&self) -> OpenPageResult<bool> {
        self.browser.is_existed()
    }

    pub fn is_incognito(&self) -> OpenPageResult<bool> {
        self.browser.is_incognito()
    }

    pub fn wait_for_new_tab(
        &self,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.browser.wait_for_new_tab(current_tab_id, timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.driver.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.driver
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_upload_paths_inputted(timeout_ms),
            WebMode::Session => Ok(false),
        }
    }

    pub fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.driver.handle_alert(accept, prompt_text, timeout_ms)
    }

    pub fn alert_text(&self) -> OpenPageResult<Option<String>> {
        self.driver.alert_text()
    }

    pub fn set_next_alert_action(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.driver.set_next_alert_action(accept, prompt_text)
    }

    pub fn wait_for_alert_closed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.driver.wait_for_alert_closed(timeout_ms)
    }

    pub fn wait_for_url_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.url()?;
            Ok(value.as_ref().is_some_and(|value| {
                if exclude {
                    !value.contains(text)
                } else {
                    value.contains(text)
                }
            }))
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
            Ok(value.as_ref().is_some_and(|value| {
                if exclude {
                    !value.contains(text)
                } else {
                    value.contains(text)
                }
            }))
        })
    }

    pub fn wait_for_load_start(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_load_start(timeout_ms),
            WebMode::Session => Ok(false),
        }
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_doc_loaded(timeout_ms),
            WebMode::Session => Ok(true),
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
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .wait_for_elements_loaded(locators, any_one, timeout_ms),
            WebMode::Session => {
                let locators = parse_locator_batch_input(locators)?;
                let timeout = Duration::from_millis(timeout_ms.max(1));
                let deadline = Instant::now() + timeout;
                loop {
                    let mut matched = 0usize;
                    for locator in &locators {
                        if !self.session.find_all(locator)?.is_empty() {
                            matched += 1;
                        }
                    }
                    if (!any_one && matched == locators.len()) || (any_one && matched > 0) {
                        return Ok(true);
                    }
                    if Instant::now() >= deadline {
                        return wait_timeout_result(
                            "WebPage::wait_for_elements_loaded()",
                            timeout_ms,
                        );
                    }
                    sleep(Duration::from_millis(50));
                }
            }
        }
    }

    pub fn wait_for_ele_displayed<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_displayed(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    pub fn wait_for_ele_hidden<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_hidden(target, remaining),
            |page, locator, remaining| {
                page.session_wait_until(remaining, || Ok(page.session.find(locator).is_err()))
            },
        )
    }

    pub fn wait_for_ele_enabled<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_enabled(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    pub fn wait_for_ele_deleted<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_deleted(target, remaining),
            |page, locator, remaining| {
                if page.session.find(locator).is_err() {
                    return Ok(locator.starts_with("xpath:"));
                }
                page.session_wait_until(remaining, || Ok(page.session.find(locator).is_err()))
            },
        )
    }

    pub fn wait_for_ele_clickable<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_page_element_target(
            target,
            timeout_ms,
            |driver, target, remaining| driver.wait_for_ele_clickable(target, remaining),
            |page, locator, remaining| {
                page.session_wait_for_element(locator, remaining, |ele| {
                    Ok(ele.attr("disabled")?.is_none())
                })
            },
        )
    }

    fn wait_for_page_element_target<'a, L, D, S>(
        &self,
        target: L,
        timeout_ms: u64,
        driver_wait: D,
        session_wait: S,
    ) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
        D: FnOnce(&Page, PageElementTarget<'a>, u64) -> OpenPageResult<bool>,
        S: FnOnce(&Self, &str, u64) -> OpenPageResult<bool>,
    {
        let target = target.into();
        match self.mode()? {
            WebMode::Driver => driver_wait(&self.driver, target, timeout_ms),
            WebMode::Session => {
                let locator = self.session_wait_target_locator(target)?;
                session_wait(self, locator.as_str(), timeout_ms)
            }
        }
    }

    fn session_wait_target_locator<'a>(
        &self,
        target: PageElementTarget<'a>,
    ) -> OpenPageResult<String> {
        match target {
            PageElementTarget::Locator(locator) => {
                Ok(Locator::from_input(locator)?.raw().to_string())
            }
            PageElementTarget::SessionElement(element) => {
                Ok(format!("xpath:{}", element.xpath()?))
            }
            PageElementTarget::OwnedSessionElement(element) => {
                Ok(format!("xpath:{}", element.xpath()?))
            }
            PageElementTarget::WebElement(element) => match element {
                WebElement::Session(element) => Ok(format!("xpath:{}", element.xpath()?)),
                WebElement::Browser(_) | WebElement::Mix { .. } => Err(OpenPageError::UnsupportedOperation(
                    "browser-backed element object is not supported for session mode wait_for_ele_*()"
                        .to_string(),
                )),
            },
            PageElementTarget::OwnedWebElement(element) => match element {
                WebElement::Session(element) => Ok(format!("xpath:{}", element.xpath()?)),
                WebElement::Browser(_) | WebElement::Mix { .. } => Err(OpenPageError::UnsupportedOperation(
                    "browser-backed element object is not supported for session mode wait_for_ele_*()"
                        .to_string(),
                )),
            },
            PageElementTarget::Element(_) => Err(OpenPageError::UnsupportedOperation(
                "browser-backed element object is not supported for session mode wait_for_ele_*()"
                    .to_string(),
            )),
            PageElementTarget::OwnedElement(_) => Err(OpenPageError::UnsupportedOperation(
                "browser-backed element object is not supported for session mode wait_for_ele_*()"
                    .to_string(),
            )),
        }
    }

    fn session_wait_for_element<F>(
        &self,
        locator: &str,
        timeout_ms: u64,
        check: F,
    ) -> OpenPageResult<bool>
    where
        F: Fn(&SessionElement) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.session.find(locator) {
                Ok(ele) => return check(&ele),
                Err(_) => {
                    sleep(Duration::from_millis(50));
                    if Instant::now() >= deadline {
                        return wait_timeout_result(
                            "WebPage::session_wait_for_element()",
                            timeout_ms,
                        );
                    }
                }
            }
        }
    }

    fn session_wait_until<F>(&self, timeout_ms: u64, check: F) -> OpenPageResult<bool>
    where
        F: Fn() -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if check()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("WebPage::session_wait_until()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for<'a, L>(&self, locator: L, timeout_ms: u64) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.find(locator.raw()) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            locator.raw(),
                            &err.to_string(),
                        )));
                    }
                    sleep(Duration::from_millis(100));
                }
            }
        }
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

    pub fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .active_element()
                .map(|element| element.map(|element| self.with_driver_element(element))),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("active_element()"),
            )),
        }
    }

    pub fn remove_element<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.remove_element(locator),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("remove_element()"),
            )),
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
    ) -> OpenPageResult<WebElement>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element_html(html, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element_html()"),
            )),
        }
    }

    pub fn add_element<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element(content, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element()"),
            )),
        }
    }

    pub fn add_ele<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<WebElement>
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
    ) -> OpenPageResult<WebElement>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
        H: Into<PageElementInfo>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .add_element_info(info, insert_to, before)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_element_info()"),
            )),
        }
    }

    pub fn main_frame_id(&self) -> OpenPageResult<String> {
        match self.mode()? {
            WebMode::Driver => self.driver.main_frame_id(),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("main_frame_id()"),
            )),
        }
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame(target)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame()"),
            )),
        }
    }

    pub fn get_frame_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_with_timeout(target, timeout_ms)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_by_index(index)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index()"),
            )),
        }
    }

    pub fn get_frame_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_by_index_with_timeout(index, timeout_ms)
                .map(|frame| self.with_driver_frame(frame)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele(target)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele()"),
            )),
        }
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_with_timeout(target, timeout_ms)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_ele_by_index<I>(&self, index: I) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_by_index(index)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_by_index()"),
            )),
        }
    }

    pub fn get_frame_ele_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<WebElement>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_by_index_with_timeout(index, timeout_ms)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_ele_by_index_with_timeout()"),
            )),
        }
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frames(locator).map(|frames| {
                frames
                    .into_iter()
                    .map(|frame| self.with_driver_frame(frame))
                    .collect()
            }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frames()"),
            )),
        }
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frames_with_timeout(locator, timeout_ms)
                .map(|frames| {
                    frames
                        .into_iter()
                        .map(|frame| self.with_driver_frame(frame))
                        .collect()
                }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frames_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frame_eles(locator).map(|elements| {
                elements
                    .into_iter()
                    .map(|element| self.with_driver_element(element))
                    .collect()
            }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_eles()"),
            )),
        }
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_eles_with_timeout(locator, timeout_ms)
                .map(|elements| {
                    elements
                        .into_iter()
                        .map(|element| self.with_driver_element(element))
                        .collect()
                }),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_eles_with_timeout()"),
            )),
        }
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frame(target),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_context()"),
            )),
        }
    }

    pub fn get_frame_context_by_index<I>(&self, index: I) -> OpenPageResult<WebFrame>
    where
        I: FrameIndexInput,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frame_by_index(index),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_context_by_index()"),
            )),
        }
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.get_frames(locator),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_contexts()"),
            )),
        }
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js()"),
            ));
        }
        self.driver.run_js(expression)
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_args()"),
            ));
        }
        self.driver.run_js_with_args(script, args, as_expr)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_with_options()"),
            ));
        }
        self.driver
            .run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded()"),
            ));
        }
        self.driver.run_js_loaded(script)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded_with_args()"),
            ));
        }
        self.driver.run_js_loaded_with_args(script, args, as_expr)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_js_loaded_with_options()"),
            ));
        }
        self.driver
            .run_js_loaded_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js()"),
            ));
        }
        self.driver.run_async_js(script)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_args()"),
            ));
        }
        self.driver.run_async_js_with_args(script, args, as_expr)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_async_js_with_options()"),
            ));
        }
        self.driver
            .run_async_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("stop_loading()"),
            ));
        }
        self.driver.stop_loading()
    }

    pub fn execute_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("execute_cdp()"),
            ));
        }
        self.driver.execute_cdp(command)
    }

    pub fn run_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_cdp()"),
            ));
        }
        self.driver.run_cdp(command)
    }

    pub fn execute_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("execute_cdp_loaded()"),
            ));
        }
        self.driver.execute_cdp_loaded(command)
    }

    pub fn run_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("run_cdp_loaded()"),
            ));
        }
        self.driver.run_cdp_loaded(command)
    }

    pub fn set_user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_user_agent(user_agent, platform),
            WebMode::Session => self.session.set_user_agent(Some(user_agent.to_string())),
        }
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        self.driver.activate()
    }

    pub fn set_headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.set_headers(headers),
            WebMode::Session => self.session.set_headers(headers),
        }
    }

    pub fn set_session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.driver.set_session_storage(item, value)
    }

    pub fn session_storage(&self, item: Option<&str>) -> OpenPageResult<Option<Value>> {
        if self.mode()? != WebMode::Driver {
            return Ok(None);
        }
        self.driver.session_storage(item).map(Some)
    }

    pub fn set_local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        self.driver.set_local_storage(item, value)
    }

    pub fn local_storage(&self, item: Option<&str>) -> OpenPageResult<Option<Value>> {
        if self.mode()? != WebMode::Driver {
            return Ok(None);
        }
        self.driver.local_storage(item).map(Some)
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("add_init_js()"),
            ));
        }
        self.driver.add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("remove_init_js()"),
            ));
        }
        self.driver.remove_init_js(script_id)
    }

    pub fn clear_cache(
        &self,
        session_storage: bool,
        local_storage: bool,
        cache: bool,
        cookies: bool,
    ) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear_cache()"),
            ));
        }
        self.driver
            .clear_cache(session_storage, local_storage, cache, cookies)
    }

    pub fn set_permission(
        &self,
        name: &str,
        setting: &str,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
    ) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_permission()"),
            ));
        }
        self.driver
            .set_permission(name, setting, origin, embedded_origin)
    }

    pub fn reset_permissions(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("reset_permissions()"),
            ));
        }
        self.driver.reset_permissions()
    }

    pub fn clipboard_read_text(&self) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clipboard_read_text()"),
            ));
        }
        self.driver.clipboard_read_text()
    }

    pub fn clipboard_write_text(&self, text: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clipboard_write_text()"),
            ));
        }
        self.driver.clipboard_write_text(text)
    }

    pub fn cookies_to_session(&self, copy_user_agent: bool) -> OpenPageResult<()> {
        let url = self.driver.url()?;
        if url.is_empty() {
            return Ok(());
        }
        if let Some(cookie_header) = self.driver.cookie_header()? {
            self.session.set_cookie_header(&url, &cookie_header)?;
        }
        if copy_user_agent {
            self.session
                .set_user_agent(Some(self.driver.user_agent()?))?;
        }
        Ok(())
    }

    pub fn cookies_to_browser(&self) -> OpenPageResult<()> {
        let Some(url) = self.session.url()? else {
            return Ok(());
        };

        let driver_url = self.driver.url()?;
        if !driver_url.starts_with("http://") && !driver_url.starts_with("https://") {
            self.driver.goto(&url)?;
        }

        if let Some(cookie_header) = self.session.cookie_header(&url)? {
            self.driver.set_cookie_header(&url, &cookie_header)?;
        }
        Ok(())
    }

    pub fn close(&self, others: bool, session: bool) -> OpenPageResult<()> {
        if others {
            self.browser.close_tabs(&self.driver, true)?;
            let target_id = self.driver.target_id();
            let deadline = Instant::now() + Duration::from_millis(1_000);
            loop {
                let tab_ids = self.tab_ids()?;
                if tab_ids.len() == 1 && tab_ids[0] == target_id {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(crate::settings::timeout_error("WebPage::close()", 1_000));
                }
                sleep(Duration::from_millis(20));
            }
        }

        self.browser.close_tabs(&self.driver, false)?;
        if session {
            self.session.close()?;
        }
        Ok(())
    }

    pub fn close_with_options(&self, others: bool, session: bool) -> OpenPageResult<()> {
        self.close(others, session)
    }

    pub fn close_driver(self) -> OpenPageResult<SessionPage> {
        self.change_mode(Some(WebMode::Session), true, true)?;
        let WebPage {
            driver, session, ..
        } = self;
        let _ = driver
            .execute_cdp(chromiumoxide::cdp::browser_protocol::browser::CloseParams::default());
        Ok(session)
    }

    pub fn close_session(self) -> OpenPageResult<Page> {
        self.change_mode(Some(WebMode::Driver), true, true)?;
        let WebPage {
            driver, session, ..
        } = self;
        session.close()?;
        Ok(driver)
    }

    pub fn quit(&self) -> OpenPageResult<()> {
        self.browser.close()
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        let driver = self.driver.reconnect(wait_ms)?;
        let browser = driver.browser().cloned().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(driver_mode_only_message("reconnect()"))
        })?;
        Ok(Self {
            browser,
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        })
    }

    pub fn with_target(&self, target_id: &str) -> OpenPageResult<Self> {
        let driver = self.browser.get_page(target_id)?;
        Ok(Self {
            browser: self.browser.clone(),
            driver,
            session: self.session.clone(),
            mode: Arc::clone(&self.mode),
        })
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedWebPage> {
        let target_id = self.driver.target_id();
        Ok(DisconnectedWebPage {
            browser: self.browser,
            session: self.session,
            mode: self.mode,
            target_id,
        })
    }

    pub fn set_auto_alert_action(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.driver.set_auto_alert_action(accept, prompt_text)
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
                return wait_timeout_result("WebPage::wait_for_change()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn set_mode(&self, mode: WebMode) -> OpenPageResult<()> {
        let mut current = self.mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "webpage mode",
                "网页模式",
            ))
        })?;
        *current = mode;
        Ok(())
    }
}

impl<'a> WebElementClicker<'a> {
    fn browser_clicker(&self) -> OpenPageResult<ElementClicker<'a>> {
        match self.element {
            WebElement::Browser(element) | WebElement::Mix { element, .. } => Ok(element.clicker()),
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clicker()"),
            )),
        }
    }

    pub fn left(&self) -> OpenPageResult<bool> {
        self.left_with_options(Some(false), None, true)
    }

    pub fn left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.element
            .click_left_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn right(&self) -> OpenPageResult<()> {
        self.element.click_right()
    }

    pub fn middle(&self, get_tab: bool) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser_clicker()?
            .middle(get_tab)
            .map(|page| page.map(|page| self.element.wrap_page(page)))
    }

    pub fn multi(&self, times: u32) -> OpenPageResult<()> {
        self.element.click_multi(times)
    }

    pub fn at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        self.element.click_at(offset_x, offset_y, button, count)
    }

    pub fn to_upload<'b, F>(
        &self,
        files: F,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        F: Into<UploadFilesInput<'b>>,
    {
        self.browser_clicker()?.to_upload(files, timeout_ms, by_js)
    }

    pub fn to_download(
        &self,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.browser_clicker()?.to_download(
            save_path,
            rename,
            suffix,
            suffix_specified,
            timeout_ms,
            by_js,
            new_tab,
        )
    }

    pub fn for_new_tab(
        &self,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>> {
        self.browser_clicker()?
            .for_new_tab(timeout_ms, by_js)
            .map(|page| page.map(|page| self.element.wrap_page(page)))
    }
}

impl WebElementScroller<'_> {
    pub fn to_top(&self) -> OpenPageResult<()> {
        self.element.scroll_to_top()
    }

    pub fn to_bottom(&self) -> OpenPageResult<()> {
        self.element.scroll_to_bottom()
    }

    pub fn to_half(&self) -> OpenPageResult<()> {
        self.element.scroll_to_half()
    }

    pub fn to_rightmost(&self) -> OpenPageResult<()> {
        self.element.scroll_to_rightmost()
    }

    pub fn to_leftmost(&self) -> OpenPageResult<()> {
        self.element.scroll_to_leftmost()
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.element.scroll_to_location(x, y)
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<()> {
        self.element.scroll_up(pixels)
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<()> {
        self.element.scroll_down(pixels)
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<()> {
        self.element.scroll_left(pixels)
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<()> {
        self.element.scroll_right(pixels)
    }

    pub fn to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        self.element.scroll_to_see(center)
    }

    pub fn to_center(&self) -> OpenPageResult<()> {
        self.element.scroll_to_center()
    }
}

impl WebElementSetter<'_> {
    pub fn inner_html(&self, html: &str) -> OpenPageResult<()> {
        self.element
            .set_property("innerHTML", &Value::String(html.to_string()))
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.element.set_property(name, value)
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.element.set_style(name, value)
    }

    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.element.set_attr(name, value)
    }

    pub fn value(&self, value: &str) -> OpenPageResult<()> {
        self.element
            .set_property("value", &Value::String(value.to_string()))
    }
}

impl WebElementSelector<'_> {
    pub fn by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.select_by_text(text)
    }

    pub fn by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.select_by_text_with_timeout(text, timeout_ms)
    }

    pub fn by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.select_by_value(value)
    }

    pub fn by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.select_by_value_with_timeout(value, timeout_ms)
    }

    pub fn by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        self.element.select_by_index(index)
    }

    pub fn by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        self.element.select_by_index_with_timeout(index, timeout_ms)
    }

    pub fn by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        self.element.select_by_indices(indices)
    }

    pub fn by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        self.element
            .select_by_indices_with_timeout(indices, timeout_ms)
    }

    pub fn by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.element.select_by_locator(locator)
    }

    pub fn by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.element
            .select_by_locator_with_timeout(locator, timeout_ms)
    }

    pub fn by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        self.element.select_by_option(option)
    }

    pub fn by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        self.element.select_by_options(options)
    }

    pub fn cancel_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.cancel_by_text(text)
    }

    pub fn cancel_by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.cancel_by_text_with_timeout(text, timeout_ms)
    }

    pub fn cancel_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.cancel_by_value(value)
    }

    pub fn cancel_by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.element.cancel_by_value_with_timeout(value, timeout_ms)
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        self.element.cancel_by_index(index)
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        self.element.cancel_by_index_with_timeout(index, timeout_ms)
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        self.element.cancel_by_indices(indices)
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        self.element
            .cancel_by_indices_with_timeout(indices, timeout_ms)
    }

    pub fn cancel_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.element.cancel_by_locator(locator)
    }

    pub fn cancel_by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.element
            .cancel_by_locator_with_timeout(locator, timeout_ms)
    }

    pub fn cancel_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        self.element.cancel_by_option(option)
    }

    pub fn cancel_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        self.element.cancel_by_options(options)
    }

    pub fn all(&self) -> OpenPageResult<()> {
        self.element.select_all()
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.element.clear_selected()
    }

    pub fn invert(&self) -> OpenPageResult<()> {
        self.element.invert_selected()
    }

    pub fn is_multi(&self) -> OpenPageResult<bool> {
        self.element.is_multi_select()
    }

    pub fn options(&self) -> OpenPageResult<Vec<WebElement>> {
        self.element.option_elements()
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<WebElement>> {
        self.element.selected_option_element()
    }

    pub fn selected_options(&self) -> OpenPageResult<Vec<WebElement>> {
        self.element.selected_option_elements()
    }
}

impl WebElementStates<'_> {
    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.element.is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.element.is_whole_in_viewport()
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.element.is_alive()
    }

    pub fn is_checked(&self) -> OpenPageResult<bool> {
        self.element.is_checked()
    }

    pub fn is_selected(&self) -> OpenPageResult<bool> {
        self.element.is_selected()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.element.is_enabled()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.element.is_displayed()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.element.is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.element.is_clickable()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.element.has_rect()
    }
}

impl WebElementRect<'_> {
    pub fn corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        self.element.rect_corners()
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        self.element.rect_viewport_corners()
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_location()
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_viewport_location()
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_screen_location()
    }

    pub fn midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_midpoint()
    }

    pub fn viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_viewport_midpoint()
    }

    pub fn click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_click_point()
    }

    pub fn viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_viewport_click_point()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_size()
    }

    pub fn screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_screen_midpoint()
    }

    pub fn screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_screen_click_point()
    }

    pub fn scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_scroll_position()
    }
}

impl WebElementWait<'_> {
    pub fn displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_displayed(timeout_ms)
    }

    pub fn hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_hidden(timeout_ms)
    }

    pub fn enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_enabled(timeout_ms)
    }

    pub fn disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_disabled(timeout_ms)
    }

    pub fn deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_deleted(timeout_ms)
    }

    pub fn clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_clickable(timeout_ms)
    }

    pub fn has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_has_rect(timeout_ms)
    }

    pub fn covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_covered(timeout_ms)
    }

    pub fn not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_not_covered(timeout_ms)
    }

    pub fn disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.element.wait_until_stop_moving(timeout_ms)
    }
}

impl WebPageScroller<'_> {
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

impl WebPageSetter<'_> {
    pub fn window(&self) -> WebPageWindowSetter<'_> {
        WebPageWindowSetter { page: self.page }
    }

    pub fn cookie(&self) -> WebPageCookieSetter<'_> {
        WebPageCookieSetter { page: self.page }
    }

    pub fn load_mode(&self) -> WebPageLoadModeSetter<'_> {
        WebPageLoadModeSetter { page: self.page }
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

    pub fn encoding<E>(&self, encoding: E) -> OpenPageResult<()>
    where
        E: Into<SessionEncodingInput>,
    {
        self.page.set_encoding(encoding)
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
        self.page.set_current_tab_download_path(path)
    }

    pub fn download_file_exists(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()> {
        self.page.set_current_tab_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.page.when_current_tab_download_file_exists(mode)
    }

    pub fn download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page
            .set_current_tab_download_file_name(rename, suffix, suffix.is_some())
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

impl WebPageCookieSetter<'_> {
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

impl WebPageWindowSetter<'_> {
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

impl WebPageLoadModeSetter<'_> {
    pub fn normal(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(crate::browser::LoadMode::Normal)
    }

    pub fn eager(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(crate::browser::LoadMode::Eager)
    }

    pub fn none(&self) -> OpenPageResult<()> {
        self.page.set_load_mode(crate::browser::LoadMode::None)
    }
}

fn webpage_timeout_seconds_to_millis(seconds: f64) -> OpenPageResult<u64> {
    if !seconds.is_finite() || seconds.is_sign_negative() {
        return Err(OpenPageError::UnsupportedOperation(
            timeout_must_be_non_negative_message(seconds),
        ));
    }
    Ok((seconds * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests;
