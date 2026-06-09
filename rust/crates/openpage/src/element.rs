use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    InsertTextParams, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::cdp::browser_protocol::page::GetResourceContentParams;
use chromiumoxide::cdp::browser_protocol::{
    dom::{
        BackendNodeId, DescribeNodeParams, GetBoxModelParams, GetFrameOwnerParams,
        GetNodeForLocationParams, RemoveAttributeParams, RequestNodeParams, ResolveNodeParams,
        SetAttributeValueParams, SetFileInputFilesParams,
    },
    page::{FrameId, GetFrameTreeParams},
};
use chromiumoxide::element::Element as OxElement;
use chromiumoxide::keys;
use chromiumoxide::layout::Point;
use chromiumoxide::page::Page as OxPage;
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::time::timeout as tokio_timeout;

use crate::browser::Browser;
use crate::download::DownloadMission;
use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfigHandle, elements_one_should_raise_when_missing,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{
    Locator, LocatorBatchInput, LocatorInput, LocatorKind, LocatorMatch, collect_locator_matches,
    parse_locator_batch_input, parse_optional_locator_input,
};
use crate::page::{
    ActionsInput, Frame, Page, PageFrameTarget, execute_page_command_async,
    execute_page_command_blocking, frame_locator_input,
};
use crate::session::{
    SessionElement, SessionXPathResult, snapshot_fragment_find_all_with_base_url,
    snapshot_fragment_find_with_base_url, snapshot_fragment_query_xpath_with_base_url,
    snapshot_fragment_root_with_base_url,
};
use crate::settings::{
    blob_src_data_url_required_message, browser_backed_element_only_message, cdp_timeout_duration,
    click_at_count_must_be_positive_message, click_failed_hidden_or_disabled_message,
    click_failed_no_rect_message, click_failed_should_raise, data_url_missing_comma_message,
    element_frame_viewport_offset_unavailable_message, element_html_unavailable_message,
    element_no_visible_rect_message, element_operation_failed_message,
    element_rect_corner_coordinate_count_message, element_rect_corners_parse_failed_message,
    element_rect_corners_unexpected_value_message, element_resource_unavailable_message,
    element_tag_name_unavailable_message, element_top_frame_check_failed_message,
    frame_index_must_start_message, frame_index_out_of_range_message,
    javascript_execution_timed_out_message, multi_select_action_required_message,
    no_new_tab_message, parent_element_index_must_start_message,
    parent_element_level_must_start_message, relative_direction_index_must_start_message,
    resolve_element_frame_id_failed_message, resolve_frame_owner_viewport_location_failed_message,
    resolve_frame_viewport_offset_failed_message,
    resolve_top_viewport_screen_origin_failed_message,
    resolve_top_window_device_pixel_ratio_failed_message, resolved_node_missing_object_id_message,
    scan_frame_marker_failed_message, scan_frame_marker_javascript_failed_message,
    select_element_required_message, session_backed_element_driver_target_message,
    set_file_input_requires_at_least_one_file_message, shadow_root_object_id_unavailable_message,
    timeout_duration_millis, timeout_error, top_window_device_pixel_ratio_not_numeric_message,
    top_window_viewport_size_lookup_failed_message, unsupported_key_message,
    unsupported_mouse_button_message, value_coordinate_not_numeric_message,
    value_coordinate_pair_exactly_two_message, value_coordinate_pair_parse_failed_message,
    value_coordinate_pair_required_message, value_non_negative_integer_required_message,
    value_number_required_message, value_state_bool_required_message,
    value_string_compatible_required_message, value_string_required_message,
    value_string_vec_array_required_message, value_string_vec_entry_required_message,
    value_unavailable_message, wait_for_locator_timed_out_message, wait_timeout_result,
};
use crate::shadow_root::ShadowRoot;
use crate::upload::UploadTracker;

const MARKER_ATTRIBUTE: &str = "data-openpage-marker";
static NEXT_MARKER_BATCH: AtomicU64 = AtomicU64::new(1);
const DEFAULT_CLICK_TIMEOUT_MS: u64 = 1_500;
const MODIFIER_ALT: i64 = 1;
const MODIFIER_CTRL: i64 = 2;
const MODIFIER_META: i64 = 4;
const MODIFIER_SHIFT: i64 = 8;

fn element_operation_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::PageOperation(element_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

async fn run_element_page_future_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| element_operation_error(operation, err))
}

async fn run_element_future_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| element_operation_error(operation, err))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelativeDirection {
    East,
    South,
    West,
    North,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementResource {
    Bytes(Vec<u8>),
    Text(String),
}

impl ElementResource {
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Bytes(_) => None,
            Self::Text(text) => Some(text),
        }
    }

    pub fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Text(_) => None,
        }
    }

    pub fn into_text(self) -> Option<String> {
        match self {
            Self::Bytes(_) => None,
            Self::Text(text) => Some(text),
        }
    }
}

#[derive(Debug)]
pub struct Element {
    runtime: Arc<Runtime>,
    page: OxPage,
    browser: Option<Browser>,
    uploader: Option<UploadTracker>,
    inner: OxElement,
    javascript_timeout_ms: u64,
    none_element_config: ElementsOneRuntimeConfigHandle,
}

pub struct ElementClicker<'a> {
    element: &'a Element,
}

pub struct ElementScroller<'a> {
    element: &'a Element,
}

pub struct ElementSetter<'a> {
    element: &'a Element,
}

pub struct ElementSelector<'a> {
    element: &'a Element,
}

pub struct ElementStates<'a> {
    element: &'a Element,
}

pub struct ElementRect<'a> {
    element: &'a Element,
}

pub struct ElementWait<'a> {
    element: &'a Element,
}

pub enum SelectIndexInput {
    Single(usize),
    Many(Vec<usize>),
}

