use super::*;

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
