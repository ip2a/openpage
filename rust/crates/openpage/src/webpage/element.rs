use super::*;

impl WebElement {
    pub(super) fn browser_element(&self) -> Option<&Element> {
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

    pub fn snapshot_root(&self) -> OpenPageResult<DocumentElement> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.snapshot_root(),
            Self::Session(element) => Ok(element.clone()),
        }
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.snapshot_find(locator),
            Self::Session(element) => element.find(locator),
        }
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
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

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<DocumentElement> {
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
    ) -> OpenPageResult<Vec<DocumentElement>> {
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
