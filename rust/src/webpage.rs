use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::browser::{Browser, LaunchOptions};
use crate::element::Element;
use crate::error::{OpenPageError, OpenPageResult};
use crate::session::{SessionElement, SessionOptions, SessionPage};

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

    pub fn html(&self) -> OpenPageResult<String> {
        match self.mode()? {
            WebMode::Driver => self.driver.html(),
            WebMode::Session => self.session.html(),
        }
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.status_code(),
        }
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.json(),
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
            self.session.set_user_agent(Some(self.driver.user_agent()?))?;
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

    fn set_mode(&self, mode: WebMode) -> OpenPageResult<()> {
        let mut current = self
            .mode
            .lock()
            .map_err(|_| OpenPageError::BrowserOperation("webpage mode lock poisoned".to_string()))?;
        *current = mode;
        Ok(())
    }
}