pub enum SelectOptionInput<'a> {
    Single(&'a Element),
    Many(Vec<&'a Element>),
}

impl From<usize> for SelectIndexInput {
    fn from(value: usize) -> Self {
        Self::Single(value)
    }
}

impl<'a> From<&'a [usize]> for SelectIndexInput {
    fn from(value: &'a [usize]) -> Self {
        Self::Many(value.to_vec())
    }
}

impl<'a> From<&'a Vec<usize>> for SelectIndexInput {
    fn from(value: &'a Vec<usize>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<Vec<usize>> for SelectIndexInput {
    fn from(value: Vec<usize>) -> Self {
        Self::Many(value)
    }
}

impl<const N: usize> From<[usize; N]> for SelectIndexInput {
    fn from(value: [usize; N]) -> Self {
        Self::Many(value.into_iter().collect())
    }
}

impl<'a, const N: usize> From<&'a [usize; N]> for SelectIndexInput {
    fn from(value: &'a [usize; N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a Element> for SelectOptionInput<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Single(value)
    }
}

impl<'a> From<&'a [&'a Element]> for SelectOptionInput<'a> {
    fn from(value: &'a [&'a Element]) -> Self {
        Self::Many(value.to_vec())
    }
}

impl<'a> From<&'a Vec<&'a Element>> for SelectOptionInput<'a> {
    fn from(value: &'a Vec<&'a Element>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<Vec<&'a Element>> for SelectOptionInput<'a> {
    fn from(value: Vec<&'a Element>) -> Self {
        Self::Many(value)
    }
}

impl<'a, const N: usize> From<[&'a Element; N]> for SelectOptionInput<'a> {
    fn from(value: [&'a Element; N]) -> Self {
        Self::Many(value.into_iter().collect())
    }
}

impl<'a, const N: usize> From<&'a [&'a Element; N]> for SelectOptionInput<'a> {
    fn from(value: &'a [&'a Element; N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl Element {
    pub(crate) fn new(
        runtime: Arc<Runtime>,
        page: OxPage,
        browser: Option<Browser>,
        uploader: Option<UploadTracker>,
        inner: OxElement,
        javascript_timeout_ms: u64,
        none_element_config: ElementsOneRuntimeConfigHandle,
    ) -> Self {
        Self {
            runtime,
            page,
            browser,
            uploader,
            inner,
            javascript_timeout_ms,
            none_element_config,
        }
    }

    pub(crate) fn backend_node_id(&self) -> BackendNodeId {
        self.inner.backend_node_id
    }

    pub(crate) fn none_element_runtime_config_handle(&self) -> &ElementsOneRuntimeConfigHandle {
        &self.none_element_config
    }

    pub fn scroll(&self) -> ElementScroller<'_> {
        ElementScroller { element: self }
    }

    pub fn clicker(&self) -> ElementClicker<'_> {
        ElementClicker { element: self }
    }

    pub fn set(&self) -> ElementSetter<'_> {
        ElementSetter { element: self }
    }

    pub fn select(&self) -> ElementSelector<'_> {
        ElementSelector { element: self }
    }

    pub fn states(&self) -> ElementStates<'_> {
        ElementStates { element: self }
    }

    pub fn rect(&self) -> ElementRect<'_> {
        ElementRect { element: self }
    }

    pub fn wait(&self) -> ElementWait<'_> {
        ElementWait { element: self }
    }

    pub fn click(&self) -> OpenPageResult<()> {
        let _ = self.click_with_options(Some(false), None, true)?;
        Ok(())
    }

    pub fn click_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        if self.click_option_via_select()? {
            return Ok(true);
        }
        if by_js == Some(true) {
            self.run_js("this.click(); return true;")?;
            return Ok(true);
        }

        let timeout_ms = timeout_ms.unwrap_or(DEFAULT_CLICK_TIMEOUT_MS).max(1);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut has_rect = self.has_rect()?;
        while !has_rect && Instant::now() < deadline {
            sleep(Duration::from_millis(1));
            has_rect = self.has_rect()?;
        }

        if !has_rect {
            if by_js == Some(false) {
                return Err(OpenPageError::PageOperation(click_failed_no_rect_message()));
            }
            self.run_js("this.click(); return true;")?;
            return Ok(true);
        }

        if wait_stop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                let _ = self.wait_until_stop_moving(remaining.as_millis() as u64);
            }
        }

        self.scroll_to_see(Some(false))?;

        let mut can_click = self.is_enabled()? && self.is_displayed()?;
        while !can_click && Instant::now() < deadline {
            sleep(Duration::from_millis(1));
            can_click = self.is_enabled()? && self.is_displayed()?;
        }

        if !can_click {
            if by_js == Some(false) {
                return Self::click_failed_result(&click_failed_hidden_or_disabled_message());
            }
            self.run_js("this.click(); return true;")?;
            return Ok(true);
        }

        if !self.is_in_viewport()? {
            self.run_js("this.click(); return true;")?;
            return Ok(true);
        }

        if by_js != Some(false) && self.is_covered().unwrap_or(false) {
            self.run_js("this.click(); return true;")?;
            return Ok(true);
        }

        match self.click_at_runtime(None, None, MouseButton::Left, 1) {
            Ok(()) => Ok(true),
            Err(err) => Self::click_failed_outcome(err, false),
        }
    }

    pub fn click_left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.click_with_options(by_js, timeout_ms, wait_stop)
    }

    fn click_option_via_select(&self) -> OpenPageResult<bool> {
        if self.tag()? != "option" {
            return Ok(false);
        }
        value_as_bool(
            self.run_js(
                "const select = this.closest('select'); \
                 if (!(select instanceof HTMLSelectElement)) return false; \
                 if (!this.selected) { \
                     this.selected = true; \
                 } else if (select.multiple) { \
                     this.selected = false; \
                 } \
                 select.dispatchEvent(new Event('input', { bubbles: true })); \
                 select.dispatchEvent(new Event('change', { bubbles: true })); \
                 return true;",
            )?,
            "option click via select",
        )
    }

    fn click_failed_result(message: &str) -> OpenPageResult<bool> {
        if click_failed_should_raise() {
            Err(OpenPageError::PageOperation(message.to_string()))
        } else {
            Ok(false)
        }
    }

    fn click_failed_outcome<T>(err: OpenPageError, fallback: T) -> OpenPageResult<T> {
        if click_failed_should_raise() {
            Err(err)
        } else {
            Ok(fallback)
        }
    }

    pub fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        validate_click_at_count(count)?;
        let button = parse_mouse_button(button)?;
        match self.click_at_runtime(offset_x, offset_y, button, count) {
            Ok(()) => Ok(()),
            Err(err) => Self::click_failed_outcome(err, ()),
        }
    }

    fn click_at_runtime(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: MouseButton,
        count: u32,
    ) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.scroll_into_view(), "scroll into view")
                .await?;
            Ok::<(), OpenPageError>(())
        })?;
        let (x, y) = self.offset_click_point(offset_x, offset_y)?;
        for click_count in 1..=count {
            self.dispatch_mouse_click(x, y, button.clone(), click_count)?;
        }
        Ok(())
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        self.click_at(None, None, "left", times)
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        self.click()
    }

    pub fn click_middle(&self) -> OpenPageResult<()> {
        self.click_at(None, None, "middle", 1)
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        self.click_at(None, None, "right", 1)
    }

    pub fn input(&self, text: &str) -> OpenPageResult<()> {
        self.input_with_options(text, false, false)
    }

    pub fn input_with_options(&self, text: &str, clear: bool, by_js: bool) -> OpenPageResult<()> {
        if self.tag()? == "input" && self.attr("type")?.as_deref() == Some("file") {
            let files = text
                .split('\n')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            return self.set_file_input_files(&files);
        }
        if by_js {
            if clear {
                self.clear_with_mode(true)?;
            }
            self.set_text_value(text)?;
            return Ok(());
        }
        if clear && should_clear_before_typing(text) {
            self.clear_with_mode(false)?;
        } else {
            self.focus_or_click()?;
        }
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            InsertTextParams::new(text.to_string()),
            "Element::input_with_options()",
        )?;
        Ok(())
    }

    pub fn input_keys_with_options(
        &self,
        values: &[String],
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<()> {
        if self.tag()? == "input" && self.attr("type")?.as_deref() == Some("file") {
            return self.set_file_input_files(values);
        }
        if by_js {
            if clear {
                self.clear_with_mode(true)?;
            }
            self.set_text_value(&values.concat())?;
            return Ok(());
        }
        if clear && should_clear_before_typing_sequence(values) {
            self.clear_with_mode(false)?;
        } else {
            self.focus_or_click()?;
        }
        let (modifiers, keys_to_type) = split_text_or_keys_with_modifiers(values);
        if modifiers != 0 {
            for key in keys_to_type {
                self.press_key_with_modifiers(&key, modifiers)?;
            }
            return Ok(());
        }
        for value in values {
            self.type_or_press(value)?;
        }
        Ok(())
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.clear_with_mode(false)
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        if by_js || cfg!(target_os = "macos") || !self.can_keyboard_clear()? {
            self.set_text_value("")?;
            return Ok(());
        }
        self.focus_or_click()?;
        self.run_js(
            "if (typeof this.select === 'function') { \
                 this.select(); \
                 return true; \
             } \
             if (this.isContentEditable) { \
                 const selection = window.getSelection(); \
                 if (!selection) return true; \
                 const range = document.createRange(); \
                 range.selectNodeContents(this); \
                 selection.removeAllRanges(); \
                 selection.addRange(range); \
             } \
             return true;",
        )?;
        self.press_key("Delete")
    }

    fn set_text_value(&self, text: &str) -> OpenPageResult<()> {
        let script = format!(
            "const value = {value}; \
             if ('value' in this) {{ \
                 this.value = value; \
             }} else {{ \
                 this.textContent = value; \
             }} \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            value = json_string(text)?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    fn can_keyboard_clear(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js("return typeof this.select === 'function' || !!this.isContentEditable;")?,
            "keyboard clear support",
        )
    }

    fn focus_or_click(&self) -> OpenPageResult<()> {
        if self.focus().is_ok() {
            return Ok(());
        }
        self.click()
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        self.run_async_js_with_args(script, &[], false)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        self.run_async_js_with_options(script, args, as_expr, None)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        let script = load_javascript_source(script)?;
        let js = build_js_invocation(script.as_ref(), args, as_expr)?;
        let timeout_ms = Some(resolve_javascript_timeout_ms(
            timeout_ms,
            self.javascript_timeout_ms,
        ));
        self.runtime.block_on(async {
            self.call_js_fn_with_timeout(js, false, timeout_ms)
                .await
                .map(|_| ())
        })
    }

    pub fn set_file_input_files(&self, files: &[String]) -> OpenPageResult<()> {
        let files = normalize_file_input_paths(files)?;
        if files.is_empty() {
            return Err(OpenPageError::PageOperation(
                set_file_input_requires_at_least_one_file_message(),
            ));
        }
        let params = SetFileInputFilesParams::builder()
            .files(files)
            .backend_node_id(self.inner.backend_node_id)
            .build()
            .map_err(|err| element_operation_error("build file input params", err))?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            params,
            "Element::set_file_input_files()",
        )?;
        Ok(())
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.press_key(key), "press key").await?;
            Ok(())
        })
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.inner_text(), "read inner text").await
        })
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        match self.property("tagName")? {
            Some(Value::String(tag)) => Ok(tag.to_ascii_lowercase()),
            Some(value) => Err(OpenPageError::JavaScript(format!(
                "tagName did not return a string: {value}"
            ))),
            None => Err(OpenPageError::ElementNotFound(
                element_tag_name_unavailable_message(),
            )),
        }
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.outer_html(), "read outer html").await
        })
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.inner_html(), "read inner html").await
        })
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        let html = self
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(element_html_unavailable_message()))?;
        let base_url = value_as_optional_string(self.property("baseURI")?, "baseURI")?;
        snapshot_fragment_root_with_base_url(&html, base_url.as_deref())
    }

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        let html = self
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(element_html_unavailable_message()))?;
        let base_url = value_as_optional_string(self.property("baseURI")?, "baseURI")?;
        snapshot_fragment_find_with_base_url(&html, locator, base_url.as_deref())
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        let html = self
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(element_html_unavailable_message()))?;
        let base_url = value_as_optional_string(self.property("baseURI")?, "baseURI")?;
        snapshot_fragment_find_all_with_base_url(&html, locator, base_url.as_deref())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        let html = self
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(element_html_unavailable_message()))?;
        let base_url = value_as_optional_string(self.property("baseURI")?, "baseURI")?;
        snapshot_fragment_query_xpath_with_base_url(&html, expression, base_url.as_deref())
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<Element>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.runtime.block_on(async {
            let attrs =
                run_element_future_with_cdp_timeout(self.inner.attributes(), "read attributes")
                    .await?;
            Ok(attrs
                .chunks(2)
                .filter_map(|chunk| match chunk {
                    [name, value] => Some((name.clone(), value.clone())),
                    _ => None,
                })
                .collect())
        })
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match name {
            "href" => {
                let raw = self.runtime.block_on(async {
                    run_element_future_with_cdp_timeout(
                        self.inner.attribute("href"),
                        "read href attribute",
                    )
                    .await
                })?;
                let Some(value) = raw else {
                    return Ok(None);
                };
                let lower = value.to_ascii_lowercase();
                if lower.starts_with("javascript:") || lower.starts_with("mailto:") {
                    return Ok(Some(value));
                }
                Ok(value_as_optional_string(self.property("href")?, "href")?
                    .filter(|value| !value.is_empty()))
            }
            "src" => {
                let raw = self.runtime.block_on(async {
                    run_element_future_with_cdp_timeout(
                        self.inner.attribute("src"),
                        "read src attribute",
                    )
                    .await
                })?;
                if raw.is_none() {
                    return Ok(None);
                }
                Ok(value_as_optional_string(self.property("src")?, "src")?
                    .filter(|value| !value.is_empty()))
            }
            "text" => self.text(),
            "innerText" => self.raw_text(),
            "html" | "outerHTML" => self.html(),
            "innerHTML" => self.inner_html(),
            _ => self.runtime.block_on(async {
                run_element_future_with_cdp_timeout(self.inner.attribute(name), "read attribute")
                    .await
            }),
        }
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.property(name), "read property").await
        })
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.property("textContent")?, "textContent")
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.property("value")?, "value")
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        let href = value_as_optional_string(self.property("href")?, "href")?;
        if href.as_deref().is_some_and(|value| !value.is_empty()) {
            return Ok(href);
        }
        value_as_optional_string(self.property("src")?, "src")
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        value_as_usize(
            self.run_js("return this.childElementCount;")?,
            "child count",
        )
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.path_via_page_marker(false)
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.path_via_page_marker(true)
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.snapshot_root()?.comments()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.snapshot_root()?.texts(text_node_only)
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        let pseudo_arg = match pseudo {
            Some(value) if !value.is_empty() => format!(", {}", json_string(value)?),
            _ => String::new(),
        };
        let script = format!(
            "return window.getComputedStyle(this{pseudo}).getPropertyValue({name});",
            pseudo = pseudo_arg,
            name = json_string(name)?,
        );
        value_as_string(self.run_js(&script)?, "style")
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        self.pseudo_content(&["::before", ":before", "before"])
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        self.pseudo_content(&["::after", ":after", "after"])
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_scroll_script("this.scrollTo(this.scrollLeft, 0); return true;")
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        self.run_scroll_script("this.scrollTo(this.scrollLeft, this.scrollHeight); return true;")
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        self.run_scroll_script(
            "this.scrollTo(this.scrollLeft, this.scrollHeight / 2); return true;",
        )
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        self.run_scroll_script("this.scrollTo(this.scrollWidth, this.scrollTop); return true;")
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        self.run_scroll_script("this.scrollTo(0, this.scrollTop); return true;")
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_scroll_script(&format!("this.scrollTo({x}, {y}); return true;"))
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_scroll_script(&format!("this.scrollBy(0, {}); return true;", -pixels))
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_scroll_script(&format!("this.scrollBy(0, {pixels}); return true;"))
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_scroll_script(&format!("this.scrollBy({}, 0); return true;", -pixels))
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_scroll_script(&format!("this.scrollBy({pixels}, 0); return true;"))
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.scroll_into_view(), "scroll into view")
                .await?;
            Ok::<(), OpenPageError>(())
        })?;
        if center == Some(true) || (center != Some(false) && self.is_covered().unwrap_or(false)) {
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 const delta = rect.top + rect.height / 2 - window.innerHeight / 2; \
                 window.scrollBy(0, delta); \
                 return true;",
            )?;
        }
        Ok(())
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        self.scroll_to_see(Some(true))
    }

    fn run_scroll_script(&self, script: &str) -> OpenPageResult<()> {
        self.run_js(script)?;
        Ok(())
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        let tag = self.tag()?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));

        if tag == "img" {
            while Instant::now() < deadline {
                if value_as_bool(
                    self.run_js(
                        "return this.complete && typeof this.naturalWidth !== 'undefined' \
                         && this.naturalWidth > 0 && typeof this.naturalHeight !== 'undefined' \
                         && this.naturalHeight > 0;",
                    )?,
                    "image loaded",
                )? {
                    break;
                }
                sleep(Duration::from_millis(50));
            }
        }

        let attr_name = src_attribute_name(&tag);
        let mut src = self
            .attr(attr_name)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                OpenPageError::ElementNotFound(format!(
                    "element <{tag}> does not have a usable {attr_name} attribute"
                ))
            })?;

        if src.to_ascii_lowercase().starts_with("data:image") {
            return decode_data_url_content(&src, base64_to_bytes).map(Some);
        }

        if src.starts_with("blob:") {
            return self.wait_for_blob_resource(&src, deadline, base64_to_bytes);
        }

        while Instant::now() < deadline {
            if let Some(current) = self.current_src(&tag)? {
                src = current;
            } else {
                sleep(Duration::from_millis(10));
                continue;
            }

            if let Some(result) = self.try_resource_content(&src, base64_to_bytes)? {
                return Ok(Some(result));
            }
            sleep(Duration::from_millis(50));
        }

        Ok(None)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<PathBuf> {
        let data = self
            .src(timeout_ms, true)?
            .ok_or_else(|| OpenPageError::PageOperation(element_resource_unavailable_message()))?;

        let tag = self.tag()?;
        let src_attr = self.attr(src_attribute_name(&tag))?;
        let current_src = self.current_src(&tag)?;
        let file_name = resolve_save_name(&tag, src_attr.as_deref(), current_src.as_deref(), name);
        let directory = path.unwrap_or_else(|| Path::new("."));
        let mut target = directory.join(file_name);
        if target.extension().is_none() {
            target.set_extension("jpg");
        }
        if rename {
            target = next_available_path(&target);
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        match data {
            ElementResource::Bytes(bytes) => fs::write(&target, bytes)?,
            ElementResource::Text(text) => fs::write(&target, text)?,
        }
        absolutize_path(target)
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.shadow_root()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.page,
                DescribeNodeParams::builder()
                    .backend_node_id(self.inner.backend_node_id)
                    .pierce(true)
                    .build(),
                "Element::shadow_root()",
            )
            .await?;
            let Some(shadow_root) = response
                .node
                .shadow_roots
                .and_then(|roots| roots.into_iter().next())
            else {
                return Ok(None);
            };

            let remote = execute_page_command_async(
                &self.page,
                chromiumoxide::cdp::browser_protocol::dom::ResolveNodeParams::builder()
                    .backend_node_id(shadow_root.backend_node_id)
                    .build(),
                "Element::shadow_root()",
            )
            .await?;
            let remote_object_id = remote.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation(shadow_root_object_id_unavailable_message())
            })?;

            Ok(Some(ShadowRoot::new(
                Arc::clone(&self.runtime),
                self.page.clone(),
                shadow_root.backend_node_id,
                remote_object_id,
                self.inner.node_id,
                self.javascript_timeout_ms,
                Arc::clone(&self.none_element_config),
            )))
        })
    }

    pub fn parent(&self) -> OpenPageResult<Element> {
        self.parent_level(1)
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<Element> {
        if level == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_level_must_start_message(),
            ));
        }
        nth_element_from_start(
            self.find_all_by_xpath(&format!("./ancestor::*[{level}]"))?,
            1,
            "parent element not found",
        )
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_index_must_start_message(),
            ));
        }
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => {
                let selector = json_string(locator.query())?;
                nth_element_from_start(
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         const items = []; \
                         let node = this.parentElement; \
                         while (node) {{ \
                             if (node.matches(selector)) items.push(node); \
                             node = node.parentElement; \
                         }} \
                         return items;",
                    ))?,
                    index,
                    "parent element not found",
                )
            }
            LocatorKind::XPath => nth_element_from_start(
                self.find_all_by_xpath(&format!(
                    "{}[{index}]",
                    normalize_axis_xpath("ancestor", locator.query())
                ))?,
                1,
                "parent element not found",
            ),
        }
    }

    pub fn child(&self) -> OpenPageResult<Element> {
        self.child_with(None::<&str>, 1)
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(
            self.children_with(locator)?,
            index,
            "child element not found",
        )
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.children_with(None::<&str>)
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.collect_relative_elements("return Array.from(this.children);"),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = json_string(locator.query())?;
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         return Array.from(this.children).filter(element => element.matches(selector));",
                    ))
                }
                LocatorKind::XPath => {
                    self.find_all_by_xpath(&normalize_child_xpath(locator.query()))
                }
            },
        }
    }

    pub fn prev(&self) -> OpenPageResult<Element> {
        self.prev_with(None::<&str>, 1)
    }

    pub fn prev_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_end(
            self.prevs_with(locator)?,
            index,
            "previous element not found",
        )
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.prevs_with(None::<&str>)
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.collect_relative_elements(
                "const items = []; \
                 let node = this.previousElementSibling; \
                 while (node) { \
                     items.push(node); \
                     node = node.previousElementSibling; \
                 } \
                 items.reverse(); \
                 return items;",
            ),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = json_string(locator.query())?;
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         const items = []; \
                         let node = this.previousElementSibling; \
                         while (node) {{ \
                             if (node.matches(selector)) items.push(node); \
                             node = node.previousElementSibling; \
                         }} \
                         items.reverse(); \
                         return items;",
                    ))
                }
                LocatorKind::XPath => self
                    .find_all_by_xpath(&normalize_axis_xpath("preceding-sibling", locator.query())),
            },
        }
    }

    pub fn next(&self) -> OpenPageResult<Element> {
        self.next_with(None::<&str>, 1)
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(self.nexts_with(locator)?, index, "next element not found")
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.nexts_with(None::<&str>)
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.collect_relative_elements(
                "const items = []; \
                 let node = this.nextElementSibling; \
                 while (node) { \
                     items.push(node); \
                     node = node.nextElementSibling; \
                 } \
                 return items;",
            ),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = json_string(locator.query())?;
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         const items = []; \
                         let node = this.nextElementSibling; \
                         while (node) {{ \
                             if (node.matches(selector)) items.push(node); \
                             node = node.nextElementSibling; \
                         }} \
                         return items;",
                    ))
                }
                LocatorKind::XPath => self
                    .find_all_by_xpath(&normalize_axis_xpath("following-sibling", locator.query())),
            },
        }
    }

    pub fn before(&self) -> OpenPageResult<Element> {
        self.before_with(None::<&str>, 1)
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_end(
            self.befores_with(locator)?,
            index,
            "preceding element not found",
        )
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.befores_with(None::<&str>)
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.collect_relative_elements(
                "const elements = Array.from(document.querySelectorAll('*')); \
                 const currentIndex = elements.indexOf(this); \
                 if (currentIndex < 0) return []; \
                 const ancestors = new Set(); \
                 for (let node = this.parentElement; node; node = node.parentElement) ancestors.add(node); \
                 return elements.slice(0, currentIndex).filter(element => !ancestors.has(element));",
            ),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = json_string(locator.query())?;
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         const elements = Array.from(document.querySelectorAll('*')); \
                         const currentIndex = elements.indexOf(this); \
                         if (currentIndex < 0) return []; \
                         const ancestors = new Set(); \
                         for (let node = this.parentElement; node; node = node.parentElement) ancestors.add(node); \
                         return elements.slice(0, currentIndex).filter(element => \
                             !ancestors.has(element) && element.matches(selector) \
                         );",
                    ))
                }
                LocatorKind::XPath => {
                    self.find_all_by_xpath(&normalize_axis_xpath("preceding", locator.query()))
                }
            },
        }
    }

    pub fn after(&self) -> OpenPageResult<Element> {
        self.after_with(None::<&str>, 1)
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(
            self.afters_with(locator)?,
            index,
            "following element not found",
        )
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.afters_with(None::<&str>)
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.collect_relative_elements(
                "const elements = Array.from(document.querySelectorAll('*')); \
                 const currentIndex = elements.indexOf(this); \
                 if (currentIndex < 0) return []; \
                 const descendants = new Set(this.querySelectorAll('*')); \
                 return elements.slice(currentIndex + 1).filter(element => !descendants.has(element));",
            ),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = json_string(locator.query())?;
                    self.collect_relative_elements(&format!(
                        "const selector = {selector}; \
                         const elements = Array.from(document.querySelectorAll('*')); \
                         const currentIndex = elements.indexOf(this); \
                         if (currentIndex < 0) return []; \
                         const descendants = new Set(this.querySelectorAll('*')); \
                         return elements.slice(currentIndex + 1).filter(element => \
                             !descendants.has(element) && element.matches(selector) \
                         );",
                    ))
                }
                LocatorKind::XPath => {
                    self.find_all_by_xpath(&normalize_axis_xpath("following", locator.query()))
                }
            },
        }
    }

    pub fn over(&self) -> OpenPageResult<Option<Element>> {
        self.covering_element()
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<Element>> {
        if timeout_ms == 0 {
            return self.covering_element();
        }

        let timeout = Duration::from_millis(timeout_ms);
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(element) = self.covering_element()? {
                return Ok(Some(element));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            sleep(Duration::from_millis(10));
        }
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        let (target_x, target_y) = self.offset_target_point(x, y)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;

        loop {
            if let Some((_, element)) = self.element_at_point(target_x, target_y)? {
                if locator
                    .as_ref()
                    .map(|locator| element.matches_locator(locator))
                    .transpose()?
                    .unwrap_or(true)
                {
                    return Ok(element);
                }
            }

            if Instant::now() >= deadline {
                return Err(OpenPageError::ElementNotFound(format!(
                    "offset() did not find a matching element at ({target_x}, {target_y})"
                )));
            }
            sleep(Duration::from_millis(10));
        }
    }

    pub fn east(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element> {
        self.find_relative_element(RelativeDirection::East, locator, pixels, index)
    }

    pub fn south(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element> {
        self.find_relative_element(RelativeDirection::South, locator, pixels, index)
    }

    pub fn west(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element> {
        self.find_relative_element(RelativeDirection::West, locator, pixels, index)
    }

    pub fn north(
        &self,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element> {
        self.find_relative_element(RelativeDirection::North, locator, pixels, index)
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        let script = format!(
            "this.removeAttribute({name}); return true;",
            name = json_string(name)?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        let script = format!(
            "this.setAttribute({name}, {value}); return true;",
            name = json_string(name)?,
            value = json_string(value)?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        let script = format!(
            "this[{name}] = {value}; return true;",
            name = json_string(name)?,
            value = serde_json::to_string(value)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        let script = format!(
            "this.style.setProperty({name}, {value}); return true;",
            name = json_string(name)?,
            value = json_string(value)?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        self.run_js_with_args(script, &[], false)
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_with_options(script, args, as_expr, None)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let script = load_javascript_source(script)?;
        let js = build_js_invocation(script.as_ref(), args, as_expr)?;
        let timeout_ms = Some(resolve_javascript_timeout_ms(
            timeout_ms,
            self.javascript_timeout_ms,
        ));
        self.runtime.block_on(async {
            let result = self.call_js_fn_with_timeout(js, true, timeout_ms).await?;
            Ok(result.result.value.unwrap_or(Value::Null))
        })
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        self.prepare_element_screenshot(scroll_to_center, timeout_ms)?;
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(
                self.inner.screenshot(CaptureScreenshotFormat::Png),
                "capture screenshot",
            )
            .await
        })
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        Ok(BASE64_STANDARD.encode(self.screenshot_bytes(scroll_to_center, timeout_ms)?))
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<PathBuf> {
        let target = resolve_screenshot_target_path(&self.tag()?, path, name)?;
        let bytes = self.screenshot_bytes(scroll_to_center, timeout_ms)?;
        fs::write(&target, bytes)?;
        Ok(target)
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(
                self.inner
                    .save_screenshot(CaptureScreenshotFormat::Png, path),
                "save screenshot",
            )
            .await?;
            Ok(())
        })
    }

    fn prepare_element_screenshot(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<()> {
        self.wait_for_image_loaded(timeout_ms)?;
        if scroll_to_center {
            self.scroll_to_center()
        } else {
            self.scroll_to_see(Some(false))
        }
    }

    fn wait_for_image_loaded(&self, timeout_ms: u64) -> OpenPageResult<()> {
        if self.tag()? != "img" {
            return Ok(());
        }
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if value_as_bool(
                self.run_js(
                    "return this.complete && typeof this.naturalWidth !== 'undefined' \
                     && this.naturalWidth > 0 && typeof this.naturalHeight !== 'undefined' \
                     && this.naturalHeight > 0;",
                )?,
                "image loaded",
            )? {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    async fn call_js_fn_with_timeout(
        &self,
        js: String,
        await_promise: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<chromiumoxide::cdp::js_protocol::runtime::CallFunctionOnReturns> {
        let future = self.inner.call_js_fn(js, await_promise);
        let result = match timeout_ms {
            Some(timeout_ms) => {
                tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), future)
                    .await
                    .map_err(|_| OpenPageError::Timeout(javascript_execution_timed_out_message()))?
            }
            None => future.await,
        }
        .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
        Ok(result)
    }

    fn type_or_press(&self, value: &str) -> OpenPageResult<()> {
        if value.is_empty() {
            return Ok(());
        }
        if value.chars().count() == 1 || keys::get_key_definition(value).is_some() {
            return self.press_key(value);
        }
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.type_str(value), "type text").await?;
            Ok(())
        })
    }

    fn press_key_with_modifiers(&self, key: &str, modifiers: i64) -> OpenPageResult<()> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(key)))?;
        let key_down = build_key_event(definition, modifiers, false);
        let key_up = build_key_event(definition, modifiers, true);
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            key_down,
            "Element::press_key_with_modifiers()",
        )?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            key_up,
            "Element::press_key_with_modifiers()",
        )?;
        Ok(())
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.focus(), "focus").await?;
            Ok(())
        })
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        let result = self.run_js(
            "const form = this.tagName === 'FORM' ? this : this.closest('form'); \
             if (!form) return false; \
             if (typeof form.requestSubmit === 'function') { \
                 form.requestSubmit(); \
                 return true; \
             } \
             form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); \
             form.submit(); \
             return true;",
        )?;
        if !value_as_bool(result, "submit")? {
            return Err(OpenPageError::PageOperation(
                "element is not inside a form".to_string(),
            ));
        }
        Ok(())
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        self.hover_with_offset(None, None)
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        if offset_x.is_none() && offset_y.is_none() {
            return self.runtime.block_on(async {
                run_element_future_with_cdp_timeout(self.inner.hover(), "hover").await?;
                Ok(())
            });
        }

        self.runtime.block_on(async {
            run_element_future_with_cdp_timeout(self.inner.scroll_into_view(), "scroll into view")
                .await?;
            Ok::<(), OpenPageError>(())
        })?;
        let (x, y) = self.offset_target_point(offset_x, offset_y)?;
        self.runtime.block_on(async {
            run_element_page_future_with_cdp_timeout(
                self.page.move_mouse(Point::new(x as f64, y as f64)),
                "move mouse",
            )
            .await?;
            Ok(())
        })
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        let start = self.clickable_point()?;
        let target = Point::new(start.x + offset_x, start.y + offset_y);
        self.drag_between(start, target, duration_secs)
    }

    pub fn drag_to(&self, target: &Element, duration_secs: f64) -> OpenPageResult<()> {
        self.drag_to_point_from_self(target.clickable_point()?, duration_secs)
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.drag_to_point_from_self(Point::new(x, y), duration_secs)
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        let script = format!(
            "const next = {checked}; \
             if (this.checked === next) return true; \
             this.checked = next; \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        let desired = !uncheck;
        if self.is_checked()? == desired {
            return Ok(());
        }
        if by_js {
            self.set_checked(desired)
        } else {
            self.click()
        }
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        self.check(true, by_js)
    }

    pub fn is_multi_select(&self) -> OpenPageResult<bool> {
        self.ensure_select_element()?;
        value_as_bool(
            self.run_js("return this instanceof HTMLSelectElement && !!this.multiple;")?,
            "multiple",
        )
    }

    pub fn option_texts(&self) -> OpenPageResult<Vec<String>> {
        value_as_string_vec(
            self.run_js(
                "if (!(this instanceof HTMLSelectElement)) return []; \
                 return Array.from(this.options).map(option => option.text);",
            )?,
            "option texts",
        )
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(
            Some(self.run_js(
                "if (!(this instanceof HTMLSelectElement)) return null; \
                 return this.selectedOptions.length ? this.selectedOptions[0].text : null;",
            )?),
            "selected option",
        )
    }

    pub fn selected_options(&self) -> OpenPageResult<Vec<String>> {
        value_as_string_vec(
            self.run_js(
                "if (!(this instanceof HTMLSelectElement)) return []; \
                 return Array.from(this.selectedOptions).map(option => option.text);",
            )?,
            "selected options",
        )
    }

    pub fn select_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for text in select_input_values(text.into()) {
            matched |= self.select_by_text_value_with_timeout(&text, None)?;
        }
        Ok(matched)
    }

    pub fn select_by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for text in select_input_values(text.into()) {
            matched |= self.select_by_text_value_with_timeout(&text, timeout_ms)?;
        }
        Ok(matched)
    }

    fn select_by_text_value_with_timeout(
        &self,
        text: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if text.is_empty() {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_text(text))?;
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const target = {text}; \
             const matches = Array.from(this.options).filter(option => option.text === target); \
             if (!matches.length) return false; \
             if (this.multiple) {{ \
                 for (const option of matches) option.selected = true; \
             }} else {{ \
                 this.value = matches[0].value; \
             }} \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            text = json_string(text)?,
        );
        value_as_bool(self.run_js(&script)?, "select_by_text")
    }

    pub fn select_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for value in select_input_values(value.into()) {
            matched |= self.select_by_value_value_with_timeout(&value, None)?;
        }
        Ok(matched)
    }

    pub fn select_by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for value in select_input_values(value.into()) {
            matched |= self.select_by_value_value_with_timeout(&value, timeout_ms)?;
        }
        Ok(matched)
    }

    fn select_by_value_value_with_timeout(
        &self,
        value: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if value.is_empty() {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_value(value))?;
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const target = {value}; \
             const matches = Array.from(this.options).filter(option => option.value === target); \
             if (!matches.length) return false; \
             if (this.multiple) {{ \
                 for (const option of matches) option.selected = true; \
             }} else {{ \
                 this.value = matches[0].value; \
             }} \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            value = json_string(value)?,
        );
        value_as_bool(self.run_js(&script)?, "select_by_value")
    }

    pub fn select_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match index.into() {
            SelectIndexInput::Single(index) => self.select_by_index_value_with_timeout(index, None),
            SelectIndexInput::Many(indices) => self.select_by_indices(&indices),
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
        match index.into() {
            SelectIndexInput::Single(index) => {
                self.select_by_index_value_with_timeout(index, timeout_ms)
            }
            SelectIndexInput::Many(indices) => {
                self.select_by_indices_with_timeout(&indices, timeout_ms)
            }
        }
    }

    fn select_by_index_value(&self, index: usize) -> OpenPageResult<bool> {
        self.select_by_index_value_with_timeout(index, None)
    }

    fn select_by_index_value_with_timeout(
        &self,
        index: usize,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if index == 0 {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_has_index(index))?;
        let zero_based = index.saturating_sub(1);
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const index = {index}; \
             if (index < 0 || index >= this.options.length) return false; \
             if (this.multiple) {{ \
                 this.options[index].selected = true; \
             }} else {{ \
                 this.selectedIndex = index; \
             }} \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            index = zero_based,
        );
        value_as_bool(self.run_js(&script)?, "select_by_index")
    }

    pub fn select_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        self.select_by_indices_with_timeout(indices, None)
    }

    pub fn select_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        let mut matched = false;
        for index in indices {
            matched |= self.select_by_index_value_with_timeout(*index, timeout_ms)?;
        }
        Ok(matched)
    }

    pub fn select_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.select_by_locator_with_timeout(locator, None)
    }

    pub fn select_by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let mut matched = false;
        for locator in parse_locator_batch_input(locator)? {
            matched |= self.select_by_locator_value_with_timeout(locator.as_str(), timeout_ms)?;
        }
        Ok(matched)
    }

    fn select_by_locator_value_with_timeout(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_locator(locator))?;
        let options = self.option_elements_matching(locator)?;
        self.set_option_elements_selected(&options, true)
    }

    pub fn cancel_by_text<'a, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for text in select_input_values(text.into()) {
            matched |= self.cancel_by_text_value_with_timeout(&text, None)?;
        }
        Ok(matched)
    }

    pub fn cancel_by_text_with_timeout<'a, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for text in select_input_values(text.into()) {
            matched |= self.cancel_by_text_value_with_timeout(&text, timeout_ms)?;
        }
        Ok(matched)
    }

    fn cancel_by_text_value_with_timeout(
        &self,
        text: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if text.is_empty() {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_text(text))?;
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const target = {text}; \
             const matches = Array.from(this.options).filter(option => option.text === target); \
             if (!matches.length) return false; \
             for (const option of matches) option.selected = false; \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            text = json_string(text)?,
        );
        value_as_bool(self.run_js(&script)?, "cancel_by_text")
    }

    pub fn cancel_by_value<'a, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for value in select_input_values(value.into()) {
            matched |= self.cancel_by_value_value_with_timeout(&value, None)?;
        }
        Ok(matched)
    }

    pub fn cancel_by_value_with_timeout<'a, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<ActionsInput<'a>>,
    {
        let mut matched = false;
        for value in select_input_values(value.into()) {
            matched |= self.cancel_by_value_value_with_timeout(&value, timeout_ms)?;
        }
        Ok(matched)
    }

    fn cancel_by_value_value_with_timeout(
        &self,
        value: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if value.is_empty() {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_value(value))?;
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const target = {value}; \
             const matches = Array.from(this.options).filter(option => option.value === target); \
             if (!matches.length) return false; \
             for (const option of matches) option.selected = false; \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            value = json_string(value)?,
        );
        value_as_bool(self.run_js(&script)?, "cancel_by_value")
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<SelectIndexInput>,
    {
        match index.into() {
            SelectIndexInput::Single(index) => self.cancel_by_index_value_with_timeout(index, None),
            SelectIndexInput::Many(indices) => self.cancel_by_indices(&indices),
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
        match index.into() {
            SelectIndexInput::Single(index) => {
                self.cancel_by_index_value_with_timeout(index, timeout_ms)
            }
            SelectIndexInput::Many(indices) => {
                self.cancel_by_indices_with_timeout(&indices, timeout_ms)
            }
        }
    }

    fn cancel_by_index_value_with_timeout(
        &self,
        index: usize,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        if index == 0 {
            return Ok(false);
        }
        self.wait_for_select_match(timeout_ms, || self.select_has_index(index))?;
        let zero_based = index.saturating_sub(1);
        let script = format!(
            "if (!(this instanceof HTMLSelectElement)) return false; \
             const index = {index}; \
             if (index < 0 || index >= this.options.length) return false; \
             this.options[index].selected = false; \
             this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
             this.dispatchEvent(new Event('change', {{ bubbles: true }})); \
             return true;",
            index = zero_based,
        );
        value_as_bool(self.run_js(&script)?, "cancel_by_index")
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        self.cancel_by_indices_with_timeout(indices, None)
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        let mut matched = false;
        for index in indices {
            matched |= self.cancel_by_index_value_with_timeout(*index, timeout_ms)?;
        }
        Ok(matched)
    }

    pub fn cancel_by_locator<'a, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        self.cancel_by_locator_with_timeout(locator, None)
    }

    pub fn cancel_by_locator_with_timeout<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let mut matched = false;
        for locator in parse_locator_batch_input(locator)? {
            matched |= self.cancel_by_locator_value_with_timeout(locator.as_str(), timeout_ms)?;
        }
        Ok(matched)
    }

    fn cancel_by_locator_value_with_timeout(
        &self,
        locator: &str,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        self.wait_for_select_match(timeout_ms, || self.select_option_count_by_locator(locator))?;
        let options = self.option_elements_matching(locator)?;
        self.set_option_elements_selected(&options, false)
    }

    pub fn option_elements(&self) -> OpenPageResult<Vec<Element>> {
        self.ensure_select_element()?;
        self.find_all("css:option")
    }

    pub fn selected_option_element(&self) -> OpenPageResult<Option<Element>> {
        Ok(self.selected_option_elements()?.into_iter().next())
    }

    pub fn selected_option_elements(&self) -> OpenPageResult<Vec<Element>> {
        self.ensure_select_element()?;
        self.find_all("css:option:checked")
    }

    pub fn select_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<SelectOptionInput<'a>>,
    {
        match option.into() {
            SelectOptionInput::Single(option) => self.select_by_option_value(option),
            SelectOptionInput::Many(options) => self.select_by_options(&options),
        }
    }

    fn select_by_option_value(&self, option: &Element) -> OpenPageResult<bool> {
        self.ensure_select_element()?;
        if !self.option_belongs_to_select(option)? {
            return Ok(false);
        }
        if self.is_multi_select()? {
            option.set_property("selected", &Value::Bool(true))?;
            self.dispatch_select_events()?;
            return Ok(true);
        }
        let index = match option.property("index")? {
            Some(value) => value_as_usize(value, "option index")?,
            None => return Ok(false),
        };
        self.select_by_index_value(index.saturating_add(1))
    }

    pub fn select_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        let mut matched = false;
        for option in options {
            matched |= self.select_by_option_value(option)?;
        }
        Ok(matched)
    }

    pub fn cancel_by_option<'a, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<SelectOptionInput<'a>>,
    {
        match option.into() {
            SelectOptionInput::Single(option) => self.cancel_by_option_value(option),
            SelectOptionInput::Many(options) => self.cancel_by_options(&options),
        }
    }

    fn cancel_by_option_value(&self, option: &Element) -> OpenPageResult<bool> {
        self.ensure_select_element()?;
        if !self.option_belongs_to_select(option)? {
            return Ok(false);
        }
        option.set_property("selected", &Value::Bool(false))?;
        self.dispatch_select_events()?;
        Ok(true)
    }

    pub fn cancel_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        let mut matched = false;
        for option in options {
            matched |= self.cancel_by_option_value(option)?;
        }
        Ok(matched)
    }

    pub fn select_all(&self) -> OpenPageResult<()> {
        self.ensure_multi_select_action("all")?;
        self.run_js(
            "for (const option of this.options) option.selected = true; \
             this.dispatchEvent(new Event('input', { bubbles: true })); \
             this.dispatchEvent(new Event('change', { bubbles: true })); \
             return true;",
        )?;
        Ok(())
    }

    pub fn invert_selected(&self) -> OpenPageResult<()> {
        self.ensure_multi_select_action("invert")?;
        self.run_js(
            "for (const option of this.options) option.selected = !option.selected; \
             this.dispatchEvent(new Event('input', { bubbles: true })); \
             this.dispatchEvent(new Event('change', { bubbles: true })); \
             return true;",
        )?;
        Ok(())
    }

    pub fn clear_selected(&self) -> OpenPageResult<()> {
        self.ensure_multi_select_action("clear")?;
        self.run_js(
            "for (const option of this.options) option.selected = false; \
             this.dispatchEvent(new Event('input', { bubbles: true })); \
             this.dispatchEvent(new Event('change', { bubbles: true })); \
             return true;",
        )?;
        Ok(())
    }

    fn select_timeout_ms(&self, requested: Option<u64>) -> OpenPageResult<u64> {
        match requested {
            Some(timeout_ms) => Ok(timeout_ms),
            None => match &self.browser {
                Some(browser) => Ok(browser.timeouts()?.implicit_wait),
                None => Ok(10_000),
            },
        }
    }

    fn ensure_select_element(&self) -> OpenPageResult<()> {
        let is_select = value_as_bool(
            self.run_js("return this instanceof HTMLSelectElement;")?,
            "select element",
        )?;
        if is_select {
            return Ok(());
        }
        Err(OpenPageError::UnsupportedOperation(
            select_element_required_message(),
        ))
    }

    fn ensure_multi_select_action(&self, action: &str) -> OpenPageResult<()> {
        if self.is_multi_select()? {
            return Ok(());
        }
        Err(OpenPageError::UnsupportedOperation(
            multi_select_action_required_message(action),
        ))
    }

    fn wait_for_select_match<F>(&self, timeout_ms: Option<u64>, predicate: F) -> OpenPageResult<()>
    where
        F: FnMut() -> OpenPageResult<bool>,
    {
        self.ensure_select_element()?;
        let timeout_ms = self.select_timeout_ms(timeout_ms)?;
        if timeout_ms == 0 {
            return Ok(());
        }
        let mut predicate = predicate;
        self.wait_until(timeout_ms, |_| predicate(), false)
            .map(|_| ())
    }

    fn select_option_count_by_text(&self, text: &str) -> OpenPageResult<bool> {
        if text.is_empty() {
            return Ok(false);
        }
        value_as_usize(
            self.run_js(&format!(
                "if (!(this instanceof HTMLSelectElement)) return 0; \
                 const target = {text}; \
                 return Array.from(this.options).filter(option => option.text === target).length;",
                text = json_string(text)?,
            ))?,
            "select option count by text",
        )
        .map(|count| count > 0)
    }

    fn select_option_count_by_value(&self, value: &str) -> OpenPageResult<bool> {
        if value.is_empty() {
            return Ok(false);
        }
        value_as_usize(
            self.run_js(&format!(
                "if (!(this instanceof HTMLSelectElement)) return 0; \
                 const target = {value}; \
                 return Array.from(this.options).filter(option => option.value === target).length;",
                value = json_string(value)?,
            ))?,
            "select option count by value",
        )
        .map(|count| count > 0)
    }

    fn select_has_index(&self, index: usize) -> OpenPageResult<bool> {
        if index == 0 {
            return Ok(false);
        }
        value_as_usize(
            self.run_js(
                "if (!(this instanceof HTMLSelectElement)) return 0; \
                 return this.options.length;",
            )?,
            "select option count",
        )
        .map(|count| count >= index)
    }

    fn select_option_count_by_locator(&self, locator: &str) -> OpenPageResult<bool> {
        Ok(!self.option_elements_matching(locator)?.is_empty())
    }

    fn option_elements_matching(&self, locator: &str) -> OpenPageResult<Vec<Element>> {
        let mut options = Vec::new();
        for element in self.find_all(locator)? {
            if element.tag()? == "option" {
                options.push(element);
            }
        }
        Ok(options)
    }

    fn option_belongs_to_select(&self, option: &Element) -> OpenPageResult<bool> {
        if option.tag()? != "option" {
            return Ok(false);
        }
        let marker = format!("{}-select-owner", next_marker_batch());
        self.set_attr(MARKER_ATTRIBUTE, &marker)?;
        let result = option.run_js(&format!(
            "const attr = {attr}; \
             const marker = {marker}; \
             const owner = this.closest('select'); \
             return !!owner && owner.getAttribute(attr) === marker;",
            attr = json_string(MARKER_ATTRIBUTE)?,
            marker = json_string(&marker)?,
        ));
        let _ = self.remove_attr(MARKER_ATTRIBUTE);
        result.and_then(|value| value_as_bool(value, "select option ownership"))
    }

    fn set_option_elements_selected(
        &self,
        options: &[Element],
        selected: bool,
    ) -> OpenPageResult<bool> {
        if options.is_empty() {
            return Ok(false);
        }
        if selected && !self.is_multi_select()? {
            return self.select_by_option(&options[0]);
        }
        let mut matched = false;
        for option in options {
            if self.option_belongs_to_select(option)? {
                option.set_property("selected", &Value::Bool(selected))?;
                matched = true;
            }
        }
        if !matched {
            return Ok(false);
        }
        self.dispatch_select_events()?;
        Ok(true)
    }

    fn dispatch_select_events(&self) -> OpenPageResult<()> {
        self.run_js(
            "this.dispatchEvent(new Event('input', { bubbles: true })); \
             this.dispatchEvent(new Event('change', { bubbles: true })); \
             return true;",
        )?;
        Ok(())
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

    pub fn rect_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        let value = self.run_js(
            "const rect = this.getBoundingClientRect(); \
             if (!rect.width || !rect.height) { return null; } \
             const scrollX = window.scrollX || document.documentElement.scrollLeft || 0; \
             const scrollY = window.scrollY || document.documentElement.scrollTop || 0; \
             return JSON.stringify([ \
                [rect.left + scrollX, rect.top + scrollY], \
                [rect.right + scrollX, rect.top + scrollY], \
                [rect.right + scrollX, rect.bottom + scrollY], \
                [rect.left + scrollX, rect.bottom + scrollY] \
             ]);",
        )?;
        match value {
            Value::Null => Ok(None),
            Value::String(serialized) => {
                let points: Vec<Vec<f64>> = serde_json::from_str(&serialized).map_err(|err| {
                    OpenPageError::Serialization(element_rect_corners_parse_failed_message(
                        &err.to_string(),
                    ))
                })?;
                let mut corners = Vec::with_capacity(points.len());
                for point in points {
                    if point.len() != 2 {
                        return Err(OpenPageError::JavaScript(
                            element_rect_corner_coordinate_count_message(),
                        ));
                    }
                    corners.push((point[0], point[1]));
                }
                Ok(Some(corners))
            }
            value => Err(OpenPageError::JavaScript(
                element_rect_corners_unexpected_value_message(&value.to_string()),
            )),
        }
    }

    pub fn rect_viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        let value = self.run_js(
            "const rect = this.getBoundingClientRect(); \
             if (!rect.width || !rect.height) { return null; } \
             return JSON.stringify([ \
                [rect.left, rect.top], \
                [rect.right, rect.top], \
                [rect.right, rect.bottom], \
                [rect.left, rect.bottom] \
             ]);",
        )?;
        match value {
            Value::Null => Ok(None),
            Value::String(serialized) => {
                let points: Vec<Vec<f64>> = serde_json::from_str(&serialized).map_err(|err| {
                    OpenPageError::Serialization(element_rect_corners_parse_failed_message(
                        &err.to_string(),
                    ))
                })?;
                let mut corners = Vec::with_capacity(points.len());
                for point in points {
                    if point.len() != 2 {
                        return Err(OpenPageError::JavaScript(
                            element_rect_corner_coordinate_count_message(),
                        ));
                    }
                    corners.push((point[0], point[1]));
                }
                Ok(Some(corners))
            }
            value => Err(OpenPageError::JavaScript(
                element_rect_corners_unexpected_value_message(&value.to_string()),
            )),
        }
    }

    pub fn rect_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(corners) = self.rect_corners()? else {
            return Ok(None);
        };
        Ok(Some((corners[0].0, corners[0].1)))
    }

    pub fn rect_viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(corners) = self.rect_viewport_corners()? else {
            return Ok(None);
        };
        Ok(Some((corners[0].0, corners[0].1)))
    }

    pub fn rect_screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(point) = self.rect_viewport_location()? else {
            return Ok(None);
        };
        self.viewport_point_to_screen(point)
    }

    pub fn rect_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(corners) = self.rect_corners()? else {
            return Ok(None);
        };
        Ok(Some(midpoint_from_corners_f64(&corners)))
    }

    pub fn rect_viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(corners) = self.rect_viewport_corners()? else {
            return Ok(None);
        };
        Ok(Some(midpoint_from_corners_f64(&corners)))
    }

    pub fn rect_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return null; } \
                 const centerX = rect.left + rect.width / 2; \
                 const top = rect.top + this.clientTop + 3; \
                 const scrollX = window.scrollX || document.documentElement.scrollLeft || 0; \
                 const scrollY = window.scrollY || document.documentElement.scrollTop || 0; \
                 return JSON.stringify([centerX + scrollX, top + scrollY]);",
            )?,
            "element click point",
        )
    }

    pub fn rect_viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return null; } \
                 const centerX = rect.left + rect.width / 2; \
                 const top = rect.top + this.clientTop + 3; \
                 return JSON.stringify([centerX, top]);",
            )?,
            "element viewport click point",
        )
    }

    pub fn rect_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(corners) = self.rect_corners()? else {
            return Ok(None);
        };
        Ok(Some((
            (corners[1].0 - corners[0].0).abs(),
            (corners[2].1 - corners[0].1).abs(),
        )))
    }

    pub fn rect_screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(point) = self.rect_viewport_midpoint()? else {
            return Ok(None);
        };
        self.viewport_point_to_screen(point)
    }

    pub fn rect_screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let Some(point) = self.rect_viewport_click_point()? else {
            return Ok(None);
        };
        self.viewport_point_to_screen(point)
    }

    pub fn rect_scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js("return JSON.stringify([this.scrollLeft, this.scrollTop]);")?,
            "element scroll position",
        )
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        Ok(self.rect_viewport_corners()?.is_some())
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

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return false; } \
                 return rect.left >= 0 && rect.top >= 0 \
                    && rect.right <= window.innerWidth \
                    && rect.bottom <= window.innerHeight;",
            )?,
            "whole_in_viewport",
        )
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        value_as_bool(
            self.run_js(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return false; } \
                 const x = Math.min(Math.max(rect.left + rect.width / 2, 0), window.innerWidth - 1); \
                 const y = Math.min(Math.max(rect.top + rect.height / 2, 0), window.innerHeight - 1); \
                 const top = document.elementFromPoint(x, y); \
                 return !!top && top !== this && !this.contains(top);",
            )?,
            "covered",
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
        self.wait_until(
            timeout_ms,
            |element| element.is_displayed().map(|value| !value),
            true,
        )
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_enabled(), false)
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(
            timeout_ms,
            |element| element.is_enabled().map(|value| !value),
            false,
        )
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(
            timeout_ms,
            |element| element.is_alive().map(|value| !value),
            true,
        )
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_clickable(), false)
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.has_rect(), false)
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(timeout_ms, |element| element.is_covered(), false)
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(
            timeout_ms,
            |element| element.is_covered().map(|value| !value),
            false,
        )
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.wait_until(
            timeout_ms,
            |element| {
                Ok(!element.is_enabled().unwrap_or(false) || !element.is_alive().unwrap_or(false))
            },
            true,
        )
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let Some(mut size) = self.rect_size()? else {
            return Ok(false);
        };
        let Some(mut location) = self.rect_location()? else {
            return Ok(false);
        };
        while Instant::now() < deadline {
            sleep(Duration::from_millis(100));
            let Some(next_size) = self.rect_size()? else {
                return Ok(false);
            };
            let Some(next_location) = self.rect_location()? else {
                return Ok(false);
            };
            if next_size == size && next_location == location {
                return Ok(true);
            }
            size = next_size;
            location = next_location;
        }
        Ok(false)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => self.runtime.block_on(async {
                let element = self
                    .inner
                    .find_element(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
                Ok(Element::new(
                    Arc::clone(&self.runtime),
                    self.page.clone(),
                    self.browser.clone(),
                    self.uploader.clone(),
                    element,
                    self.javascript_timeout_ms,
                    Arc::clone(&self.none_element_config),
                ))
            }),
            LocatorKind::XPath => nth_element_from_start(
                self.find_all_by_xpath(locator.query())?,
                1,
                "child element not found",
            ),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let frame_element = self.resolve_frame_target(target.into())?;
        self.page_wrapper().frame_from_element(frame_element)
    }

    pub fn get_frame_with_timeout<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self
                .resolve_backend_node_id(self.backend_node_id())
                .and_then(|element| element.get_frame(target))
            {
                Ok(frame) => return Ok(frame),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_by_index(&self, index: usize) -> OpenPageResult<Frame> {
        self.get_frame(index)
    }

    pub fn get_frame_by_index_with_timeout(
        &self,
        index: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<Frame> {
        self.get_frame_with_timeout(index, timeout_ms)
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => self.runtime.block_on(async {
                let elements = self
                    .inner
                    .find_elements(locator.query().to_string())
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
                Ok(elements
                    .into_iter()
                    .map(|element| {
                        Element::new(
                            Arc::clone(&self.runtime),
                            self.page.clone(),
                            self.browser.clone(),
                            self.uploader.clone(),
                            element,
                            self.javascript_timeout_ms,
                            Arc::clone(&self.none_element_config),
                        )
                    })
                    .collect())
            }),
            LocatorKind::XPath => self.find_all_by_xpath(locator.query()),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    fn wait_until<F>(
        &self,
        timeout_ms: u64,
        mut predicate: F,
        treat_errors_as_success: bool,
    ) -> OpenPageResult<bool>
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
                return wait_timeout_result("Element::wait_until()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn pseudo_content(&self, pseudos: &[&str]) -> OpenPageResult<String> {
        let mut last = String::new();
        for pseudo in pseudos {
            let value = self.style("content", Some(pseudo))?;
            if value != "content" {
                return Ok(value);
            }
            last = value;
        }
        Ok(last)
    }

    fn resolve_frame_target<'a>(&self, target: PageFrameTarget<'a>) -> OpenPageResult<Element> {
        match target {
            PageFrameTarget::Locator(locator) => {
                let locator = frame_locator_input(locator)?;
                self.find(locator.as_str())
            }
            PageFrameTarget::Index(index) => self.frame_element_by_index(index),
            PageFrameTarget::Element(element) => self.find_frame_element_from_object(element),
            PageFrameTarget::WebElement(element) => match element {
                crate::webpage::WebElement::Browser(element)
                | crate::webpage::WebElement::Mix { element, .. } => {
                    self.find_frame_element_from_object(element)
                }
                crate::webpage::WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                    session_backed_element_driver_target_message(
                        "WebElement",
                        "element frame",
                        "元素 frame 定位",
                    ),
                )),
            },
            PageFrameTarget::Frame(frame) => {
                self.find_frame_element_from_object(frame.frame_element())
            }
            PageFrameTarget::WebFrame(frame) => {
                self.find_frame_element_from_object(frame.frame_element())
            }
        }
    }

    fn frame_element_by_index(&self, index: isize) -> OpenPageResult<Element> {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                frame_index_must_start_message(),
            ));
        }

        let frames = self.find_all("css:iframe,frame")?;
        let resolved_index = if index > 0 {
            (index as usize).checked_sub(1)
        } else {
            frames.len().checked_sub(index.unsigned_abs())
        };
        resolved_index
            .and_then(|resolved_index| frames.into_iter().nth(resolved_index))
            .ok_or_else(|| OpenPageError::ElementNotFound(frame_index_out_of_range_message(index)))
    }

    fn find_frame_element_from_object(&self, element: &Element) -> OpenPageResult<Element> {
        let batch = next_marker_batch();
        let marker = format!("{batch}-frame");
        element.set_attr(MARKER_ATTRIBUTE, &marker)?;
        let selector = format!(r#"css:[{MARKER_ATTRIBUTE}="{marker}"]"#);
        let result = self.find(selector.as_str());
        let cleanup = element.remove_attr(MARKER_ATTRIBUTE);
        match (result, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(err), Ok(())) => Err(err),
            (Ok(_), Err(err)) => Err(err),
            (Err(err), Err(_)) => Err(err),
        }
    }

    fn offset_target_point(&self, x: Option<f64>, y: Option<f64>) -> OpenPageResult<(i64, i64)> {
        if x.is_none() && y.is_none() {
            let (point_x, point_y) = self
                .rect_viewport_midpoint()?
                .ok_or_else(|| OpenPageError::PageOperation(element_no_visible_rect_message()))?;
            return Ok((point_x.round() as i64, point_y.round() as i64));
        }

        let (left, top) = self
            .rect_viewport_location()?
            .ok_or_else(|| OpenPageError::PageOperation(element_no_visible_rect_message()))?;
        Ok((
            (left + x.unwrap_or(0.0)).round() as i64,
            (top + y.unwrap_or(0.0)).round() as i64,
        ))
    }

    fn offset_click_point(&self, x: Option<f64>, y: Option<f64>) -> OpenPageResult<(i64, i64)> {
        let (frame_offset_x, frame_offset_y) = self
            .frame_viewport_offset()
            .map_err(|err| {
                OpenPageError::PageOperation(resolve_frame_viewport_offset_failed_message(
                    &err.to_string(),
                ))
            })?
            .ok_or_else(|| {
                OpenPageError::PageOperation(element_frame_viewport_offset_unavailable_message())
            })?;
        if x.is_none() && y.is_none() {
            let (point_x, point_y) = self
                .rect_viewport_click_point()?
                .ok_or_else(|| OpenPageError::PageOperation(element_no_visible_rect_message()))?;
            return Ok((
                (frame_offset_x + point_x).round() as i64,
                (frame_offset_y + point_y).round() as i64,
            ));
        }

        let (left, top) = self
            .rect_viewport_location()?
            .ok_or_else(|| OpenPageError::PageOperation(element_no_visible_rect_message()))?;
        Ok((
            (frame_offset_x + left + x.unwrap_or(0.0)).round() as i64,
            (frame_offset_y + top + y.unwrap_or(0.0)).round() as i64,
        ))
    }

    fn covering_element(&self) -> OpenPageResult<Option<Element>> {
        Ok(self
            .collect_relative_elements(
                "const rect = this.getBoundingClientRect(); \
                 if (!rect.width || !rect.height) { return []; } \
                 const x = Math.min(Math.max(rect.left + rect.width / 2, 0), window.innerWidth - 1); \
                 const y = Math.min(Math.max(rect.top + rect.height / 2, 0), window.innerHeight - 1); \
                 const top = document.elementFromPoint(x, y); \
                 if (!top || top === this || this.contains(top)) { return []; } \
                 return [top];",
            )?
            .into_iter()
            .next())
    }

    fn find_relative_element(
        &self,
        direction: RelativeDirection,
        locator: Option<&str>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element> {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                relative_direction_index_must_start_message(),
            ));
        }

        let locator = parse_optional_locator(locator)?;
        let corners = self
            .rect_corners()?
            .ok_or_else(|| OpenPageError::PageOperation(element_no_visible_rect_message()))?;
        let ((mut x, mut y), step_x, step_y) = relative_search_seed(&corners, direction);

        if let Some(pixels) = pixels {
            x += step_x * pixels;
            y += step_y * pixels;
            if let Some((_, element)) = self.element_at_point(x, y)? {
                if locator
                    .as_ref()
                    .map(|locator| element.matches_locator(locator))
                    .transpose()?
                    .unwrap_or(true)
                {
                    return Ok(element);
                }
            }
            return Err(OpenPageError::ElementNotFound(format!(
                "{}() did not find a matching element at ({x}, {y})",
                direction.method_name()
            )));
        }

        let (viewport_width, viewport_height) = self.viewport_size()?;
        let mut current_backend = None;
        let mut matches = 0usize;
        while relative_search_in_bounds(x, y, viewport_width, viewport_height, direction) {
            x += step_x * 8;
            y += step_y * 8;
            if let Some((backend_node_id, element)) = self.element_at_point(x, y)? {
                if current_backend == Some(backend_node_id) {
                    continue;
                }
                current_backend = Some(backend_node_id);

                if locator
                    .as_ref()
                    .map(|locator| element.matches_locator(locator))
                    .transpose()?
                    .unwrap_or(true)
                {
                    matches += 1;
                    if matches == index {
                        return Ok(element);
                    }
                }
            }
        }

        Err(OpenPageError::ElementNotFound(format!(
            "{}() did not find element #{index}",
            direction.method_name()
        )))
    }

    fn viewport_size(&self) -> OpenPageResult<(i64, i64)> {
        let (width, height) = value_as_f64_pair(
            self.run_js("return JSON.stringify([window.innerWidth, window.innerHeight]);")?,
            "viewport size",
        )?;
        Ok((width.round() as i64, height.round() as i64))
    }

    fn element_at_point(&self, x: i64, y: i64) -> OpenPageResult<Option<(BackendNodeId, Element)>> {
        let params = GetNodeForLocationParams::builder()
            .x(x)
            .y(y)
            .include_user_agent_shadow_dom(true)
            .ignore_pointer_events_none(false)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        let backend_node_id = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            params,
            "Element::element_at_point()",
        )?
        .backend_node_id;
        let element = self.resolve_backend_node_id(backend_node_id)?;
        Ok(Some((backend_node_id, element)))
    }

    fn resolve_backend_node_id(&self, backend_node_id: BackendNodeId) -> OpenPageResult<Element> {
        let node_id = self.resolve_backend_node_to_node_id(backend_node_id)?;
        self.resolve_node_id(node_id, "backend node could not be resolved to an element")
    }

    fn resolve_backend_node_to_node_id(
        &self,
        backend_node_id: BackendNodeId,
    ) -> OpenPageResult<chromiumoxide::cdp::browser_protocol::dom::NodeId> {
        self.runtime.block_on(async {
            let resolved = execute_page_command_async(
                &self.page,
                ResolveNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
                "Element::resolve_backend_node_to_node_id()",
            )
            .await?;
            let object_id = resolved.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation(resolved_node_missing_object_id_message())
            })?;
            let requested = execute_page_command_async(
                &self.page,
                RequestNodeParams::new(object_id),
                "Element::resolve_backend_node_to_node_id()",
            )
            .await?;
            Ok::<chromiumoxide::cdp::browser_protocol::dom::NodeId, OpenPageError>(
                requested.node_id,
            )
        })
    }

    fn resolve_node_id(
        &self,
        node_id: chromiumoxide::cdp::browser_protocol::dom::NodeId,
        error_message: &str,
    ) -> OpenPageResult<Element> {
        let batch = next_marker_batch();
        let marker = format!("{batch}-0");

        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            SetAttributeValueParams::new(node_id, MARKER_ATTRIBUTE, marker.clone()),
            "Element::resolve_node_id()",
        )?;

        let element = self.runtime.block_on(async {
            let xpath = format!("//*[@{MARKER_ATTRIBUTE}='{marker}']");
            let element = self
                .page
                .find_xpath(xpath)
                .await
                .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
            Ok::<Element, OpenPageError>(Element::new(
                Arc::clone(&self.runtime),
                self.page.clone(),
                self.browser.clone(),
                self.uploader.clone(),
                element,
                self.javascript_timeout_ms,
                Arc::clone(&self.none_element_config),
            ))
        });

        let cleanup = self.runtime.block_on(async {
            let _ = execute_page_command_async(
                &self.page,
                RemoveAttributeParams::new(node_id, MARKER_ATTRIBUTE),
                "Element::resolve_node_id()",
            )
            .await;
            Ok::<(), OpenPageError>(())
        });

        match (element, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(_), Ok(())) => Err(OpenPageError::ElementNotFound(error_message.to_string())),
            (Err(err), Err(_)) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    pub(crate) fn matches_locator(&self, locator: &Locator) -> OpenPageResult<bool> {
        match locator.kind() {
            LocatorKind::Css => value_as_bool(
                self.run_js(&format!(
                    "return this.matches({selector});",
                    selector = json_string(locator.query())?,
                ))?,
                "css locator match",
            ),
            LocatorKind::XPath => value_as_bool(
                self.run_js(&format!(
                    "const xpath = {xpath}; \
                     const root = this.getRootNode(); \
                     const context = root && root.nodeType === Node.DOCUMENT_FRAGMENT_NODE ? root : this.ownerDocument; \
                     const result = this.ownerDocument.evaluate(xpath, context, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null); \
                     for (let i = 0; i < result.snapshotLength; i += 1) {{ \
                         if (result.snapshotItem(i) === this) return true; \
                     }} \
                     return false;",
                    xpath = json_string(locator.query())?,
                ))?,
                "xpath locator match",
            ),
        }
    }

    fn page_wrapper(&self) -> Page {
        let page = Page::new(Arc::clone(&self.runtime), self.page.clone());
        match &self.browser {
            Some(browser) => page.with_browser(browser.clone()),
            None => page,
        }
    }

    fn path_via_page_marker(&self, xpath: bool) -> OpenPageResult<String> {
        let marker = format!("{}-path", next_marker_batch());
        self.set_attr(MARKER_ATTRIBUTE, &marker)?;
        let selector = format!(r#"[{MARKER_ATTRIBUTE}="{marker}"]"#);
        let script = if xpath {
            format!(
                "(() => {{ \
                    const el = document.querySelector({selector}); \
                    if (!el || el.nodeType !== Node.ELEMENT_NODE) return null; \
                    let path = ''; \
                    let node = el; \
                    while (node && node.nodeType === Node.ELEMENT_NODE) {{ \
                        const tag = node.nodeName.toLowerCase(); \
                        let sib = node; \
                        let nth = 0; \
                        while (sib) {{ \
                            if (sib.nodeType === Node.ELEMENT_NODE && sib.nodeName.toLowerCase() === tag) nth += 1; \
                            sib = sib.previousSibling; \
                        }} \
                        path = '/' + tag + '[' + nth + ']' + path; \
                        node = node.parentNode; \
                    }} \
                    return path; \
                }})()",
                selector = json_string(&selector)?,
            )
        } else {
            format!(
                "(() => {{ \
                    const el = document.querySelector({selector}); \
                    if (!el || el.nodeType !== Node.ELEMENT_NODE) return null; \
                    let path = ''; \
                    let node = el; \
                    while (node && node.nodeType === Node.ELEMENT_NODE) {{ \
                        const id = node.getAttribute('id'); \
                        if (id) {{ \
                            path = '>' + node.tagName.toLowerCase() + '#' + id + path; \
                            node = node.parentNode; \
                            continue; \
                        }} \
                        let sib = node; \
                        let nth = 0; \
                        while (sib) {{ \
                            if (sib.nodeType === Node.ELEMENT_NODE) nth += 1; \
                            sib = sib.previousSibling; \
                        }} \
                        path = '>' + node.tagName.toLowerCase() + ':nth-child(' + nth + ')' + path; \
                        node = node.parentNode; \
                    }} \
                    return path.startsWith('>') ? path.slice(1) : path; \
                }})()",
                selector = json_string(&selector)?,
            )
        };
        let page = self.page_wrapper();
        let result = page.run_js(&script);
        let cleanup = self.remove_attr(MARKER_ATTRIBUTE);
        let value = match (result, cleanup) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(err), Ok(())) => Err(err),
            (Ok(_), Err(err)) => Err(err),
            (Err(err), Err(_)) => Err(err),
        }?;
        value_as_string(value, if xpath { "xpath" } else { "css path" })
    }

    fn viewport_point_to_screen(&self, point: (f64, f64)) -> OpenPageResult<Option<(f64, f64)>> {
        let (viewport_screen_x, viewport_screen_y) =
            self.top_viewport_screen_origin().map_err(|err| {
                OpenPageError::PageOperation(resolve_top_viewport_screen_origin_failed_message(
                    &err.to_string(),
                ))
            })?;
        let Some((frame_offset_x, frame_offset_y)) =
            self.frame_viewport_offset().map_err(|err| {
                OpenPageError::PageOperation(resolve_frame_viewport_offset_failed_message(
                    &err.to_string(),
                ))
            })?
        else {
            return Ok(None);
        };
        let device_pixel_ratio = self.top_window_device_pixel_ratio().map_err(|err| {
            OpenPageError::PageOperation(resolve_top_window_device_pixel_ratio_failed_message(
                &err.to_string(),
            ))
        })?;
        Ok(Some((
            (viewport_screen_x + frame_offset_x + point.0) * device_pixel_ratio,
            (viewport_screen_y + frame_offset_y + point.1) * device_pixel_ratio,
        )))
    }

    fn top_viewport_screen_origin(&self) -> OpenPageResult<(f64, f64)> {
        let page = self.page_wrapper();
        let window_state = page.window_state()?;
        let (window_left, window_top) = page.window_location()?;
        let (window_width, window_height) = page.window_size()?;
        let (viewport_width, viewport_height) = value_as_f64_pair(
            page.run_js("[window.innerWidth, window.innerHeight]")
                .map_err(|err| {
                    OpenPageError::PageOperation(top_window_viewport_size_lookup_failed_message(
                        &err.to_string(),
                    ))
                })?,
            "top window viewport size with scrollbar",
        )?;

        let (window_left, window_top) =
            if matches!(window_state.as_str(), "maximized" | "fullscreen") {
                (0.0, 0.0)
            } else {
                (window_left as f64 + 7.0, window_top as f64)
            };

        let (window_width, window_height) = match window_state.as_str() {
            "fullscreen" => (window_width as f64, window_height as f64),
            "maximized" => (window_width as f64 - 16.0, window_height as f64 - 16.0),
            _ => (window_width as f64 - 16.0, window_height as f64 - 7.0),
        };

        Ok((
            window_left + window_width - viewport_width,
            window_top + window_height - viewport_height,
        ))
    }

    fn frame_viewport_offset(&self) -> OpenPageResult<Option<(f64, f64)>> {
        let page = self.page_wrapper();
        let main_frame_id = page.main_frame_id()?;
        let current_frame_id = self.element_frame_id().map_err(|err| {
            OpenPageError::PageOperation(resolve_element_frame_id_failed_message(&err.to_string()))
        })?;
        if current_frame_id == main_frame_id {
            return Ok(Some((0.0, 0.0)));
        }

        let absolute_owner_location = self
            .frame_owner_viewport_location(&current_frame_id)
            .map_err(|err| {
                OpenPageError::PageOperation(resolve_frame_owner_viewport_location_failed_message(
                    &current_frame_id,
                    &err.to_string(),
                ))
            })?;
        if absolute_owner_location.is_some() {
            return Ok(absolute_owner_location);
        }

        let mut current_frame_id = current_frame_id;
        let mut offset_x = 0.0;
        let mut offset_y = 0.0;
        while current_frame_id != main_frame_id {
            let Some((owner_x, owner_y)) = self
                .frame_owner_viewport_location(&current_frame_id)
                .map_err(|err| {
                    OpenPageError::PageOperation(
                        resolve_frame_owner_viewport_location_failed_message(
                            &current_frame_id,
                            &err.to_string(),
                        ),
                    )
                })?
            else {
                return Ok(None);
            };
            offset_x += owner_x;
            offset_y += owner_y;
            let Some(parent_frame_id) = page.frame_parent_id(&current_frame_id)? else {
                break;
            };
            current_frame_id = parent_frame_id;
        }

        Ok(Some((offset_x, offset_y)))
    }

    fn element_frame_id(&self) -> OpenPageResult<String> {
        let page = self.page_wrapper();
        let main_frame_id = page.main_frame_id()?;

        let described_frame_id = match self.describe_frame_id() {
            Ok(frame_id) => frame_id,
            Err(err) if should_fallback_frame_id_lookup(&err) => None,
            Err(err) => return Err(err),
        };
        if let Some(frame_id) = described_frame_id {
            return Ok(frame_id);
        }
        if value_as_bool(
            self.run_js("return window.top === window;")
                .map_err(|err| {
                    OpenPageError::PageOperation(element_top_frame_check_failed_message(
                        &err.to_string(),
                    ))
                })?,
            "element top-frame check",
        )? {
            return Ok(main_frame_id);
        }

        let marker = format!("{}-frame-owner", next_marker_batch());
        self.set_attr(MARKER_ATTRIBUTE, &marker)?;
        let selector = format!(r#"css:[{MARKER_ATTRIBUTE}="{marker}"]"#);

        let detected_frame_id = (|| -> OpenPageResult<Option<String>> {
            let mut deferred_scan_error = None;
            for frame_id in page.download_scope_frame_ids()? {
                if frame_id == main_frame_id {
                    continue;
                }
                let owner_element = self.frame_owner_element(&frame_id)?;
                let frame = Frame::new(
                    page.clone(),
                    frame_id.clone(),
                    owner_element,
                    Arc::clone(self.none_element_runtime_config_handle()),
                );
                match frame.find(selector.as_str()) {
                    Ok(_) => return Ok(Some(frame_id)),
                    Err(OpenPageError::ElementNotFound(_)) => {}
                    Err(OpenPageError::JavaScript(message))
                        if message.contains("No value found") =>
                    {
                        if deferred_scan_error.is_none() {
                            deferred_scan_error = Some(OpenPageError::PageOperation(
                                scan_frame_marker_javascript_failed_message(&frame_id, &message),
                            ));
                        }
                    }
                    Err(err) => {
                        return Err(OpenPageError::PageOperation(
                            scan_frame_marker_failed_message(&frame_id, &err.to_string()),
                        ));
                    }
                }
            }
            if let Some(err) = deferred_scan_error {
                return Err(err);
            }
            Ok(None)
        })()?;

        let _ = self.remove_attr(MARKER_ATTRIBUTE);
        Ok(detected_frame_id.unwrap_or(main_frame_id))
    }

    fn describe_frame_id(&self) -> OpenPageResult<Option<String>> {
        let mut last_error = None;

        let describe_result = |params: DescribeNodeParams| {
            self.runtime.block_on(async {
                execute_page_command_async(&self.page, params, "Element::describe_frame_id()")
                    .await
                    .map(|response| {
                        response
                            .node
                            .frame_id
                            .map(|frame_id| frame_id.as_ref().to_string())
                    })
            })
        };

        for params in [
            DescribeNodeParams::builder()
                .object_id(self.inner.remote_object_id.clone())
                .build(),
            DescribeNodeParams::builder()
                .node_id(self.inner.node_id)
                .build(),
            DescribeNodeParams::builder()
                .backend_node_id(self.inner.backend_node_id)
                .build(),
        ] {
            match describe_result(params) {
                Ok(Some(frame_id)) => return Ok(Some(frame_id)),
                Ok(None) => {}
                Err(err) => last_error = Some(err),
            }
        }

        if let Some(err) = last_error {
            Err(err)
        } else {
            Ok(None)
        }
    }

    fn frame_owner_element(&self, frame_id: &str) -> OpenPageResult<Element> {
        let (owner_node_id, owner_backend_node_id) = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.page,
                GetFrameOwnerParams::new(FrameId::new(frame_id.to_string())),
                "Element::frame_owner_element()",
            )
            .await?;
            Ok::<
                (
                    Option<chromiumoxide::cdp::browser_protocol::dom::NodeId>,
                    BackendNodeId,
                ),
                OpenPageError,
            >((response.node_id, response.backend_node_id))
        })?;
        if let Some(node_id) = owner_node_id {
            self.resolve_node_id(node_id, "frame owner could not be resolved to an element")
        } else {
            self.resolve_backend_node_id(owner_backend_node_id)
        }
    }

    fn frame_owner_viewport_location(&self, frame_id: &str) -> OpenPageResult<Option<(f64, f64)>> {
        let (owner_node_id, owner_backend_node_id) = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.page,
                GetFrameOwnerParams::new(FrameId::new(frame_id.to_string())),
                "Element::frame_owner_viewport_location()",
            )
            .await?;
            Ok::<
                (
                    Option<chromiumoxide::cdp::browser_protocol::dom::NodeId>,
                    BackendNodeId,
                ),
                OpenPageError,
            >((response.node_id, response.backend_node_id))
        })?;

        let model = self.runtime.block_on(async {
            let params = if let Some(node_id) = owner_node_id {
                GetBoxModelParams::builder().node_id(node_id).build()
            } else {
                GetBoxModelParams::builder()
                    .backend_node_id(owner_backend_node_id)
                    .build()
            };
            execute_page_command_async(
                &self.page,
                params,
                "Element::frame_owner_viewport_location()",
            )
            .await
        });
        match model {
            Ok(response) => {
                let quad = &response.model.border;
                if quad.inner().len() < 2 {
                    return Err(OpenPageError::PageOperation(
                        "frame owner box model did not contain a valid quad".to_string(),
                    ));
                }
                Ok(Some((quad.inner()[0], quad.inner()[1])))
            }
            Err(OpenPageError::PageOperation(message))
                if message.contains("Could not compute box model") =>
            {
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    fn top_window_device_pixel_ratio(&self) -> OpenPageResult<f64> {
        let page = self.page_wrapper();
        page.run_js("window.devicePixelRatio || 1")
            .map_err(|err| {
                OpenPageError::PageOperation(format!(
                    "top window devicePixelRatio lookup failed: {err}"
                ))
            })?
            .as_f64()
            .ok_or_else(|| {
                OpenPageError::JavaScript(top_window_device_pixel_ratio_not_numeric_message())
            })
    }

    fn with_marker_locator<R, F>(&self, action: F) -> OpenPageResult<R>
    where
        F: FnOnce(&Page, &str) -> OpenPageResult<R>,
    {
        let marker = format!("{}-clicker", next_marker_batch());
        self.set_attr(MARKER_ATTRIBUTE, &marker)?;
        let page = self.page_wrapper();
        let locator = format!(r#"xpath://*[@{MARKER_ATTRIBUTE}="{marker}"]"#);
        let result = action(&page, locator.as_str());
        let _ = self.remove_attr(MARKER_ATTRIBUTE);
        result
    }

    fn clickable_point(&self) -> OpenPageResult<Point> {
        self.runtime.block_on(async {
            self.inner
                .scroll_into_view()
                .await
                .map_err(|err| element_operation_error("scroll into view", err))?
                .clickable_point()
                .await
                .map_err(|err| element_operation_error("resolve clickable point", err))
        })
    }

    fn drag_to_point_from_self(&self, target: Point, duration_secs: f64) -> OpenPageResult<()> {
        let start = self.clickable_point()?;
        self.drag_between(start, target, duration_secs)
    }

    fn dispatch_mouse_click(
        &self,
        x: i64,
        y: i64,
        button: MouseButton,
        click_count: u32,
    ) -> OpenPageResult<()> {
        let point = Point {
            x: x as f64,
            y: y as f64,
        };
        let buttons = mouse_button_buttons(&button);
        self.runtime.block_on(async {
            run_element_page_future_with_cdp_timeout(self.page.move_mouse(point), "move mouse")
                .await?;

            let mut pressed = DispatchMouseEventParams::new(
                DispatchMouseEventType::MousePressed,
                x as f64,
                y as f64,
            );
            pressed.button = Some(button.clone());
            pressed.buttons = Some(buttons);
            pressed.click_count = Some(click_count.into());
            execute_page_command_async(&self.page, pressed, "Element::click_at_point()").await?;

            let mut released = DispatchMouseEventParams::new(
                DispatchMouseEventType::MouseReleased,
                x as f64,
                y as f64,
            );
            released.button = Some(button);
            released.buttons = Some(0);
            released.click_count = Some(click_count.into());
            execute_page_command_async(&self.page, released, "Element::click_at_point()").await?;
            Ok(())
        })
    }

    fn drag_between(&self, start: Point, end: Point, duration_secs: f64) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_element_page_future_with_cdp_timeout(self.page.move_mouse(start), "move mouse")
                .await?;
            let mut pressed = DispatchMouseEventParams::new(
                DispatchMouseEventType::MousePressed,
                start.x,
                start.y,
            );
            pressed.button = Some(MouseButton::Left);
            pressed.click_count = Some(1);
            execute_page_command_async(&self.page, pressed, "Element::drag_between()").await?;
            Ok::<(), OpenPageError>(())
        })?;

        let path = drag_path(start, end, duration_secs);
        let pause = drag_step_pause(duration_secs, path.len());
        let path_len = path.len();
        for (index, point) in path.into_iter().enumerate() {
            self.runtime.block_on(async {
                run_element_page_future_with_cdp_timeout(self.page.move_mouse(point), "move mouse")
                    .await?;
                Ok::<(), OpenPageError>(())
            })?;
            if index + 1 < path_len {
                if let Some(pause) = pause {
                    sleep(pause);
                }
            }
        }

        self.runtime.block_on(async {
            let mut released =
                DispatchMouseEventParams::new(DispatchMouseEventType::MouseReleased, end.x, end.y);
            released.button = Some(MouseButton::Left);
            released.click_count = Some(1);
            execute_page_command_async(&self.page, released, "Element::drag_between()").await?;
            Ok(())
        })
    }

    fn current_src(&self, tag: &str) -> OpenPageResult<Option<String>> {
        if tag == "link" {
            return self.attr("href");
        }
        value_as_optional_string(
            Some(self.run_js("return this.currentSrc || this.src || null;")?),
            "current src",
        )
    }

    fn wait_for_blob_resource(
        &self,
        src: &str,
        deadline: Instant,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        while Instant::now() < deadline {
            if let Some(result) = self.try_blob_resource(src, base64_to_bytes)? {
                return Ok(Some(result));
            }
            sleep(Duration::from_millis(50));
        }
        Ok(None)
    }

    fn try_blob_resource(
        &self,
        src: &str,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        let script = format!(
            "return fetch({src}) \
                .then(response => response.blob()) \
                .then(blob => new Promise(resolve => {{ \
                    const reader = new FileReader(); \
                    reader.onloadend = () => resolve(typeof reader.result === 'string' ? reader.result : null); \
                    reader.onerror = () => resolve(null); \
                    reader.readAsDataURL(blob); \
                }})) \
                .catch(() => null);",
            src = json_string(src)?,
        );
        match self.run_js(&script)? {
            Value::Null => Ok(None),
            Value::String(data_url) => {
                decode_data_url_content(&data_url, base64_to_bytes).map(Some)
            }
            other => Err(OpenPageError::JavaScript(
                blob_src_data_url_required_message(&other.to_string()),
            )),
        }
    }

    fn try_resource_content(
        &self,
        src: &str,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        let result = self.runtime.block_on(async {
            let node = match execute_page_command_async(
                &self.page,
                DescribeNodeParams::builder()
                    .backend_node_id(self.inner.backend_node_id)
                    .build(),
                "Element::try_resource_content()",
            )
            .await
            {
                Ok(response) => response.node,
                Err(_) => return Ok(None),
            };

            let frame_id = match node.frame_id {
                Some(frame_id) => frame_id,
                None => match execute_page_command_async(
                    &self.page,
                    GetFrameTreeParams::default(),
                    "Element::try_resource_content()",
                )
                .await
                {
                    Ok(response) => response.frame_tree.frame.id,
                    Err(_) => return Ok(None),
                },
            };

            let response = match execute_page_command_async(
                &self.page,
                GetResourceContentParams::new(frame_id, src.to_string()),
                "Element::try_resource_content()",
            )
            .await
            {
                Ok(response) => response,
                Err(_) => return Ok(None),
            };

            decode_resource_content(response.content, response.base64_encoded, base64_to_bytes)
                .map(Some)
        })?;
        Ok(result)
    }

    fn find_all_by_xpath(&self, xpath: &str) -> OpenPageResult<Vec<Element>> {
        let xpath = normalize_relative_xpath(xpath);
        self.collect_relative_elements(&format!(
            "const xpath = {xpath}; \
             const result = []; \
             const iterator = document.evaluate(xpath, this, null, XPathResult.ORDERED_NODE_ITERATOR_TYPE, null); \
             for (let node = iterator.iterateNext(); node; node = iterator.iterateNext()) {{ \
                 if (node instanceof Element) result.push(node); \
             }} \
             return result;",
            xpath = json_string(&xpath)?,
        ))
    }

    fn collect_relative_elements(&self, script: &str) -> OpenPageResult<Vec<Element>> {
        let batch = next_marker_batch();
        let markers_json = value_as_string(
            self.run_js(&format!(
                "const attr = {attr}; \
                 const batch = {batch}; \
                 const elements = (() => {{ {script} }})() || []; \
                 let index = 0; \
                 const markers = Array.from(elements) \
                    .filter(element => element instanceof Element) \
                    .map(element => {{ \
                        const marker = `${{batch}}-${{index++}}`; \
                        element.setAttribute(attr, marker); \
                        return marker; \
                    }}); \
                 return JSON.stringify(markers);",
                attr = json_string(MARKER_ATTRIBUTE)?,
                batch = json_string(&batch)?,
            ))?,
            "relative element markers",
        )?;
        let markers: Vec<String> = serde_json::from_str(&markers_json)
            .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
        let elements = self.resolve_marked_elements(&markers);
        let cleanup = self.clear_markers(&batch);
        match (elements, cleanup) {
            (Ok(elements), Ok(())) => Ok(elements),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn resolve_marked_elements(&self, markers: &[String]) -> OpenPageResult<Vec<Element>> {
        self.runtime.block_on(async {
            let mut elements = Vec::with_capacity(markers.len());
            for marker in markers {
                let selector = format!("[{MARKER_ATTRIBUTE}=\"{marker}\"]");
                let element = self
                    .page
                    .find_element(selector)
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
                elements.push(Element::new(
                    Arc::clone(&self.runtime),
                    self.page.clone(),
                    self.browser.clone(),
                    self.uploader.clone(),
                    element,
                    self.javascript_timeout_ms,
                    Arc::clone(&self.none_element_config),
                ));
            }
            Ok(elements)
        })
    }

    fn clear_markers(&self, batch: &str) -> OpenPageResult<()> {
        self.run_js(&format!(
            "const attr = {attr}; \
             const batch = {batch}; \
             document.querySelectorAll(`[${{attr}}^=\"${{batch}}-\"]`) \
                 .forEach(element => element.removeAttribute(attr)); \
             return true;",
            attr = json_string(MARKER_ATTRIBUTE)?,
            batch = json_string(batch)?,
        ))?;
        Ok(())
    }
}

impl<'a> ElementClicker<'a> {
    fn browser(&self) -> OpenPageResult<&Browser> {
        self.element.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_element_only_message(
                "clicker() tab-aware helpers",
            ))
        })
    }

    fn timeout_ms(&self, requested: Option<u64>) -> OpenPageResult<u64> {
        match requested {
            Some(timeout_ms) => Ok(timeout_ms),
            None => match &self.element.browser {
                Some(browser) => Ok(browser.timeouts()?.implicit_wait),
                None => Ok(10_000),
            },
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

    pub fn middle(&self, get_tab: bool) -> OpenPageResult<Option<Page>> {
        if get_tab && self.element.browser.as_ref().is_none() {
            return Err(OpenPageError::UnsupportedOperation(
                browser_backed_element_only_message("clicker().middle(get_tab=true)"),
            ));
        }
        let timeout_ms = self.timeout_ms(None)?;
        let browser = self.element.browser.as_ref();
        let current_tab_id = match browser {
            Some(browser) => Some(
                browser
                    .newest_tab_id()?
                    .unwrap_or_else(|| self.element.page.target_id().as_ref().to_string()),
            ),
            None => None,
        };
        if get_tab && let Some(browser) = browser {
            browser.activate_tab(self.element.page.target_id().as_ref())?;
        }
        self.element.click_middle()?;

        let detect_timeout_ms = if get_tab {
            timeout_ms
        } else {
            timeout_ms.min(500)
        };
        if let Some(browser) = browser {
            if let Some(target_id) =
                browser.wait_for_new_tab(current_tab_id.as_deref(), detect_timeout_ms)?
            {
                if get_tab {
                    return browser
                        .wait_for_page(&target_id, detect_timeout_ms)
                        .map(Some);
                }
                return Ok(None);
            }
        }
        if get_tab {
            return Err(OpenPageError::PageOperation(no_new_tab_message()));
        }
        Ok(None)
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
        let timeout_ms = self.timeout_ms(timeout_ms)?;
        let uploader = self.element.uploader.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_element_only_message(
                "clicker().to_upload()",
            ))
        })?;
        uploader.set_files(files)?;
        if !self.left_with_options(Some(by_js), Some(timeout_ms), true)? {
            return Ok(false);
        }
        uploader.wait_until_inputted(timeout_ms)
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
        self.element.with_marker_locator(|page, locator| {
            page.click_to_download(
                locator,
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            )
        })
    }

    pub fn for_new_tab(
        &self,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>> {
        let browser = self.browser()?;
        let timeout_ms = self.timeout_ms(timeout_ms)?;
        let current_tab_id = browser
            .newest_tab_id()?
            .unwrap_or_else(|| self.element.page.target_id().as_ref().to_string());
        browser.activate_tab(self.element.page.target_id().as_ref())?;
        let _ = self.left_with_options(Some(by_js), Some(timeout_ms), true)?;
        if let Some(target_id) = browser.wait_for_new_tab(Some(&current_tab_id), timeout_ms)? {
            return browser.wait_for_page(&target_id, timeout_ms).map(Some);
        }
        Err(OpenPageError::PageOperation(no_new_tab_message()))
    }
}

