use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde_json::Value;

use crate::browser::{Browser, BrowserTabReference};
use crate::element::{Element, ElementResource};
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::Locator;
use crate::page::{Frame, Page};
use crate::session::{SessionElement, SessionPage, snapshot_find_all, snapshot_root};
use crate::settings::{
    data_url_missing_comma_message, get_blob_data_url_required_message,
    get_blob_resolve_failed_message, get_blob_url_required_message,
    make_session_ele_index_out_of_range_message, make_session_ele_index_resolution_failed_message,
    make_session_ele_index_zero_message, timeout_error,
};
use crate::shadow_root::ShadowRoot;
use crate::webpage::{WebElement, WebFrame, WebPage};

const DEFAULT_PROJECT_CONFIGS_NAME: &str = "dp_configs.ini";
const WAIT_UNTIL_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct Keys;

impl Keys {
    pub const BACKSPACE: &str = "Backspace";
    pub const TAB: &str = "Tab";
    pub const ENTER: &str = "Enter";
    pub const RETURN: &str = "Enter";
    pub const SHIFT: &str = "Shift";
    pub const CONTROL: &str = "Control";
    pub const CTRL: &str = "Control";
    pub const ALT: &str = "Alt";
    pub const ESCAPE: &str = "Escape";
    pub const ESC: &str = "Escape";
    pub const SPACE: &str = " ";
    pub const META: &str = "Meta";
    pub const COMMAND: &str = "Meta";
    pub const DELETE: &str = "Delete";
    pub const DEL: &str = "Delete";

    pub const CTRL_COMM: &str = if cfg!(target_os = "macos") {
        Self::META
    } else {
        Self::CONTROL
    };
    pub const CTRL_A: [&'static str; 2] = [Self::CTRL_COMM, "a"];
    pub const CTRL_C: [&'static str; 2] = [Self::CTRL_COMM, "c"];
    pub const CTRL_X: [&'static str; 2] = [Self::CTRL_COMM, "x"];
    pub const CTRL_V: [&'static str; 2] = [Self::CTRL_COMM, "v"];
    pub const CTRL_Z: [&'static str; 2] = [Self::CTRL_COMM, "z"];
    pub const CTRL_Y: [&'static str; 2] = [Self::CTRL_COMM, "y"];
}

pub struct By;

impl By {
    pub const ID: &str = "id";
    pub const XPATH: &str = "xpath";
    pub const LINK_TEXT: &str = "link text";
    pub const PARTIAL_LINK_TEXT: &str = "partial link text";
    pub const NAME: &str = "name";
    pub const TAG_NAME: &str = "tag name";
    pub const CLASS_NAME: &str = "class name";
    pub const CSS_SELECTOR: &str = "css selector";
}

pub enum BlobSource<'a> {
    Page(&'a Page),
    Frame(&'a Frame),
    WebPage(&'a WebPage),
    WebFrame(&'a WebFrame),
}

impl<'a> From<&'a Page> for BlobSource<'a> {
    fn from(value: &'a Page) -> Self {
        Self::Page(value)
    }
}

impl<'a> From<&'a Frame> for BlobSource<'a> {
    fn from(value: &'a Frame) -> Self {
        Self::Frame(value)
    }
}

impl<'a> From<&'a WebPage> for BlobSource<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::WebPage(value)
    }
}

impl<'a> From<&'a WebFrame> for BlobSource<'a> {
    fn from(value: &'a WebFrame) -> Self {
        Self::WebFrame(value)
    }
}

pub enum TreeSource<'a> {
    Page(&'a Page),
    Frame(&'a Frame),
    Element(&'a Element),
    SessionPage(&'a SessionPage),
    SessionElement(&'a SessionElement),
    WebPage(&'a WebPage),
    WebFrame(&'a WebFrame),
    WebElement(&'a WebElement),
    ShadowRoot(&'a ShadowRoot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeTextInput {
    Disabled,
    Full,
    Limit(usize),
}

impl From<bool> for TreeTextInput {
    fn from(value: bool) -> Self {
        if value { Self::Full } else { Self::Disabled }
    }
}

impl From<usize> for TreeTextInput {
    fn from(value: usize) -> Self {
        if value == 0 {
            Self::Disabled
        } else {
            Self::Limit(value)
        }
    }
}

impl From<u32> for TreeTextInput {
    fn from(value: u32) -> Self {
        Self::from(value as usize)
    }
}

impl From<u64> for TreeTextInput {
    fn from(value: u64) -> Self {
        if value == 0 {
            Self::Disabled
        } else {
            Self::Limit(value as usize)
        }
    }
}

impl From<i32> for TreeTextInput {
    fn from(value: i32) -> Self {
        if value <= 0 {
            Self::Disabled
        } else {
            Self::Limit(value as usize)
        }
    }
}

impl From<i64> for TreeTextInput {
    fn from(value: i64) -> Self {
        if value <= 0 {
            Self::Disabled
        } else {
            Self::Limit(value as usize)
        }
    }
}

impl From<isize> for TreeTextInput {
    fn from(value: isize) -> Self {
        if value <= 0 {
            Self::Disabled
        } else {
            Self::Limit(value as usize)
        }
    }
}

impl<'a> From<&'a Page> for TreeSource<'a> {
    fn from(value: &'a Page) -> Self {
        Self::Page(value)
    }
}

impl<'a> From<&'a Frame> for TreeSource<'a> {
    fn from(value: &'a Frame) -> Self {
        Self::Frame(value)
    }
}

impl<'a> From<&'a Element> for TreeSource<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Element(value)
    }
}

impl<'a> From<&'a SessionPage> for TreeSource<'a> {
    fn from(value: &'a SessionPage) -> Self {
        Self::SessionPage(value)
    }
}

impl<'a> From<&'a SessionElement> for TreeSource<'a> {
    fn from(value: &'a SessionElement) -> Self {
        Self::SessionElement(value)
    }
}

impl<'a> From<&'a WebPage> for TreeSource<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::WebPage(value)
    }
}

impl<'a> From<&'a WebFrame> for TreeSource<'a> {
    fn from(value: &'a WebFrame) -> Self {
        Self::WebFrame(value)
    }
}

impl<'a> From<&'a WebElement> for TreeSource<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
    }
}

impl<'a> From<&'a ShadowRoot> for TreeSource<'a> {
    fn from(value: &'a ShadowRoot) -> Self {
        Self::ShadowRoot(value)
    }
}

