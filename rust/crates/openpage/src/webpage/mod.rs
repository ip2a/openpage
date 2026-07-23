mod assets;
mod cookies;
mod element;
mod extraction;
mod frame;
mod html;
mod operations;
mod parsing;
mod request;
mod response;
mod settings;
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
    CookieEntry, CookieInput, DocumentElement, HeadersInput, Session, SessionDownload,
    SessionEncodingInput, SessionOptions, SessionXPathResult,
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
    Session(DocumentElement),
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
    session: Session,
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
    session: Session,
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
        let session = Session::new(session_options)?;
        Ok(Self {
            browser,
            driver,
            session,
            mode: Arc::new(Mutex::new(mode)),
        })
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