impl ElementScroller<'_> {
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

impl ElementSetter<'_> {
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

impl ElementSelector<'_> {
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
        I: Into<SelectOptionInput<'a>>,
    {
        self.element.select_by_option(option)
    }

    pub fn by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
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
        I: Into<SelectOptionInput<'a>>,
    {
        self.element.cancel_by_option(option)
    }

    pub fn cancel_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
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

    pub fn options(&self) -> OpenPageResult<Vec<Element>> {
        self.element.option_elements()
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<Element>> {
        self.element.selected_option_element()
    }

    pub fn selected_options(&self) -> OpenPageResult<Vec<Element>> {
        self.element.selected_option_elements()
    }
}

impl ElementStates<'_> {
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

impl ElementRect<'_> {
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

    pub fn screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_screen_midpoint()
    }

    pub fn screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_screen_click_point()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_size()
    }

    pub fn scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.element.rect_scroll_position()
    }
}

impl ElementWait<'_> {
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

fn mouse_button_buttons(button: &MouseButton) -> i64 {
    match button {
        MouseButton::None => 0,
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
    }
}

fn select_input_values(input: ActionsInput<'_>) -> Vec<String> {
    match input {
        ActionsInput::Single(value) => vec![value.into_owned()],
        ActionsInput::Many(values) => values.into_iter().map(|value| value.into_owned()).collect(),
    }
}

