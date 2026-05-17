use std::path::Path;
use std::sync::Arc;

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
}