pub enum MakeSessionEleSource<'a> {
    Html(&'a str),
    Page(&'a Page),
    Frame(&'a Frame),
    Element(&'a Element),
    SessionPage(&'a SessionPage),
    SessionElement(&'a SessionElement),
    WebPage(&'a WebPage),
    WebFrame(&'a WebFrame),
    WebElement(&'a WebElement),
    ShadowRoot(&'a ShadowRoot),
}

impl<'a> From<&'a str> for MakeSessionEleSource<'a> {
    fn from(value: &'a str) -> Self {
        Self::Html(value)
    }
}

impl<'a> From<&'a Page> for MakeSessionEleSource<'a> {
    fn from(value: &'a Page) -> Self {
        Self::Page(value)
    }
}

impl<'a> From<&'a Frame> for MakeSessionEleSource<'a> {
    fn from(value: &'a Frame) -> Self {
        Self::Frame(value)
    }
}

impl<'a> From<&'a Element> for MakeSessionEleSource<'a> {
    fn from(value: &'a Element) -> Self {
        Self::Element(value)
    }
}

impl<'a> From<&'a SessionPage> for MakeSessionEleSource<'a> {
    fn from(value: &'a SessionPage) -> Self {
        Self::SessionPage(value)
    }
}

impl<'a> From<&'a SessionElement> for MakeSessionEleSource<'a> {
    fn from(value: &'a SessionElement) -> Self {
        Self::SessionElement(value)
    }
}

impl<'a> From<&'a WebPage> for MakeSessionEleSource<'a> {
    fn from(value: &'a WebPage) -> Self {
        Self::WebPage(value)
    }
}

impl<'a> From<&'a WebFrame> for MakeSessionEleSource<'a> {
    fn from(value: &'a WebFrame) -> Self {
        Self::WebFrame(value)
    }
}

impl<'a> From<&'a WebElement> for MakeSessionEleSource<'a> {
    fn from(value: &'a WebElement) -> Self {
        Self::WebElement(value)
    }
}

impl<'a> From<&'a ShadowRoot> for MakeSessionEleSource<'a> {
    fn from(value: &'a ShadowRoot) -> Self {
        Self::ShadowRoot(value)
    }
}

pub enum MakeSessionEleLocator<'a> {
    Locator(&'a str),
    By(&'a str, &'a str),
}

impl<'a> From<&'a str> for MakeSessionEleLocator<'a> {
    fn from(value: &'a str) -> Self {
        Self::Locator(value)
    }
}

impl<'a> From<(&'a str, &'a str)> for MakeSessionEleLocator<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::By(value.0, value.1)
    }
}

#[derive(Clone, Debug)]
pub enum MakeSessionEleResult {
    One(SessionElement),
    Many(Vec<SessionElement>),
}

pub fn make_session_ele<'a, S, L>(
    source: S,
    loc: Option<L>,
    index: Option<isize>,
) -> OpenPageResult<MakeSessionEleResult>
where
    S: Into<MakeSessionEleSource<'a>>,
    L: Into<MakeSessionEleLocator<'a>>,
{
    let source = source.into();
    let locator = match loc.map(Into::into) {
        Some(MakeSessionEleLocator::Locator(locator)) => Some(locator.to_string()),
        Some(MakeSessionEleLocator::By(by, value)) => {
            Some(Locator::from_by(by, value)?.raw().to_string())
        }
        None => None,
    };
    let elements = match locator.as_deref() {
        Some(locator) => source.find_all(locator)?,
        None => vec![source.root()?],
    };

    match index {
        Some(index) => Ok(MakeSessionEleResult::One(select_session_element(
            elements, index,
        )?)),
        None => Ok(MakeSessionEleResult::Many(elements)),
    }
}