fn value_as_bool(value: Value, name: &str) -> OpenPageResult<bool> {
    match value {
        Value::Bool(value) => Ok(value),
        other => Err(OpenPageError::JavaScript(
            value_state_bool_required_message(name, &other.to_string()),
        )),
    }
}

fn value_as_f64_pair(value: Value, name: &str) -> OpenPageResult<(f64, f64)> {
    let values = match value {
        Value::Array(values) => values,
        Value::String(serialized) => {
            serde_json::from_str::<Vec<Value>>(&serialized).map_err(|err| {
                OpenPageError::Serialization(value_coordinate_pair_parse_failed_message(
                    name,
                    &err.to_string(),
                ))
            })?
        }
        _ => {
            return Err(OpenPageError::JavaScript(
                value_coordinate_pair_required_message(name),
            ));
        }
    };
    if values.len() != 2 {
        return Err(OpenPageError::JavaScript(format!(
            "{}",
            value_coordinate_pair_exactly_two_message(name)
        )));
    }
    let x = values[0].as_f64().ok_or_else(|| {
        OpenPageError::JavaScript(value_coordinate_not_numeric_message(
            name,
            "x",
            &values[0].to_string(),
        ))
    })?;
    let y = values[1].as_f64().ok_or_else(|| {
        OpenPageError::JavaScript(value_coordinate_not_numeric_message(
            name,
            "y",
            &values[1].to_string(),
        ))
    })?;
    Ok((x, y))
}

