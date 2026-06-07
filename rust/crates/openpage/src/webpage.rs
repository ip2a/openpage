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
    Actions, ActionsInput, Frame, FrameRect, FrameScroller, FrameSetter, FrameStates, FrameWait,
    Page, PageElementContent, PageElementInfo, PageElementTarget, PageFrameTarget, PageSaveContent,
};
use crate::screencast::Screencast;
use crate::session::{
    CookieEntry, CookieInput, SessionDownload, SessionElement, SessionOptions, SessionPage,
    SessionXPathResult,
};
use crate::settings::{
    component_state_lock_poisoned_message, driver_mode_only_message,
    timeout_must_be_non_negative_message, wait_timeout_result,
};
use crate::shadow_root::ShadowRoot;

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
            _ => Err(OpenPageError::BrowserOperation(format!(
                "mode must be 'd' or 's', got {mode}"
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
    Session(SessionElement),
}

pub enum WebFrame {
    Browser(Frame),
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

pub struct WebPageWindowSetter<'a> {
    page: &'a WebPage,
}

pub struct WebPageLoadModeSetter<'a> {
    page: &'a WebPage,
}

pub enum WebSelectOptionInput<'a> {
    Single(&'a WebElement),
    Many(Vec<&'a WebElement>),
}

impl<'a> From<&'a WebElement> for WebSelectOptionInput<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::Single(value)
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

impl WebFrame {
    pub fn scroll(&self) -> FrameScroller<'_> {
        match self {
            Self::Browser(frame) => frame.scroll(),
        }
    }

    pub fn set(&self) -> FrameSetter<'_> {
        match self {
            Self::Browser(frame) => frame.set(),
        }
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        match self {
            Self::Browser(frame) => frame.set_cookies(cookies),
        }
    }

    pub fn states(&self) -> FrameStates<'_> {
        match self {
            Self::Browser(frame) => frame.states(),
        }
    }

    pub fn wait(&self) -> FrameWait<'_> {
        match self {
            Self::Browser(frame) => frame.wait(),
        }
    }

    pub fn rect(&self) -> FrameRect<'_> {
        match self {
            Self::Browser(frame) => frame.rect(),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Browser(frame) => frame.id(),
        }
    }

    pub fn frame_element(&self) -> &Element {
        match self {
            Self::Browser(frame) => frame.frame_element(),
        }
    }

    pub fn frame_ele(&self) -> &Element {
        self.frame_element()
    }

    pub fn owner(&self) -> &crate::page::Page {
        match self {
            Self::Browser(frame) => frame.owner(),
        }
    }

    pub fn tab(&self) -> &crate::page::Page {
        match self {
            Self::Browser(frame) => frame.tab(),
        }
    }

    pub fn tab_id(&self) -> String {
        match self {
            Self::Browser(frame) => frame.tab_id(),
        }
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.set_none_element_value(value, on_off),
        }
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.set_raise_when_ele_not_found(on_off),
        }
    }

    pub fn name(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.name(),
        }
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.tag(),
        }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.link(),
        }
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        match self {
            Self::Browser(frame) => frame.attrs(),
        }
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.attr(name),
        }
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        match self {
            Self::Browser(frame) => frame.property(name),
        }
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.style(name, pseudo),
        }
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.css_path(),
        }
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.xpath(),
        }
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        match self {
            Self::Browser(frame) => frame.child_count(),
        }
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        match self {
            Self::Browser(frame) => frame.sr(),
        }
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        match self {
            Self::Browser(frame) => frame.shadow_root(),
        }
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.url(),
        }
    }

    pub fn parent_id(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.parent_id(),
        }
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.title(),
        }
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.download_path(),
        }
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.download(url),
        }
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.download_to(url, path),
        }
    }

    pub fn html(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.html(),
        }
    }

    pub fn inner_html(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.inner_html(),
        }
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        match self {
            Self::Browser(frame) => frame.run_js(expression),
        }
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        match self {
            Self::Browser(frame) => frame.run_js_with_args(script, args, as_expr),
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
            Self::Browser(frame) => frame.run_js_with_options(script, args, as_expr, timeout_ms),
        }
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        match self {
            Self::Browser(frame) => frame.run_js_loaded(script),
        }
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        match self {
            Self::Browser(frame) => frame.run_js_loaded_with_args(script, args, as_expr),
        }
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        match self {
            Self::Browser(frame) => {
                frame.run_js_loaded_with_options(script, args, as_expr, timeout_ms)
            }
        }
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.run_async_js(script),
        }
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.run_async_js_with_args(script, args, as_expr),
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
            Self::Browser(frame) => {
                frame.run_async_js_with_options(script, args, as_expr, timeout_ms)
            }
        }
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.add_init_js(script),
        }
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.remove_init_js(script_id),
        }
    }

    pub fn refresh(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.refresh(),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.remove_attr(name),
        }
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.set_attr(name, value),
        }
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.set_property(name, value),
        }
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.set_style(name, value),
        }
    }

    pub fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        match self {
            Self::Browser(frame) => frame
                .active_element()
                .map(|element| element.map(WebElement::Browser)),
        }
    }

    pub fn active_ele(&self) -> OpenPageResult<Option<WebElement>> {
        self.active_element()
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self {
            Self::Browser(frame) => frame
                .ele(locator.raw())
                .map(|element| element.map(WebElement::Browser)),
        }
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(frame) => frame.find(locator).map(WebElement::Browser),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(frame) => frame
                .find_all(locator)
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
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
            Self::Browser(frame) => frame
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item.elements.into_iter().map(WebElement::Browser).collect(),
                        })
                        .collect()
                }),
        }
    }

    pub fn parent(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(frame) => frame.parent().map(WebElement::Browser),
        }
    }

    pub fn prev(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(frame) => frame.prev().map(WebElement::Browser),
        }
    }

    pub fn next(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(frame) => frame.next().map(WebElement::Browser),
        }
    }

    pub fn before(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(frame) => frame.before().map(WebElement::Browser),
        }
    }

    pub fn after(&self) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(frame) => frame.after().map(WebElement::Browser),
        }
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<WebElement>> {
        match self {
            Self::Browser(frame) => frame
                .prevs()
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
        }
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<WebElement>> {
        match self {
            Self::Browser(frame) => frame
                .nexts()
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
        }
    }

    pub fn befores(&self) -> OpenPageResult<Vec<WebElement>> {
        match self {
            Self::Browser(frame) => frame
                .befores()
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
        }
    }

    pub fn afters(&self) -> OpenPageResult<Vec<WebElement>> {
        match self {
            Self::Browser(frame) => frame
                .afters()
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
        }
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        match self {
            Self::Browser(frame) => frame.screenshot_bytes(scroll_to_center, timeout_ms),
        }
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match self {
            Self::Browser(frame) => frame.screenshot_base64(scroll_to_center, timeout_ms),
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
            Self::Browser(frame) => frame.get_screenshot(path, name, scroll_to_center, timeout_ms),
        }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_top(),
        }
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_bottom(),
        }
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_half(),
        }
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_rightmost(),
        }
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_leftmost(),
        }
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_to_location(x, y),
        }
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_up(pixels),
        }
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_down(pixels),
        }
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_left(pixels),
        }
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(frame) => frame.scroll_right(pixels),
        }
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        match self {
            Self::Browser(frame) => frame.scroll_position(),
        }
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(frame) => frame.location(),
        }
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(frame) => frame.viewport_location(),
        }
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(frame) => frame.screen_location(),
        }
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(frame) => frame.size(),
        }
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(frame) => frame.viewport_size(),
        }
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        match self {
            Self::Browser(frame) => frame.corners(),
        }
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        match self {
            Self::Browser(frame) => frame.viewport_corners(),
        }
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(frame) => frame.ready_state(),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_loading(),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_alive(),
        }
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_displayed(),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_enabled(),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.has_rect(),
        }
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_in_viewport(),
        }
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_whole_in_viewport(),
        }
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_covered(),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.is_clickable(),
        }
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.has_alert(),
        }
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_for_doc_loaded(timeout_ms),
        }
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_displayed(timeout_ms),
        }
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_hidden(timeout_ms),
        }
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_enabled(timeout_ms),
        }
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_disabled(timeout_ms),
        }
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_deleted(timeout_ms),
        }
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_clickable(timeout_ms),
        }
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_has_rect(timeout_ms),
        }
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_covered(timeout_ms),
        }
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_not_covered(timeout_ms),
        }
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_disabled_or_deleted(timeout_ms),
        }
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(frame) => frame.wait_until_stop_moving(timeout_ms),
        }
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(frame) => frame.snapshot_root(),
        }
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(frame) => frame.snapshot_find(locator),
        }
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Browser(frame) => frame.snapshot_find_all(locator),
        }
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(frame) => frame.snapshot_find_by(by, value),
        }
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Browser(frame) => frame.snapshot_find_all_by(by, value),
        }
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        match self {
            Self::Browser(frame) => frame.snapshot_query_xpath(expression),
        }
    }

    pub fn listener(&self) -> Listener {
        match self {
            Self::Browser(frame) => frame.listener(),
        }
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn console(&self) -> Console {
        match self {
            Self::Browser(frame) => frame.console(),
        }
    }
}

