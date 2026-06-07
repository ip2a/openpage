# OpenPage Rust 代码库公开 API 总览

> 生成日期：2026-05-24
> 代码库路径：/Volumes/data0/data4work/2026_5/openpage/rust/src/

---

## 目录

1. [核心类型与错误](#1-核心类型与错误)
2. [浏览器连接与启动](#2-浏览器连接与启动)
3. [页面操作](#3-页面操作)
4. [元素定位与操作](#4-元素定位与操作)
5. [Session 模式（HTTP 静态解析）](#5-session-模式)
6. [WebPage 统一封装](#6-webpage-统一封装)
7. [下载管理](#7-下载管理)
8. [网络监听](#8-网络监听)
9. [请求拦截](#9-请求拦截)
10. [弹窗与控制台](#10-弹窗与控制台)
11. [录屏](#11-录屏)
12. [Shadow DOM](#12-shadow-dom)
13. [文件上传](#13-文件上传)
14. [窗口控制](#14-窗口控制)
15. [Python 绑定](#15-python-绑定)
16. [CLI](#16-cli)

---

## 1. 核心类型与错误

### 文件：`error.rs`

```rust
pub enum OpenPageError {
    BrowserLaunch(String),
    BrowserOperation(String),
    PageOperation(String),
    ElementNotFound(String),
    UnsupportedLocator(String),
    UnsupportedOperation(String),
    JavaScript(String),
    Http(String),
    Io(String),
    Timeout(String),
    Serialization(String),
}

pub type OpenPageResult<T> = Result<T, OpenPageError>;
```

---

## 2. 浏览器连接与启动

### 文件：`browser.rs`

#### 枚举

```rust
pub enum DownloadFileExistsMode { Rename, Overwrite, Skip }
pub enum LoadMode { Normal, Eager, None }
```

#### 结构体

```rust
pub struct LaunchOptions {
    pub browser_path: Option<PathBuf>,
    pub download_path: Option<PathBuf>,
    pub download_file_exists: DownloadFileExistsMode,
    pub load_mode: LoadMode,
    pub user_data_dir: Option<PathBuf>,
    pub remote_debugging_port: Option<u16>,
    pub headless: bool,
    pub width: u32,
    pub height: u32,
    pub no_sandbox: bool,
}

pub struct Browser { ... }
pub struct TabInfo { ... }
```

#### 公开方法（Browser）

| 方法 | 签名 |
|------|------|
| launch | `pub fn launch(options: LaunchOptions) -> OpenPageResult<Self>` |
| connect | `pub fn connect(debugger_url: &str) -> OpenPageResult<Self>` |
| new_page | `pub fn new_page(&self, url: Option<&str>) -> OpenPageResult<Page>` |
| new_tab | `pub fn new_tab(&self, url: Option<&str>, background: bool) -> OpenPageResult<Page>` |
| pages | `pub fn pages(&self) -> OpenPageResult<Vec<Page>>` |
| get_page | `pub fn get_page(&self, target_id: &str) -> OpenPageResult<Page>` |
| tabs_count | `pub fn tabs_count(&self) -> OpenPageResult<usize>` |
| tab_ids | `pub fn tab_ids(&self) -> OpenPageResult<Vec<String>>` |
| tab_infos | `pub fn tab_infos(&self) -> OpenPageResult<Vec<TabInfo>>` |
| latest_tab | `pub fn latest_tab(&self) -> OpenPageResult<Option<Page>>` |
| activate_tab | `pub fn activate_tab(&self, target_id: &str) -> OpenPageResult<()>` |
| close_tabs | `pub fn close_tabs(&self, target_ids: &[String], others: bool) -> OpenPageResult<usize>` |
| version | `pub fn version(&self) -> OpenPageResult<String>` |
| is_alive | `pub fn is_alive(&self) -> OpenPageResult<bool>` |
| is_headless | `pub fn is_headless(&self) -> bool` |
| is_existed | `pub fn is_existed(&self) -> OpenPageResult<bool>` |
| is_incognito | `pub fn is_incognito(&self) -> OpenPageResult<bool>` |
| browser_pid | `pub fn browser_pid(&self) -> Option<u32>` |
| wait_for_new_tab | `pub fn wait_for_new_tab(&self, current_tab_id: Option<&str>, timeout_ms: u64) -> OpenPageResult<Option<String>>` |
| download_path | `pub fn download_path(&self) -> OpenPageResult<Option<String>>` |
| set_download_path | `pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()>` |
| download_file_exists_mode | `pub fn download_file_exists_mode(&self) -> OpenPageResult<String>` |
| set_download_file_exists_mode | `pub fn set_download_file_exists_mode(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()>` |
| load_mode | `pub fn load_mode(&self) -> OpenPageResult<String>` |
| set_load_mode | `pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()>` |
| page_download_path | `pub fn page_download_path(&self, target_id: &str) -> OpenPageResult<Option<String>>` |
| set_page_download_path | `pub fn set_page_download_path(&self, target_id: &str, path: impl AsRef<Path>) -> OpenPageResult<()>` |
| page_download_file_exists_mode | `pub fn page_download_file_exists_mode(&self, target_id: &str) -> OpenPageResult<String>` |
| set_page_download_file_exists_mode | `pub fn set_page_download_file_exists_mode(&self, target_id: &str, mode: DownloadFileExistsMode) -> OpenPageResult<()>` |
| set_page_download_filename | `pub fn set_page_download_filename(&self, target_id: &str, rename: Option<&str>, suffix: Option<&str>, suffix_specified: bool) -> OpenPageResult<()>` |
| download_missions | `pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>>` |
| last_download | `pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>>` |
| wait_for_download | `pub fn wait_for_download(&self, filename: Option<&str>, timeout_ms: u64) -> OpenPageResult<String>` |
| wait_for_download_begin | `pub fn wait_for_download_begin(&self, timeout_ms: u64, cancel_it: bool) -> OpenPageResult<Option<DownloadMission>>` |
| wait_for_downloads_done | `pub fn wait_for_downloads_done(&self, timeout_ms: u64, cancel_if_timeout: bool) -> OpenPageResult<bool>` |
| cancel_download | `pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()>` |
| close | `pub fn close(&self) -> OpenPageResult<()>` |

---

## 3. 页面操作

### 文件：`page.rs`

#### 结构体

```rust
pub struct Page { ... }
pub struct Frame<'a> { ... }
pub struct FrameScroller<'a> { ... }
pub struct FrameSetter<'a> { ... }
pub struct FrameStates<'a> { ... }
pub struct FrameWait<'a> { ... }
pub struct FrameRect<'a> { ... }
```

#### Page 公开方法

**导航与基础信息**
- `goto(url: &str) -> OpenPageResult<()>`
- `url() -> OpenPageResult<String>`
- `title() -> OpenPageResult<String>`
- `target_id() -> String`
- `html() -> OpenPageResult<String>`
- `evaluate(expression: &str) -> OpenPageResult<Value>`
- `refresh(ignore_cache: bool) -> OpenPageResult<()>`
- `back(steps: usize) -> OpenPageResult<bool>`
- `forward(steps: usize) -> OpenPageResult<bool>`
- `stop_loading() -> OpenPageResult<()>`

**元素查找**
- `find(locator: &str) -> OpenPageResult<Element>`
- `find_all(locator: &str) -> OpenPageResult<Vec<Element>>`
- `wait_for(locator: &str, timeout_ms: u64) -> OpenPageResult<Element>`
- `wait_for_elements_loaded(locators: &[String], any_one: bool, timeout_ms: u64) -> OpenPageResult<bool>`
- `wait_for_ele_displayed(locator: &str, timeout_ms: u64) -> OpenPageResult<bool>`
- `wait_for_ele_hidden(locator: &str, timeout_ms: u64) -> OpenPageResult<bool>`
- `wait_for_ele_enabled(locator: &str, timeout_ms: u64) -> OpenPageResult<bool>`
- `wait_for_ele_deleted(locator: &str, timeout_ms: u64) -> OpenPageResult<bool>`
- `wait_for_ele_clickable(locator: &str, timeout_ms: u64) -> OpenPageResult<bool>`

**便捷操作**
- `click(locator: &str) -> OpenPageResult<()>`
- `fill(locator: &str, text: &str) -> OpenPageResult<()>`
- `text(locator: &str) -> OpenPageResult<Option<String>>`
- `attr(locator: &str, name: &str) -> OpenPageResult<Option<String>>`
- `active_element() -> OpenPageResult<Option<Element>>`
- `remove_element(locator: &str) -> OpenPageResult<bool>`
- `add_element_html(locator: &str, html: &str, position: &str) -> OpenPageResult<Element>`

**Frame / iframe**
- `get_frame(locator: &str) -> OpenPageResult<Element>`
- `get_frame_by_index(index: usize) -> OpenPageResult<Element>`
- `get_frames(locator: Option<&str>) -> OpenPageResult<Vec<Element>>`
- `get_frame_context(locator: &str) -> OpenPageResult<Frame>`
- `get_frame_context_by_index(index: usize) -> OpenPageResult<Frame>`
- `get_frame_contexts(locator: Option<&str>) -> OpenPageResult<Vec<Frame>>`

**截图与 PDF**
- `save_screenshot(path, full_page: bool) -> OpenPageResult<()>`
- `screenshot_bytes(format, quality, full_page) -> OpenPageResult<Vec<u8>>`
- `screenshot_base64(format, quality, full_page) -> OpenPageResult<String>`
- `get_screenshot(format, quality, full_page) -> OpenPageResult<(Vec<u8>, String)>`
- `save_pdf(path) -> OpenPageResult<()>`

**窗口控制**
- `activate() -> OpenPageResult<()>`
- `window_hide() -> OpenPageResult<()>`
- `window_show() -> OpenPageResult<()>`
- `window_state() -> OpenPageResult<String>`
- `window_size() -> OpenPageResult<(i64, i64)>`
- `window_location() -> OpenPageResult<(i64, i64)>`
- `window_max() / window_min() / window_full() / window_normal() -> OpenPageResult<()>`
- `window_size_set(width, height) -> OpenPageResult<()>`
- `window_location_set(left, top) -> OpenPageResult<()>`

**存储与缓存**
- `set_session_storage(item, value) -> OpenPageResult<()>`
- `session_storage(item) -> OpenPageResult<Value>`
- `set_local_storage(item, value) -> OpenPageResult<()>`
- `local_storage(item) -> OpenPageResult<Value>`
- `add_init_js(script) -> OpenPageResult<String>`
- `remove_init_js(script_id) -> OpenPageResult<()>`
- `clear_cache(clear_cache, clear_cookies, clear_storage) -> OpenPageResult<()>`

**网络与请求**
- `set_blocked_urls(patterns) -> OpenPageResult<()>`
- `set_upload_files(files) -> OpenPageResult<()>`
- `set_user_agent(user_agent, platform) -> OpenPageResult<()>`
- `set_headers(headers) -> OpenPageResult<()>`
- `cookie_header() -> OpenPageResult<Option<String>>`
- `cookies() -> OpenPageResult<Vec<CookieEntry>>`
- `set_cookie_header(url, cookie_header) -> OpenPageResult<()>`

**状态与等待**
- `ready_state() -> OpenPageResult<String>`
- `is_loading() -> OpenPageResult<bool>`
- `is_alive() -> OpenPageResult<bool>`
- `wait_for_url_change(text, exclude, timeout_ms) -> OpenPageResult<bool>`
- `wait_for_title_change(text, exclude, timeout_ms) -> OpenPageResult<bool>`
- `wait_for_load_start(timeout_ms) -> OpenPageResult<bool>`
- `wait_for_doc_loaded(timeout_ms) -> OpenPageResult<bool>`

**子对象获取**
- `listener() -> Listener`
- `interceptor() -> Interceptor`
- `console() -> Console`
- `screencast() -> Screencast`

**Alert 处理**
- `has_alert() -> OpenPageResult<bool>`
- `handle_alert(accept, prompt_text, timeout_ms) -> OpenPageResult<Option<String>>`
- `set_next_alert_action(accept, prompt_text) -> OpenPageResult<()>`
- `set_auto_alert_action(accept, prompt_text) -> OpenPageResult<()>`
- `wait_for_alert_closed(timeout_ms) -> OpenPageResult<bool>`

**Snapshot（静态解析回退）**
- `snapshot_root() -> OpenPageResult<SessionElement>`
- `snapshot_find(locator) -> OpenPageResult<SessionElement>`
- `snapshot_find_all(locator) -> OpenPageResult<Vec<SessionElement>>`
- `snapshot_query_xpath(expression) -> OpenPageResult<Vec<SessionXPathResult>>`
- `find_locators(locators, any_one, first_match_only) -> OpenPageResult<Vec<LocatorMatch<Element>>>`

**CDP 执行**
- `execute_cdp<T>(command: T) -> OpenPageResult<T::Response>`
- `execute_cdp_loaded<T>(command: T) -> OpenPageResult<T::Response>`

**关闭**
- `close(self) -> OpenPageResult<()>`

#### Frame 公开方法

Frame 是 iframe 的上下文包装器，提供与 Page/Element 类似的方法：
- `id() -> &str`
- `frame_element() -> &Element`
- `scroll() -> FrameScroller`
- `set() -> FrameSetter`
- `states() -> FrameStates`
- `wait() -> FrameWait`
- `rect() -> FrameRect`
- `link(), tag(), attrs(), attr(), property(), style(), css_path(), xpath(), child_count()`
- `sr() / shadow_root() -> Option<ShadowRoot>`
- `name(), url(), parent_id(), title(), download_path(), inner_html(), html()`
- `run_js(expression) -> Value`
- `refresh(), remove_attr(), set_attr(), set_property(), set_style()`
- `active_element() -> Option<Element>`
- `find(), find_all(), parent(), prev(), next(), before(), after(), prevs(), nexts(), befores(), afters()`
- `screenshot_bytes(), screenshot_base64(), get_screenshot()`
- `scroll_to_top(), scroll_to_bottom(), scroll_to_half(), scroll_to_rightmost(), scroll_to_leftmost(), scroll_to_location(), scroll_up(), scroll_down(), scroll_left(), scroll_right()`
- `scroll_position(), viewport_location(), screen_location(), viewport_size(), location(), size(), viewport_corners(), corners()`
- `ready_state(), is_loading(), is_alive(), is_displayed(), is_enabled(), has_rect(), is_in_viewport(), is_whole_in_viewport(), is_covered(), is_clickable()`
- `has_alert(), wait_for_doc_loaded(), wait_until_displayed(), wait_until_hidden(), wait_until_enabled(), wait_until_disabled(), wait_until_deleted(), wait_until_clickable(), wait_until_has_rect(), wait_until_covered(), wait_until_not_covered(), wait_until_disabled_or_deleted()`
- `snapshot_root(), snapshot_find(), snapshot_find_all(), snapshot_query_xpath(), find_locators()`
- `listener(), console()`
- FrameSetter: `attr(), property(), style()`
- FrameScroller: `to_top(), to_bottom(), to_half(), to_rightmost(), to_leftmost(), to_location(), up(), down(), left(), right(), to_see(), to_center()`
- FrameWait: `is_loading(), is_alive(), ready_state(), is_displayed(), is_enabled(), has_rect(), is_in_viewport(), is_whole_in_viewport(), is_covered(), is_clickable(), has_alert(), doc_loaded(), displayed(), hidden(), enabled(), disabled(), deleted(), clickable(), has_rect(), covered(), not_covered(), disabled_or_deleted(), stop_moving()`
- FrameStates: `location(), viewport_location(), screen_location(), size(), viewport_size(), corners(), viewport_corners(), scroll_position()`
- FrameRect: `location(), screen_location(), size(), corners()`

---

## 4. 元素定位与操作

### 文件：`locator.rs`

```rust
pub enum LocatorKind { Css, XPath }

pub struct Locator {
    kind: LocatorKind,
    query: String,
    raw: String,
}

pub struct LocatorMatch<T> {
    pub locator: String,
    pub elements: Vec<T>,
}

pub fn collect_locator_matches<T>(...) -> Vec<LocatorMatch<T>>
```

**Locator 方法**
- `parse(raw) -> OpenPageResult<Self>` 支持 `css:`, `xpath:`, `tag:`, `t:`, `@attr=value` 语法
- `kind() -> LocatorKind`
- `query() -> &str`
- `raw() -> &str`

### 文件：`element.rs`

```rust
pub enum ElementResource { Bytes(Vec<u8>), Text(String) }

pub struct Element {
    runtime: Arc<Runtime>,
    page: OxPage,
    inner: OxElement,
    marker: Option<String>,
}
```

#### Element 公开方法

**输入与交互**
- `click() -> OpenPageResult<()>`
- `input(text: &str) -> OpenPageResult<()>`
- `input_with_options(text, clear, by_js) -> OpenPageResult<()>`
- `input_keys_with_options(text, clear, by_js, delay_ms) -> OpenPageResult<()>`
- `clear() -> OpenPageResult<()>`
- `clear_with_mode(by_js) -> OpenPageResult<()>`
- `press_key(key: &str) -> OpenPageResult<()>`
- `set_file_input_files(files) -> OpenPageResult<()>`

**属性与内容**
- `text() -> OpenPageResult<Option<String>>`
- `tag() -> OpenPageResult<String>`
- `html() / inner_html() -> OpenPageResult<Option<String>>`
- `attrs() -> OpenPageResult<Vec<(String, String)>>`
- `attr(name) -> OpenPageResult<Option<String>>`
- `property(name) -> OpenPageResult<Option<Value>>`
- `raw_text() -> OpenPageResult<Option<String>>`
- `value() -> OpenPageResult<Option<String>>`
- `link() -> OpenPageResult<Option<String>>`
- `child_count() -> OpenPageResult<usize>`
- `css_path() / xpath() -> OpenPageResult<String>`
- `comments() -> OpenPageResult<Vec<String>>`
- `texts(text_node_only) -> OpenPageResult<Vec<String>>`
- `style(name, pseudo) -> OpenPageResult<String>`
- `pseudo_before() / pseudo_after() -> OpenPageResult<String>`

**滚动**
- `scroll_to_top() / scroll_to_bottom() / scroll_to_half() / scroll_to_rightmost() / scroll_to_leftmost()`
- `scroll_to_location(x, y)`
- `scroll_up(pixels) / scroll_down(pixels) / scroll_left(pixels) / scroll_right(pixels)`
- `scroll_to_see(center) -> OpenPageResult<()>`
- `scroll_to_center() -> OpenPageResult<()>`

**资源与 Shadow DOM**
- `src(timeout_ms, base64_to_bytes) -> OpenPageResult<Option<ElementResource>>`
- `save(path, name, timeout_ms, rename) -> OpenPageResult<PathBuf>`
- `sr() / shadow_root() -> OpenPageResult<Option<ShadowRoot>>`

**DOM 遍历**
- `parent() / parent_level(level) / parent_with(locator, index)`
- `child() / child_with(locator, index) / children() / children_with(locator)`
- `prev() / prev_with(locator, index) / prevs() / prevs_with(locator)`
- `next() / next_with(locator, index) / nexts() / nexts_with(locator)`
- `before() / before_with(locator, index) / befores() / befores_with(locator)`
- `after() / after_with(locator, index) / afters() / afters_with(locator)`
- `over() / over_with_timeout(timeout_ms) -> Option<Element>`
- `offset(x, y) -> OpenPageResult<Element>`
- `east() / south() / west() / north() -> OpenPageResult<Element>`

**属性修改**
- `remove_attr(name) -> OpenPageResult<()>`
- `set_attr(name, value) -> OpenPageResult<()>`
- `set_property(name, value) -> OpenPageResult<()>`
- `set_style(name, value) -> OpenPageResult<()>`

**JavaScript 执行**
- `run_js(script) -> OpenPageResult<Value>`
- `run_js_with_args(script, args) -> OpenPageResult<Value>`
- `run_js_with_options(script, args, by_value, await_promise, timeout_ms) -> OpenPageResult<Value>`
- `run_async_js(script) -> OpenPageResult<()>`
- `run_async_js_with_args(script, args) -> OpenPageResult<()>`
- `run_async_js_with_options(script, args, timeout_ms) -> OpenPageResult<()>`

**截图**
- `screenshot_bytes(format, quality) -> OpenPageResult<Vec<u8>>`
- `screenshot_base64(format, quality) -> OpenPageResult<String>`
- `get_screenshot(format, quality) -> OpenPageResult<(Vec<u8>, String)>`
- `save_screenshot(path) -> OpenPageResult<()>`

**鼠标与拖拽**
- `focus() -> OpenPageResult<()>`
- `hover() -> OpenPageResult<()>`
- `hover_with_offset(x, y) -> OpenPageResult<()>`
- `drag(offset_x, offset_y, duration_secs) -> OpenPageResult<()>`
- `drag_to(target, duration_secs) -> OpenPageResult<()>`
- `drag_to_point(x, y, duration_secs) -> OpenPageResult<()>`

**表单与选择**
- `set_checked(checked) -> OpenPageResult<()>`
- `check(uncheck, by_js) -> OpenPageResult<()>`
- `uncheck(by_js) -> OpenPageResult<()>`
- `is_multi_select() -> OpenPageResult<bool>`
- `option_texts() -> OpenPageResult<Vec<String>>`
- `selected_option() -> OpenPageResult<Option<String>>`
- `selected_options() -> OpenPageResult<Vec<String>>`
- `select_by_text(text) -> OpenPageResult<bool>`
- `select_by_value(value) -> OpenPageResult<bool>`
- `select_by_index(index) -> OpenPageResult<bool>`
- `clear_selected() -> OpenPageResult<()>`
- `is_selected() -> OpenPageResult<bool>`
- `is_checked() -> OpenPageResult<bool>`

**状态检测**
- `is_displayed() -> OpenPageResult<bool>`
- `is_enabled() -> OpenPageResult<bool>`
- `is_alive() -> OpenPageResult<bool>`
- `has_rect() -> OpenPageResult<bool>`
- `is_in_viewport() -> OpenPageResult<bool>`
- `is_whole_in_viewport() -> OpenPageResult<bool>`
- `is_covered() -> OpenPageResult<bool>`
- `is_clickable() -> OpenPageResult<bool>`

**几何信息**
- `rect_corners() -> OpenPageResult<Option<Vec<(f64, f64)>>>`
- `rect_location() -> OpenPageResult<Option<(f64, f64)>>`
- `rect_screen_location() -> OpenPageResult<Option<(f64, f64)>>`
- `rect_midpoint() -> OpenPageResult<Option<(f64, f64)>>`
- `rect_size() -> OpenPageResult<Option<(f64, f64)>>`

**等待**
- `wait_until_displayed(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_hidden(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_enabled(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_disabled(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_deleted(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_clickable(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_has_rect(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_covered(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_not_covered(timeout_ms) -> OpenPageResult<bool>`
- `wait_until_disabled_or_deleted(timeout_ms) -> OpenPageResult<bool>`

**元素内查找**
- `find(locator) -> OpenPageResult<Element>`
- `find_all(locator) -> OpenPageResult<Vec<Element>>`
- `snapshot_root() / snapshot_find() / snapshot_find_all() / snapshot_query_xpath() / find_locators()`

---

## 5. Session 模式

### 文件：`session.rs`

Session 模式是基于 HTTP 的纯静态 HTML 解析模式，不依赖浏览器 CDP，使用 `reqwest` + `scraper` + `skyscraper` 进行解析和 XPath 查询。

```rust
pub struct SessionOptions {
    pub timeout_secs: u64,
    pub user_agent: Option<String>,
}

pub struct SessionPage { ... }
pub struct CookieEntry { name, value, domain }
pub struct SessionElement { ... }

pub enum SessionXPathResult {
    Document, Element(SessionElement), Text(String), Comment(String),
    Attribute { name, value }, ProcessingInstruction { target, data },
    Doctype { name, public_id, system_id },
    Boolean(bool), Integer(i64), Number(f64), String(String),
    QName { namespace_uri, local_name, prefix },
    Function(String),
}
```

#### SessionPage 方法

- `new(options: SessionOptions) -> OpenPageResult<Self>`
- `get(url) -> OpenPageResult<bool>`
- `post_json(url, payload) -> OpenPageResult<bool>`
- `url() / status_code() / html() / raw_data() / encoding() / json() / title() / user_agent()`
- `is_alive() / is_loading() / ready_state() / is_headless()`
- `cookies() -> Vec<CookieEntry>`
- `root() -> SessionElement`
- `set_user_agent(user_agent) / set_headers(headers)`
- `cookie_header(url) / set_cookie_header(url, cookie_header)`
- `find(locator) / find_all(locator) / query_xpath(expression) / find_locators(...)`

#### SessionElement 方法

- `find() / find_all() / query_xpath() / find_locators()`
- `tag() / text() / html() / inner_html() / raw_text()`
- `attrs() / attr(name) / link() / child_count() / css_path() / xpath() / comments() / texts(text_node_only)`
- `parent() / parent_level(level) / parent_with(locator, index)`
- `child() / child_node() / child_with(locator, index) / child_node_with(...)`
- `children() / children_nodes() / children_with(locator) / children_nodes_with(...)`
- `prev() / prev_node() / prev_with(locator, index) / prev_node_with(...)`
- `prevs() / prev_nodes() / prevs_with(locator) / prev_nodes_with(...)`
- `next() / next_node() / next_with(locator, index) / next_node_with(...)`
- `nexts() / next_nodes() / nexts_with(locator) / next_nodes_with(...)`
- `before() / before_node() / before_with(locator, index) / before_node_with(...)`
- `befores() / before_nodes() / befores_with(locator) / before_nodes_with(...)`
- `after() / after_node() / after_with(locator, index) / after_node_with(...)`
- `afters() / after_nodes() / afters_with(locator) / after_nodes_with(...)`

#### 独立辅助函数

- `cookies_from_header(url, cookie_header) -> Vec<CookieEntry>`
- `snapshot_root(html) / snapshot_find(html, locator) / snapshot_find_all(...) / snapshot_query_xpath(...)`
- `snapshot_fragment_root(html) / snapshot_fragment_find(...) / snapshot_fragment_find_all(...) / snapshot_fragment_query_xpath(...)`
- 带 `base_url` 变体的上述函数

---

## 6. WebPage 统一封装

### 文件：`webpage.rs`

WebPage 提供统一的 API，内部根据模式（Driver / Session）自动分发到 Page 或 SessionPage。

```rust
pub enum WebMode { Driver, Session }

pub enum WebElement {
    Browser(Element),
    Session(SessionElement),
}

pub enum WebFrame {
    Browser(Frame),
}

pub struct DisconnectedWebPage {
    target_id: String,
    ...
}

pub struct WebPage { ... }
```

#### WebPage 核心方法

- `new(mode, launch_options, session_options) -> OpenPageResult<Self>`
- `mode() -> OpenPageResult<WebMode>`
- `change_mode(mode, keep_cookies) -> OpenPageResult<()>`
- `disconnect(self) -> OpenPageResult<DisconnectedWebPage>`
- `reconnect(wait_ms) -> OpenPageResult<Self>` (DisconnectedWebPage)
- `quit() -> OpenPageResult<()>`

**浏览器级方法**（委托给 Browser）
- `tabs_count(), tab_ids(), target_id(), tab_infos(), latest_tab(), new_tab(), activate_tab(), close_tabs()`
- `download_path(), set_download_path(), current_tab_download_path(), set_current_tab_download_path()`
- `download_file_exists_mode(), set_download_file_exists_mode(), current_tab_download_file_exists_mode(), set_current_tab_download_file_exists_mode()`
- `set_current_tab_download_filename(rename, suffix, suffix_specified)`
- `wait_for_download(), download_missions(), last_download(), wait_for_download_begin(), wait_for_downloads_done()`
- `wait_for_new_tab()`
- `is_existed(), is_incognito(), browser_pid()`

**页面级方法**（统一封装）
- `get(url) / post_json(url, payload) -> OpenPageResult<bool>`
- `url() / title() / html() / raw_data() / encoding() / status_code() / json() / user_agent()`
- `cookies() / set_cookie_header() / cookies_to_session() / cookies_to_browser()`
- `find(locator) -> WebElement / find_all(locator) -> Vec<WebElement>`
- `active_element() -> Option<WebElement>`
- `remove_element(locator) / add_element_html(locator, html, position)`
- `click(locator) / fill(locator, text) / text(locator) / attr(locator, name)`
- `wait_for(locator, timeout_ms) -> WebElement`
- `wait_for_elements_loaded() / wait_for_ele_displayed() / wait_for_ele_hidden() / wait_for_ele_enabled() / wait_for_ele_deleted() / wait_for_ele_clickable()`
- `goto(url) / refresh(ignore_cache) / back(steps) / forward(steps) / stop_loading()`
- `save_screenshot(path, full_page) / screenshot_bytes() / screenshot_base64() / get_screenshot()`
- `save_pdf(path)`
- `run_js(expression) / execute_cdp() / execute_cdp_loaded()`
- `set_user_agent() / set_headers() / set_blocked_urls()`
- `set_session_storage() / session_storage() / set_local_storage() / local_storage()`
- `add_init_js() / remove_init_js() / clear_cache()`
- `set_upload_files()`
- `window_state() / window_size() / window_location() / window_max() / window_min() / window_full() / window_normal() / window_hide() / window_show() / window_size_set() / window_location_set()`
- `load_mode() / set_load_mode()`
- `activate()`
- `wait_for_url_change() / wait_for_title_change() / wait_for_load_start() / wait_for_doc_loaded()`
- `is_alive() / is_loading() / ready_state() / is_headless() / has_alert()`
- `listener() / interceptor() / console() / screencast()`
- `handle_alert() / set_next_alert_action() / set_auto_alert_action() / wait_for_alert_closed()`
- `snapshot_find() / snapshot_find_all() / snapshot_query_xpath() / snapshot_root() / find_locators()`

**Frame 支持**
- `get_frame() / get_frame_by_index() / get_frames() -> WebElement`
- `get_frame_context() / get_frame_context_by_index() / get_frame_contexts() -> WebFrame`

#### WebElement 方法（统一封装）

WebElement 对 Browser(Element) 和 Session(SessionElement) 提供统一接口：
- `tag() / text() / html() / inner_html() / raw_text() / attrs() / attr() / property() / link() / child_count() / css_path() / xpath() / comments() / texts()`
- `style() / pseudo_before() / pseudo_after()`
- `scroll_to_top() / scroll_to_bottom() / scroll_to_half() / scroll_to_rightmost() / scroll_to_leftmost() / scroll_to_location() / scroll_up() / scroll_down() / scroll_left() / scroll_right() / scroll_to_see() / scroll_to_center()`
- `src() / save() / shadow_root()`
- `parent() / parent_level() / parent_with() / child() / child_with() / children() / children_with()`
- `prev() / prev_with() / prevs() / prevs_with() / next() / next_with() / nexts() / nexts_with()`
- `before() / before_with() / befores() / befores_with() / after() / after_with() / afters() / afters_with()`
- `over() / over_with_timeout() / offset() / east() / south() / west() / north()`
- `click() / input() / input_with_options() / input_keys_with_options() / clear() / clear_with_mode() / set_file_input_files() / press_key()`
- `run_js() / run_js_with_args() / run_js_with_options() / run_async_js() / run_async_js_with_args() / run_async_js_with_options()`
- `save_screenshot() / screenshot_bytes() / screenshot_base64() / get_screenshot()`
- `focus() / hover() / hover_with_offset() / drag() / drag_to_element() / drag_to_point()`
- `remove_attr() / set_attr() / set_property() / set_style()`
- `set_checked() / check() / uncheck() / is_multi_select() / option_texts() / selected_option() / selected_options() / select_by_text() / select_by_value() / select_by_index() / clear_selected()`
- `rect_location() / rect_screen_location() / rect_midpoint() / rect_size()`

---

## 7. 下载管理

### 文件：`download.rs`

```rust
pub enum DownloadState { Running, Completed, Canceled, Skipped }

pub struct DownloadInfo { ... }

pub struct DownloadMission { ... }
```

#### DownloadMission 方法

- `guid() -> String`
- `url() -> OpenPageResult<String>`
- `suggested_filename() -> OpenPageResult<String>`
- `state() -> OpenPageResult<String>`
- `received_bytes() -> OpenPageResult<u64>`
- `total_bytes() -> OpenPageResult<Option<u64>>`
- `final_path() -> OpenPageResult<Option<String>>`
- `is_done() -> OpenPageResult<bool>`
- `wait(timeout_ms) -> OpenPageResult<String>`
- `cancel() -> OpenPageResult<()>`

---

## 8. 网络监听

### 文件：`listener.rs`

```rust
pub struct ListenerRequest { ... }
pub struct ListenerRequestExtraInfo { ... }
pub struct ListenerAssociatedCookie { ... }
pub struct ListenerResponse { ... }
pub struct ListenerResponseExtraInfo { ... }
pub struct ListenerBlockedSetCookie { ... }
pub struct ListenerExemptedSetCookie { ... }
pub struct ListenerFailInfo { ... }
pub struct ListenerPacket { ... }

pub struct Listener { ... }
pub struct ListenerSteps { ... }
```

#### Listener 方法

- `new(runtime, page) -> Self`
- `new_for_frame(runtime, page, frame_id) -> Self`
- `start(targets, is_regex, methods, resource_types) -> OpenPageResult<()>`
- `set_targets(targets, is_regex, methods, resource_types) -> OpenPageResult<()>`
- `wait(count, timeout_ms, fit_count) -> OpenPageResult<Vec<ListenerPacket>>`
- `wait_one(timeout_ms) -> OpenPageResult<ListenerPacket>`
- `steps(timeout_ms) -> ListenerSteps`
- `clear() -> OpenPageResult<()>`
- `pause(clear) -> OpenPageResult<()>`
- `resume() -> OpenPageResult<()>`
- `wait_until_idle(timeout_ms, targets_only) -> OpenPageResult<bool>`
- `wait_silent(timeout_ms, idle_ms, packet_threshold) -> OpenPageResult<bool>`
- `stop() -> OpenPageResult<()>`
- `is_listening() -> OpenPageResult<bool>`
- `wait_extra_info(timeout_ms) -> OpenPageResult<bool>`

#### 数据访问方法

- `ListenerRequest::params() -> HashMap<String, String>`
- `ListenerRequest::post_data_json() -> Option<Value>`
- `ListenerRequest::cookies() -> Vec<Value>`
- `ListenerRequestExtraInfo::cookies() -> Vec<Value>`
- `ListenerResponse::raw_body() -> Option<&str>`
- `ListenerResponse::body_bytes() -> OpenPageResult<Option<Vec<u8>>>`
- `ListenerResponse::body_text() -> OpenPageResult<Option<String>>`
- `ListenerResponse::body_json() -> OpenPageResult<Option<Value>>`

---

## 9. 请求拦截

### 文件：`intercept.rs`

```rust
pub struct InterceptedRequestInfo { ... }
pub struct Interceptor { ... }
pub struct InterceptedRequest { ... }
```

#### Interceptor 方法

- `new(runtime, page) -> Self`
- `start(targets, is_regex, methods, resource_types) -> OpenPageResult<()>`
- `wait(timeout_ms) -> OpenPageResult<Option<InterceptedRequest>>`
- `stop() -> OpenPageResult<()>`
- `is_listening() -> OpenPageResult<bool>`

#### InterceptedRequest 方法

- `request_id() -> String`
- `frame_id() -> String`
- `url() -> String`
- `method() -> String`
- `headers() -> HashMap<String, String>`
- `resource_type() -> String`
- `has_post_data() -> bool`
- `post_data_entries() -> usize`
- `continue_request(url, method, headers, post_data) -> OpenPageResult<()>`
- `fail(reason) -> OpenPageResult<()>`
- `fulfill(response_code, body, headers, response_phrase) -> OpenPageResult<()>`

---

## 10. 弹窗与控制台

### 文件：`alert.rs`

```rust
pub struct AlertTracker { ... }
```

#### AlertTracker 方法

- `new(runtime, page) -> Self`
- `has_alert() -> OpenPageResult<bool>`
- `handle_alert(accept, prompt_text, timeout_ms) -> OpenPageResult<Option<String>>`
- `set_next_alert_action(accept, prompt_text) -> OpenPageResult<()>`
- `set_auto_alert_action(accept, prompt_text) -> OpenPageResult<()>`
- `wait_for_alert_closed(timeout_ms) -> OpenPageResult<bool>`

### 文件：`console.rs`

```rust
pub struct ConsoleMessage {
    pub all_info: Value,
    pub source: String,
    pub level: String,
    pub text: String,
    pub url: Option<String>,
    pub line: Option<i64>,
    pub column: Option<i64>,
    pub args: Vec<Value>,
}

pub struct Console { ... }
pub struct ConsoleSteps { ... }
```

#### Console 方法

- `new(runtime, page) -> Self`
- `start() -> OpenPageResult<()>`
- `stop() -> OpenPageResult<()>`
- `clear() -> OpenPageResult<()>`
- `wait(timeout_ms) -> OpenPageResult<Option<ConsoleMessage>>`
- `messages() -> OpenPageResult<Vec<ConsoleMessage>>`
- `steps(timeout_ms) -> ConsoleSteps`
- `is_listening() -> OpenPageResult<bool>`

#### ConsoleMessage 方法

- `body() -> Value`

---

## 11. 录屏

### 文件：`screencast.rs`

```rust
pub enum ScreencastMode { Imgs, FrugalImgs }

pub struct Screencast { ... }
```

#### Screencast 方法

- `new(runtime, page) -> Self`
- `set_mode(mode) -> OpenPageResult<()>`
- `mode() -> OpenPageResult<ScreencastMode>`
- `set_save_path(path) -> OpenPageResult<PathBuf>`
- `start(save_path) -> OpenPageResult<PathBuf>`
- `stop() -> OpenPageResult<PathBuf>`
- `is_running() -> OpenPageResult<bool>`

**模式说明**
- `Imgs`: 使用 `Page.screenshot()` 轮询截图（每 40ms），停止时等待任务结束
- `FrugalImgs`: 使用 CDP `StartScreencast` 事件流，按帧接收 base64 JPEG 数据，停止时发送 `StopScreencast` 并中止任务

---

## 12. Shadow DOM

### 文件：`shadow_root.rs`

```rust
pub struct ShadowRoot { ... }
```

#### ShadowRoot 方法

- `tag() -> String`
- `html() / inner_html() -> OpenPageResult<String>`
- `host() -> OpenPageResult<Element>`
- `backend_node_id() -> BackendNodeId`
- `run_js(script) / run_js_with_args() / run_js_with_options() -> OpenPageResult<Value>`
- `run_async_js(script) / run_async_js_with_args() / run_async_js_with_options() -> OpenPageResult<()>`
- `snapshot_root() / snapshot_find() / snapshot_find_all() -> OpenPageResult<SessionElement>`
- `is_enabled() / is_alive() -> OpenPageResult<bool>`
- `find(locator) / find_all(locator) -> OpenPageResult<Element>`
- `child() / child_with() / children() / children_with()`
- `parent() / parent_level() / parent_with()`
- `next() / next_with() / nexts() / nexts_with()`
- `before() / before_with() / befores() / befores_with()`
- `after() / after_with() / afters() / afters_with()`

---

## 13. 文件上传

### 文件：`upload.rs`

```rust
pub struct UploadTracker { ... }
```

#### UploadTracker 方法

- `new(runtime, page) -> Self`
- `set_files(files) -> OpenPageResult<()>`

功能：通过监听 CDP `EventFileChooserOpened` 事件，在文件选择对话框打开时自动填充指定文件路径。

---

## 14. 窗口控制

### 文件：`window.rs`

```rust
pub fn set_app_visibility(pid: u32, visible: bool) -> OpenPageResult<()>
pub fn activate_app(pid: u32) -> OpenPageResult<()>
```

功能：macOS 专用，通过 `osascript` 控制浏览器应用的显示/隐藏和前台激活。

---

## 15. Python 绑定

### 文件：`python.rs`

通过 PyO3 将所有 Rust 公开 API 暴露给 Python。使用 `py.detach()` 模式将阻塞操作放到独立线程执行，避免 GIL 阻塞。

#### Python 类列表

| PyO3 类 | 对应 Rust 类型 | 说明 |
|---------|---------------|------|
| `PyBrowser` | `Browser` | 浏览器管理 |
| `PyPage` | `Page` | CDP 页面 |
| `PyElement` | `Element` | CDP 元素 |
| `PySessionPage` | `SessionPage` | HTTP 会话页面 |
| `PySessionElement` | `SessionElement` | HTTP 会话元素 |
| `PyWebPage` | `WebPage` | 统一页面封装 |
| `PyListener` | `Listener` | 网络监听 |
| `PyInterceptor` | `Interceptor` | 请求拦截 |
| `PyInterceptedRequest` | `InterceptedRequest` | 被拦截请求 |
| `PyListenerPacket` | `ListenerPacket` | 监听数据包 |
| `PyListenerRequest` | `ListenerRequest` | 请求信息 |
| `PyListenerRequestExtraInfo` | `ListenerRequestExtraInfo` | 请求额外信息 |
| `PyListenerResponse` | `ListenerResponse` | 响应信息 |
| `PyListenerResponseExtraInfo` | `ListenerResponseExtraInfo` | 响应额外信息 |
| `PyListenerFailInfo` | `ListenerFailInfo` | 失败信息 |
| `PyDownloadMission` | `DownloadMission` | 下载任务 |

#### 注册入口

```rust
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()>
```

在 `lib.rs` 中通过 `#[pymodule] fn openpage_rs(...)` 注册为 `openpage_rs` 模块。

---

## 16. CLI

### 文件：`cli/mod.rs`

```rust
pub fn run() -> OpenPageResult<i32>
pub fn run_from_args<I, T>(args: I) -> OpenPageResult<i32>
```

### 文件：`cli/args.rs`

```rust
pub struct Cli { ... }
pub enum Command { Browser(BrowserCommand), Page(PageCommand), Element(ElementCommand), Serve(ServeArgs) }
pub struct ServeArgs { ... }
pub enum BrowserCommand { Start(BrowserStartArgs), Stop, Status }
pub struct BrowserStartArgs { ... }
pub struct SessionArgs { ... }
pub enum PageCommand { New(PageNewArgs), Get(PageGetArgs), Screenshot(PageScreenshotArgs), ... }
pub struct PageNewArgs { ... }
pub struct PageGetArgs { ... }
pub struct PageScreenshotArgs { ... }
pub enum ElementCommand { Text(ElementSelectorArgs), Html(ElementSelectorArgs), Click(ElementSelectorArgs), Input(ElementInputArgs), Attr(ElementAttrArgs) }
pub struct ElementSelectorArgs { ... }
pub struct ElementInputArgs { ... }
pub struct ElementAttrArgs { ... }
pub struct JsArgs { ... }
```

### 文件：`cli/oneshot.rs`

```rust
pub fn run(command: Command) -> OpenPageResult<()>
```

处理一次性 CLI 命令：
- `browser start/stop/status`
- `page new/get/url/title/html/screenshot`
- `element text/html/click/input/attr`
- `js execute`

### 文件：`cli/protocol.rs`

```rust
pub struct Request { id, method, params }
pub struct Response { id, result }
pub struct ResponseError { id, error }

pub fn simple_ok(result: Value) -> Value
pub fn simple_error(kind, message) -> Value
```

### 文件：`cli/serve.rs`

```rust
pub fn run(args: ServeArgs) -> OpenPageResult<()>
```

Stdio serve 模式：读取 stdin 的 NDJSON 请求，通过 `WebPage` 统一 API 分发处理，输出 NDJSON 响应到 stdout。

支持的操作包括：
- `webpage.create`, `webpage.get`, `webpage.change_mode`
- 元素查找、点击、输入、属性获取
- 等待操作（wait_for, wait_until 系列）
- 下载监听与等待
- Alert 处理
- 截图
- JS 执行
- 窗口控制

### 文件：`bin/openpage.rs`

```rust
fn main() { cli::run() }
```

简单的二进制入口。

---

## 文件清单

| 文件 | 行数 | 说明 |
|------|------|------|
| `src/lib.rs` | 51 | 模块声明与公开重导出 |
| `src/error.rs` | 36 | 错误类型 |
| `src/browser.rs` | ~1153 | 浏览器管理 |
| `src/page.rs` | ~2680 | CDP 页面操作 |
| `src/element.rs` | ~2984 | CDP 元素操作 |
| `src/webpage.rs` | ~2848 | 统一页面封装 |
| `src/session.rs` | ~2646 | HTTP 静态解析模式 |
| `src/locator.rs` | 133 | 定位器解析 |
| `src/download.rs` | 521 | 下载管理 |
| `src/listener.rs` | ~2214 | 网络监听 |
| `src/intercept.rs` | ~628 | 请求拦截 |
| `src/alert.rs` | 276 | 弹窗追踪 |
| `src/console.rs` | 474 | 控制台消息 |
| `src/screencast.rs` | 325 | 录屏 |
| `src/shadow_root.rs` | ~747 | Shadow DOM |
| `src/upload.rs` | 178 | 文件上传 |
| `src/window.rs` | 64 | 窗口控制（macOS） |
| `src/python.rs` | ~2962 | Python 绑定 |
| `src/cli/mod.rs` | 58 | CLI 入口 |
| `src/cli/args.rs` | 148 | CLI 参数定义 |
| `src/cli/oneshot.rs` | ~498 | 一次性命令 |
| `src/cli/protocol.rs` | 97 | NDJSON 协议 |
| `src/cli/serve.rs` | ~531 | Serve 模式 |
| `src/bin/openpage.rs` | 11 | 二进制入口 |