pub fn make_session_ele_by<'a, S>(
    source: S,
    loc: Option<(&'a str, &'a str)>,
    index: Option<isize>,
) -> OpenPageResult<MakeSessionEleResult>
where
    S: Into<MakeSessionEleSource<'a>>,
{
    make_session_ele(source, loc, index)
}

pub fn wait_until<T, F>(timeout: Duration, mut function: F) -> OpenPageResult<T>
where
    F: FnMut() -> Option<T>,
{
    let start = Instant::now();
    loop {
        if let Some(value) = function() {
            return Ok(value);
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(timeout_error("wait_until()", timeout.as_millis() as u64));
        }

        let remaining = timeout.saturating_sub(elapsed);
        sleep(remaining.min(WAIT_UNTIL_POLL_INTERVAL));
    }
}

pub fn from_debugger_address(debugger_url: &str) -> OpenPageResult<Page> {
    let browser = Browser::connect(debugger_url)?;
    page_from_latest_tab_or_new_page(&browser)
}

pub fn from_selenium_debugger_address(debugger_url: &str) -> OpenPageResult<Page> {
    from_debugger_address(debugger_url)
}

pub fn from_playwright_debugger_address(debugger_url: &str) -> OpenPageResult<Page> {
    from_debugger_address(debugger_url)
}

pub fn get_blob<'a, S>(source: S, url: &str, as_bytes: bool) -> OpenPageResult<ElementResource>
where
    S: Into<BlobSource<'a>>,
{
    let source = source.into();
    get_blob_with_runner(url, as_bytes, |script| source.run_js(script))
}

pub fn get_blob_bytes<'a, S>(source: S, url: &str) -> OpenPageResult<Vec<u8>>
where
    S: Into<BlobSource<'a>>,
{
    let source = source.into();
    get_blob_bytes_with_runner(url, |script| source.run_js(script))
}

pub fn get_blob_text<'a, S>(source: S, url: &str) -> OpenPageResult<String>
where
    S: Into<BlobSource<'a>>,
{
    let source = source.into();
    get_blob_text_with_runner(url, |script| source.run_js(script))
}

pub fn tree<'a, S, T>(source: S, text: T, show_js: bool, show_css: bool) -> OpenPageResult<String>
where
    S: Into<TreeSource<'a>>,
    T: Into<TreeTextInput>,
{
    let source = source.into();
    let text = text.into();
    let root = source.snapshot_root()?;
    let mut lines = vec![format_tree_label(&root, text, show_js, show_css)?];
    append_tree_children(&root, "", text, show_js, show_css, &mut lines)?;
    Ok(lines.join("\n"))
}

pub fn print_tree<'a, S, T>(source: S, text: T, show_js: bool, show_css: bool) -> OpenPageResult<()>
where
    S: Into<TreeSource<'a>>,
    T: Into<TreeTextInput>,
{
    println!("{}", tree(source, text, show_js, show_css)?);
    Ok(())
}

pub fn configs_to_here(save_name: Option<&Path>) -> OpenPageResult<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let path = resolve_configs_to_here_target(save_name, current_dir.as_path());
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, load_default_configs_ini_contents()?)?;
    Ok(path)
}

impl BlobSource<'_> {
    fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        match self {
            Self::Page(page) => page.run_js(script),
            Self::Frame(frame) => frame.run_js(script),
            Self::WebPage(page) => page.run_js(script),
            Self::WebFrame(frame) => frame.run_js(script),
        }
    }
}

impl TreeSource<'_> {
    fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self {
            Self::Page(page) => page.snapshot_root(),
            Self::Frame(frame) => frame.snapshot_root(),
            Self::Element(element) => element.snapshot_root(),
            Self::SessionPage(page) => page.root(),
            Self::SessionElement(element) => Ok((*element).clone()),
            Self::WebPage(page) => page.snapshot_root(),
            Self::WebFrame(frame) => frame.snapshot_root(),
            Self::WebElement(element) => element.snapshot_root(),
            Self::ShadowRoot(root) => root.snapshot_root(),
        }
    }
}

impl MakeSessionEleSource<'_> {
    fn root(&self) -> OpenPageResult<SessionElement> {
        match self {
            Self::Html(html) => snapshot_root(html),
            Self::Page(page) => page.snapshot_root(),
            Self::Frame(frame) => frame.snapshot_root(),
            Self::Element(element) => element.snapshot_root(),
            Self::SessionPage(page) => page.root(),
            Self::SessionElement(element) => Ok((*element).clone()),
            Self::WebPage(page) => page.snapshot_root(),
            Self::WebFrame(frame) => frame.snapshot_root(),
            Self::WebElement(element) => element.snapshot_root(),
            Self::ShadowRoot(root) => root.snapshot_root(),
        }
    }

    fn find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        match self {
            Self::Html(html) => snapshot_find_all(html, locator),
            Self::Page(page) => page.snapshot_find_all(locator),
            Self::Frame(frame) => frame.snapshot_find_all(locator),
            Self::Element(element) => element.snapshot_find_all(locator),
            Self::SessionPage(page) => page.find_all(locator),
            Self::SessionElement(element) => element.find_all(locator),
            Self::WebPage(page) => page.snapshot_find_all(locator),
            Self::WebFrame(frame) => frame.snapshot_find_all(locator),
            Self::WebElement(element) => element.snapshot_find_all(locator),
            Self::ShadowRoot(root) => root.snapshot_find_all(locator),
        }
    }
}

