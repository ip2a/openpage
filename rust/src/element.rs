use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::element::Element as OxElement;
use serde_json::Value;
use tokio::runtime::Runtime;

use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{Locator, LocatorKind};

#[derive(Debug)]
pub struct Element {
    runtime: Arc<Runtime>,
    inner: OxElement,
}

impl Element {
    pub(crate) fn new(runtime: Arc<Runtime>, inner: OxElement) -> Self {
        Self { runtime, inner }
    }

    pub fn click(&self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .click()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn input(&self, text: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .click()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            self.inner
                .type_str(text)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .call_js_fn(
                    "function() { if ('value' in this) { this.value = ''; } else { this.textContent = ''; } }",
                    true,
                )
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .press_key(key)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .inner_text()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .outer_html()
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            self.inner
                .attribute(name)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))
        })
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        let js = format!("function() {{ {script} }}");
        self.runtime.block_on(async {
            let result = self
                .inner
                .call_js_fn(js, true)
                .await
                .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
            Ok(result.result.value.unwrap_or(Value::Null))
        })
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            self.inner
                .save_screenshot(CaptureScreenshotFormat::Png, path)
                .await
                .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn is_selected(&self) -> OpenPageResult<bool> {
        value_as_bool(self.run_js("return !!this.selected;")?, "selected")
    }

    pub fn is_checked(&self) -> OpenPageResult<bool> {
        value_as_bool(self.run_js("return !!this.checked;")?, "checked")
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const style = window.getComputedStyle(this); \
                 return !(style.visibility === 'hidden' || style.display === 'none' || this.hidden);",
            )?,
            "displayed",
        )
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        value_as_bool(self.run_js("return !this.disabled;")?, "enabled")
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self.run_js("return !!this.isConnected;") {
            Ok(value) => value_as_bool(value, "alive"),
            Err(_) => Ok(false),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 return !!(rect.width && rect.height);",
            )?,
            "has_rect",
        )
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return false; } \
                 const x = rect.left + rect.width / 2; \
                 const y = rect.top + rect.height / 2; \
                 return x >= 0 && y >= 0 && x <= window.innerWidth && y <= window.innerHeight;",
            )?,
            "in_viewport",
        )
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const style = window.getComputedStyle(this); \
                 const rect = this.getBoundingClientRect(); \
                 return !!(rect.width && rect.height) \
                    && !this.disabled \
                    && !(style.visibility === 'hidden' || style.display === 'none' || this.hidden) \
                    && style.pointerEvents !== 'none';",
            )?,
            "clickable",
        )
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_displayed(), false)
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_displayed().map(|value| !value), true)
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_enabled(), false)
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_enabled().map(|value| !value), false)
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_alive().map(|value| !value), true)
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
                LocatorKind::XPath => {
                    return Err(OpenPageError::UnsupportedLocator(
                        "xpath child element lookup is not implemented yet".to_string(),
                    ));
                }
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
                LocatorKind::XPath => {
                    return Err(OpenPageError::UnsupportedLocator(
                        "xpath child element lookup is not implemented yet".to_string(),
                    ));
                }
            };
            Ok(elements
                .into_iter()
                .map(|element| Element::new(Arc::clone(&self.runtime), element))
                .collect())
        })
    }

    fn wait_until<F>(&self, timeout_ms: u64, mut predicate: F, treat_errors_as_success: bool) -> OpenPageResult<bool>
    where
        F: FnMut(&Self) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match predicate(self) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(_) if treat_errors_as_success => return Ok(true),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(err);
                    }
                }
            }

            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }
}

fn value_as_bool(value: Value, name: &str) -> OpenPageResult<bool> {
    match value {
        Value::Bool(value) => Ok(value),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} state script did not return a bool: {other}"
        ))),
    }
}