fn value_as_optional_f64_pair(value: Value, name: &str) -> OpenPageResult<Option<(f64, f64)>> {
    match value {
        Value::Null => Ok(None),
        other => value_as_f64_pair(other, name).map(Some),
    }
}

fn value_as_optional_string(value: Option<Value>, name: &str) -> OpenPageResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(Value::Bool(value)) => Ok(Some(value.to_string())),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(other) => Err(OpenPageError::JavaScript(
            value_string_compatible_required_message(name, &other.to_string()),
        )),
    }
}

fn value_as_usize(value: Value, name: &str) -> OpenPageResult<usize> {
    match value {
        Value::Number(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                OpenPageError::JavaScript(value_non_negative_integer_required_message(
                    name,
                    &value.to_string(),
                ))
            }),
        other => Err(OpenPageError::JavaScript(value_number_required_message(
            name,
            &other.to_string(),
        ))),
    }
}

#[cfg(test)]
fn element_path_script(xpath: bool) -> &'static str {
    if xpath {
        "function(){ \
            let el = this; \
            if (!el || el.nodeType !== Node.ELEMENT_NODE) return null; \
            let path = ''; \
            while (el && el.nodeType === Node.ELEMENT_NODE) { \
                const tag = el.nodeName.toLowerCase(); \
                let sib = el; \
                let nth = 0; \
                while (sib) { \
                    if (sib.nodeType === Node.ELEMENT_NODE && sib.nodeName.toLowerCase() === tag) nth += 1; \
                    sib = sib.previousSibling; \
                } \
                path = '/' + tag + '[' + nth + ']' + path; \
                el = el.parentNode; \
            } \
            return path; \
        }"
    } else {
        "function(){ \
            let el = this; \
            if (!el || el.nodeType !== Node.ELEMENT_NODE) return null; \
            let path = ''; \
            while (el && el.nodeType === Node.ELEMENT_NODE) { \
                const id = el.getAttribute('id'); \
                if (id) { \
                    path = '>' + el.tagName.toLowerCase() + '#' + id + path; \
                    el = el.parentNode; \
                    continue; \
                } \
                let sib = el; \
                let nth = 0; \
                while (sib) { \
                    if (sib.nodeType === Node.ELEMENT_NODE) nth += 1; \
                    sib = sib.previousSibling; \
                } \
                path = '>' + el.tagName.toLowerCase() + ':nth-child(' + nth + ')' + path; \
                el = el.parentNode; \
            } \
            return path.startsWith('>') ? path.slice(1) : path; \
        }"
    }
}