fn select_session_element(
    elements: Vec<SessionElement>,
    index: isize,
) -> OpenPageResult<SessionElement> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            make_session_ele_index_zero_message(),
        ));
    }

    let len = elements.len() as isize;
    let resolved = if index > 0 { index - 1 } else { len + index };
    if resolved < 0 || resolved >= len {
        return Err(OpenPageError::ElementNotFound(
            make_session_ele_index_out_of_range_message(index),
        ));
    }

    elements.into_iter().nth(resolved as usize).ok_or_else(|| {
        OpenPageError::ElementNotFound(make_session_ele_index_resolution_failed_message())
    })
}

fn resolve_configs_to_here_target(save_name: Option<&Path>, current_dir: &Path) -> PathBuf {
    let mut target = match save_name {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => current_dir.join(path),
        None => current_dir.join(DEFAULT_PROJECT_CONFIGS_NAME),
    };

    if target.extension().is_none() {
        target.set_extension("ini");
    }

    target
}

fn load_default_configs_ini_contents() -> OpenPageResult<String> {
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini");
    match std::fs::read_to_string(default_path) {
        Ok(content) => Ok(content),
        Err(_) => Ok(include_str!("../configs.ini").to_string()),
    }
}

fn page_from_latest_tab_or_new_page(browser: &Browser) -> OpenPageResult<Page> {
    match browser.latest_tab()? {
        Some(BrowserTabReference::Page(page)) => Ok(page),
        Some(BrowserTabReference::WebPage(page)) => browser.get_page(&page.target_id()),
        Some(BrowserTabReference::Id(target_id)) => browser.get_page(&target_id),
        None => browser.new_page(None),
    }
}

fn get_blob_with_runner<F>(url: &str, as_bytes: bool, run_js: F) -> OpenPageResult<ElementResource>
where
    F: FnOnce(&str) -> OpenPageResult<Value>,
{
    if !url.starts_with("blob:") {
        return Err(OpenPageError::UnsupportedOperation(
            get_blob_url_required_message(url),
        ));
    }

    let script = build_blob_fetch_script(url)?;
    let result = run_js(&script)?;
    decode_blob_fetch_result(result, as_bytes)
}

fn get_blob_bytes_with_runner<F>(url: &str, run_js: F) -> OpenPageResult<Vec<u8>>
where
    F: FnOnce(&str) -> OpenPageResult<Value>,
{
    match get_blob_with_runner(url, true, run_js)? {
        ElementResource::Bytes(bytes) => Ok(bytes),
        ElementResource::Text(text) => BASE64_STANDARD
            .decode(text)
            .map_err(|err| OpenPageError::Serialization(err.to_string())),
    }
}

fn get_blob_text_with_runner<F>(url: &str, run_js: F) -> OpenPageResult<String>
where
    F: FnOnce(&str) -> OpenPageResult<Value>,
{
    match get_blob_with_runner(url, false, run_js)? {
        ElementResource::Text(text) => Ok(text),
        ElementResource::Bytes(bytes) => Ok(BASE64_STANDARD.encode(bytes)),
    }
}

fn build_blob_fetch_script(url: &str) -> OpenPageResult<String> {
    let encoded_url =
        serde_json::to_string(url).map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    Ok(format!(
        "return fetch({encoded_url}) \
            .then(response => response.blob()) \
            .then(blob => new Promise(resolve => {{ \
                const reader = new FileReader(); \
                reader.onloadend = () => resolve(typeof reader.result === 'string' ? reader.result : null); \
                reader.onerror = () => resolve(null); \
                reader.readAsDataURL(blob); \
            }})) \
            .catch(() => null);"
    ))
}

fn decode_blob_fetch_result(result: Value, as_bytes: bool) -> OpenPageResult<ElementResource> {
    match result {
        Value::String(data_url) => decode_blob_data_url(&data_url, as_bytes),
        Value::Null => Err(OpenPageError::JavaScript(get_blob_resolve_failed_message())),
        other => Err(OpenPageError::JavaScript(
            get_blob_data_url_required_message(&other.to_string()),
        )),
    }
}

fn decode_blob_data_url(data_url: &str, as_bytes: bool) -> OpenPageResult<ElementResource> {
    let (_, payload) = data_url
        .split_once(',')
        .ok_or_else(|| OpenPageError::Serialization(data_url_missing_comma_message()))?;
    if !as_bytes {
        return Ok(ElementResource::Text(payload.to_string()));
    }
    let bytes = BASE64_STANDARD
        .decode(payload)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    Ok(ElementResource::Bytes(bytes))
}

fn append_tree_children(
    element: &SessionElement,
    prefix: &str,
    text: TreeTextInput,
    show_js: bool,
    show_css: bool,
    lines: &mut Vec<String>,
) -> OpenPageResult<()> {
    let children = element.children()?;
    let length = children.len();
    for (index, child) in children.iter().enumerate() {
        let is_last = index + 1 == length;
        let tail = if is_last {
            "└───"
        } else {
            "├───"
        };
        lines.push(format!(
            "{prefix}{tail}{}",
            format_tree_label(child, text, show_js, show_css)?
        ));
        let next_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        append_tree_children(child, &next_prefix, text, show_js, show_css, lines)?;
    }
    Ok(())
}

