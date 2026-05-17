use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, PrintToPdfParams};
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chromiumoxide::page::{Page as OxPage, ScreenshotParams};
use serde_json::Value;
use tokio::runtime::Runtime;
use url::Url;

use crate::element::Element;
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{Locator, LocatorKind};
use crate::session::{CookieEntry, SessionElement, cookies_from_header, snapshot_find, snapshot_find_all, snapshot_root};

#[derive(Clone, Debug)]
pub struct Page {
    runtime: Arc<Runtime>,
    inner: OxPage,
}

impl Page {
    pub(crate) fn new(runtime: Arc<Runtime>, inner: OxPage) -> Self {
        Self { runtime, inner }
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .goto(url)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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

    pub fn html(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            self.inner
                .content()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub fn evaluate(&self, expression: &str) -> OpenPageResult<Value> {
        self.runtime.block_on(async {
            let result = self
                .inner
                .evaluate(expression)
                .await
                .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
            result
                .into_value::<Value>()
                .map_err(|err| OpenPageError::JavaScript(err.to_string()))
        })
    }

    pub fn find(&self, locator: &str) -> OpenPageResult<Element> {
        let locator = Locator::parse(locator)?;
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
            Ok(Element::new(Arc::clone(&self.runtime), element))
        })
    }

    pub fn find_all(&self, locator: &str) -> OpenPageResult<Vec<Element>> {
        let locator = Locator::parse(locator)?;
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
                .map(|element| Element::new(Arc::clone(&self.runtime), element))
                .collect())
        })
    }

    pub fn wait_for(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<Element> {
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

    pub fn click(&self, locator: &str) -> OpenPageResult<()> {
        self.wait_for(locator, 10_000)?.click()
    }

    pub fn fill(&self, locator: &str, text: &str) -> OpenPageResult<()> {
        self.wait_for(locator, 10_000)?.input(text)
    }

    pub fn text(&self, locator: &str) -> OpenPageResult<Option<String>> {
        self.wait_for(locator, 10_000)?.text()
    }

    pub fn attr(&self, locator: &str, name: &str) -> OpenPageResult<Option<String>> {
        self.wait_for(locator, 10_000)?.attr(name)
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

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .save_pdf(PrintToPdfParams::default(), path)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        self.evaluate(script)
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        snapshot_find(&self.html()?, locator)
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        snapshot_find_all(&self.html()?, locator)
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

    pub fn close(self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .close()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
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