fn normalize_relative_xpath(xpath: &str) -> String {
    let xpath = xpath.trim();
    if xpath.starts_with('/') {
        format!(".{xpath}")
    } else {
        xpath.to_string()
    }
}

fn normalize_child_xpath(xpath: &str) -> String {
    let xpath = xpath.trim().trim_start_matches(['.', '/']);
    format!("./{xpath}")
}

fn normalize_axis_xpath(axis: &str, xpath: &str) -> String {
    let xpath = xpath.trim().trim_start_matches(['.', '/']);
    format!("./{axis}::{xpath}")
}

fn value_as_string(value: Value, name: &str) -> OpenPageResult<String> {
    match value {
        Value::String(value) => Ok(value),
        Value::Null => Err(OpenPageError::ElementNotFound(value_unavailable_message(
            name,
        ))),
        other => Err(OpenPageError::JavaScript(value_string_required_message(
            name,
            &other.to_string(),
        ))),
    }
}

fn value_as_string_vec(value: Value, name: &str) -> OpenPageResult<Vec<String>> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Null => Ok(None),
                Value::String(value) => Ok(Some(value)),
                Value::Bool(value) => Ok(Some(value.to_string())),
                Value::Number(value) => Ok(Some(value.to_string())),
                other => Err(OpenPageError::JavaScript(
                    value_string_vec_entry_required_message(name, &other.to_string()),
                )),
            })
            .filter_map(|item| match item {
                Ok(Some(value)) => Some(Ok(value)),
                Ok(None) => None,
                Err(err) => Some(Err(err)),
            })
            .collect(),
        other => Err(OpenPageError::JavaScript(
            value_string_vec_array_required_message(name, &other.to_string()),
        )),
    }
}