fn format_tree_label(
    element: &SessionElement,
    text: TreeTextInput,
    show_js: bool,
    show_css: bool,
) -> OpenPageResult<String> {
    let tag = element.tag()?;
    let attrs = element
        .attrs()?
        .into_iter()
        .map(|(name, value)| format!("{name}='{value}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut label = if attrs.is_empty() {
        format!("<{tag}>")
    } else {
        format!("<{tag} {attrs}>")
    };

    if text != TreeTextInput::Disabled && should_include_tree_text(&tag, show_js, show_css) {
        if let Some(text_value) = direct_tree_text(element)? {
            label.push(' ');
            label.push_str(tree_text_output(&text_value, text).as_str());
        }
    }

    Ok(label.replace('\n', " "))
}

fn tree_text_output(text: &str, mode: TreeTextInput) -> String {
    match mode {
        TreeTextInput::Disabled => String::new(),
        TreeTextInput::Full => text.to_string(),
        TreeTextInput::Limit(limit) => text.chars().take(limit).collect(),
    }
}

fn should_include_tree_text(tag: &str, show_js: bool, show_css: bool) -> bool {
    match tag {
        "script" => show_js,
        "style" => show_css,
        _ => true,
    }
}

fn direct_tree_text(element: &SessionElement) -> OpenPageResult<Option<String>> {
    let text = element
        .texts(true)?
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok((!text.is_empty()).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::{
        By, DEFAULT_PROJECT_CONFIGS_NAME, Keys, MakeSessionEleResult, TreeTextInput,
        build_blob_fetch_script, configs_to_here, decode_blob_fetch_result, format_tree_label,
        from_debugger_address, from_playwright_debugger_address, from_selenium_debugger_address,
        get_blob_bytes_with_runner, get_blob_text_with_runner, get_blob_with_runner,
        make_session_ele, make_session_ele_by, print_tree, resolve_configs_to_here_target, tree,
        tree_text_output, wait_until,
    };
    use serde_json::Value;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::element::ElementResource;
    use crate::page::Page;
    use crate::session::{snapshot_find, snapshot_root};
    use crate::settings::scoped_test_settings;
    use crate::{SessionOptions, SessionPage, Settings};

    #[test]
    fn debugger_address_docking_helper_signatures_return_page() {
        let _ = from_debugger_address as fn(&str) -> crate::OpenPageResult<Page>;
        let _ = from_selenium_debugger_address as fn(&str) -> crate::OpenPageResult<Page>;
        let _ = from_playwright_debugger_address as fn(&str) -> crate::OpenPageResult<Page>;
    }

    const HTML: &str = r#"
<html>
  <body>
    <div class="item">alpha</div>
    <div class="item">beta</div>
    <div class="item">gamma</div>
  </body>
</html>
"#;

    #[test]
    fn wait_until_returns_first_truthy_value_before_timeout() {
        let attempts = AtomicUsize::new(0);

        let value = wait_until(Duration::from_millis(200), || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            (attempt >= 2).then_some("ready")
        })
        .expect("wait until should succeed");

        assert_eq!(value, "ready");
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn wait_until_returns_timeout_error_when_value_never_arrives() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let error = wait_until::<(), _>(Duration::from_millis(30), || None)
            .expect_err("wait until should time out");

        match error {
            crate::OpenPageError::Timeout(message) => {
                assert_eq!(message, "wait_until() timed out after 30 ms");
            }
            other => panic!("unexpected error: {other:?}"),
        }

        Settings::set_language("cn");

        let error = wait_until::<(), _>(Duration::from_millis(30), || None)
            .expect_err("wait until should localize timeout");

        match error {
            crate::OpenPageError::Timeout(message) => {
                assert_eq!(message, "wait_until() 等待超时（30 ms）");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn get_blob_rejects_non_blob_urls() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let error = get_blob_with_runner("https://example.com/demo.png", true, |_| {
            panic!("non-blob url should fail before JS runs")
        })
        .expect_err("non-blob url should be rejected");

        match error {
            crate::OpenPageError::UnsupportedOperation(message) => {
                assert!(message.contains("only accepts blob: urls"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        Settings::set_language("cn");

        let error = get_blob_with_runner("https://example.com/demo.png", true, |_| {
            panic!("non-blob url should fail before JS runs")
        })
        .expect_err("non-blob url should localize");

        match error {
            crate::OpenPageError::UnsupportedOperation(message) => {
                assert!(message.contains("只接受 blob: url"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn get_blob_decodes_bytes_from_data_url() {
        let result = get_blob_with_runner("blob:https://example.com/demo", true, |_| {
            Ok(Value::String("data:text/plain;base64,aGVsbG8=".to_string()))
        })
        .expect("blob bytes should decode");

        assert_eq!(result, ElementResource::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn get_blob_bytes_helper_returns_bytes() {
        let result = get_blob_bytes_with_runner("blob:https://example.com/demo", |_| {
            Ok(Value::String("data:text/plain;base64,aGVsbG8=".to_string()))
        })
        .expect("blob bytes helper should return bytes");

        assert_eq!(result, b"hello".to_vec());
    }

    #[test]
    fn get_blob_returns_base64_payload_when_as_bytes_is_false() {
        let result = get_blob_with_runner("blob:https://example.com/demo", false, |_| {
            Ok(Value::String("data:text/plain;base64,aGVsbG8=".to_string()))
        })
        .expect("blob text should keep base64 payload");

        assert_eq!(result, ElementResource::Text("aGVsbG8=".to_string()));
    }

    #[test]
    fn get_blob_text_helper_returns_base64_payload() {
        let result = get_blob_text_with_runner("blob:https://example.com/demo", |_| {
            Ok(Value::String("data:text/plain;base64,aGVsbG8=".to_string()))
        })
        .expect("blob text helper should return base64 payload");

        assert_eq!(result, "aGVsbG8=");
    }

    #[test]
    fn get_blob_builds_fetch_script_with_quoted_url() {
        let script =
            build_blob_fetch_script("blob:https://example.com/demo?id=1").expect("build script");

        assert!(script.contains("fetch(\"blob:https://example.com/demo?id=1\")"));
        assert!(script.contains("readAsDataURL(blob)"));
    }

    #[test]
    fn get_blob_reports_non_string_results() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let error = decode_blob_fetch_result(Value::Bool(true), true)
            .expect_err("non-string blob result should fail");

        match error {
            crate::OpenPageError::JavaScript(message) => {
                assert!(message.contains("expected a data URL string"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        Settings::set_language("cn");

        let error = decode_blob_fetch_result(Value::Bool(true), true)
            .expect_err("non-string blob result should localize");

        match error {
            crate::OpenPageError::JavaScript(message) => {
                assert!(message.contains("需要 data URL 字符串"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn get_blob_null_and_malformed_data_url_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_null = decode_blob_fetch_result(Value::Null, true)
            .expect_err("null blob result should fail")
            .to_string();
        assert!(english_null.contains("failed to resolve blob content"));

        let english_malformed =
            decode_blob_fetch_result(Value::String("data:text/plain".into()), true)
                .expect_err("malformed data URL should fail")
                .to_string();
        assert!(english_malformed.contains("data URL did not contain a comma separator"));

        Settings::set_language("cn");

        let chinese_null = decode_blob_fetch_result(Value::Null, true)
            .expect_err("null blob result should localize")
            .to_string();
        assert!(chinese_null.contains("未能解析 blob 内容"));

        let chinese_malformed =
            decode_blob_fetch_result(Value::String("data:text/plain".into()), true)
                .expect_err("malformed data URL should localize")
                .to_string();
        assert!(chinese_malformed.contains("data URL 不包含逗号分隔符"));
    }

    #[test]
    fn tree_formats_nested_structure_without_text() {
        let root =
            snapshot_root(r#"<html><body><div id="main"><span>hello</span></div></body></html>"#)
                .expect("snapshot root");

        let rendered = tree(&root, false, false, false).expect("render tree");

        assert_eq!(
            rendered,
            "<html>\n├───<head>\n└───<body>\n    └───<div id='main'>\n        └───<span>"
        );
    }

    #[test]
    fn tree_includes_text_and_skips_script_and_style_by_default() {
        let root = snapshot_root(
            r#"<html><body><div id="main">hi<script>const x = 1;</script><style>.a{color:red}</style><span>hello</span></div></body></html>"#,
        )
        .expect("snapshot root");

        let rendered = tree(&root, true, false, false).expect("render tree with text");

        assert!(rendered.contains("<div id='main'> hi"));
        assert!(rendered.contains("<span> hello"));
        assert!(!rendered.contains("const x = 1;"));
        assert!(!rendered.contains(".a{color:red}"));
    }

    #[test]
    fn tree_can_include_script_and_style_text_when_requested() {
        let root = snapshot_root(
            r#"<html><body><script>const x = 1;</script><style>.a{color:red}</style></body></html>"#,
        )
        .expect("snapshot root");

        let rendered = tree(&root, true, true, true).expect("render tree with script and style");

        assert!(rendered.contains("<script> const x = 1;"));
        assert!(rendered.contains("<style> .a{color:red}"));
    }

    #[test]
    fn tree_text_can_be_truncated_by_length_input() {
        let root = snapshot_root(r#"<html><body><div id="main">abcdef</div></body></html>"#)
            .expect("snapshot root");

        let rendered = tree(&root, 3usize, false, false).expect("render tree with truncated text");

        assert!(rendered.contains("<div id='main'> abc"));
        assert!(!rendered.contains("abcdef"));
    }

    #[test]
    fn tree_text_zero_and_negative_inputs_disable_text_output() {
        let root = snapshot_root(r#"<html><body><div id="main">abcdef</div></body></html>"#)
            .expect("snapshot root");

        let zero_rendered = tree(&root, 0usize, false, false).expect("render tree with zero limit");
        let negative_rendered =
            tree(&root, -1isize, false, false).expect("render tree with negative limit");

        assert_eq!(
            zero_rendered,
            "<html>\n├───<head>\n└───<body>\n    └───<div id='main'>"
        );
        assert_eq!(negative_rendered, zero_rendered);
    }

    #[test]
    fn print_tree_accepts_tree_inputs() {
        let root = snapshot_root(r#"<html><body><div id="main">hello</div></body></html>"#)
            .expect("snapshot root");

        print_tree(&root, true, false, false).expect("print tree");
    }

    #[test]
    fn tree_text_mode_helper_matches_reference_semantics() {
        assert_eq!(tree_text_output("abcdef", TreeTextInput::Disabled), "");
        assert_eq!(tree_text_output("abcdef", TreeTextInput::Full), "abcdef");
        assert_eq!(tree_text_output("abcdef", TreeTextInput::Limit(3)), "abc");
    }

    #[test]
    fn format_tree_label_omits_text_when_script_and_style_are_hidden() {
        let script = snapshot_find(
            r#"<html><body><script type="text/javascript">const x = 1;</script></body></html>"#,
            "script",
        )
        .expect("snapshot script");

        let label = format_tree_label(&script, TreeTextInput::Full, false, false)
            .expect("format script label");

        assert_eq!(label, "<script type='text/javascript'>");
    }

    #[test]
    fn make_session_ele_returns_root_when_no_locator_is_provided() {
        let result = make_session_ele(HTML, None::<&str>, Some(1)).expect("make session root");

        match result {
            MakeSessionEleResult::One(element) => {
                assert_eq!(element.tag().expect("root tag"), "html".to_string());
            }
            MakeSessionEleResult::Many(_) => panic!("expected single root element"),
        }
    }

    #[test]
    fn make_session_ele_supports_positive_and_negative_indexes() {
        let second = make_session_ele(HTML, Some(".item"), Some(2)).expect("second item");
        let last = make_session_ele(HTML, Some(".item"), Some(-1)).expect("last item");

        match second {
            MakeSessionEleResult::One(element) => {
                assert_eq!(
                    element.text().expect("second text"),
                    Some("beta".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single second element"),
        }

        match last {
            MakeSessionEleResult::One(element) => {
                assert_eq!(
                    element.text().expect("last text"),
                    Some("gamma".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single last element"),
        }
    }

    #[test]
    fn make_session_ele_index_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let zero = make_session_ele(HTML, Some(".item"), Some(0))
            .expect_err("index zero should be rejected")
            .to_string();
        assert!(zero.contains("index starts from 1 and does not accept 0"));

        let positive_out_of_range = make_session_ele(HTML, Some(".item"), Some(4))
            .expect_err("positive out-of-range index should be rejected")
            .to_string();
        assert!(positive_out_of_range.contains("index 4 is out of range"));

        let negative_out_of_range = make_session_ele(HTML, Some(".item"), Some(-4))
            .expect_err("negative out-of-range index should be rejected")
            .to_string();
        assert!(negative_out_of_range.contains("index -4 is out of range"));

        Settings::set_language("cn");

        let zero = make_session_ele(HTML, Some(".item"), Some(0))
            .expect_err("index zero should localize")
            .to_string();
        assert!(zero.contains("index 从 1 开始，不接受 0"));

        let positive_out_of_range = make_session_ele(HTML, Some(".item"), Some(4))
            .expect_err("positive out-of-range index should localize")
            .to_string();
        assert!(positive_out_of_range.contains("index 4 超出范围"));

        let negative_out_of_range = make_session_ele(HTML, Some(".item"), Some(-4))
            .expect_err("negative out-of-range index should localize")
            .to_string();
        assert!(negative_out_of_range.contains("index -4 超出范围"));
    }

    #[test]
    fn make_session_ele_returns_all_elements_when_index_is_none() {
        let result = make_session_ele(HTML, Some(".item"), None).expect("all items");

        match result {
            MakeSessionEleResult::Many(elements) => {
                assert_eq!(elements.len(), 3);
                assert_eq!(
                    elements[0].text().expect("first text"),
                    Some("alpha".to_string())
                );
            }
            MakeSessionEleResult::One(_) => panic!("expected element list"),
        }
    }

    #[test]
    fn make_session_ele_accepts_session_element_sources() {
        let root = snapshot_root(HTML).expect("root element");
        let result = make_session_ele(&root, Some(".item"), Some(2)).expect("second item");

        match result {
            MakeSessionEleResult::One(element) => {
                assert_eq!(
                    element.text().expect("second text"),
                    Some("beta".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single session element"),
        }
    }

    #[test]
    fn make_session_ele_returns_same_session_element_when_no_locator_is_given() {
        let element = snapshot_find(HTML, ".item").expect("first item");
        let result =
            make_session_ele(&element, None::<&str>, Some(1)).expect("same session element");

        match result {
            MakeSessionEleResult::One(found) => {
                assert_eq!(found.tag().expect("tag"), "div".to_string());
                assert_eq!(
                    found.attr("class").expect("class"),
                    Some("item".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected same session element"),
        }
    }

    #[test]
    fn make_session_ele_accepts_session_page_sources() {
        let path = make_temp_file(
            "make-session-ele-page",
            r#"<html><body><div class="item">alpha</div><div class="item">beta</div></body></html>"#,
        );
        let page = SessionPage::new(SessionOptions::default()).expect("session page");
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
        );

        let result = make_session_ele(&page, Some(".item"), Some(-1)).expect("last item");

        match result {
            MakeSessionEleResult::One(element) => {
                assert_eq!(element.text().expect("last text"), Some("beta".to_string()));
            }
            MakeSessionEleResult::Many(_) => panic!("expected single page result"),
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn make_session_ele_accepts_by_locator_tuples() {
        let html = r#"
<html>
  <body>
    <div id="root">
      <a href="/demo">demo link</a>
      <div class="item">alpha</div>
      <div class="item">beta</div>
    </div>
  </body>
</html>
"#;

        let by_id = make_session_ele(html, Some((By::ID, "root")), Some(1)).expect("by id result");
        let by_css = make_session_ele(html, Some((By::CSS_SELECTOR, ".item")), Some(-1))
            .expect("by css result");

        match by_id {
            MakeSessionEleResult::One(element) => {
                assert_eq!(element.tag().expect("root tag"), "div".to_string());
            }
            MakeSessionEleResult::Many(_) => panic!("expected single id element"),
        }

        match by_css {
            MakeSessionEleResult::One(element) => {
                assert_eq!(
                    element.text().expect("last item text"),
                    Some("beta".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single css element"),
        }
    }

    #[test]
    fn make_session_ele_by_supports_css_selector_and_id_mappings() {
        let html = r#"
<html>
  <body>
    <div id="root">
      <div class="item">alpha</div>
      <div class="item">beta</div>
    </div>
  </body>
</html>
"#;

        let result = make_session_ele_by(html, Some((By::CSS_SELECTOR, ".item")), Some(-1))
            .expect("by css result");

        match result {
            MakeSessionEleResult::One(element) => {
                assert_eq!(
                    element.text().expect("last item text"),
                    Some("beta".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single css element"),
        }
    }

    #[test]
    fn make_session_ele_by_supports_link_text_mappings() {
        let html = r#"<html><body><a href="/demo">demo link</a></body></html>"#;
        let result = make_session_ele_by(html, Some((By::LINK_TEXT, "demo link")), Some(1))
            .expect("by link text result");

        match result {
            MakeSessionEleResult::One(element) => {
                assert_eq!(element.tag().expect("link tag"), "a".to_string());
                assert_eq!(
                    element.link().expect("resolved link"),
                    Some("/demo".to_string())
                );
            }
            MakeSessionEleResult::Many(_) => panic!("expected single link element"),
        }
    }

    #[test]
    fn make_session_ele_rejects_unknown_by_values() {
        let error = make_session_ele(HTML, Some(("unsupported", "demo")), Some(1))
            .expect_err("unsupported by should fail");

        match error {
            crate::OpenPageError::UnsupportedLocator(message) => {
                assert!(message.contains("unsupported By locator"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn make_session_ele_by_rejects_unknown_by_values() {
        let error = make_session_ele_by(HTML, Some(("unsupported", "demo")), Some(1))
            .expect_err("unsupported by should fail");

        match error {
            crate::OpenPageError::UnsupportedLocator(message) => {
                assert!(message.contains("unsupported By locator"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn resolve_configs_to_here_target_defaults_to_dp_configs_in_current_dir() {
        let dir = make_temp_dir("configs-to-here-default-name");
        let target = resolve_configs_to_here_target(None, dir.as_path());

        assert_eq!(target, dir.join(DEFAULT_PROJECT_CONFIGS_NAME));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_configs_to_here_target_appends_ini_extension_when_missing() {
        let dir = make_temp_dir("configs-to-here-custom-name");
        let target =
            resolve_configs_to_here_target(Some(Path::new("project_config")), dir.as_path());

        assert_eq!(target, dir.join("project_config.ini"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn configs_to_here_writes_default_configs_snapshot() {
        let dir = make_temp_dir("configs-to-here-write");
        let target = dir.join("copied.ini");
        let saved = configs_to_here(Some(target.as_path())).expect("copy configs ini");
        let content = fs::read_to_string(&saved).expect("read copied configs ini");

        assert_eq!(saved, target);
        assert!(content.contains("[chromium_options]"));
        assert!(content.contains("[session_options]"));
        assert!(content.contains("load_mode = normal"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_exposes_reference_aliases_and_shortcuts() {
        assert_eq!(Keys::RETURN, Keys::ENTER);
        assert_eq!(Keys::CTRL, Keys::CONTROL);
        assert_eq!(Keys::ESC, Keys::ESCAPE);
        assert_eq!(Keys::DEL, Keys::DELETE);
        assert_eq!(Keys::CTRL_A[0], Keys::CTRL_COMM);
        assert_eq!(Keys::CTRL_A[1], "a");
        assert_eq!(Keys::CTRL_V[0], Keys::CTRL_COMM);
        assert_eq!(Keys::CTRL_V[1], "v");
    }

    fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("openpage-tools-{prefix}-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn make_temp_file(prefix: &str, content: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("openpage-tools-{prefix}-{unique}.html"));
        fs::write(&path, content).expect("write temp html");
        path
    }
}