impl WebElement {
    pub(crate) fn none_element_runtime_config_handle(
        &self,
    ) -> Option<&ElementsOneRuntimeConfigHandle> {
        match self {
            Self::Browser(element) => Some(element.none_element_runtime_config_handle()),
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
            Self::Browser(element) => element.tag(),
            Self::Session(element) => element.tag(),
        }
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.text(),
            Self::Session(element) => element.text(),
        }
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.html(),
            Self::Session(element) => element.html(),
        }
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.inner_html(),
            Self::Session(element) => element.inner_html(),
        }
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(element) => element.snapshot_root(),
            Self::Session(element) => Ok(element.clone()),
        }
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(element) => element.snapshot_find(locator),
            Self::Session(element) => element.find(locator),
        }
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Browser(element) => element.snapshot_find_all(locator),
            Self::Session(element) => element.find_all(locator),
        }
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        match self {
            Self::Browser(element) => element.snapshot_find_by(by, value),
            Self::Session(element) => element.find_by(by, value),
        }
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Browser(element) => element.snapshot_find_all_by(by, value),
            Self::Session(element) => element.find_all_by(by, value),
        }
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        match self {
            Self::Browser(element) => element.snapshot_query_xpath(expression),
            Self::Session(element) => element.query_xpath(expression),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self {
            Self::Browser(element) => element
                .ele(locator.raw())
                .map(|value| value.map(Self::Browser)),
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
            Self::Browser(element) => element.get_frame(target).map(WebFrame::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame()"),
            )),
        }
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<WebFrame> {
        match self {
            Self::Browser(element) => element.get_frame_by_index(index).map(WebFrame::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_frame_by_index()"),
            )),
        }
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.find(locator).map(Self::Browser),
            Self::Session(element) => element.find(locator).map(Self::Session),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) => element
                .find_all(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
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
            Self::Browser(element) => element
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item.elements.into_iter().map(WebElement::Browser).collect(),
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
            Self::Browser(element) => element.attrs(),
            Self::Session(element) => element.attrs(),
        }
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.attr(name),
            Self::Session(element) => element.attr(name),
        }
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        match self {
            Self::Browser(element) => element.property(name),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("property()"),
            )),
        }
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.raw_text(),
            Self::Session(element) => element.raw_text(),
        }
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.value(),
            Self::Session(element) => element.attr("value"),
        }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.link(),
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
            Self::Browser(element) => element.child_count(),
            Self::Session(element) => Ok(element.children()?.len()),
        }
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.css_path(),
            Self::Session(element) => element.css_path(),
        }
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.xpath(),
            Self::Session(element) => element.xpath(),
        }
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) => element.comments(),
            Self::Session(element) => element.comments(),
        }
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) => element.texts(text_node_only),
            Self::Session(element) => element.texts(text_node_only),
        }
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_displayed(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_displayed()"),
            )),
        }
    }

    pub fn is_checked(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_checked(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_checked()"),
            )),
        }
    }

    pub fn is_selected(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_selected()"),
            )),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_enabled(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_enabled()"),
            )),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_alive(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_alive()"),
            )),
        }
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_in_viewport(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_in_viewport()"),
            )),
        }
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_whole_in_viewport(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_whole_in_viewport()"),
            )),
        }
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_covered(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_covered()"),
            )),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_clickable(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("is_clickable()"),
            )),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.has_rect(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("has_rect()"),
            )),
        }
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.style(name, pseudo),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("style()"),
            )),
        }
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.pseudo_before(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("pseudo_before()"),
            )),
        }
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.pseudo_after(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("pseudo_after()"),
            )),
        }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_top(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_top()"),
            )),
        }
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_bottom(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_bottom()"),
            )),
        }
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_half(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_half()"),
            )),
        }
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_rightmost(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_rightmost()"),
            )),
        }
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_leftmost(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_leftmost()"),
            )),
        }
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_location(x, y),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_location()"),
            )),
        }
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_up(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_up()"),
            )),
        }
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_down(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_down()"),
            )),
        }
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_left(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_left()"),
            )),
        }
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_right(pixels),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_right()"),
            )),
        }
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_see(center),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("scroll_to_see()"),
            )),
        }
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.scroll_to_center(),
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
            Self::Browser(element) => element.src(timeout_ms, base64_to_bytes),
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
            Self::Browser(element) => element.save(path, name, timeout_ms, rename),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save()"),
            )),
        }
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        match self {
            Self::Browser(element) => element.shadow_root(),
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
            Self::Browser(element) => element.parent().map(Self::Browser),
            Self::Session(element) => element.parent().map(Self::Session),
        }
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) => element.parent_level(level).map(Self::Browser),
            Self::Session(element) => element.parent_level(level).map(Self::Session),
        }
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.parent_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element.child_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element
                .children_with(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
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
            Self::Browser(element) => element.prev_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element
                .prevs_with(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
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
            Self::Browser(element) => element.next_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element
                .nexts_with(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
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
            Self::Browser(element) => element.before_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element
                .befores_with(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
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
            Self::Browser(element) => element.after_with(locator, index).map(Self::Browser),
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
            Self::Browser(element) => element
                .afters_with(locator)
                .map(|elements| elements.into_iter().map(Self::Browser).collect()),
            Self::Session(element) => element
                .afters_with(locator)
                .map(|elements| elements.into_iter().map(Self::Session).collect()),
        }
    }

    pub fn over(&self) -> OpenPageResult<Option<WebElement>> {
        match self {
            Self::Browser(element) => element.over().map(|value| value.map(Self::Browser)),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("over()"),
            )),
        }
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<WebElement>> {
        match self {
            Self::Browser(element) => element
                .over_with_timeout(timeout_ms)
                .map(|value| value.map(Self::Browser)),
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
            Self::Browser(element) => element.offset(locator, x, y, timeout_ms).map(Self::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("offset()"),
            )),
        }
    }

    pub fn east(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) => element.east(locator, pixels, index).map(Self::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("east()"),
            )),
        }
    }

    pub fn south(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) => element.south(locator, pixels, index).map(Self::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("south()"),
            )),
        }
    }

    pub fn west(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) => element.west(locator, pixels, index).map(Self::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("west()"),
            )),
        }
    }

    pub fn north(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<WebElement> {
        match self {
            Self::Browser(element) => element.north(locator, pixels, index).map(Self::Browser),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("north()"),
            )),
        }
    }

    pub fn click(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click(),
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
            Self::Browser(element) => element.click_with_options(by_js, timeout_ms, wait_stop),
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
            Self::Browser(element) => element.click_at(offset_x, offset_y, button, count),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_at()"),
            )),
        }
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click_multi(times),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_multi()"),
            )),
        }
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click_left(),
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
            Self::Browser(element) => element.click_left_with_options(by_js, timeout_ms, wait_stop),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_left_with_options()"),
            )),
        }
    }

    pub fn click_middle(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click_middle(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_middle()"),
            )),
        }
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click_right(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("click_right()"),
            )),
        }
    }

    pub fn input(&self, text: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.input(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input()"),
            )),
        }
    }

    pub fn input_with_options(&self, text: &str, clear: bool, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.input_with_options(text, clear, by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input_with_options()"),
            )),
        }
    }

    pub fn input_keys_with_options(
        &self,
        values: &[String],
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.input_keys_with_options(values, clear, by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("input_keys_with_options()"),
            )),
        }
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.clear(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear()"),
            )),
        }
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.submit(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("submit()"),
            )),
        }
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.clear_with_mode(by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("clear_with_mode()"),
            )),
        }
    }

    pub fn set_file_input_files(&self, files: &[String]) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.set_file_input_files(files),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("set_file_input_files()"),
            )),
        }
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.press_key(key),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("press_key()"),
            )),
        }
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        match self {
            Self::Browser(element) => element.run_js(script),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_js() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.run_js_with_args(script, args, as_expr),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_js_with_args() is only available in driver mode".to_string(),
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
            Self::Browser(element) => {
                element.run_js_with_options(script, args, as_expr, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_js_with_options() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.run_async_js(script),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_async_js() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.run_async_js_with_args(script, args, as_expr),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_async_js_with_args() is only available in driver mode".to_string(),
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
            Self::Browser(element) => {
                element.run_async_js_with_options(script, args, as_expr, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "run_async_js_with_options() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.save_screenshot(path),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "save_screenshot() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        match self {
            Self::Browser(element) => element.screenshot_bytes(scroll_to_center, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "screenshot_bytes() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) => element.screenshot_base64(scroll_to_center, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "screenshot_base64() is only available in driver mode".to_string(),
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
            Self::Browser(element) => {
                element.get_screenshot(path, name, scroll_to_center, timeout_ms)
            }
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "get_screenshot() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.focus(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "focus() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.hover(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "hover() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.hover_with_offset(offset_x, offset_y),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "hover_with_offset() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.drag(offset_x, offset_y, duration_secs),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "drag() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn drag_to_element(&self, target: &WebElement, duration_secs: f64) -> OpenPageResult<()> {
        match (self, target) {
            (Self::Browser(element), Self::Browser(target)) => {
                element.drag_to(target, duration_secs)
            }
            (Self::Browser(_), Self::Session(_)) => Err(OpenPageError::UnsupportedOperation(
                "drag_to_element() target must be a driver element".to_string(),
            )),
            (Self::Session(_), _) => Err(OpenPageError::UnsupportedOperation(
                "drag_to_element() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.drag_to_point(x, y, duration_secs),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "drag_to_point() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.remove_attr(name),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "remove_attr() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.set_attr(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "set_attr() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.set_property(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "set_property() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.set_style(name, value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "set_style() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.set_checked(checked),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "set_checked() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.check(uncheck, by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "check() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.uncheck(by_js),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "uncheck() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn is_multi_select(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.is_multi_select(),
            Self::Session(element) => Ok(element.attr("multiple")?.is_some()),
        }
    }

    pub fn option_texts(&self) -> OpenPageResult<Vec<String>> {
        match self {
            Self::Browser(element) => element.option_texts(),
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
            Self::Browser(element) => element.selected_option(),
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
            Self::Browser(element) => element.selected_options(),
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
            Self::Browser(_) => self.find_all("css:option:checked"),
            Self::Session(_) => self.find_all("css:option[selected]"),
        }
    }

    pub fn select_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.select_by_text(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_text() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.select_by_text_with_timeout(text, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_text_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.select_by_value(value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_value() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.select_by_value_with_timeout(value, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_value_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.select_by_locator(locator),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_locator() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.select_by_locator_with_timeout(locator, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_locator_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) => element.select_by_index(index),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_index() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.select_by_index_with_timeout(index, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_index_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.select_by_indices(indices),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_indices() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.select_by_indices_with_timeout(indices, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_by_indices_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn select_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        match option.into() {
            WebSelectOptionInput::Single(option) => self.select_by_option_value(option),
            WebSelectOptionInput::Many(options) => self.select_by_options(&options),
        }
    }

    fn select_by_option_value(&self, option: &WebElement) -> OpenPageResult<bool> {
        match (self, option) {
            (Self::Browser(element), WebElement::Browser(option)) => {
                element.select_by_option(option)
            }
            (Self::Browser(_), WebElement::Session(_)) => Err(OpenPageError::UnsupportedOperation(
                "select_by_option() requires a browser-backed option element".to_string(),
            )),
            (Self::Session(_), _) => Err(OpenPageError::UnsupportedOperation(
                "select_by_option() is only available in driver mode".to_string(),
            )),
        }
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
            Self::Browser(element) => element.cancel_by_text(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_text() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.cancel_by_text_with_timeout(text, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_text_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.cancel_by_value(value),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_value() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.cancel_by_value_with_timeout(value, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_value_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match self {
            Self::Browser(element) => element.cancel_by_index(index),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_index() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.cancel_by_index_with_timeout(index, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_index_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.cancel_by_indices(indices),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_indices() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.cancel_by_indices_with_timeout(indices, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_indices_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self {
            Self::Browser(element) => element.cancel_by_locator(locator),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_locator() is only available in driver mode".to_string(),
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
            Self::Browser(element) => element.cancel_by_locator_with_timeout(locator, timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_locator_with_timeout() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn cancel_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<WebSelectOptionInput<'a>>,
    {
        match option.into() {
            WebSelectOptionInput::Single(option) => self.cancel_by_option_value(option),
            WebSelectOptionInput::Many(options) => self.cancel_by_options(&options),
        }
    }

    fn cancel_by_option_value(&self, option: &WebElement) -> OpenPageResult<bool> {
        match (self, option) {
            (Self::Browser(element), WebElement::Browser(option)) => {
                element.cancel_by_option(option)
            }
            (Self::Browser(_), WebElement::Session(_)) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_option() requires a browser-backed option element".to_string(),
            )),
            (Self::Session(_), _) => Err(OpenPageError::UnsupportedOperation(
                "cancel_by_option() is only available in driver mode".to_string(),
            )),
        }
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
            Self::Browser(element) => element.select_all(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "select_all() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn invert_selected(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.invert_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "invert_selected() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn clear_selected(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.clear_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "clear_selected() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self {
            Self::Browser(element) => element.rect_corners(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_corners() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self {
            Self::Browser(element) => element.rect_viewport_corners(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_viewport_corners() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_location() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_viewport_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_viewport_location() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_screen_location(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_screen_location() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_midpoint() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_viewport_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_viewport_midpoint() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_click_point(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_click_point() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_viewport_click_point(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_viewport_click_point() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_size(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_size() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_screen_midpoint(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_screen_midpoint() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_screen_click_point(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_screen_click_point() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn rect_scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self {
            Self::Browser(element) => element.rect_scroll_position(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "rect_scroll_position() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_displayed(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_displayed() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_hidden(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_hidden() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_enabled(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_enabled() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_disabled(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_disabled() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_deleted(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_deleted() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_clickable(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_clickable() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_has_rect(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_has_rect() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_covered(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_covered() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_not_covered(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_not_covered() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_disabled_or_deleted(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_disabled_or_deleted() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) => element.wait_until_stop_moving(timeout_ms),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "wait_until_stop_moving() is only available in driver mode".to_string(),
            )),
        }
    }
}

impl<'a> From<&'a WebPage> for BrowserTabSelector<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::Id(std::borrow::Cow::Owned(value.target_id()))
    }
}

impl<'a> From<&'a WebPage> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a WebPage) -> Self {
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
                "navigation_snapshot() is only available in driver mode".to_string(),
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
                "actions() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn new_actions(&self) -> OpenPageResult<Actions> {
        match self.mode()? {
            WebMode::Driver => Ok(self.driver.new_actions()),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "new_actions() is only available in driver mode".to_string(),
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
        self.browser
            .get_tab(id_or_num, title, url, tab_type, as_id)
            .map(|reference| reference.map(|reference| self.mix_tab_reference(reference)))
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

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => self.browser.download_path(),
            WebMode::Session => self.session.download_path().map(Some),
        }
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

    pub fn set_blocked_urls(&self, patterns: &[String]) -> OpenPageResult<()> {
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

    pub fn when_current_tab_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.browser
            .when_page_download_file_exists(&self.driver.target_id(), mode)
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

    pub fn set_current_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_current_tab_download_filename(rename, suffix, suffix_specified)
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
                "click_to_download() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn click_to_upload(
        &self,
        locator: &str,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .click_to_upload(locator, files, timeout_ms, by_js),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "click_to_upload() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn click_for_new_tab(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<crate::page::Page>> {
        match self.mode()? {
            WebMode::Driver => self.driver.click_for_new_tab(locator, timeout_ms, by_js),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "click_for_new_tab() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn click_middle(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<crate::page::Page>> {
        match self.mode()? {
            WebMode::Driver => self.driver.click_middle(locator, timeout_ms, get_tab),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "click_middle() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn set_upload_files(&self, files: &[String]) -> OpenPageResult<()> {
        self.driver.set_upload_files(files)
    }

    pub fn set_upload_paths(&self, files: &[String]) -> OpenPageResult<()> {
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
                        "set_timeouts(page_load/script) is only available in driver mode"
                            .to_string(),
                    ));
                }
                if let Some(base_secs) = base_secs {
                    if !base_secs.is_finite() || base_secs.is_sign_negative() {
                        return Err(OpenPageError::UnsupportedOperation(format!(
                            "set_timeouts(base) requires a finite non-negative number, got {base_secs}"
                        )));
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

    pub fn interceptor(&self) -> Interceptor {
        self.driver.interceptor()
    }

    pub fn console(&self) -> Console {
        self.driver.console()
    }

    pub fn screencast(&self) -> Screencast {
        self.driver.screencast()
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

    pub fn post(&self, url: &str) -> OpenPageResult<bool> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.post(url)
    }

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<bool> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.post_json(url, payload)
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.download(url)
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.download_to(url, path)
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

    pub fn html(&self) -> OpenPageResult<String> {
        match self.mode()? {
            WebMode::Driver => self.driver.html(),
            WebMode::Session => self.session.html(),
        }
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "save_screenshot() is only available in driver mode".to_string(),
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
                "save() is only available in driver mode".to_string(),
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
                "save_with_options() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .save_with_options(path, name, as_pdf, pdf_options)
    }

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "save_pdf() is only available in driver mode".to_string(),
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
                "screenshot_bytes() is only available in driver mode".to_string(),
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
                "screenshot_base64() is only available in driver mode".to_string(),
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
                "get_screenshot() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .get_screenshot(path, name, full_page, left_top, right_bottom)
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_position() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_position()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "viewport_size() is only available in driver mode".to_string(),
            ));
        }
        self.driver.viewport_size()
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "refresh() is only available in driver mode".to_string(),
            ));
        }
        self.driver.refresh(ignore_cache)
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "back() is only available in driver mode".to_string(),
            ));
        }
        self.driver.back(steps)
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "forward() is only available in driver mode".to_string(),
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
                "scroll_to_top() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_top()
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_to_bottom() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_bottom()
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_to_half() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_half()
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_to_rightmost() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_rightmost()
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_to_leftmost() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_leftmost()
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_to_location() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_to_location(x, y)
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_up() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_up(pixels)
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_down() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_down(pixels)
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_left() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_left(pixels)
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "scroll_right() is only available in driver mode".to_string(),
            ));
        }
        self.driver.scroll_right(pixels)
    }

    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        match self.mode()? {
            WebMode::Driver => Ok(Vec::new()),
            WebMode::Session => self.session.raw_data(),
        }
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.encoding(),
        }
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.status_code(),
        }
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        match self.mode()? {
            WebMode::Driver => self.driver.cookies(),
            WebMode::Session => self.session.cookies(),
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

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.json(),
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
            PageElementTarget::WebElement(element) => match element {
                WebElement::Session(element) => Ok(format!("xpath:{}", element.xpath()?)),
                WebElement::Browser(_) => Err(OpenPageError::UnsupportedOperation(
                    "browser-backed element object is not supported for session mode wait_for_ele_*()"
                        .to_string(),
                )),
            },
            PageElementTarget::Element(_) => Err(OpenPageError::UnsupportedOperation(
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

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.find(locator).map(WebElement::Browser),
            WebMode::Session => self.session.find(locator).map(WebElement::Session),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .ele(locator.raw())
                .map(|element| element.map(WebElement::Browser)),
            WebMode::Session => self
                .session
                .ele(locator.raw())
                .map(|element| element.map(WebElement::Session)),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .find_all(locator)
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
            WebMode::Session => self
                .session
                .find_all(locator)
                .map(|elements| elements.into_iter().map(WebElement::Session).collect()),
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
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item.elements.into_iter().map(WebElement::Browser).collect(),
                        })
                        .collect()
                }),
            WebMode::Session => self
                .session
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

    pub fn active_element(&self) -> OpenPageResult<Option<WebElement>> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .active_element()
                .map(|element| element.map(WebElement::Browser)),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "active_element() is only available in driver mode".to_string(),
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
                "remove_element() is only available in driver mode".to_string(),
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
                .map(WebElement::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "add_element_html() is only available in driver mode".to_string(),
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
                .map(WebElement::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "add_element() is only available in driver mode".to_string(),
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
                .map(WebElement::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "add_element_info() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<WebFrame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frame(target).map(WebFrame::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<WebFrame> {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frame_by_index(index).map(WebFrame::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame_by_index() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<WebElement>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.get_frame_ele(target).map(WebElement::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame_ele() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame_ele_by_index(&self, index: usize) -> OpenPageResult<WebElement> {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_ele_by_index(index)
                .map(WebElement::Browser),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame_ele_by_index() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebFrame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frames(locator)
                .map(|frames| frames.into_iter().map(WebFrame::Browser).collect()),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frames() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .get_frame_eles(locator)
                .map(|elements| elements.into_iter().map(WebElement::Browser).collect()),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame_eles() is only available in driver mode".to_string(),
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
                "get_frame_context() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn get_frame_context_by_index(&self, index: usize) -> OpenPageResult<WebFrame> {
        match self.mode()? {
            WebMode::Driver => self.get_frame_by_index(index),
            WebMode::Session => Err(OpenPageError::UnsupportedOperation(
                "get_frame_context_by_index() is only available in driver mode".to_string(),
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
                "get_frame_contexts() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find(locator),
            WebMode::Session => self.session.find(locator),
        }
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_all(locator),
            WebMode::Session => self.session.find_all(locator),
        }
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_by(by, value),
            WebMode::Session => self.session.find_by(by, value),
        }
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_all_by(by, value),
            WebMode::Session => self.session.find_all_by(by, value),
        }
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_query_xpath(expression),
            WebMode::Session => self.session.query_xpath(expression),
        }
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_root(),
            WebMode::Session => self.session.root(),
        }
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "run_js() is only available in driver mode".to_string(),
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
                "run_js_with_args() is only available in driver mode".to_string(),
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
                "run_js_with_options() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .run_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "run_js_loaded() is only available in driver mode".to_string(),
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
                "run_js_loaded_with_args() is only available in driver mode".to_string(),
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
                "run_js_loaded_with_options() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .run_js_loaded_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "run_async_js() is only available in driver mode".to_string(),
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
                "run_async_js_with_args() is only available in driver mode".to_string(),
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
                "run_async_js_with_options() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .run_async_js_with_options(script, args, as_expr, timeout_ms)
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "stop_loading() is only available in driver mode".to_string(),
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
                "execute_cdp() is only available in driver mode".to_string(),
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
                "run_cdp() is only available in driver mode".to_string(),
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
                "execute_cdp_loaded() is only available in driver mode".to_string(),
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
                "run_cdp_loaded() is only available in driver mode".to_string(),
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

    pub fn set_headers(&self, headers: &[(String, String)]) -> OpenPageResult<()> {
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
                "add_init_js() is only available in driver mode".to_string(),
            ));
        }
        self.driver.add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "remove_init_js() is only available in driver mode".to_string(),
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
                "clear_cache() is only available in driver mode".to_string(),
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
                "set_permission() is only available in driver mode".to_string(),
            ));
        }
        self.driver
            .set_permission(name, setting, origin, embedded_origin)
    }

    pub fn reset_permissions(&self) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "reset_permissions() is only available in driver mode".to_string(),
            ));
        }
        self.driver.reset_permissions()
    }

    pub fn clipboard_read_text(&self) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "clipboard_read_text() is only available in driver mode".to_string(),
            ));
        }
        self.driver.clipboard_read_text()
    }

    pub fn clipboard_write_text(&self, text: &str) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                "clipboard_write_text() is only available in driver mode".to_string(),
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
            OpenPageError::UnsupportedOperation(
                "reconnect() is only available in driver mode".to_string(),
            )
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
            WebElement::Browser(element) => Ok(element.clicker()),
            WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "clicker() is only available in driver mode".to_string(),
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

    pub fn middle(&self, get_tab: bool) -> OpenPageResult<Option<crate::page::Page>> {
        self.browser_clicker()?.middle(get_tab)
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

    pub fn to_upload(
        &self,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
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
    ) -> OpenPageResult<Option<crate::page::Page>> {
        self.browser_clicker()?.for_new_tab(timeout_ms, by_js)
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

    pub fn load_mode(&self) -> WebPageLoadModeSetter<'_> {
        WebPageLoadModeSetter { page: self.page }
    }

    pub fn blocked_urls(&self, patterns: &[String]) -> OpenPageResult<()> {
        self.page.set_blocked_urls(patterns)
    }

    pub fn headers(&self, headers: &[(String, String)]) -> OpenPageResult<()> {
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

    pub fn upload_files(&self, files: &[String]) -> OpenPageResult<()> {
        self.page.set_upload_files(files)
    }

    pub fn upload_paths(&self, files: &[String]) -> OpenPageResult<()> {
        self.page.set_upload_paths(files)
    }

    pub fn activate(&self) -> OpenPageResult<()> {
        self.page.activate()
    }

    pub fn retry_times(&self, times: usize) -> OpenPageResult<()> {
        self.page.set_retry(Some(times), None)
    }

    pub fn retry_interval(&self, interval_secs: f64) -> OpenPageResult<()> {
        self.page.set_retry(None, Some(interval_secs))
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
mod tests {
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};

    use super::{WebElement, WebFrame, WebMode, WebPage, webpage_timeout_seconds_to_millis};
    use crate::browser::{BrowserTabReference, LaunchOptions};
    use crate::element_list::ElementsListExt;
    use crate::session::snapshot_root;
    use crate::settings::scoped_test_settings;
    use crate::{
        By, DownloadFileExistsMode, Element, Frame, LocatorInput, OpenPageError, OpenPageResult,
        Page, SessionCookieParam, SessionElement, SessionOptions, SessionPage, Settings,
        ShadowRoot,
    };

    fn runtime_test_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openpage-webpage-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn launch_headless_test_webpage(
        name: &str,
        mode: WebMode,
    ) -> crate::OpenPageResult<(WebPage, PathBuf)> {
        let temp_dir = runtime_test_temp_dir(name);
        fs::create_dir_all(&temp_dir).expect("create runtime test temp dir");

        let mut options = LaunchOptions::default();
        options.headless(true);
        options.auto_port(true);
        options.new_env(true);
        options.set_tmp_path(&temp_dir);
        options.set_timeouts(Some(1.0), Some(5.0), Some(1.0));

        WebPage::new(mode, options, SessionOptions::default()).map(|page| (page, temp_dir))
    }

    fn write_test_html(path: &Path, html: &str) -> crate::OpenPageResult<()> {
        fs::write(path, html).map_err(|err| {
            crate::OpenPageError::PageOperation(format!(
                "write runtime session test html {}: {err}",
                path.display()
            ))
        })
    }

    #[test]
    fn webpage_timeout_validation_follows_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english = webpage_timeout_seconds_to_millis(f64::NAN)
            .expect_err("english timeout validation should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("timeout must be a finite non-negative number")
        ));

        Settings::set_language("cn");

        let chinese = webpage_timeout_seconds_to_millis(f64::NAN)
            .expect_err("chinese timeout validation should fail");
        assert!(matches!(
            chinese,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("timeout 必须是有限且非负的数字")
        ));
    }

    #[test]
    fn web_element_session_frame_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let element = WebElement::Session(
            snapshot_root("<html><body><iframe></iframe></body></html>")
                .expect("session snapshot root should parse"),
        );
        let english = match element.get_frame("tag:iframe") {
            Err(error) => error,
            Ok(_) => panic!("session-backed WebElement get_frame should fail"),
        };
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("get_frame() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese = match element.get_frame_by_index(1) {
            Err(error) => error,
            Ok(_) => panic!("session-backed WebElement get_frame_by_index should fail"),
        };
        assert!(matches!(
            chinese,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("get_frame_by_index() 仅在 driver 模式可用")
        ));
    }

    #[test]
    fn web_element_session_driver_only_info_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let element = WebElement::Session(
            snapshot_root("<html><body><input id='q' value='rust'></body></html>")
                .expect("session snapshot root should parse"),
        );
        let english = element
            .property("value")
            .expect_err("session-backed WebElement property should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("property() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_state = element
            .is_clickable()
            .expect_err("session-backed WebElement is_clickable should fail");
        assert!(matches!(
            chinese_state,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("is_clickable() 仅在 driver 模式可用")
        ));
        let chinese_style = element
            .style("display", None)
            .expect_err("session-backed WebElement style should fail");
        assert!(matches!(
            chinese_style,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("style() 仅在 driver 模式可用")
        ));
    }

    #[test]
    fn web_element_session_driver_only_scroll_resource_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let element = WebElement::Session(
            snapshot_root("<html><body><img id='logo' src='logo.png'></body></html>")
                .expect("session snapshot root should parse"),
        );
        let english = element
            .scroll_to_top()
            .expect_err("session-backed WebElement scroll_to_top should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("scroll_to_top() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_src = element
            .src(100, false)
            .expect_err("session-backed WebElement src should fail");
        assert!(matches!(
            chinese_src,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("src() 仅在 driver 模式可用")
        ));
        let chinese_shadow = element
            .shadow_root()
            .expect_err("session-backed WebElement shadow_root should fail");
        assert!(matches!(
            chinese_shadow,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("shadow_root() 仅在 driver 模式可用")
        ));
    }

    #[test]
    fn web_element_session_driver_only_interaction_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let element = WebElement::Session(
            snapshot_root("<html><body><button id='ok'>OK</button></body></html>")
                .expect("session snapshot root should parse"),
        );
        let english = match element.offset(None::<&str>, None, None, 100) {
            Err(error) => error,
            Ok(_) => panic!("session-backed WebElement offset should fail"),
        };
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("offset() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_click = element
            .click()
            .expect_err("session-backed WebElement click should fail");
        assert!(matches!(
            chinese_click,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("click() 仅在 driver 模式可用")
        ));
        let chinese_input = element
            .input("text")
            .expect_err("session-backed WebElement input should fail");
        assert!(matches!(
            chinese_input,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("input() 仅在 driver 模式可用")
        ));
        let chinese_key = element
            .press_key("Enter")
            .expect_err("session-backed WebElement press_key should fail");
        assert!(matches!(
            chinese_key,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("press_key() 仅在 driver 模式可用")
        ));
    }

    #[test]
    fn webpage_browser_info_wrapper_signatures_accept_calls() {
        fn assert_calls(page: &WebPage) {
            let _ = page.browser_pid();
            let _ = page.process_id();
            let _ = page.browser_version();
            let _ = page.address();
            let _ = page.reconnect(0);
            let _ = page.close(false, false);
            let _ = page.close(true, true);
        }

        let _ = assert_calls as fn(&WebPage);
    }

    #[test]
    fn webpage_close_driver_and_close_session_signatures_accept_roundtrip_types() {
        let _ = WebPage::close_driver as fn(WebPage) -> OpenPageResult<SessionPage>;
        let _ = WebPage::close_session as fn(WebPage) -> OpenPageResult<Page>;
    }

    #[test]
    fn webpage_exposes_browser_info_wrappers_at_runtime() {
        let (page, temp_dir) =
            launch_headless_test_webpage("browser-info-wrappers", WebMode::Driver)
                .expect("launch headless webpage");

        let result = (|| -> crate::OpenPageResult<()> {
            assert_eq!(page.process_id(), page.browser_pid());
            assert_eq!(page.process_id(), page.browser.process_id());
            assert_eq!(page.address()?, page.browser.address());
            assert_eq!(page.browser_version()?, page.browser.version()?);
            Ok(())
        })();

        let close_result = page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage: {err}");
        }
        result.expect("webpage browser-info wrapper regression");
    }

    #[test]
    fn webpage_reconnect_rebuilds_browser_connection() {
        let (page, temp_dir) = launch_headless_test_webpage("webpage-reconnect", WebMode::Driver)
            .expect("launch headless webpage");

        let result = (|| -> crate::OpenPageResult<WebPage> {
            page.driver.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="msg">webpage reconnect</div>';
                    return true;
                })()"#,
            )?;

            let reconnected = page.reconnect(0)?;
            assert_eq!(reconnected.target_id(), page.target_id());
            assert_eq!(reconnected.address()?, page.address()?);
            assert_eq!(reconnected.process_id(), page.process_id());
            assert_eq!(
                reconnected
                    .driver
                    .run_js("document.querySelector('#msg').textContent")?,
                Value::from("webpage reconnect")
            );
            Ok(reconnected)
        })();

        let reconnected = match result {
            Ok(page) => page,
            Err(err) => {
                let _ = page.quit();
                let _ = fs::remove_dir_all(&temp_dir);
                panic!("webpage reconnect regression failed before cleanup: {err}");
            }
        };

        let close_result = reconnected.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage after reconnect: {err}");
        }
    }

    #[test]
    fn webpage_close_driver_returns_session_page_with_synced_state() {
        let (page, temp_dir) =
            launch_headless_test_webpage("webpage-close-driver", WebMode::Driver)
                .expect("launch headless webpage");
        let html_path = temp_dir.join("close-driver.html");
        let html_path_str = html_path.to_str().expect("html path str");

        let result = (|| -> crate::OpenPageResult<SessionPage> {
            write_test_html(
                &html_path,
                r#"
                <html>
                  <body>
                    <div id="msg">driver close</div>
                  </body>
                </html>
                "#,
            )?;
            assert!(page.get(html_path_str)?);

            let session_page = page.close_driver()?;
            let url = session_page.url()?.ok_or_else(|| {
                OpenPageError::PageOperation("session url missing after close_driver".to_string())
            })?;
            assert!(
                url.starts_with("file://") || url == html_path_str,
                "unexpected session url after close_driver: {url}"
            );
            assert!(
                session_page.html()?.contains("driver close"),
                "session html should keep driver page content"
            );
            Ok(session_page)
        })();

        let session_page = match result {
            Ok(page) => page,
            Err(err) => {
                let _ = fs::remove_dir_all(&temp_dir);
                panic!("webpage close_driver regression failed before cleanup: {err}");
            }
        };

        let close_result = session_page.close();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close session page after close_driver: {err}");
        }
    }

    #[test]
    fn webpage_close_session_returns_driver_page_with_synced_state() {
        let (page, temp_dir) =
            launch_headless_test_webpage("webpage-close-session", WebMode::Session)
                .expect("launch headless webpage");
        let html_path = temp_dir.join("close-session.html");
        let html_path_str = html_path.to_str().expect("html path str");

        let result = (|| -> crate::OpenPageResult<Page> {
            write_test_html(
                &html_path,
                r#"
                <html>
                  <body>
                    <div id="msg">session close</div>
                  </body>
                </html>
                "#,
            )?;
            assert!(page.get(html_path_str)?);

            let driver_page = page.close_session()?;
            assert!(driver_page.wait_for_doc_loaded(5_000)?);
            assert_eq!(
                driver_page.run_js("document.querySelector('#msg').textContent")?,
                Value::from("session close")
            );
            Ok(driver_page)
        })();

        let driver_page = match result {
            Ok(page) => page,
            Err(err) => {
                let _ = fs::remove_dir_all(&temp_dir);
                panic!("webpage close_session regression failed before cleanup: {err}");
            }
        };

        let close_result = driver_page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close driver page after close_session: {err}");
        }
    }

    #[test]
    fn webpage_close_can_close_other_tabs_without_closing_self() {
        let (page, temp_dir) =
            launch_headless_test_webpage("webpage-close-others", WebMode::Driver)
                .expect("launch headless webpage");

        let result = (|| -> crate::OpenPageResult<()> {
            let baseline_tabs = page.tabs_count()?;
            assert!(baseline_tabs >= 1);
            let extra = page.new_tab(None, false, false, false)?;
            assert_eq!(page.tabs_count()?, baseline_tabs + 1);

            page.close(true, true)?;
            assert_eq!(page.tabs_count()?, 1);

            let current = page
                .get_tab(Some(&page), None, None, None::<&str>, false)?
                .expect("current tab should still exist");
            match current {
                BrowserTabReference::WebPage(current_page) => {
                    assert_eq!(current_page.target_id(), page.target_id());
                }
                BrowserTabReference::Page(current_page) => {
                    panic!(
                        "webpage.get_tab() should return webpage, got page {}",
                        current_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("current tab should stay as webpage, got id {id}");
                }
            }

            assert!(
                page.tab_ids()?
                    .into_iter()
                    .all(|target_id| target_id != extra.target_id()),
                "other tabs should be closed"
            );
            Ok(())
        })();

        let close_result = page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage: {err}");
        }
        result.expect("webpage close others regression");
    }

    #[test]
    fn webpage_tab_wrappers_return_webpage_objects_when_requested() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_singleton_tab_obj(true);

        let (page, temp_dir) =
            launch_headless_test_webpage("webpage-tab-wrappers", WebMode::Driver)
                .expect("launch headless webpage");

        let result = (|| -> crate::OpenPageResult<()> {
            let current = page
                .get_tab(Some(&page), None, None, None::<&str>, false)?
                .expect("current tab should resolve");
            match current {
                BrowserTabReference::WebPage(current_page) => {
                    assert_eq!(current_page.target_id(), page.target_id());
                    assert_eq!(current_page.mode()?, page.mode()?);
                }
                BrowserTabReference::Page(current_page) => {
                    panic!(
                        "webpage.get_tab() should return webpage, got page {}",
                        current_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("webpage.get_tab() should return webpage, got id {id}");
                }
            }

            let latest = page.latest_tab()?.expect("latest tab should exist");
            match latest {
                BrowserTabReference::WebPage(latest_page) => {
                    assert_eq!(latest_page.target_id(), page.target_id());
                }
                BrowserTabReference::Page(latest_page) => {
                    panic!(
                        "webpage.latest_tab() should return webpage, got page {}",
                        latest_page.target_id()
                    );
                }
                BrowserTabReference::Id(id) => {
                    panic!("webpage.latest_tab() should return webpage, got id {id}");
                }
            }

            let tab_types = ["page", "tab"];
            let tabs = page.get_tabs(None, None, Some(&tab_types[..]), false)?;
            assert!(
                tabs.into_iter()
                    .all(|reference| matches!(reference, BrowserTabReference::WebPage(_))),
                "webpage.get_tabs() should return webpage objects when as_id=false"
            );
            Ok(())
        })();

        let close_result = page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage: {err}");
        }
        result.expect("webpage tab wrapper regression");
    }

    #[test]
    fn page_and_frame_find_signatures_accept_by_tuples() {
        fn assert_calls(page: &Page, frame: &Frame) {
            let _ = page.find((By::ID, "root"));
            let _ = page.find_all((By::CLASS_NAME, "item"));
            let _ = frame.find((By::ID, "root"));
            let _ = frame.find_all((By::CLASS_NAME, "item"));
        }

        let _ = assert_calls as fn(&Page, &Frame);
    }

    #[test]
    fn webpage_and_webframe_find_signatures_accept_by_tuples() {
        fn assert_calls(page: &WebPage, frame: &WebFrame) {
            let _ = page.find((By::ID, "root"));
            let _ = page.find_all((By::CLASS_NAME, "item"));
            let _ = frame.find((By::ID, "root"));
            let _ = frame.find_all((By::CLASS_NAME, "item"));
        }

        let _ = assert_calls as fn(&WebPage, &WebFrame);
    }

    #[test]
    fn page_frame_element_and_web_wrappers_find_locators_accept_locator_inputs() {
        fn assert_calls(
            page: &Page,
            frame: &Frame,
            element: &Element,
            web_page: &WebPage,
            web_frame: &WebFrame,
            web_element: &WebElement,
        ) {
            let locators = vec!["#root".to_string(), ".item".to_string()];
            let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
            let mixed_locators = [
                LocatorInput::from("#root"),
                LocatorInput::from((By::CLASS_NAME, "item")),
            ];

            let _ = page.find_locators((By::ID, "root"), true, true);
            let _ = page.find_locators(&locators, false, false);
            let _ = page.find_locators(&tuple_locators, false, false);
            let _ = page.find_locators(&mixed_locators, false, false);
            let _ = frame.find_locators((By::ID, "root"), true, true);
            let _ = frame.find_locators(&locators, false, false);
            let _ = frame.find_locators(&tuple_locators, false, false);
            let _ = frame.find_locators(&mixed_locators, false, false);
            let _ = element.find_locators((By::CLASS_NAME, "item"), true, true);
            let _ = element.find_locators(&locators, false, false);
            let _ = element.find_locators(&tuple_locators, false, false);
            let _ = element.find_locators(&mixed_locators, false, false);
            let _ = web_page.find_locators((By::ID, "root"), true, true);
            let _ = web_page.find_locators(&locators, false, false);
            let _ = web_page.find_locators(&tuple_locators, false, false);
            let _ = web_page.find_locators(&mixed_locators, false, false);
            let _ = web_frame.find_locators((By::ID, "root"), true, true);
            let _ = web_frame.find_locators(&locators, false, false);
            let _ = web_frame.find_locators(&tuple_locators, false, false);
            let _ = web_frame.find_locators(&mixed_locators, false, false);
            let _ = web_element.find_locators((By::CLASS_NAME, "item"), true, true);
            let _ = web_element.find_locators(&locators, false, false);
            let _ = web_element.find_locators(&tuple_locators, false, false);
            let _ = web_element.find_locators(&mixed_locators, false, false);
        }

        let _ = assert_calls as fn(&Page, &Frame, &Element, &WebPage, &WebFrame, &WebElement);
    }

    #[test]
    fn page_and_webpage_frame_lookup_signatures_accept_by_tuples_and_object_refs() {
        fn assert_calls(
            page: &Page,
            frame: &Frame,
            element: &Element,
            web_page: &WebPage,
            web_frame: &WebFrame,
            web_element: &WebElement,
        ) {
            let _ = page.get_frame((By::ID, "theFrame"));
            let _ = page.get_frame_ele((By::ID, "theFrame"));
            let _ = page.get_frame(1usize);
            let _ = page.get_frame_ele(1usize);
            let _ = page.get_frame(-1isize);
            let _ = page.get_frame_ele(-1isize);
            let _ = page.get_frame(element);
            let _ = page.get_frame_ele(element);
            let _ = page.get_frame(frame);
            let _ = page.get_frame_ele(frame);
            let _ = page.get_frames(Some((By::TAG_NAME, "iframe")));
            let _ = page.get_frame_eles(Some((By::TAG_NAME, "iframe")));
            let _ = page.get_frame_context((By::ID, "theFrame"));
            let _ = page.get_frame_context(1usize);
            let _ = page.get_frame_context(-1isize);
            let _ = page.get_frame_context(element);
            let _ = page.get_frame_context(frame);
            let _ = page.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
            let _ = element.get_frame((By::ID, "theFrame"));
            let _ = element.get_frame(1usize);
            let _ = element.get_frame(frame);
            let _ = web_page.get_frame((By::ID, "theFrame"));
            let _ = web_page.get_frame_ele((By::ID, "theFrame"));
            let _ = web_page.get_frame(1usize);
            let _ = web_page.get_frame_ele(1usize);
            let _ = web_page.get_frame(-1isize);
            let _ = web_page.get_frame_ele(-1isize);
            let _ = web_page.get_frame(web_element);
            let _ = web_page.get_frame_ele(web_element);
            let _ = web_page.get_frame(web_frame);
            let _ = web_page.get_frame_ele(web_frame);
            let _ = web_page.get_frames(Some((By::TAG_NAME, "iframe")));
            let _ = web_page.get_frame_eles(Some((By::TAG_NAME, "iframe")));
            let _ = web_page.get_frame_context((By::ID, "theFrame"));
            let _ = web_page.get_frame_context(1usize);
            let _ = web_page.get_frame_context(-1isize);
            let _ = web_page.get_frame_context(web_element);
            let _ = web_page.get_frame_context(web_frame);
            let _ = web_page.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
            let _ = web_element.get_frame((By::ID, "theFrame"));
            let _ = web_element.get_frame(1usize);
            let _ = web_element.get_frame(web_frame);
        }

        let _ = assert_calls as fn(&Page, &Frame, &Element, &WebPage, &WebFrame, &WebElement);
    }

    #[test]
    fn webpage_get_frame_returns_webframe_objects_at_runtime() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let (page, temp_dir) =
            launch_headless_test_webpage("webpage-get-frame-objects", WebMode::Driver)
                .expect("launch headless webpage");

        let result = (|| -> crate::OpenPageResult<()> {
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

            let frame = page.get_frame("css:#demo-frame")?;
            let frame_by_index = page.get_frame(1usize)?;
            let frame_ele = page.get_frame_ele("css:#demo-frame")?;
            let frames = page.get_frames(Some((By::TAG_NAME, "iframe")))?;
            let frame_context = page.get_frame_context("css:#demo-frame")?;

            assert_eq!(frame.attr("id")?, Some("demo-frame".to_string()));
            assert_eq!(frame_by_index.attr("name")?, Some("demo-frame".to_string()));
            assert_eq!(frame_ele.attr("id")?, Some("demo-frame".to_string()));
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].attr("id")?, Some("demo-frame".to_string()));
            assert_eq!(frame_context.attr("id")?, Some("demo-frame".to_string()));
            frame.set_none_element_value(Some("missing"), true)?;
            assert_eq!(
                frame_context.ele(".does-not-exist")?.text()?,
                Some("missing".to_string())
            );
            Ok(())
        })();

        let close_result = page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage: {err}");
        }
        result.expect("webpage get_frame runtime regression");
    }

    #[test]
    fn page_frame_webpage_and_webframe_js_helper_signatures_accept_common_inputs() {
        fn assert_calls(page: &Page, frame: &Frame, web_page: &WebPage, web_frame: &WebFrame) {
            let args = [Value::from(1), Value::from(2)];

            let _ = page.run_js_loaded("1 + 2");
            let _ =
                page.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ = page.run_js_loaded_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = page.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ = page.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
            let _ = page.run_async_js("window.__pageAsync = true;");
            let _ =
                page.run_async_js_with_args("window.__pageArg = arguments[0];", &args[..1], false);
            let _ = page.run_async_js_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );

            let _ = frame.run_js_loaded("1 + 2");
            let _ =
                frame.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ = frame.run_js_loaded_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = frame.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ =
                frame.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
            let _ = frame.run_async_js("window.__frameAsync = true;");
            let _ = frame.run_async_js_with_args(
                "window.__frameArg = arguments[0];",
                &args[..1],
                false,
            );
            let _ = frame.run_async_js_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = frame.add_init_js("window.__frameInit = true;");
            let _ = frame.remove_init_js(None);

            let _ = web_page.run_js_loaded("1 + 2");
            let _ = web_page.run_js_loaded_with_args(
                "return arguments[0] + arguments[1];",
                &args,
                false,
            );
            let _ = web_page.run_js_loaded_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = web_page.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ =
                web_page.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
            let _ = web_page.run_async_js("window.__webPageAsync = true;");
            let _ = web_page.run_async_js_with_args(
                "window.__webPageArg = arguments[0];",
                &args[..1],
                false,
            );
            let _ = web_page.run_async_js_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );

            let _ = web_frame.run_js_loaded("1 + 2");
            let _ = web_frame.run_js_loaded_with_args(
                "return arguments[0] + arguments[1];",
                &args,
                false,
            );
            let _ = web_frame.run_js_loaded_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = web_frame.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
            let _ = web_frame.run_js_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = web_frame.run_async_js("window.__webFrameAsync = true;");
            let _ = web_frame.run_async_js_with_args(
                "window.__webFrameArg = arguments[0];",
                &args[..1],
                false,
            );
            let _ = web_frame.run_async_js_with_options(
                "arguments[0] + arguments[1]",
                &args,
                true,
                Some(500),
            );
            let _ = web_frame.add_init_js("window.__webFrameInit = true;");
            let _ = web_frame.remove_init_js(None);
        }

        let _ = assert_calls as fn(&Page, &Frame, &WebPage, &WebFrame);
    }

    #[test]
    fn page_and_webpage_run_cdp_alias_signatures_accept_command_types() {
        fn assert_calls(page: &Page, web_page: &WebPage) {
            let _ = page.run_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
            let _ = page.run_cdp_loaded(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
            let _ = web_page.run_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
            let _ =
                web_page.run_cdp_loaded(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
        }

        let _ = assert_calls as fn(&Page, &WebPage);
    }

    #[test]
    fn page_and_webpage_runtime_setting_signatures_accept_common_inputs() {
        fn assert_calls(page: &Page, web_page: &WebPage) {
            let _ = page.retry_times();
            let _ = page.retry_interval();
            let _ = page.timeouts();
            let _ = page.set_retry(Some(5), Some(0.25));
            let _ = page.set_timeouts(Some(1.5), Some(6.0), Some(0.75));

            let _ = web_page.retry_times();
            let _ = web_page.retry_interval();
            let _ = web_page.timeouts();
            let _ = web_page.set_retry(Some(5), Some(0.25));
            let _ = web_page.set_timeouts(Some(1.5), Some(6.0), Some(0.75));
        }

        let _ = assert_calls as fn(&Page, &WebPage);
    }

    #[test]
    fn page_and_webpage_set_wrapper_signatures_accept_common_inputs() {
        fn assert_calls(page: &Page, web_page: &WebPage) {
            let headers = [("Accept".to_string(), "text/html".to_string())];
            let urls = vec!["*.css*".to_string()];
            let files = vec!["/tmp/demo.txt".to_string()];
            let cookies = json!({"sid": "abc", "domain": ".example.test", "path": "/"});

            let _ = page.set().window().max();
            let _ = page.set().window().mini();
            let _ = page.set().window().full();
            let _ = page.set().window().normal();
            let _ = page.set().window().size(Some(800), Some(600));
            let _ = page.set().window().location(Some(10), Some(20));
            let _ = page.set().window().hide();
            let _ = page.set().window().show();
            let _ = page.set().load_mode().normal();
            let _ = page.set().load_mode().eager();
            let _ = page.set().load_mode().none();
            let _ = page.set().blocked_urls(&urls);
            let _ = page.set().headers(&headers);
            let _ = page.set().user_agent("demo-agent", Some("linux"));
            let _ = page.set().session_storage("foo", Some("bar"));
            let _ = page.set().local_storage("foo", Some("bar"));
            let _ = page.set().auto_handle_alert(Some(true), Some("ok"));
            let _ = page.set().cookies(&cookies);
            let _ = page.set().clear_cookies();
            let _ = page.set().remove_cookie("sid", None, None, None);
            let _ = page.set().download_path("/tmp");
            let _ = page
                .set()
                .download_file_exists(DownloadFileExistsMode::Rename);
            let _ = page.set().when_download_file_exists("rename");
            let _ = page.set().download_file_name(Some("file"), Some(".txt"));
            let _ = page.set().upload_files(&files);
            let _ = page.set().upload_paths(&files);
            let _ = page.set().activate();
            let _ = page.set().retry_times(5);
            let _ = page.set().retry_interval(0.25);
            let _ = page.set().timeouts(Some(1.0), Some(2.0), Some(3.0));

            let _ = web_page.set().window().max();
            let _ = web_page.set().window().mini();
            let _ = web_page.set().window().full();
            let _ = web_page.set().window().normal();
            let _ = web_page.set().window().size(Some(800), Some(600));
            let _ = web_page.set().window().location(Some(10), Some(20));
            let _ = web_page.set().window().hide();
            let _ = web_page.set().window().show();
            let _ = web_page.set().load_mode().normal();
            let _ = web_page.set().load_mode().eager();
            let _ = web_page.set().load_mode().none();
            let _ = web_page.set().blocked_urls(&urls);
            let _ = web_page.set().headers(&headers);
            let _ = web_page.set().user_agent("demo-agent", Some("linux"));
            let _ = web_page.set().session_storage("foo", Some("bar"));
            let _ = web_page.set().local_storage("foo", Some("bar"));
            let _ = web_page.set().auto_handle_alert(Some(true), Some("ok"));
            let _ = web_page.set().cookies(&cookies);
            let _ = web_page.set().clear_cookies();
            let _ = web_page.set().remove_cookie("sid", None, None, None);
            let _ = web_page.set().download_path("/tmp");
            let _ = web_page
                .set()
                .download_file_exists(DownloadFileExistsMode::Rename);
            let _ = web_page.set().when_download_file_exists("rename");
            let _ = web_page
                .set()
                .download_file_name(Some("file"), Some(".txt"));
            let _ = web_page.set().upload_files(&files);
            let _ = web_page.set().upload_paths(&files);
            let _ = web_page.set().activate();
            let _ = web_page.set().retry_times(5);
            let _ = web_page.set().retry_interval(0.25);
            let _ = web_page.set().timeouts(Some(1.0), Some(2.0), Some(3.0));
        }

        let _ = assert_calls as fn(&Page, &WebPage);
    }

    #[test]
    fn page_and_webpage_scroll_wrapper_signatures_accept_common_inputs() {
        fn assert_calls(page: &Page, web_page: &WebPage) {
            let _ = page.scroll().to_top();
            let _ = page.scroll().to_bottom();
            let _ = page.scroll().to_half();
            let _ = page.scroll().to_rightmost();
            let _ = page.scroll().to_leftmost();
            let _ = page.scroll().to_location(10.0, 20.0);
            let _ = page.scroll().up(10.0);
            let _ = page.scroll().down(10.0);
            let _ = page.scroll().left(10.0);
            let _ = page.scroll().right(10.0);

            let _ = web_page.scroll().to_top();
            let _ = web_page.scroll().to_bottom();
            let _ = web_page.scroll().to_half();
            let _ = web_page.scroll().to_rightmost();
            let _ = web_page.scroll().to_leftmost();
            let _ = web_page.scroll().to_location(10.0, 20.0);
            let _ = web_page.scroll().up(10.0);
            let _ = web_page.scroll().down(10.0);
            let _ = web_page.scroll().left(10.0);
            let _ = web_page.scroll().right(10.0);
        }

        let _ = assert_calls as fn(&Page, &WebPage);
    }

    #[test]
    fn page_and_webpage_actions_signatures_accept_locators_elements_and_coordinates() {
        fn assert_calls(
            page: &Page,
            web_page: &WebPage,
            element: &Element,
            web_element: &WebElement,
        ) {
            let _ = page.actions();
            let _ = page
                .new_actions()
                .move_to((10, 20), None, None, 0.0)
                .and_then(|actions| actions.move_to((By::ID, "root"), Some(3.0), Some(4.0), 0.0))
                .and_then(|actions| actions.move_to(element, None, None, 0.0))
                .and_then(|actions| actions.click(Some(element), 1))
                .and_then(|actions| actions.r_click(Some((By::ID, "root")), 1))
                .and_then(|actions| actions.m_click(Some((12.0, 24.0)), 1))
                .and_then(|actions| actions.hold(Some(element)))
                .and_then(|actions| actions.release(Some((By::ID, "root"))))
                .and_then(|actions| actions.r_hold(Some((By::ID, "root"))))
                .and_then(|actions| actions.r_release(Some((By::ID, "root"))))
                .and_then(|actions| actions.m_hold(Some((By::ID, "root"))))
                .and_then(|actions| actions.m_release(Some((By::ID, "root"))))
                .and_then(|actions| actions.scroll(120.0, 0.0, Some((By::ID, "root"))))
                .and_then(|actions| actions.key_down("Shift"))
                .and_then(|actions| actions.key_up("Shift"))
                .and_then(|actions| actions.input("demo"))
                .and_then(|actions| actions.r#type(["Control", "a"]))
                .and_then(|actions| actions.type_with_interval("demo", 0.05))
                .and_then(|actions| actions.type_keys(vec!["b", "c"]))
                .and_then(|actions| actions.type_keys_with_interval(["d", "e"], 0.05))
                .and_then(|actions| {
                    actions.drag_in(
                        element,
                        crate::ActionsDragData::files(["./fixtures/demo.txt"]),
                    )
                })
                .and_then(|actions| {
                    actions.drag_in((By::ID, "root"), crate::ActionsDragData::text("demo"))
                })
                .and_then(|actions| {
                    actions.drag_in(
                        (By::ID, "root"),
                        crate::ActionsDragData::link("https://example.test", "Example"),
                    )
                })
                .and_then(|actions| {
                    actions.drag_in(
                        (By::ID, "root"),
                        crate::ActionsDragData::html("<b>demo</b>", "https://example.test/base"),
                    )
                })
                .and_then(|actions| actions.r#move(5.0, 6.0, 0.0))
                .and_then(|actions| actions.wait(0.0, None));

            let _ = web_page.actions();
            let _ = web_page
                .new_actions()
                .and_then(|mut actions| {
                    actions.drag_in(web_element, crate::ActionsDragData::text("demo"))?;
                    actions.drag_in(
                        web_element,
                        crate::ActionsDragData::link("https://example.test", "Example"),
                    )?;
                    actions.drag_in(
                        web_element,
                        crate::ActionsDragData::html("<b>demo</b>", "https://example.test/base"),
                    )?;
                    Ok(actions)
                })
                .and_then(|mut actions| actions.move_to(web_element, None, None, 0.0).map(|_| ()));
        }

        let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement);
    }

    #[test]
    fn element_and_webelement_object_wrappers_expose_scroll_set_and_select_signatures() {
        fn assert_calls(element: &Element, web_element: &WebElement) {
            let _ = element.states().is_in_viewport();
            let _ = element.states().is_whole_in_viewport();
            let _ = element.states().is_alive();
            let _ = element.states().is_checked();
            let _ = element.states().is_selected();
            let _ = element.states().is_enabled();
            let _ = element.states().is_displayed();
            let _ = element.states().is_covered();
            let _ = element.states().is_clickable();
            let _ = element.states().has_rect();

            let _ = element.rect().corners();
            let _ = element.rect().viewport_corners();
            let _ = element.rect().location();
            let _ = element.rect().viewport_location();
            let _ = element.rect().screen_location();
            let _ = element.rect().midpoint();
            let _ = element.rect().viewport_midpoint();
            let _ = element.rect().click_point();
            let _ = element.rect().viewport_click_point();
            let _ = element.rect().screen_midpoint();
            let _ = element.rect().screen_click_point();
            let _ = element.rect().size();
            let _ = element.rect().scroll_position();

            let _ = element.wait().displayed(1_000);
            let _ = element.wait().hidden(1_000);
            let _ = element.wait().enabled(1_000);
            let _ = element.wait().disabled(1_000);
            let _ = element.wait().deleted(1_000);
            let _ = element.wait().clickable(1_000);
            let _ = element.wait().has_rect(1_000);
            let _ = element.wait().covered(1_000);
            let _ = element.wait().not_covered(1_000);
            let _ = element.wait().disabled_or_deleted(1_000);
            let _ = element.wait().stop_moving(1_000);

            let _ = element.scroll().to_top();
            let _ = element.scroll().to_bottom();
            let _ = element.scroll().to_half();
            let _ = element.scroll().to_rightmost();
            let _ = element.scroll().to_leftmost();
            let _ = element.scroll().to_location(10.0, 20.0);
            let _ = element.scroll().up(10.0);
            let _ = element.scroll().down(10.0);
            let _ = element.scroll().left(10.0);
            let _ = element.scroll().right(10.0);
            let _ = element.scroll().to_see(Some(true));
            let _ = element.scroll().to_center();

            let _ = element.set().inner_html("<span>demo</span>");
            let _ = element.set().property("value", &serde_json::json!("demo"));
            let _ = element.set().style("display", "block");
            let _ = element.set().attr("data-role", "demo");
            let _ = element.set().value("demo");

            let _ = element.select().by_text("demo");
            let _ = element.select().by_text(["demo", "alt"]);
            let _ = element.select().by_text_with_timeout("demo", Some(1_000));
            let _ = element.select().by_value("demo");
            let _ = element.select().by_value(["demo", "alt"]);
            let _ = element.select().by_value_with_timeout("demo", Some(1_000));
            let _ = element.select().by_index(1);
            let _ = element.select().by_index([1, 2]);
            let _ = element.select().by_index_with_timeout([1, 2], Some(1_000));
            let _ = element.select().by_indices(&[1, 2]);
            let _ = element
                .select()
                .by_indices_with_timeout(&[1, 2], Some(1_000));
            let _ = element.select().by_locator("css:option");
            let locator_list = vec!["css:option".to_string(), "css:option.demo".to_string()];
            let _ = element.select().by_locator(&locator_list);
            let _ = element
                .select()
                .by_locator_with_timeout(&locator_list, Some(1_000));
            let _ = element.select().by_option(element);
            let _ = element.select().by_option([element, element]);
            let option_refs = [element, element];
            let _ = element.select().by_options(&option_refs);
            let _ = element.select().cancel_by_text("demo");
            let _ = element.select().cancel_by_text(["demo", "alt"]);
            let _ = element
                .select()
                .cancel_by_text_with_timeout("demo", Some(1_000));
            let _ = element.select().cancel_by_value("demo");
            let _ = element.select().cancel_by_value(["demo", "alt"]);
            let _ = element
                .select()
                .cancel_by_value_with_timeout("demo", Some(1_000));
            let _ = element.select().cancel_by_index(1);
            let _ = element.select().cancel_by_index([1, 2]);
            let _ = element
                .select()
                .cancel_by_index_with_timeout([1, 2], Some(1_000));
            let _ = element.select().cancel_by_indices(&[1, 2]);
            let _ = element
                .select()
                .cancel_by_indices_with_timeout(&[1, 2], Some(1_000));
            let _ = element.select().cancel_by_locator("css:option");
            let _ = element.select().cancel_by_locator(&locator_list);
            let _ = element
                .select()
                .cancel_by_locator_with_timeout(&locator_list, Some(1_000));
            let _ = element.select().cancel_by_option(element);
            let _ = element.select().cancel_by_option([element, element]);
            let _ = element.select().cancel_by_options(&option_refs);
            let _ = element.select().all();
            let _ = element.select().clear();
            let _ = element.select().invert();
            let _ = element.select().is_multi();
            let _ = element.select().options();
            let _ = element.select().selected_option();
            let _ = element.select().selected_options();

            let _ = web_element.states().is_in_viewport();
            let _ = web_element.states().is_whole_in_viewport();
            let _ = web_element.states().is_alive();
            let _ = web_element.states().is_checked();
            let _ = web_element.states().is_selected();
            let _ = web_element.states().is_enabled();
            let _ = web_element.states().is_displayed();
            let _ = web_element.states().is_covered();
            let _ = web_element.states().is_clickable();
            let _ = web_element.states().has_rect();

            let _ = web_element.rect().corners();
            let _ = web_element.rect().viewport_corners();
            let _ = web_element.rect().location();
            let _ = web_element.rect().viewport_location();
            let _ = web_element.rect().screen_location();
            let _ = web_element.rect().midpoint();
            let _ = web_element.rect().viewport_midpoint();
            let _ = web_element.rect().click_point();
            let _ = web_element.rect().viewport_click_point();
            let _ = web_element.rect().screen_midpoint();
            let _ = web_element.rect().screen_click_point();
            let _ = web_element.rect().size();
            let _ = web_element.rect().scroll_position();

            let _ = web_element.wait().displayed(1_000);
            let _ = web_element.wait().hidden(1_000);
            let _ = web_element.wait().enabled(1_000);
            let _ = web_element.wait().disabled(1_000);
            let _ = web_element.wait().deleted(1_000);
            let _ = web_element.wait().clickable(1_000);
            let _ = web_element.wait().has_rect(1_000);
            let _ = web_element.wait().covered(1_000);
            let _ = web_element.wait().not_covered(1_000);
            let _ = web_element.wait().disabled_or_deleted(1_000);
            let _ = web_element.wait().stop_moving(1_000);

            let _ = web_element.scroll().to_top();
            let _ = web_element.scroll().to_bottom();
            let _ = web_element.scroll().to_half();
            let _ = web_element.scroll().to_rightmost();
            let _ = web_element.scroll().to_leftmost();
            let _ = web_element.scroll().to_location(10.0, 20.0);
            let _ = web_element.scroll().up(10.0);
            let _ = web_element.scroll().down(10.0);
            let _ = web_element.scroll().left(10.0);
            let _ = web_element.scroll().right(10.0);
            let _ = web_element.scroll().to_see(Some(true));
            let _ = web_element.scroll().to_center();

            let _ = web_element.set().inner_html("<span>demo</span>");
            let _ = web_element
                .set()
                .property("value", &serde_json::json!("demo"));
            let _ = web_element.set().style("display", "block");
            let _ = web_element.set().attr("data-role", "demo");
            let _ = web_element.set().value("demo");

            let _ = web_element.select().by_text("demo");
            let _ = web_element.select().by_text(["demo", "alt"]);
            let _ = web_element
                .select()
                .by_text_with_timeout("demo", Some(1_000));
            let _ = web_element.select().by_value("demo");
            let _ = web_element.select().by_value(["demo", "alt"]);
            let _ = web_element
                .select()
                .by_value_with_timeout("demo", Some(1_000));
            let _ = web_element.select().by_index(1);
            let _ = web_element.select().by_index([1, 2]);
            let _ = web_element
                .select()
                .by_index_with_timeout([1, 2], Some(1_000));
            let _ = web_element.select().by_indices(&[1, 2]);
            let _ = web_element
                .select()
                .by_indices_with_timeout(&[1, 2], Some(1_000));
            let _ = web_element.select().by_locator("css:option");
            let web_locator_list = vec!["css:option".to_string(), "css:option.demo".to_string()];
            let _ = web_element.select().by_locator(&web_locator_list);
            let _ = web_element
                .select()
                .by_locator_with_timeout(&web_locator_list, Some(1_000));
            let _ = web_element.select().by_option(web_element);
            let _ = web_element.select().by_option([web_element, web_element]);
            let web_option_refs = [web_element, web_element];
            let _ = web_element.select().by_options(&web_option_refs);
            let _ = web_element.select().cancel_by_text("demo");
            let _ = web_element.select().cancel_by_text(["demo", "alt"]);
            let _ = web_element
                .select()
                .cancel_by_text_with_timeout("demo", Some(1_000));
            let _ = web_element.select().cancel_by_value("demo");
            let _ = web_element.select().cancel_by_value(["demo", "alt"]);
            let _ = web_element
                .select()
                .cancel_by_value_with_timeout("demo", Some(1_000));
            let _ = web_element.select().cancel_by_index(1);
            let _ = web_element.select().cancel_by_index([1, 2]);
            let _ = web_element
                .select()
                .cancel_by_index_with_timeout([1, 2], Some(1_000));
            let _ = web_element.select().cancel_by_indices(&[1, 2]);
            let _ = web_element
                .select()
                .cancel_by_indices_with_timeout(&[1, 2], Some(1_000));
            let _ = web_element.select().cancel_by_locator("css:option");
            let _ = web_element.select().cancel_by_locator(&web_locator_list);
            let _ = web_element
                .select()
                .cancel_by_locator_with_timeout(&web_locator_list, Some(1_000));
            let _ = web_element.select().cancel_by_option(web_element);
            let _ = web_element
                .select()
                .cancel_by_option([web_element, web_element]);
            let _ = web_element.select().cancel_by_options(&web_option_refs);
            let _ = web_element.select().all();
            let _ = web_element.select().clear();
            let _ = web_element.select().invert();
            let _ = web_element.select().is_multi();
            let _ = web_element.select().options();
            let _ = web_element.select().selected_option();
            let _ = web_element.select().selected_options();
        }

        let _ = assert_calls as fn(&Element, &WebElement);
    }

    #[test]
    fn element_and_webelement_clicker_expose_signatures() {
        fn assert_calls(element: &Element, web_element: &WebElement) {
            let files = vec!["./fixtures/demo.txt".to_string()];

            let _ = element.click_with_options(None, Some(1_000), true);
            let _ = element.click_left_with_options(Some(false), Some(1_000), false);
            let _ = element.clicker().left();
            let _ = element
                .clicker()
                .left_with_options(Some(true), Some(1_000), false);
            let _ = element.clicker().right();
            let _ = element.clicker().middle(true);
            let _ = element.clicker().multi(2);
            let _ = element.clicker().at(Some(5.0), Some(6.0), "left", 1);
            let _ = element.clicker().to_upload(&files, Some(1_000), false);
            let _ =
                element
                    .clicker()
                    .to_download(None, None, None, false, Some(1_000), false, false);
            let _ = element.clicker().for_new_tab(Some(1_000), false);

            let _ = web_element.click_with_options(None, Some(1_000), true);
            let _ = web_element.click_left_with_options(Some(false), Some(1_000), false);
            let _ = web_element.clicker().left();
            let _ = web_element
                .clicker()
                .left_with_options(Some(true), Some(1_000), false);
            let _ = web_element.clicker().right();
            let _ = web_element.clicker().middle(true);
            let _ = web_element.clicker().multi(2);
            let _ = web_element.clicker().at(Some(5.0), Some(6.0), "left", 1);
            let _ = web_element.clicker().to_upload(&files, Some(1_000), false);
            let _ = web_element.clicker().to_download(
                None,
                None,
                None,
                false,
                Some(1_000),
                false,
                false,
            );
            let _ = web_element.clicker().for_new_tab(Some(1_000), false);
        }

        let _ = assert_calls as fn(&Element, &WebElement);
    }

    #[test]
    fn page_webpage_and_session_element_lists_expose_getter_and_filter_signatures() {
        fn assert_calls(
            page_elements: &Vec<Element>,
            web_elements: &Vec<WebElement>,
            session_elements: &Vec<crate::SessionElement>,
        ) {
            let search = crate::ElementsSearch::new()
                .displayed(true)
                .enabled(true)
                .tag("button");

            let _ = page_elements.get().attrs("href");
            let _ = page_elements.get().links();
            let _ = page_elements.get().texts();
            let _ = web_elements.get().attrs("href");
            let _ = web_elements.get().links();
            let _ = web_elements.get().texts();
            let _ = session_elements.get().attrs("href");
            let _ = session_elements.get().links();
            let _ = session_elements.get().texts();

            let _ = page_elements.filter().displayed(true);
            let _ = page_elements.filter().checked(true);
            let _ = page_elements.filter().selected(true);
            let _ = page_elements.filter().enabled(true);
            let _ = page_elements.filter().clickable(true);
            let _ = page_elements.filter().have_rect(true);
            let _ = page_elements
                .filter()
                .attr("href", "https://example.test", true);
            let _ = page_elements.filter().text("demo", true, true);
            let _ = page_elements.filter().tag("button", true);
            let _ = page_elements.filter().style("display", "block", true);
            let _ = page_elements.filter().property("id", "root", true);
            let _ = page_elements.filter().get().texts();
            let _ = page_elements.search(&search);
            let _ = page_elements.search_one(&search);
            let _ = page_elements.search_one_at(2, &search);
            let _ = page_elements.filter_one().displayed(true);
            let _ = page_elements.filter_one().checked(true);
            let _ = page_elements.filter_one().selected(true);
            let _ = page_elements.filter_one_at(2).enabled(true);
            let _ = page_elements.filter_one().clickable(true);
            let _ = page_elements.filter_one().have_rect(true);
            let _ = page_elements
                .filter_one()
                .attr("href", "https://example.test", true);
            let _ = page_elements.filter_one().text("demo", true, true);
            let _ = page_elements.filter_one().tag("button", true);
            let _ = page_elements.filter_one().style("display", "block", true);
            let _ = page_elements.filter_one().property("id", "root", true);
            let _ = page_elements.filter().search(&search);
            let _ = page_elements.filter_one().search(&search);
            let _ = page_elements
                .filter_one()
                .tag("button", true)
                .and_then(|element| element.text());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.is_displayed());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.html());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.inner_html());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.value());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.click());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.input("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.clear());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.focus());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.hover());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.clicker().left());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.clicker().middle(false));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.remove_attr("data-role"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.check(false, true));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.uncheck(true));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set_value("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set_attr("data-role", "demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set_style("display", "block"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set_inner_html("<span>demo</span>"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set().value("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set().attr("data-role", "demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.set().property("tabIndex", &serde_json::json!(3)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_top());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_bottom());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_location(1.0, 2.0));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_up(1.0));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_down(1.0));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_left(1.0));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_right(1.0));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_see(Some(true)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_center());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_top());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_half());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_rightmost());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_text("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_text_with_timeout("demo", Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_value("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_value_with_timeout("demo", Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_index(1));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_index([1, 2]));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_locator("css:option"));
            let _ = page_elements.search_one(&search).and_then(|element| {
                element.select_by_locator_with_timeout("css:option", Some(1_000))
            });
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_indices(&[1, 2]));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_by_indices_with_timeout(&[1, 2], Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_text("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_text_with_timeout("demo", Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_value("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_value_with_timeout("demo", Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_index(1));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_index([1, 2]));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_indices(&[1, 2]));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_indices_with_timeout(&[1, 2], Some(1_000)));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_locator("css:option"));
            let _ = page_elements.search_one(&search).and_then(|element| {
                element.cancel_by_locator_with_timeout("css:option", Some(1_000))
            });
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_clear());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_all());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_invert());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_is_multi());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_options());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_selected_option());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select_selected_options());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select().by_text("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select().cancel_by_value("demo"));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select().is_multi());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.select().selected_options());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.attrs());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.child_count());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.css_path());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.xpath());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.comments());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.states().is_alive());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.states().is_clickable());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().corners());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_corners());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().location());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_location());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().midpoint());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_midpoint());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().click_point());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_click_point());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_location());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_midpoint());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_click_point());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().scroll_position());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.rect().size());
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.wait().displayed(1_000));
            let _ = page_elements
                .search_one(&search)
                .and_then(|element| element.wait().stop_moving(1_000));

            let _ = web_elements.filter().displayed(true);
            let _ = web_elements.filter().checked(true);
            let _ = web_elements.filter().selected(true);
            let _ = web_elements.filter().enabled(true);
            let _ = web_elements.filter().clickable(true);
            let _ = web_elements.filter().have_rect(true);
            let _ = web_elements
                .filter()
                .attr("href", "https://example.test", true);
            let _ = web_elements.filter().text("demo", true, true);
            let _ = web_elements.filter().tag("button", true);
            let _ = web_elements.filter().style("display", "block", true);
            let _ = web_elements.filter().property("id", "root", true);
            let _ = web_elements.filter().get().texts();
            let _ = web_elements.search(&search);
            let _ = web_elements.search_one(&search);
            let _ = web_elements.search_one_at(2, &search);
            let _ = web_elements.filter_one().displayed(true);
            let _ = web_elements.filter_one().checked(true);
            let _ = web_elements.filter_one().selected(true);
            let _ = web_elements.filter_one_at(2).enabled(true);
            let _ = web_elements.filter_one().clickable(true);
            let _ = web_elements.filter_one().have_rect(true);
            let _ = web_elements
                .filter_one()
                .attr("href", "https://example.test", true);
            let _ = web_elements.filter_one().text("demo", true, true);
            let _ = web_elements.filter_one().tag("button", true);
            let _ = web_elements.filter_one().style("display", "block", true);
            let _ = web_elements.filter_one().property("id", "root", true);
            let _ = web_elements.filter().search(&search);
            let _ = web_elements.filter_one().search(&search);
            let _ = web_elements
                .filter_one()
                .tag("button", true)
                .and_then(|element| element.attr("id"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.is_enabled());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.html());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.inner_html());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.value());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.click());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.input("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.clear());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.focus());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.hover());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.clicker().left());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.clicker().middle(false));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.remove_attr("data-role"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.check(false, true));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.uncheck(true));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set_value("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set_attr("data-role", "demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set_style("display", "block"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set_inner_html("<span>demo</span>"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set().value("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set().attr("data-role", "demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.set().property("tabIndex", &serde_json::json!(3)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_top());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_bottom());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_location(1.0, 2.0));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_up(1.0));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_down(1.0));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_left(1.0));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_right(1.0));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_see(Some(true)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll_to_center());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_top());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_half());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.scroll().to_rightmost());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_text("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_text_with_timeout("demo", Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_value("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_value_with_timeout("demo", Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_index(1));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_index([1, 2]));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_locator("css:option"));
            let _ = web_elements.search_one(&search).and_then(|element| {
                element.select_by_locator_with_timeout("css:option", Some(1_000))
            });
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_indices(&[1, 2]));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_by_indices_with_timeout(&[1, 2], Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_text("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_text_with_timeout("demo", Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_value("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_value_with_timeout("demo", Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_index(1));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_index([1, 2]));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_indices(&[1, 2]));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_indices_with_timeout(&[1, 2], Some(1_000)));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.cancel_by_locator("css:option"));
            let _ = web_elements.search_one(&search).and_then(|element| {
                element.cancel_by_locator_with_timeout("css:option", Some(1_000))
            });
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_clear());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_all());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_invert());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_is_multi());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_options());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_selected_option());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select_selected_options());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select().by_text("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select().cancel_by_value("demo"));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select().is_multi());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.select().selected_options());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.attrs());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.child_count());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.css_path());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.xpath());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.comments());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.states().is_alive());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.states().is_clickable());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().corners());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_corners());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().location());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_location());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().midpoint());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_midpoint());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().click_point());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().viewport_click_point());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_location());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_midpoint());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().screen_click_point());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().scroll_position());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.rect().size());
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.wait().displayed(1_000));
            let _ = web_elements
                .search_one(&search)
                .and_then(|element| element.wait().stop_moving(1_000));

            let _ = session_elements
                .filter()
                .attr("href", "https://example.test", true);
            let _ = session_elements.filter().text("demo", true, true);
            let _ = session_elements.filter().tag("a", true);
            let _ = session_elements.filter().get().texts();
            let _ = session_elements
                .filter_one()
                .attr("href", "https://example.test", true);
            let _ = session_elements.filter_one().text("demo", true, true);
            let _ = session_elements.filter_one().tag("a", true);
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.tag());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.html());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.inner_html());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.value());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.attrs());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.child_count());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.css_path());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.xpath());
            let _ = session_elements
                .filter_one()
                .tag("a", true)
                .and_then(|element| element.comments());
        }

        let _ = assert_calls as fn(&Vec<Element>, &Vec<WebElement>, &Vec<crate::SessionElement>);
    }

    #[test]
    fn page_and_webpage_page_operation_signatures_accept_by_tuples_and_element_refs() {
        fn assert_calls(
            page: &Page,
            web_page: &WebPage,
            element: &Element,
            web_element: &WebElement,
        ) {
            let info = [("innerText", "demo"), ("href", "https://example.test")];
            let value_info = [
                ("tabIndex", serde_json::json!(3)),
                ("draggable", serde_json::json!(false)),
            ];

            let _ = page.remove_element((By::ID, "root"));
            let _ = page.remove_element(element);
            let _ = page.remove_ele((By::ID, "root"));
            let _ = page.remove_ele(element);
            let _ = page.add_element_html(
                "<div>demo</div>",
                Some((By::ID, "root")),
                Some((By::TAG_NAME, "span")),
            );
            let _ = page.add_element("<div>demo</div>", Some(element), Some(element));
            let _ = page.add_element(("a", &info), None::<&str>, None::<&str>);
            let _ = page.add_ele("<div>demo</div>", Some(element), Some(element));
            let _ = page.add_ele(("a", &info), None::<&str>, None::<&str>);
            let _ = page.add_element_html("<div>demo</div>", Some(element), Some(element));
            let _ = page.add_element_info(("a", &info), None::<&str>, None::<&str>);
            let _ = page.add_element_info(("a", &info), Some(element), Some(element));
            let _ = page.add_element_info(("button", &value_info), Some(element), Some(element));
            let _ = web_page.remove_element((By::ID, "root"));
            let _ = web_page.remove_element(web_element);
            let _ = web_page.remove_ele((By::ID, "root"));
            let _ = web_page.remove_ele(web_element);
            let _ = web_page.add_element_html(
                "<div>demo</div>",
                Some((By::ID, "root")),
                Some((By::TAG_NAME, "span")),
            );
            let _ = web_page.add_element("<div>demo</div>", Some(web_element), Some(web_element));
            let _ = web_page.add_element(("a", &info), None::<&str>, None::<&str>);
            let _ = web_page.add_ele("<div>demo</div>", Some(web_element), Some(web_element));
            let _ = web_page.add_ele(("a", &info), None::<&str>, None::<&str>);
            let _ =
                web_page.add_element_html("<div>demo</div>", Some(web_element), Some(web_element));
            let _ = web_page.add_element_info(("a", &info), None::<&str>, None::<&str>);
            let _ = web_page.add_element_info(("a", &info), Some(web_element), Some(web_element));
            let _ = web_page.add_element_info(
                ("button", &value_info),
                Some(web_element),
                Some(web_element),
            );
        }

        let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement);
    }

    #[test]
    fn page_and_webpage_wait_signatures_accept_by_tuples_and_element_refs() {
        fn assert_calls(
            page: &Page,
            web_page: &WebPage,
            element: &Element,
            web_element: &WebElement,
            session_element: &SessionElement,
        ) {
            let locators = vec!["#root".to_string(), ".item".to_string()];
            let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
            let mixed_locators = [
                LocatorInput::from("#root"),
                LocatorInput::from((By::CLASS_NAME, "item")),
            ];
            let session_web_element = WebElement::Session(session_element.clone());

            let _ = page.wait_for((By::ID, "root"), 1_000);
            let _ = page.wait_for_elements_loaded((By::ID, "root"), false, 1_000);
            let _ = page.wait_for_elements_loaded(&locators, false, 1_000);
            let _ = page.wait_for_elements_loaded(&tuple_locators, false, 1_000);
            let _ = page.wait_for_elements_loaded(&mixed_locators, false, 1_000);
            let _ = page.wait_for_ele_displayed((By::ID, "root"), 1_000);
            let _ = page.wait_for_ele_hidden((By::ID, "root"), 1_000);
            let _ = page.wait_for_ele_enabled((By::ID, "root"), 1_000);
            let _ = page.wait_for_ele_deleted((By::ID, "root"), 1_000);
            let _ = page.wait_for_ele_clickable((By::ID, "root"), 1_000);
            let _ = page.wait_for_ele_displayed(element, 1_000);
            let _ = page.wait_for_ele_hidden(element, 1_000);
            let _ = page.wait_for_ele_enabled(element, 1_000);
            let _ = page.wait_for_ele_deleted(element, 1_000);
            let _ = page.wait_for_ele_clickable(element, 1_000);
            let _ = page.wait_for_ele_displayed(session_element, 1_000);
            let _ = page.wait_for_ele_hidden(session_element, 1_000);
            let _ = page.wait_for_ele_enabled(session_element, 1_000);
            let _ = page.wait_for_ele_deleted(session_element, 1_000);
            let _ = page.wait_for_ele_clickable(session_element, 1_000);
            let _ = page.wait_for_ele_displayed(&session_web_element, 1_000);
            let _ = page.wait_for_ele_hidden(&session_web_element, 1_000);
            let _ = page.wait_for_ele_enabled(&session_web_element, 1_000);
            let _ = page.wait_for_ele_deleted(&session_web_element, 1_000);
            let _ = page.wait_for_ele_clickable(&session_web_element, 1_000);
            let _ = web_page.wait_for_elements_loaded((By::ID, "root"), false, 1_000);
            let _ = web_page.wait_for_elements_loaded(&locators, false, 1_000);
            let _ = web_page.wait_for_elements_loaded(&tuple_locators, false, 1_000);
            let _ = web_page.wait_for_elements_loaded(&mixed_locators, false, 1_000);
            let _ = web_page.wait_for_ele_displayed((By::ID, "root"), 1_000);
            let _ = web_page.wait_for_ele_hidden((By::ID, "root"), 1_000);
            let _ = web_page.wait_for_ele_enabled((By::ID, "root"), 1_000);
            let _ = web_page.wait_for_ele_deleted((By::ID, "root"), 1_000);
            let _ = web_page.wait_for_ele_clickable((By::ID, "root"), 1_000);
            let _ = web_page.wait_for_ele_displayed(web_element, 1_000);
            let _ = web_page.wait_for_ele_hidden(web_element, 1_000);
            let _ = web_page.wait_for_ele_enabled(web_element, 1_000);
            let _ = web_page.wait_for_ele_deleted(web_element, 1_000);
            let _ = web_page.wait_for_ele_clickable(web_element, 1_000);
            let _ = web_page.wait_for_ele_displayed(session_element, 1_000);
            let _ = web_page.wait_for_ele_hidden(session_element, 1_000);
            let _ = web_page.wait_for_ele_enabled(session_element, 1_000);
            let _ = web_page.wait_for_ele_deleted(session_element, 1_000);
            let _ = web_page.wait_for_ele_clickable(session_element, 1_000);
            let _ = web_page.wait_for_ele_displayed(&session_web_element, 1_000);
            let _ = web_page.wait_for_ele_hidden(&session_web_element, 1_000);
            let _ = web_page.wait_for_ele_enabled(&session_web_element, 1_000);
            let _ = web_page.wait_for_ele_deleted(&session_web_element, 1_000);
            let _ = web_page.wait_for_ele_clickable(&session_web_element, 1_000);
        }

        let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement, &SessionElement);
    }

    #[test]
    fn page_webpage_and_session_page_set_cookies_accept_supported_inputs() {
        fn assert_calls(page: &Page, web_page: &WebPage, session_page: &SessionPage) {
            let cookie = SessionCookieParam {
                name: "sid".to_string(),
                value: "abc".to_string(),
                url: Some("https://example.test/".to_string()),
                domain: None,
                path: Some("/".to_string()),
                secure: true,
                http_only: true,
                same_site: Some("Lax".to_string()),
            };
            let cookies = vec![cookie.clone()];
            let cookie_json = json!({
                "token": "xyz",
                "domain": ".example.test",
                "path": "/",
                "secure": true,
                "httpOnly": true,
                "sameSite": "Strict"
            });

            let _ = page.set_cookies("sid=abc; domain=.example.test; path=/");
            let _ = page.set_cookies(&cookie);
            let _ = page.set_cookies(&cookies);
            let _ = page.set_cookies(&cookie_json);

            let _ = web_page.set_cookies("sid=abc; domain=.example.test; path=/");
            let _ = web_page.set_cookies(&cookie);
            let _ = web_page.set_cookies(&cookies);
            let _ = web_page.set_cookies(&cookie_json);

            let _ = session_page.set_cookies("sid=abc; domain=.example.test; path=/");
            let _ = session_page.set_cookies(&cookie);
            let _ = session_page.set_cookies(&cookies);
            let _ = session_page.set_cookies(&cookie_json);
        }

        let _ = assert_calls as fn(&Page, &WebPage, &SessionPage);
    }

    #[test]
    fn webpage_session_wait_for_ele_methods_accept_session_element_targets_at_runtime() {
        let (page, temp_dir) =
            launch_headless_test_webpage("session-wait-ele-targets", WebMode::Session)
                .expect("launch headless webpage");
        let html_path = temp_dir.join("session-wait.html");
        let html_path_str = html_path.to_str().expect("html path str");

        let result = (|| -> crate::OpenPageResult<()> {
            write_test_html(
                &html_path,
                r#"
                <html>
                  <body>
                    <button id="ready">Ready</button>
                    <button id="delete-me">Delete me</button>
                  </body>
                </html>
                "#,
            )?;
            assert!(page.get(html_path_str)?);

            let ready = page.snapshot_find("#ready")?;
            let ready_web = page.find("#ready")?;
            let delete_me = page.snapshot_find("#delete-me")?;
            let delete_me_web = page.find("#delete-me")?;

            assert!(page.wait_for_ele_displayed(&ready, 1_000)?);
            assert!(page.wait_for_ele_enabled(&ready, 1_000)?);
            assert!(page.wait_for_ele_clickable(&ready, 1_000)?);
            assert!(page.wait_for_ele_displayed(&ready_web, 1_000)?);
            assert!(page.wait_for_ele_enabled(&ready_web, 1_000)?);
            assert!(page.wait_for_ele_clickable(&ready_web, 1_000)?);

            write_test_html(
                &html_path,
                r#"
                <html>
                  <body>
                    <button id="ready">Ready</button>
                  </body>
                </html>
                "#,
            )?;
            assert!(page.get(html_path_str)?);

            assert!(page.wait_for_ele_hidden(&delete_me, 1_000)?);
            assert!(page.wait_for_ele_deleted(&delete_me, 1_000)?);
            assert!(page.wait_for_ele_hidden(&delete_me_web, 1_000)?);
            assert!(page.wait_for_ele_deleted(&delete_me_web, 1_000)?);
            Ok(())
        })();

        let close_result = page.quit();
        let _ = fs::remove_dir_all(&temp_dir);

        if let Err(err) = close_result {
            panic!("close headless webpage: {err}");
        }
        result.expect("session element target wait regression");
    }

    #[test]
    fn element_shadow_root_and_webelement_find_signatures_accept_by_tuples() {
        fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
            let _ = element.find((By::ID, "root"));
            let _ = element.find_all((By::CLASS_NAME, "item"));
            let _ = shadow_root.find((By::ID, "root"));
            let _ = shadow_root.find_all((By::CLASS_NAME, "item"));
            let _ = web_element.find((By::ID, "root"));
            let _ = web_element.find_all((By::CLASS_NAME, "item"));
        }

        let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
    }

    #[test]
    fn element_shadow_root_and_webelement_parent_child_signatures_accept_by_tuples() {
        fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
            let _ = element.parent_with((By::ID, "root"), 1);
            let _ = element.child_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = element.children_with(Some((By::CLASS_NAME, "item")));
            let _ = shadow_root.parent_with((By::ID, "root"), 1);
            let _ = shadow_root.child_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = shadow_root.children_with(Some((By::CLASS_NAME, "item")));
            let _ = web_element.parent_with((By::ID, "root"), 1);
            let _ = web_element.child_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = web_element.children_with(Some((By::CLASS_NAME, "item")));
        }

        let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
    }

    #[test]
    fn element_shadow_root_and_webelement_prev_next_signatures_accept_by_tuples() {
        fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
            let _ = element.prev_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = element.prevs_with(Some((By::CLASS_NAME, "item")));
            let _ = element.next_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = element.nexts_with(Some((By::CLASS_NAME, "item")));
            let _ = shadow_root.next_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = shadow_root.nexts_with(Some((By::CLASS_NAME, "item")));
            let _ = web_element.prev_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = web_element.prevs_with(Some((By::CLASS_NAME, "item")));
            let _ = web_element.next_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = web_element.nexts_with(Some((By::CLASS_NAME, "item")));
        }

        let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
    }

    #[test]
    fn element_shadow_root_and_webelement_before_after_signatures_accept_by_tuples() {
        fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
            let _ = element.before_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = element.befores_with(Some((By::CLASS_NAME, "item")));
            let _ = element.after_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = element.afters_with(Some((By::CLASS_NAME, "item")));
            let _ = shadow_root.before_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = shadow_root.befores_with(Some((By::CLASS_NAME, "item")));
            let _ = shadow_root.after_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = shadow_root.afters_with(Some((By::CLASS_NAME, "item")));
            let _ = web_element.before_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = web_element.befores_with(Some((By::CLASS_NAME, "item")));
            let _ = web_element.after_with(Some((By::CLASS_NAME, "item")), 1);
            let _ = web_element.afters_with(Some((By::CLASS_NAME, "item")));
        }

        let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
    }

    #[test]
    fn element_and_webelement_offset_signatures_accept_by_tuples() {
        fn assert_calls(element: &Element, web_element: &WebElement) {
            let _ = element.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
            let _ = web_element.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
        }

        let _ = assert_calls as fn(&Element, &WebElement);
    }
}