fn json_string(value: &str) -> OpenPageResult<String> {
    serde_json::to_string(value).map_err(|err| OpenPageError::Serialization(err.to_string()))
}

pub(crate) fn build_js_invocation(
    script: &str,
    args: &[Value],
    as_expr: bool,
) -> OpenPageResult<String> {
    let args_json =
        serde_json::to_string(args).map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    if as_expr {
        Ok(format!(
            "function() {{ const __args = {args_json}; return ((...args) => ({script}))(...__args); }}"
        ))
    } else {
        Ok(format!(
            "function() {{ const __args = {args_json}; return (function(...args) {{ {script} }}).apply(this, __args); }}"
        ))
    }
}

fn should_fallback_frame_id_lookup(error: &OpenPageError) -> bool {
    match error {
        OpenPageError::PageOperation(message) => {
            message.contains("Could not find node with given id")
                || message.contains("No object Id found")
                || message.contains("No object id found")
        }
        _ => false,
    }
}

fn parse_optional_locator(locator: Option<&str>) -> OpenPageResult<Option<Locator>> {
    let Some(locator) = locator.map(str::trim).filter(|locator| !locator.is_empty()) else {
        return Ok(None);
    };
    Locator::parse(locator).map(Some)
}

pub(crate) fn resolve_javascript_timeout_ms(
    requested: Option<u64>,
    default_timeout_ms: u64,
) -> u64 {
    requested.unwrap_or(default_timeout_ms).max(1)
}

fn parse_mouse_button(button: &str) -> OpenPageResult<MouseButton> {
    button
        .parse::<MouseButton>()
        .map_err(|_| OpenPageError::PageOperation(unsupported_mouse_button_message(button)))
}

