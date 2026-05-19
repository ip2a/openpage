use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::browser::{Browser, LaunchOptions};
use crate::download::DownloadMission;
use crate::element::Element;
use crate::error::{OpenPageError, OpenPageResult};
use crate::listener::Listener;
use crate::session::{CookieEntry, SessionElement, SessionOptions, SessionPage};

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

impl WebElement {
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

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self {
            Self::Browser(element) => element.attr(name),
            Self::Session(element) => element.attr(name),
        }
    }

    pub fn click(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.click(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "click() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn input(&self, text: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.input(text),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "input() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.clear(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "clear() is only available in driver mode".to_string(),
            )),
        }
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.press_key(key),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "press_key() is only available in driver mode".to_string(),
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

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        match self {
            Self::Browser(element) => element.save_screenshot(path),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                "save_screenshot() is only available in driver mode".to_string(),
            )),
        }
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
        self.mode
            .lock()
            .map(|mode| *mode)
            .map_err(|_| OpenPageError::BrowserOperation("webpage mode lock poisoned".to_string()))
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        self.browser.tabs_count()
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        self.browser.tab_ids()
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.browser.download_path()
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.browser.set_download_path(path)
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

    pub fn listener(&self) -> Listener {
        self.driver.listener()
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

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<bool> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.post_json(url, payload)
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
        self.browser.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.browser
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn wait_for_url_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.url()?;
            Ok(value
                .as_ref()
                .is_some_and(|value| if exclude { !value.contains(text) } else { value.contains(text) }))
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
            Ok(value
                .as_ref()
                .is_some_and(|value| if exclude { !value.contains(text) } else { value.contains(text) }))
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

    pub fn wait_for_elements_loaded(
        &self,
        locators: &[String],
        any_one: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_elements_loaded(locators, any_one, timeout_ms),
            WebMode::Session => {
                let timeout = Duration::from_millis(timeout_ms.max(1));
                let deadline = Instant::now() + timeout;
                loop {
                    let mut matched = 0usize;
                    for locator in locators {
                        if !self.session.find_all(locator)?.is_empty() {
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
        }
    }

    pub fn wait_for_ele_displayed(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_ele_displayed(locator, timeout_ms),
            WebMode::Session => self.session_wait_for_element(locator, timeout_ms, |ele| {
                Ok(ele.attr("disabled")?.is_none())
            }),
        }
    }

    pub fn wait_for_ele_hidden(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_ele_hidden(locator, timeout_ms),
            WebMode::Session => match self.session.find(locator) {
                Ok(_) => Ok(false),
                Err(_) => Ok(true),
            },
        }
    }

    pub fn wait_for_ele_enabled(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_ele_enabled(locator, timeout_ms),
            WebMode::Session => self.session_wait_for_element(locator, timeout_ms, |ele| {
                Ok(ele.attr("disabled")?.is_none())
            }),
        }
    }

    pub fn wait_for_ele_deleted(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_ele_deleted(locator, timeout_ms),
            WebMode::Session => self.session_wait_for_element(locator, timeout_ms, |_ele| {
                Ok(self.session.find(locator).is_err())
            }),
        }
    }

    pub fn wait_for_ele_clickable(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.mode()? {
            WebMode::Driver => self.driver.wait_for_ele_clickable(locator, timeout_ms),
            WebMode::Session => self.session_wait_for_element(locator, timeout_ms, |ele| {
                Ok(ele.attr("disabled")?.is_none())
            }),
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
                        return Ok(false);
                    }
                }
            }
        }
    }

    pub fn find(&self, locator: &str) -> OpenPageResult<WebElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.find(locator).map(WebElement::Browser),
            WebMode::Session => self.session.find(locator).map(WebElement::Session),
        }
    }

    pub fn find_all(&self, locator: &str) -> OpenPageResult<Vec<WebElement>> {
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

    pub fn quit(&self) -> OpenPageResult<()> {
        self.browser.close()
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

    fn set_mode(&self, mode: WebMode) -> OpenPageResult<()> {
        let mut current = self.mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation("webpage mode lock poisoned".to_string())
        })?;
        *current = mode;
        Ok(())
    }
}