fn validate_click_at_count(count: u32) -> OpenPageResult<()> {
    if count == 0 {
        Err(OpenPageError::PageOperation(
            click_at_count_must_be_positive_message(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn load_javascript_source(script: &str) -> OpenPageResult<Cow<'_, str>> {
    match fs::metadata(script) {
        Ok(metadata) if metadata.is_file() => match fs::read_to_string(script) {
            Ok(source) => Ok(Cow::Owned(source)),
            Err(_) => Ok(Cow::Borrowed(script)),
        },
        _ => Ok(Cow::Borrowed(script)),
    }
}

impl RelativeDirection {
    fn method_name(self) -> &'static str {
        match self {
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
            Self::North => "north",
        }
    }
}

fn midpoint_from_corners_f64(corners: &[(f64, f64)]) -> (f64, f64) {
    let left = corners[0].0;
    let top = corners[0].1;
    let right = corners[1].0;
    let bottom = corners[2].1;
    ((left + right) / 2.0, (top + bottom) / 2.0)
}

fn relative_search_seed(
    corners: &[(f64, f64)],
    direction: RelativeDirection,
) -> ((i64, i64), i64, i64) {
    let left = corners[0].0.round() as i64;
    let top = corners[0].1.round() as i64;
    let right = corners[1].0.round() as i64;
    let bottom = corners[2].1.round() as i64;
    let mid_x = ((corners[0].0 + corners[1].0) / 2.0).round() as i64;
    let mid_y = ((corners[0].1 + corners[2].1) / 2.0).round() as i64;
    match direction {
        RelativeDirection::East => ((right, mid_y), 1, 0),
        RelativeDirection::South => ((mid_x, bottom), 0, 1),
        RelativeDirection::West => ((left, mid_y), -1, 0),
        RelativeDirection::North => ((mid_x, top), 0, -1),
    }
}

fn relative_search_in_bounds(
    x: i64,
    y: i64,
    viewport_width: i64,
    viewport_height: i64,
    direction: RelativeDirection,
) -> bool {
    match direction {
        RelativeDirection::East | RelativeDirection::West => {
            x > 0 && x < viewport_width && y >= 0 && y < viewport_height
        }
        RelativeDirection::South | RelativeDirection::North => {
            y > 0 && y < viewport_height && x >= 0 && x < viewport_width
        }
    }
}

fn next_marker_batch() -> String {
    format!(
        "openpage-{}",
        NEXT_MARKER_BATCH.fetch_add(1, Ordering::Relaxed)
    )
}

fn nth_element_from_start(
    elements: Vec<Element>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    elements
        .into_iter()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn nth_element_from_end(
    elements: Vec<Element>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    elements
        .into_iter()
        .rev()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn src_attribute_name(tag: &str) -> &str {
    if tag == "link" { "href" } else { "src" }
}

fn decode_data_url_content(
    data_url: &str,
    base64_to_bytes: bool,
) -> OpenPageResult<ElementResource> {
    let (_, payload) = data_url
        .split_once(',')
        .ok_or_else(|| OpenPageError::Serialization(data_url_missing_comma_message()))?;
    if base64_to_bytes {
        let bytes = BASE64_STANDARD
            .decode(payload.as_bytes())
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        Ok(ElementResource::Bytes(bytes))
    } else {
        Ok(ElementResource::Text(payload.to_string()))
    }
}

fn decode_resource_content(
    content: String,
    base64_encoded: bool,
    base64_to_bytes: bool,
) -> OpenPageResult<ElementResource> {
    if base64_encoded && base64_to_bytes {
        let bytes = BASE64_STANDARD
            .decode(content.as_bytes())
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        Ok(ElementResource::Bytes(bytes))
    } else {
        Ok(ElementResource::Text(content))
    }
}

fn resolve_save_name(
    tag: &str,
    src_attr: Option<&str>,
    current_src: Option<&str>,
    requested_name: Option<&str>,
) -> String {
    requested_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(sanitize_file_name)
        .or_else(|| default_data_image_name(tag, src_attr))
        .or_else(|| current_src.and_then(file_name_from_src))
        .unwrap_or_else(|| sanitize_file_name(tag))
}

fn default_data_image_name(tag: &str, src_attr: Option<&str>) -> Option<String> {
    if tag != "img" {
        return None;
    }
    let src = src_attr?;
    let lower = src.to_ascii_lowercase();
    if !lower.starts_with("data:image/") {
        return None;
    }
    let (_, rest) = src.split_once("data:image/")?;
    let (ext, _) = rest.split_once(';')?;
    let ext = sanitize_file_name(ext);
    if ext.is_empty() {
        None
    } else {
        Some(format!("img.{ext}"))
    }
}

fn file_name_from_src(src: &str) -> Option<String> {
    let value = src.trim();
    if value.is_empty() {
        return None;
    }
    let candidate = match url::Url::parse(value) {
        Ok(url) => url
            .path_segments()
            .and_then(|mut segments| segments.next_back().map(str::to_string))
            .unwrap_or_default(),
        Err(_) => value
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string(),
    };
    let sanitized = sanitize_file_name(&candidate);
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = name
        .trim()
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            _ => ch,
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() {
        "resource".to_string()
    } else {
        sanitized
    }
}

fn next_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("resource");
    let extension = path.extension().and_then(|value| value.to_str());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for index in 1.. {
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}_{index}.{extension}"),
            _ => format!("{stem}_{index}"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("path candidate iteration should always return")
}

fn absolutize_path(path: PathBuf) -> OpenPageResult<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn resolve_screenshot_target_path(
    tag: &str,
    path: Option<&Path>,
    name: Option<&str>,
) -> OpenPageResult<PathBuf> {
    let mut target = match (path, name) {
        (Some(path), Some(name)) => path.join(sanitize_file_name(name)),
        (Some(path), None) if path.extension().is_some() => path.to_path_buf(),
        (Some(path), None) => path.join(format!("{tag}.png")),
        (None, Some(name)) => PathBuf::from(sanitize_file_name(name)),
        (None, None) => PathBuf::from(format!("{tag}.png")),
    };
    if target.extension().is_none() {
        target.set_extension("png");
    }
    let target = absolutize_path(target)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(target)
}

fn normalize_file_input_paths(files: &[String]) -> OpenPageResult<Vec<String>> {
    files
        .iter()
        .flat_map(|value| value.split('\n'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|file| {
            let path = PathBuf::from(file);
            let absolute = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()?.join(path)
            };
            Ok(absolute.to_string_lossy().into_owned())
        })
        .collect()
}

fn drag_path(start: Point, end: Point, duration_secs: f64) -> Vec<Point> {
    let steps = drag_step_count(duration_secs);
    (1..=steps)
        .map(|step| {
            let ratio = step as f64 / steps as f64;
            Point::new(
                start.x + (end.x - start.x) * ratio,
                start.y + (end.y - start.y) * ratio,
            )
        })
        .collect()
}

fn drag_step_count(duration_secs: f64) -> usize {
    if duration_secs <= 0.0 {
        1
    } else {
        ((duration_secs * 20.0).ceil() as usize).max(2)
    }
}

fn drag_step_pause(duration_secs: f64, steps: usize) -> Option<Duration> {
    if duration_secs <= 0.0 || steps <= 1 {
        None
    } else {
        Some(Duration::from_secs_f64(duration_secs / (steps - 1) as f64))
    }
}

fn should_clear_before_typing(text: &str) -> bool {
    !matches!(text, "\n" | "\u{e006}" | "\u{e007}")
}

fn should_clear_before_typing_sequence(values: &[String]) -> bool {
    !(values.len() == 1 && !should_clear_before_typing(values[0].as_str()))
}

fn split_text_or_keys_with_modifiers(values: &[String]) -> (i64, Vec<String>) {
    let mut modifiers = 0;
    let mut result = Vec::new();
    for value in values {
        if let Some(bit) = modifier_bit(value) {
            modifiers |= bit;
            continue;
        }
        if keys::get_key_definition(value).is_some() {
            result.push(value.clone());
            continue;
        }
        result.extend(value.chars().map(|ch| ch.to_string()));
    }
    (modifiers, result)
}

fn modifier_bit(value: &str) -> Option<i64> {
    match value.to_ascii_lowercase().as_str() {
        "alt" => Some(MODIFIER_ALT),
        "control" | "ctrl" => Some(MODIFIER_CTRL),
        "meta" | "command" | "cmd" => Some(MODIFIER_META),
        "shift" => Some(MODIFIER_SHIFT),
        _ => None,
    }
}

fn build_key_event(
    definition: &keys::KeyDefinition,
    modifiers: i64,
    key_up: bool,
) -> DispatchKeyEventParams {
    let mut builder = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::RawKeyDown)
        .modifiers(modifiers)
        .key(definition.key)
        .code(definition.code)
        .windows_virtual_key_code(definition.key_code)
        .native_virtual_key_code(definition.key_code);

    let mut has_text = false;
    if let Some(text) = definition.text {
        builder = builder.unmodified_text(text);
        if modifiers & !MODIFIER_SHIFT == 0 {
            builder = builder.text(text);
            has_text = !text.is_empty();
        } else {
            builder = builder.text("");
        }
    } else if definition.key.len() == 1 {
        builder = builder.unmodified_text(definition.key);
        if modifiers & !MODIFIER_SHIFT == 0 {
            builder = builder.text(definition.key);
            has_text = true;
        } else {
            builder = builder.text("");
        }
    }

    if cfg!(target_os = "macos") && (modifiers & MODIFIER_META) != 0 && !key_up {
        if let Some(commands) = mac_meta_commands(definition.key) {
            builder = builder.commands(commands.iter().copied());
        }
    }

    let event_type = if key_up {
        DispatchKeyEventType::KeyUp
    } else if has_text {
        DispatchKeyEventType::KeyDown
    } else {
        DispatchKeyEventType::RawKeyDown
    };

    builder
        .r#type(event_type)
        .build()
        .expect("DispatchKeyEventParams should build with required type")
}

fn mac_meta_commands(key: &str) -> Option<&'static [&'static str]> {
    match key.to_ascii_lowercase().as_str() {
        "a" => Some(&["selectAll"]),
        "c" => Some(&["copy"]),
        "x" => Some(&["cut"]),
        "v" => Some(&["paste"]),
        "z" => Some(&["undo"]),
        "y" => Some(&["redo"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        element_operation_error, parse_mouse_button, resolve_javascript_timeout_ms,
        run_element_future_with_cdp_timeout, run_element_page_future_with_cdp_timeout,
        validate_click_at_count,
    };

    #[test]
    fn resolve_javascript_timeout_ms_prefers_explicit_value() {
        assert_eq!(resolve_javascript_timeout_ms(Some(250), 30_000), 250);
        assert_eq!(resolve_javascript_timeout_ms(Some(0), 30_000), 1);
        assert_eq!(resolve_javascript_timeout_ms(None, 30_000), 30_000);
    }

    #[test]
    fn parse_mouse_button_errors_follow_settings_language() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();

        let error = parse_mouse_button("side").expect_err("invalid mouse button should fail");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "unsupported mouse button: side"),
            "unexpected error: {error}"
        );

        crate::Settings::set_language("cn");

        let error = parse_mouse_button("side").expect_err("invalid mouse button should fail");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "不支持的鼠标按钮: side"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn element_operation_errors_follow_settings_language() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();

        let error = element_operation_error("read attribute", "boom");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "element operation read attribute failed: boom"),
            "unexpected error: {error}"
        );

        crate::Settings::set_language("cn");

        let error = element_operation_error("read attribute", "boom");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "元素操作 read attribute 失败: boom"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn element_read_operations_respect_global_timeout_setting() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();
        crate::Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("runtime");

        let text_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Option<String>, &'static str>(Some("hello".to_string()))
                },
                "read inner text",
            ))
            .expect_err("element text read should time out");
        assert!(
            matches!(text_error, crate::OpenPageError::Timeout(ref message) if message.contains("read inner text")),
            "unexpected text timeout error: {text_error}"
        );

        let attrs_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<String>, &'static str>(vec!["class".to_string(), "demo".to_string()])
                },
                "read attributes",
            ))
            .expect_err("element attrs read should time out");

        crate::Settings::reset();

        assert!(
            matches!(attrs_error, crate::OpenPageError::Timeout(ref message) if message.contains("read attributes")),
            "unexpected attrs timeout error: {attrs_error}"
        );
    }

    #[test]
    fn element_lightweight_operations_respect_global_timeout_setting() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();
        crate::Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("runtime");

        let screenshot_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<Vec<u8>, &'static str>(vec![1, 2, 3])
                },
                "capture screenshot",
            ))
            .expect_err("element screenshot should time out");
        assert!(
            matches!(screenshot_error, crate::OpenPageError::Timeout(ref message) if message.contains("capture screenshot")),
            "unexpected screenshot timeout error: {screenshot_error}"
        );

        let focus_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "focus",
            ))
            .expect_err("element focus should time out");

        crate::Settings::reset();

        assert!(
            matches!(focus_error, crate::OpenPageError::Timeout(ref message) if message.contains("focus")),
            "unexpected focus timeout error: {focus_error}"
        );
    }

    #[test]
    fn element_key_and_scroll_operations_respect_global_timeout_setting() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();
        crate::Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("runtime");

        let press_key_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "press key",
            ))
            .expect_err("element press_key should time out");
        assert!(
            matches!(press_key_error, crate::OpenPageError::Timeout(ref message) if message.contains("press key")),
            "unexpected press key timeout error: {press_key_error}"
        );

        let scroll_error = runtime
            .block_on(run_element_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "scroll into view",
            ))
            .expect_err("element scroll_into_view should time out");

        crate::Settings::reset();

        assert!(
            matches!(scroll_error, crate::OpenPageError::Timeout(ref message) if message.contains("scroll into view")),
            "unexpected scroll timeout error: {scroll_error}"
        );
    }

    #[test]
    fn element_mouse_move_operations_respect_global_timeout_setting() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();
        crate::Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("runtime");

        let move_error = runtime
            .block_on(run_element_page_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "move mouse",
            ))
            .expect_err("element move_mouse should time out");

        crate::Settings::reset();

        assert!(
            matches!(move_error, crate::OpenPageError::Timeout(ref message) if message.contains("move mouse")),
            "unexpected move mouse timeout error: {move_error}"
        );
    }

    #[test]
    fn validate_click_at_count_errors_follow_settings_language() {
        let _guard = crate::settings::scoped_test_settings();
        crate::Settings::reset();

        validate_click_at_count(1).expect("positive click count should pass");
        let error = validate_click_at_count(0).expect_err("zero click count should fail");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "click_at() count must be >= 1"),
            "unexpected error: {error}"
        );

        crate::Settings::set_language("cn");

        let error = validate_click_at_count(0).expect_err("zero click count should fail");
        assert!(
            matches!(error, crate::OpenPageError::PageOperation(ref message) if message == "click_at() 次数必须大于等于 1"),
            "unexpected error: {error}"
        );
    }

    use std::time::Duration;

    use super::{
        ElementResource, MODIFIER_ALT, MODIFIER_CTRL, MODIFIER_META, MODIFIER_SHIFT,
        RelativeDirection, decode_data_url_content, decode_resource_content, drag_path,
        drag_step_count, drag_step_pause, element_path_script, file_name_from_src,
        mac_meta_commands, modifier_bit, next_available_path, normalize_axis_xpath,
        normalize_child_xpath, normalize_file_input_paths, relative_search_in_bounds,
        resolve_save_name, resolve_screenshot_target_path, sanitize_file_name,
        should_clear_before_typing, should_clear_before_typing_sequence,
        split_text_or_keys_with_modifiers, src_attribute_name, value_as_usize,
    };
    use crate::Keys;
    use chromiumoxide::layout::Point;
    use tokio::runtime::Runtime;

    #[test]
    fn src_attribute_name_prefers_href_for_link() {
        assert_eq!(src_attribute_name("link"), "href");
        assert_eq!(src_attribute_name("img"), "src");
    }

    #[test]
    fn decode_data_url_content_can_decode_bytes() {
        let value =
            decode_data_url_content("data:image/png;base64,aGVsbG8=", true).expect("decode");
        assert_eq!(value, ElementResource::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn decode_data_url_content_can_keep_base64_payload() {
        let value =
            decode_data_url_content("data:image/png;base64,aGVsbG8=", false).expect("decode");
        assert_eq!(value, ElementResource::Text("aGVsbG8=".to_string()));
    }

    #[test]
    fn decode_resource_content_can_decode_page_base64() {
        let value = decode_resource_content("aGVsbG8=".to_string(), true, true).expect("decode");
        assert_eq!(value, ElementResource::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn element_resource_accessors_return_matching_variant_values() {
        let bytes = ElementResource::Bytes(b"hello".to_vec());
        assert_eq!(bytes.as_bytes(), Some(&b"hello"[..]));
        assert_eq!(bytes.as_text(), None);
        assert_eq!(bytes.clone().into_bytes(), Some(b"hello".to_vec()));
        assert_eq!(bytes.into_text(), None);

        let text = ElementResource::Text("aGVsbG8=".to_string());
        assert_eq!(text.as_bytes(), None);
        assert_eq!(text.as_text(), Some("aGVsbG8="));
        assert_eq!(text.clone().into_bytes(), None);
        assert_eq!(text.into_text(), Some("aGVsbG8=".to_string()));
    }

    #[test]
    fn resolve_save_name_prefers_requested_name() {
        assert_eq!(
            resolve_save_name(
                "img",
                None,
                Some("https://example.com/a.png"),
                Some("a:b.png")
            ),
            "a_b.png"
        );
    }

    #[test]
    fn resolve_save_name_uses_data_image_extension() {
        assert_eq!(
            resolve_save_name(
                "img",
                Some("data:image/png;base64,aGVsbG8="),
                Some("https://example.com/a.webp"),
                None,
            ),
            "img.png"
        );
    }

    #[test]
    fn file_name_from_src_strips_query() {
        assert_eq!(
            file_name_from_src("https://example.com/assets/demo.png?x=1#part"),
            Some("demo.png".to_string())
        );
    }

    #[test]
    fn sanitize_file_name_falls_back_when_empty() {
        assert_eq!(sanitize_file_name("..."), "resource");
        assert_eq!(sanitize_file_name("a/b:c"), "a_b_c");
    }

    #[test]
    fn next_available_path_appends_numeric_suffix() {
        let root =
            std::env::temp_dir().join(format!("openpage-rust-element-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("image.png");
        std::fs::write(&path, b"first").expect("seed file");
        let next = next_available_path(&path);
        assert_eq!(
            next.file_name().and_then(|value| value.to_str()),
            Some("image_1.png")
        );
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("cleanup temp dir");
        }
    }

    #[test]
    fn resolve_screenshot_target_path_defaults_to_tag_name() {
        let path = resolve_screenshot_target_path("div", None, None).expect("path");
        assert!(path.is_absolute());
        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("div.png")
        );
    }

    #[test]
    fn resolve_screenshot_target_path_appends_png_extension() {
        let path = resolve_screenshot_target_path("img", None, Some("shot")).expect("path");
        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("shot.png")
        );
    }

    #[test]
    fn normalize_file_input_paths_splits_lines_and_absolutizes() {
        let values = normalize_file_input_paths(&[
            "a.txt\nb.txt".to_string(),
            "/tmp/demo.txt".to_string(),
            "   ".to_string(),
        ])
        .expect("paths should normalize");
        assert_eq!(values.len(), 3);
        assert!(values[0].ends_with("/a.txt") || values[0].ends_with("\\a.txt"));
        assert!(values[1].ends_with("/b.txt") || values[1].ends_with("\\b.txt"));
        assert_eq!(values[2], "/tmp/demo.txt".to_string());
    }

    #[test]
    fn normalize_child_xpath_converts_to_direct_child_query() {
        assert_eq!(normalize_child_xpath("./div"), "./div");
        assert_eq!(normalize_child_xpath("//div"), "./div");
    }

    #[test]
    fn normalize_axis_xpath_prefixes_requested_axis() {
        assert_eq!(
            normalize_axis_xpath("following-sibling", "./span[@x='1']"),
            "./following-sibling::span[@x='1']"
        );
        assert_eq!(normalize_axis_xpath("preceding", "//a"), "./preceding::a");
    }

    #[test]
    fn drag_path_interpolates_from_start_to_end() {
        let path = drag_path(Point::new(10.0, 20.0), Point::new(30.0, 50.0), 0.5);
        assert_eq!(path.first().expect("first point"), &Point::new(12.0, 23.0));
        assert_eq!(path.last().expect("last point"), &Point::new(30.0, 50.0));
        assert_eq!(path.len(), 10);
    }

    #[test]
    fn drag_step_count_keeps_zero_duration_to_single_move() {
        assert_eq!(drag_step_count(0.0), 1);
        assert_eq!(drag_step_count(-1.0), 1);
        assert_eq!(drag_step_count(0.1), 2);
    }

    #[test]
    fn drag_step_pause_uses_duration_between_moves() {
        assert_eq!(drag_step_pause(0.0, 5), None);
        assert_eq!(drag_step_pause(0.2, 1), None);
        assert_eq!(drag_step_pause(0.3, 4), Some(Duration::from_secs_f64(0.1)));
    }

    #[test]
    fn should_clear_before_typing_skips_enter_keys() {
        assert!(should_clear_before_typing("demo"));
        assert!(!should_clear_before_typing("\n"));
        assert!(!should_clear_before_typing("\u{e006}"));
        assert!(!should_clear_before_typing("\u{e007}"));
    }

    #[test]
    fn should_clear_before_typing_sequence_matches_enter_shortcut() {
        assert!(should_clear_before_typing_sequence(&["demo".to_string()]));
        assert!(!should_clear_before_typing_sequence(&["\n".to_string()]));
        assert!(should_clear_before_typing_sequence(&[
            "\n".to_string(),
            "a".to_string()
        ]));
    }

    #[test]
    fn modifier_bit_supports_common_aliases() {
        assert_eq!(modifier_bit("Alt"), Some(MODIFIER_ALT));
        assert_eq!(modifier_bit("Ctrl"), Some(MODIFIER_CTRL));
        assert_eq!(modifier_bit("command"), Some(MODIFIER_META));
        assert_eq!(modifier_bit("Shift"), Some(MODIFIER_SHIFT));
        assert_eq!(modifier_bit("Delete"), None);
    }

    #[test]
    fn split_text_or_keys_with_modifiers_keeps_special_keys() {
        let (modifiers, values) = split_text_or_keys_with_modifiers(&[
            "Meta".to_string(),
            "ab".to_string(),
            "Delete".to_string(),
        ]);
        assert_eq!(modifiers, MODIFIER_META);
        assert_eq!(values, vec!["a", "b", "Delete"]);
    }

    #[test]
    fn split_text_or_keys_with_modifiers_accepts_public_keys_shortcuts() {
        let values = Keys::CTRL_A
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let (modifiers, keys) = split_text_or_keys_with_modifiers(&values);

        if cfg!(target_os = "macos") {
            assert_eq!(modifiers, MODIFIER_META);
        } else {
            assert_eq!(modifiers, MODIFIER_CTRL);
        }
        assert_eq!(keys, vec!["a"]);
    }

    #[test]
    fn mac_meta_commands_matches_reference_shortcuts() {
        assert_eq!(mac_meta_commands("a"), Some(&["selectAll"][..]));
        assert_eq!(mac_meta_commands("z"), Some(&["undo"][..]));
        assert_eq!(mac_meta_commands("q"), None);
    }

    #[test]
    fn value_as_usize_accepts_non_negative_integers() {
        assert_eq!(
            value_as_usize(serde_json::Value::from(3), "count").expect("count"),
            3
        );
    }

    #[test]
    fn element_path_script_matches_reference_shapes() {
        let xpath = element_path_script(true);
        let css = element_path_script(false);
        assert!(xpath.contains("nodeName.toLowerCase() === tag"));
        assert!(css.contains("tagName.toLowerCase() + '#' + id"));
    }

    #[test]
    fn relative_search_in_bounds_allows_zero_on_fixed_axis() {
        assert!(relative_search_in_bounds(
            10,
            0,
            100,
            100,
            RelativeDirection::East
        ));
        assert!(relative_search_in_bounds(
            0,
            10,
            100,
            100,
            RelativeDirection::South
        ));
        assert!(!relative_search_in_bounds(
            0,
            10,
            100,
            100,
            RelativeDirection::West
        ));
        assert!(!relative_search_in_bounds(
            10,
            0,
            100,
            100,
            RelativeDirection::North
        ));
    }
}
