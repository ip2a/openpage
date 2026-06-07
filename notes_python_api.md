# OpenPage Python API 完整统计 (修正版)

## 文件结构

- python/openpage/__init__.py - 公开 API 导出
- python/openpage/_compat.py - 兼容层，所有 Python 包装类
- python/openpage/py.typed - PEP 561 类型标记
- python/examples/ - 示例代码
- python/tests/test_openpage.py - 测试代码

## __init__.py 导出的公开 API (共 17 个)

- **Browser** (class, 17 个公开成员)
- **ChromiumOptions** (class, 17 个公开成员)
- **ChromiumPage** (class, 38 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 13 个公开成员)
- **Listener** (class, 10 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 29 个公开成员)
- **SessionElement** (class, 22 个公开成员)
- **SessionOptions** (class, 4 个公开成员)
- **SessionPage** (class, 16 个公开成员)
- **WebPage** (class, 36 个公开成员)

## _compat.py 中的内部辅助类 (未在 __all__ 中导出，共 16 个)

- **BrowserSetter** (class, 1 个公开成员)
- **BrowserStates** (class, 4 个公开成员)
- **BrowserWait** (class, 3 个公开成员)
- **ChromiumPageSetter** (class, 13 个公开成员)
- **ElementStates** (class, 10 个公开成员)
- **ElementWait** (class, 10 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **LoadModeSetter** (class, 3 个公开成员)
- **PageSetter** (class, 13 个公开成员)
- **PageStates** (class, 7 个公开成员)
- **PageWait** (class, 14 个公开成员)
- **WebPageSetter** (class, 13 个公开成员)
- **WebPageStates** (class, 7 个公开成员)
- **WebPageWait** (class, 14 个公开成员)
- **WindowSetter** (class, 8 个公开成员)

## 底层 Rust 模块 openpage_rs 中的类 (共 16 个)

- **Browser** (class, 28 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 31 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **Listener** (class, 9 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 64 个公开成员)
- **SessionElement** (class, 20 个公开成员)
- **SessionPage** (class, 23 个公开成员)
- **WebPage** (class, 83 个公开成员)
- **openpage_rs** (module)

=== Browser ===
  def close(self) -> 'None'
  @property download_file_exists_mode
  def download_missions(self) -> "list['DownloadMission']"
  @property download_path
  def get_page(self, target_id: 'str') -> "'Page'"
  def last_download(self) -> "'DownloadMission | None'"
  def launch(options: 'ChromiumOptions | None' = None) -> "'Browser'"
  def new_page(self, url: 'str | None' = None) -> "'Page'"
  @property set
  def set_download_file_exists_mode(self, mode: 'str') -> 'None'
  def set_download_path(self, path: 'str') -> 'None'
  @property states
  @property tab_ids
  @property tabs_count
  @property version
  @property wait
  def wait_for_download(self, filename: 'str | None' = None, timeout: 'float' = 10.0) -> 'str'

=== ChromiumOptions ===
  def headless(self, on_off: 'bool' = True) -> "'ChromiumOptions'"
  def no_sandbox(self, on_off: 'bool' = True) -> "'ChromiumOptions'"
  def set_browser_path(self, path: 'str') -> "'ChromiumOptions'"
  def set_download_path(self, path: 'str') -> "'ChromiumOptions'"
  def set_file_exists(self, mode: 'str') -> "'ChromiumOptions'"
  def set_load_mode(self, value: 'str') -> "'ChromiumOptions'"
  def set_user_data_path(self, path: 'str') -> "'ChromiumOptions'"
  def set_window_size(self, width: 'int', height: 'int') -> "'ChromiumOptions'"

=== ChromiumPage ===
  def attr(self, locator: 'str', name: 'str') -> 'str | None'
  def click(self, locator: 'str') -> 'None'
  def close(self) -> 'None'
  def cookies(self) -> 'list[dict[str, str | None]]'
  def download_missions(self) -> "list['DownloadMission']"
  @property download_path
  def ele(self, locator: 'str', timeout: 'float' = 10.0) -> "'Element'"
  def eles(self, locator: 'str') -> "list['Element']"
  def evaluate(self, expression: 'str') -> 'Any'
  def get(self, url: 'str') -> 'bool'
  def get_tab(self, target_id: 'str') -> "'Page'"
  def goto(self, url: 'str') -> 'None'
  def handle_alert(self, accept: 'bool' = True, send: 'str | None' = None, timeout: 'float' = 10.0, next_one: 'bool' = False) -> 'str | bool | None'
  @property html
  def input(self, locator: 'str', text: 'str') -> 'None'
  @property intercept
  def last_download(self) -> "'DownloadMission | None'"
  @property listen
  def new_tab(self, url: 'str | None' = None) -> "'Page'"
  def quit(self) -> 'None'
  def run_js(self, expression: 'str') -> 'Any'
  def s_ele(self, locator: 'str | None' = None) -> "'SessionElement'"
  def s_eles(self, locator: 'str') -> "list['SessionElement']"
  def save_pdf(self, path: 'str') -> 'None'
  def save_screenshot(self, path: 'str', full_page: 'bool' = True) -> 'None'
  @property set
  def set_download_path(self, path: 'str') -> 'None'
  @property states
  @property tab_id
  @property tab_ids
  @property tabs_count
  def text(self, locator: 'str') -> 'str | None'
  @property title
  @property url
  @property user_agent
  @property wait
  def wait_for(self, locator: 'str', timeout: 'float' = 10.0) -> "'Element'"
  def wait_for_download(self, filename: 'str | None' = None, timeout: 'float' = 10.0) -> 'str'

=== DownloadMission ===
  def cancel(self) -> 'None'
  @property final_path
  @property guid
  @property is_done
  @property received_bytes
  @property state
  @property suggested_filename
  @property total_bytes
  @property url
  def wait(self, timeout: 'float' = 10.0) -> 'str'

=== Element ===
  def attr(self, name: 'str') -> 'str | None'
  def clear(self) -> 'None'
  def click(self) -> 'None'
  def ele(self, locator: 'str') -> "'Element'"
  def eles(self, locator: 'str') -> "list['Element']"
  @property html
  def input(self, text: 'str') -> 'None'
  def press(self, key: 'str') -> 'None'
  def run_js(self, script: 'str') -> 'Any'
  def save_screenshot(self, path: 'str') -> 'None'
  @property states
  @property text
  @property wait

=== Listener ===
  def clear(self) -> 'None'
  @property listening
  def pause(self, clear: 'bool' = True) -> 'None'
  def resume(self) -> 'None'
  def set_targets(self, targets: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = True, is_regex: 'bool' = False, method: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = True, res_type: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = True) -> 'None'
  def start(self, targets: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None, is_regex: 'bool' = False, method: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None, res_type: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None) -> 'None'
  def steps(self, count: 'int | None' = None, timeout: 'float | None' = None, gap: 'int' = 1)
  def stop(self) -> 'None'
  def wait(self, count: 'int' = 1, timeout: 'float | None' = None, fit_count: 'bool' = True) -> "'ListenerPacket | list[ListenerPacket]'"
  def wait_silent(self, timeout: 'float | None' = None, targets_only: 'bool' = False) -> 'bool'

=== ListenerFailInfo ===
  @property blocked_reason
  @property canceled
  @property error_text

=== ListenerPacket ===
  @property fail_info
  @property is_failed
  @property method
  @property request
  @property resource_type
  @property response
  @property target
  @property url

=== ListenerRequest ===
  @property extra_info
  @property headers
  @property method
  @property post_data
  @property url

=== ListenerRequestExtraInfo ===
  @property headers

=== ListenerResponse ===
  @property body
  @property body_base64
  @property extra_info
  @property headers
  @property mime_type
  @property status
  @property status_text
  @property url

=== ListenerResponseExtraInfo ===
  @property headers
  @property headers_text
  @property status_code

=== Page ===
  def attr(self, locator: 'str', name: 'str') -> 'str | None'
  def click(self, locator: 'str') -> 'None'
  def close(self) -> 'None'
  def cookies(self) -> 'list[dict[str, str | None]]'
  def ele(self, locator: 'str', timeout: 'float' = 10.0) -> "'Element'"
  def eles(self, locator: 'str') -> "list['Element']"
  def evaluate(self, expression: 'str') -> 'Any'
  def get(self, url: 'str') -> 'bool'
  def goto(self, url: 'str') -> 'None'
  def handle_alert(self, accept: 'bool' = True, send: 'str | None' = None, timeout: 'float' = 10.0, next_one: 'bool' = False) -> 'str | bool | None'
  @property html
  def input(self, locator: 'str', text: 'str') -> 'None'
  @property intercept
  @property listen
  def new_tab(self, url: 'str | None' = None) -> "'Page'"
  def run_js(self, expression: 'str') -> 'Any'
  def s_ele(self, locator: 'str | None' = None) -> "'SessionElement'"
  def s_eles(self, locator: 'str') -> "list['SessionElement']"
  def save_pdf(self, path: 'str') -> 'None'
  def save_screenshot(self, path: 'str', full_page: 'bool' = True) -> 'None'
  @property set
  @property states
  @property tab_id
  def text(self, locator: 'str') -> 'str | None'
  @property title
  @property url
  @property user_agent
  @property wait
  def wait_for(self, locator: 'str', timeout: 'float' = 10.0) -> "'Element'"

=== SessionElement ===
  def after(self, locator: 'str | None' = None, index: 'int' = 1) -> "'SessionElement'"
  def afters(self, locator: 'str | None' = None) -> "list['SessionElement']"
  def attr(self, name: 'str') -> 'str | None'
  @property attrs
  def before(self, locator: 'str | None' = None, index: 'int' = 1) -> "'SessionElement'"
  def befores(self, locator: 'str | None' = None) -> "list['SessionElement']"
  def child(self, locator: 'str | None' = None, index: 'int' = 1) -> "'SessionElement'"
  def children(self, locator: 'str | None' = None) -> "list['SessionElement']"
  def ele(self, locator: 'str') -> "'SessionElement'"
  def eles(self, locator: 'str') -> "list['SessionElement']"
  @property html
  @property inner_html
  def next(self, locator: 'str | None' = None, index: 'int' = 1) -> "'SessionElement'"
  def nexts(self, locator: 'str | None' = None) -> "list['SessionElement']"
  def parent(self) -> "'SessionElement'"
  def prev(self, locator: 'str | None' = None, index: 'int' = 1) -> "'SessionElement'"
  def prevs(self, locator: 'str | None' = None) -> "list['SessionElement']"
  @property raw_text
  def s_ele(self, locator: 'str | None' = None) -> "'SessionElement'"
  def s_eles(self, locator: 'str') -> "list['SessionElement']"
  @property tag
  @property text

=== SessionOptions ===
  def set_timeout(self, timeout_secs: 'int') -> "'SessionOptions'"
  def set_user_agent(self, user_agent: 'str') -> "'SessionOptions'"

=== SessionPage ===
  def cookies(self) -> 'list[dict[str, str | None]]'
  def ele(self, locator: 'str') -> "'SessionElement'"
  def eles(self, locator: 'str') -> "list['SessionElement']"
  @property encoding
  def get(self, url: 'str') -> 'bool'
  @property html
  @property json
  def post(self, url: 'str', payload: 'dict[str, Any] | None' = None) -> 'bool'
  @property raw_data
  def s_ele(self, locator: 'str | None' = None) -> "'SessionElement'"
  def s_eles(self, locator: 'str') -> "list['SessionElement']"
  def set_user_agent(self, user_agent: 'str | None') -> 'None'
  @property status_code
  @property title
  @property url
  @property user_agent

=== WebPage ===
  def change_mode(self, mode: 'str | None' = None, go: 'bool' = True, copy_cookies: 'bool' = True) -> 'None'
  def cookies(self) -> 'list[dict[str, str | None]]'
  def cookies_to_browser(self) -> 'None'
  def cookies_to_session(self, copy_user_agent: 'bool' = True) -> 'None'
  @property download_file_exists_mode
  def download_missions(self) -> "list['DownloadMission']"
  @property download_path
  def ele(self, locator: 'str') -> 'Any'
  def eles(self, locator: 'str') -> 'list[Any]'
  @property encoding
  def get(self, url: 'str') -> 'bool'
  def handle_alert(self, accept: 'bool' = True, send: 'str | None' = None, timeout: 'float' = 10.0, next_one: 'bool' = False) -> 'str | bool | None'
  @property html
  @property intercept
  @property json
  def last_download(self) -> "'DownloadMission | None'"
  @property listen
  @property mode
  def post(self, url: 'str', payload: 'dict[str, Any] | None' = None) -> 'bool'
  def quit(self) -> 'None'
  @property raw_data
  def run_js(self, expression: 'str') -> 'Any'
  def s_ele(self, locator: 'str | None' = None) -> "'SessionElement'"
  def s_eles(self, locator: 'str') -> "list['SessionElement']"
  @property set
  def set_download_file_exists_mode(self, mode: 'str') -> 'None'
  def set_download_path(self, path: 'str') -> 'None'
  @property states
  @property status_code
  @property tab_ids
  @property tabs_count
  @property title
  @property url
  @property user_agent
  @property wait
  def wait_for_download(self, filename: 'str | None' = None, timeout: 'float' = 10.0) -> 'str'


# === Internal Helper Classes (not exported in __all__ but accessible via attributes) ===


## BrowserWait

- def download_begin(self, timeout: 'float' = 10.0, cancel_it: 'bool' = False) -> "'DownloadMission | bool'"
- def downloads_done(self, timeout: 'float' = 10.0, cancel_if_timeout: 'bool' = True) -> 'bool'
- def new_tab(self, timeout: 'float' = 10.0, curr_tab: 'str | None' = None) -> 'str | bool'

## BrowserStates

- @property is_alive
- @property is_existed
- @property is_headless
- @property is_incognito

## BrowserSetter

- @property load_mode

## LoadModeSetter

- def eager(self) -> 'None'
- def none(self) -> 'None'
- def normal(self) -> 'None'

## PageSetter

- def activate(self) -> 'None'
- def auto_handle_alert(self, on_off: 'bool | None' = True, accept: 'bool' = True, send: 'str | None' = None) -> 'None'
- def blocked_urls(self, urls: 'str | list[str] | tuple[str, ...] | set[str] | None') -> 'None'
- def download_file_exists(self, mode: 'str') -> 'None'
- def download_file_name(self, name: 'str | None' = None, suffix: 'str | None' = None) -> 'None'
- def download_path(self, path: 'str') -> 'None'
- def headers(self, headers: 'dict[str, str]') -> 'None'
- @property load_mode
- def local_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def session_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def upload_files(self, files: 'Any') -> 'None'
- def user_agent(self, ua: 'str', platform: 'str | None' = None) -> 'None'
- @property window

## ChromiumPageSetter

- def activate(self) -> 'None'
- def auto_handle_alert(self, on_off: 'bool | None' = True, accept: 'bool' = True, send: 'str | None' = None) -> 'None'
- def blocked_urls(self, urls: 'str | list[str] | tuple[str, ...] | set[str] | None') -> 'None'
- def download_file_exists(self, mode: 'str') -> 'None'
- def download_file_name(self, name: 'str | None' = None, suffix: 'str | None' = None) -> 'None'
- def download_path(self, path: 'str') -> 'None'
- def headers(self, headers: 'dict[str, str]') -> 'None'
- @property load_mode
- def local_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def session_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def upload_files(self, files: 'Any') -> 'None'
- def user_agent(self, ua: 'str', platform: 'str | None' = None) -> 'None'
- @property window

## PageWait

- def alert_closed(self, timeout: 'float' = 10.0) -> "'Page | bool'"
- def all_downloads_done(self, timeout: 'float' = 10.0, cancel_if_timeout: 'bool' = True) -> 'bool'
- def doc_loaded(self, timeout: 'float' = 10.0) -> 'bool'
- def download_begin(self, timeout: 'float' = 10.0, cancel_it: 'bool' = False) -> "'DownloadMission | bool'"
- def downloads_done(self, timeout: 'float' = 10.0, cancel_if_timeout: 'bool' = True) -> 'bool'
- def ele_clickable(self, loc_or_ele: "str | 'Element'", timeout: 'float' = 10.0) -> "'Element | bool'"
- def ele_deleted(self, loc_or_ele: "str | 'Element'", timeout: 'float' = 10.0) -> "'Element | bool'"
- def ele_displayed(self, loc_or_ele: "str | 'Element'", timeout: 'float' = 10.0) -> "'Element | bool'"
- def ele_enabled(self, loc_or_ele: "str | 'Element'", timeout: 'float' = 10.0) -> "'Element | bool'"
- def ele_hidden(self, loc_or_ele: "str | 'Element'", timeout: 'float' = 10.0) -> "'Element | bool'"
- def eles_loaded(self, locators: 'str | list[str] | tuple[str, ...] | set[str]', timeout: 'float' = 10.0, any_one: 'bool' = False) -> 'bool'
- def load_start(self, timeout: 'float' = 10.0) -> 'bool'
- def title_change(self, text: 'str', exclude: 'bool' = False, timeout: 'float' = 10.0) -> "'Page | bool'"
- def url_change(self, text: 'str', exclude: 'bool' = False, timeout: 'float' = 10.0) -> "'Page | bool'"

## PageStates

- @property has_alert
- @property is_alive
- @property is_existed
- @property is_headless
- @property is_incognito
- @property is_loading
- @property ready_state

## WindowSetter

- def full(self) -> 'None'
- def hide(self) -> 'None'
- def location(self, x: 'int | None' = None, y: 'int | None' = None) -> 'None'
- def max(self) -> 'None'
- def mini(self) -> 'None'
- def normal(self) -> 'None'
- def show(self) -> 'None'
- def size(self, width: 'int | None' = None, height: 'int | None' = None) -> 'None'

## WebPageWait

- def alert_closed(self, timeout: 'float' = 10.0) -> "'WebPage | bool'"
- def all_downloads_done(self, timeout: 'float' = 10.0, cancel_if_timeout: 'bool' = True) -> 'bool'
- def doc_loaded(self, timeout: 'float' = 10.0) -> 'bool'
- def download_begin(self, timeout: 'float' = 10.0, cancel_it: 'bool' = False) -> "'DownloadMission | bool'"
- def ele_clickable(self, locator: 'str', timeout: 'float' = 10.0) -> 'Any'
- def ele_deleted(self, locator: 'str', timeout: 'float' = 10.0) -> 'Any'
- def ele_displayed(self, locator: 'str', timeout: 'float' = 10.0) -> 'Any'
- def ele_enabled(self, locator: 'str', timeout: 'float' = 10.0) -> 'Any'
- def ele_hidden(self, locator: 'str', timeout: 'float' = 10.0) -> 'Any'
- def eles_loaded(self, locators: 'str | list[str] | tuple[str, ...] | set[str]', timeout: 'float' = 10.0, any_one: 'bool' = False) -> 'bool'
- def load_start(self, timeout: 'float' = 10.0) -> 'bool'
- def new_tab(self, timeout: 'float' = 10.0, curr_tab: 'str | None' = None) -> 'str | bool'
- def title_change(self, text: 'str', exclude: 'bool' = False, timeout: 'float' = 10.0) -> "'WebPage | bool'"
- def url_change(self, text: 'str', exclude: 'bool' = False, timeout: 'float' = 10.0) -> "'WebPage | bool'"

## WebPageSetter

- def activate(self) -> 'None'
- def auto_handle_alert(self, on_off: 'bool | None' = True, accept: 'bool' = True, send: 'str | None' = None) -> 'None'
- def blocked_urls(self, urls: 'str | list[str] | tuple[str, ...] | set[str] | None') -> 'None'
- def download_file_exists(self, mode: 'str') -> 'None'
- def download_file_name(self, name: 'str | None' = None, suffix: 'str | None' = None) -> 'None'
- def download_path(self, path: 'str') -> 'None'
- def headers(self, headers: 'dict[str, str]') -> 'None'
- @property load_mode
- def local_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def session_storage(self, item: 'str', value: 'str | bool | None') -> 'None'
- def upload_files(self, files: 'Any') -> 'None'
- def user_agent(self, ua: 'str', platform: 'str | None' = None) -> 'None'
- @property window

## WebPageStates

- @property has_alert
- @property is_alive
- @property is_existed
- @property is_headless
- @property is_incognito
- @property is_loading
- @property ready_state

## ElementStates

- @property has_rect
- @property is_alive
- @property is_checked
- @property is_clickable
- @property is_covered
- @property is_displayed
- @property is_enabled
- @property is_in_viewport
- @property is_selected
- @property is_whole_in_viewport

## ElementWait

- def clickable(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def covered(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def deleted(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def disabled(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def disabled_or_deleted(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def displayed(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def enabled(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def has_rect(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def hidden(self, timeout: 'float' = 10.0) -> "'Element | bool'"
- def not_covered(self, timeout: 'float' = 10.0) -> "'Element | bool'"

## Interceptor

- @property listening
- def start(self, targets: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None, is_regex: 'bool' = False, method: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None, res_type: 'str | list[str] | tuple[str, ...] | set[str] | bool | None' = None) -> 'None'
- def stop(self) -> 'None'
- def wait(self, timeout: 'float | None' = None) -> "'InterceptedRequest | bool'"

## InterceptedRequest

- def continue_request(self, url: 'str | None' = None, method: 'str | None' = None, headers: 'dict[str, str] | None' = None, post_data: 'str | bytes | None' = None) -> 'None'
- def fail(self, reason: 'str' = 'BlockedByClient') -> 'None'
- @property frame_id
- def fulfill(self, response_code: 'int' = 200, body: 'str | bytes | None' = None, headers: 'dict[str, str] | None' = None, response_phrase: 'str | None' = None, body_base64: 'bool' = False) -> 'None'
- @property has_post_data
- @property headers
- @property method
- @property post_data_entries
- @property request_id
- @property resource_type
- @property url


# === Examples API Usage ===

## basic_usage.py
- ChromiumPage()
- page.get(url)
- page.url
- page.title
- page.ele(locator).text
- page.run_js(expression)
- page.quit()

## test_baidu.py
- ChromiumPage()
- page.get(url)
- page.url
- page.title
- page.html
- page.ele(locator)
- ele.text
- ele.attr(name)
- ele.states.is_displayed
- ele.states.is_enabled
- ele.states.has_rect
- page.run_js(expression)
- page.save_screenshot(path, full_page=False)
- page.wait.ele_displayed(locator, timeout)
- page.wait.ele_enabled(locator, timeout)
- page.quit()

## test_openpage_userdata.py
- ChromiumOptions().set_user_data_path(path)
- ChromiumPage(options)
- page.get(url)
- page.ele(locator)
- ele.attr(name)
- ele.text
- ele.html
- page.eles(locator)
- page.run_js(expression)
- page.quit()

## webpage_modes.py
- ChromiumOptions()
- WebPage(mode="d", chromium_options=...)
- page.get(url)
- page.ele(locator).text
- page.change_mode(mode, go, copy_cookies)
- page.json
- page.quit()


# === Tests API Usage Summary ===

## Imports used in tests
- from openpage import Browser
- from openpage import ChromiumOptions
- from openpage import ChromiumPage
- from openpage import DownloadMission
- from openpage import SessionPage
- from openpage import SessionOptions
- from openpage import WebPage

## Browser API
- Browser.launch(options)
- browser.states.is_alive
- browser.states.is_headless
- browser.states.is_existed
- browser.states.is_incognito
- browser.new_page(url)
- browser.tabs_count
- browser.tab_ids
- browser.wait.new_tab(timeout, curr_tab)
- browser.wait.download_begin(timeout, cancel_it)
- browser.wait.downloads_done(timeout, cancel_if_timeout)
- browser.set.load_mode.normal() / .eager() / .none()
- browser.close()

## ChromiumPage API
- ChromiumPage(options)
- page.get(url)
- page.goto(url)
- page.url
- page.title
- page.html
- page.user_agent
- page.tab_id
- page.tab_ids
- page.tabs_count
- page.cookies()
- page.run_js(expression)
- page.evaluate(expression)
- page.ele(locator, timeout)
- page.eles(locator)
- page.s_ele(locator)
- page.s_eles(locator)
- page.wait_for(locator, timeout)
- page.click(locator)
- page.input(locator, text)
- page.text(locator)
- page.attr(locator, name)
- page.save_screenshot(path, full_page)
- page.save_pdf(path)
- page.new_tab(url)
- page.close()
- page.quit()
- page.get_tab(target_id)
- page.download_path
- page.set_download_path(path)
- page.wait_for_download(filename, timeout)
- page.download_missions()
- page.last_download()
- page.handle_alert(accept, send, timeout, next_one)
- page.listen
- page.intercept
- page.wait.ele_displayed(loc_or_ele, timeout)
- page.wait.ele_hidden(loc_or_ele, timeout)
- page.wait.ele_deleted(loc_or_ele, timeout)
- page.wait.ele_enabled(loc_or_ele, timeout)
- page.wait.ele_clickable(loc_or_ele, timeout)
- page.wait.eles_loaded(locators, timeout, any_one)
- page.wait.url_change(text, exclude, timeout)
- page.wait.title_change(text, exclude, timeout)
- page.wait.load_start(timeout)
- page.wait.doc_loaded(timeout)
- page.wait.alert_closed(timeout)
- page.wait.download_begin(timeout, cancel_it)
- page.wait.downloads_done(timeout, cancel_if_timeout)
- page.wait.all_downloads_done(timeout, cancel_if_timeout)
- page.states.ready_state
- page.states.is_loading
- page.states.is_alive
- page.states.is_headless
- page.states.has_alert
- page.states.is_existed
- page.states.is_incognito
- page.set.window.max() / .mini() / .full() / .normal() / .hide() / .show()
- page.set.window.size(width, height)
- page.set.window.location(x, y)
- page.set.load_mode.normal() / .eager() / .none()
- page.set.blocked_urls(urls)
- page.set.headers(headers)
- page.set.user_agent(ua, platform)
- page.set.session_storage(item, value)
- page.set.local_storage(item, value)
- page.set.auto_handle_alert(on_off, accept, send)
- page.set.download_path(path)
- page.set.download_file_exists(mode)
- page.set.download_file_name(name, suffix)
- page.set.upload_files(files)
- page.set.activate()

## SessionPage API
- SessionPage(options)
- page.get(url)
- page.post(url, payload)
- page.url
- page.status_code
- page.raw_data
- page.encoding
- page.html
- page.json
- page.title
- page.user_agent
- page.set_user_agent(user_agent)
- page.cookies()
- page.ele(locator)
- page.eles(locator)
- page.s_ele(locator)
- page.s_eles(locator)

## WebPage API
- WebPage(mode, chromium_options, session_or_options)
- page.mode
- page.get(url)
- page.post(url, payload)
- page.url
- page.title
- page.user_agent
- page.html
- page.raw_data
- page.encoding
- page.status_code
- page.json
- page.tabs_count
- page.tab_ids
- page.download_path
- page.download_file_exists_mode
- page.set_download_path(path)
- page.set_download_file_exists_mode(mode)
- page.wait_for_download(filename, timeout)
- page.download_missions()
- page.last_download()
- page.cookies()
- page.change_mode(mode, go, copy_cookies)
- page.cookies_to_session(copy_user_agent)
- page.cookies_to_browser()
- page.handle_alert(accept, send, timeout, next_one)
- page.listen
- page.intercept
- page.ele(locator)
- page.eles(locator)
- page.s_ele(locator)
- page.s_eles(locator)
- page.run_js(expression)
- page.quit()
- page.wait.new_tab(timeout, curr_tab)
- page.wait.all_downloads_done(timeout, cancel_if_timeout)
- page.wait.download_begin(timeout, cancel_it)
- page.wait.url_change(text, exclude, timeout)
- page.wait.title_change(text, exclude, timeout)
- page.wait.load_start(timeout)
- page.wait.doc_loaded(timeout)
- page.wait.eles_loaded(locators, timeout, any_one)
- page.wait.alert_closed(timeout)
- page.wait.ele_displayed(locator, timeout)
- page.wait.ele_hidden(locator, timeout)
- page.wait.ele_enabled(locator, timeout)
- page.wait.ele_deleted(locator, timeout)
- page.wait.ele_clickable(locator, timeout)
- page.states.is_alive
- page.states.is_loading
- page.states.ready_state
- page.states.is_headless
- page.states.has_alert
- page.states.is_existed
- page.states.is_incognito
- page.set.window.max() / .mini() / .full() / .normal() / .hide() / .show()
- page.set.window.size(width, height)
- page.set.window.location(x, y)
- page.set.load_mode.normal() / .eager() / .none()
- page.set.blocked_urls(urls)
- page.set.headers(headers)
- page.set.user_agent(ua, platform)
- page.set.session_storage(item, value)
- page.set.local_storage(item, value)
- page.set.auto_handle_alert(on_off, accept, send)
- page.set.download_path(path)
- page.set.download_file_exists(mode)
- page.set.download_file_name(name, suffix)
- page.set.upload_files(files)
- page.set.activate()

## Element API
- ele.click()
- ele.input(text)
- ele.clear()
- ele.press(key)
- ele.text
- ele.html
- ele.attr(name)
- ele.run_js(script)
- ele.states.is_selected
- ele.states.is_checked
- ele.states.is_displayed
- ele.states.is_enabled
- ele.states.is_alive
- ele.states.has_rect
- ele.states.is_in_viewport
- ele.states.is_whole_in_viewport
- ele.states.is_covered
- ele.states.is_clickable
- ele.wait.displayed(timeout)
- ele.wait.hidden(timeout)
- ele.wait.enabled(timeout)
- ele.wait.disabled(timeout)
- ele.wait.deleted(timeout)
- ele.wait.clickable(timeout)
- ele.wait.has_rect(timeout)
- ele.wait.covered(timeout)
- ele.wait.not_covered(timeout)
- ele.wait.disabled_or_deleted(timeout)
- ele.ele(locator)
- ele.eles(locator)
- ele.save_screenshot(path)

## SessionElement API
- se.tag
- se.text
- se.html
- se.inner_html
- se.raw_text
- se.attrs
- se.attr(name)
- se.ele(locator)
- se.eles(locator)
- se.child(locator, index)
- se.parent()
- se.children(locator)
- se.prev(locator, index)
- se.next(locator, index)
- se.before(locator, index)
- se.after(locator, index)
- se.prevs(locator)
- se.nexts(locator)
- se.befores(locator)
- se.afters(locator)
- se.s_ele(locator)
- se.s_eles(locator)

## DownloadMission API
- mission.guid
- mission.url
- mission.suggested_filename
- mission.state
- mission.received_bytes
- mission.total_bytes
- mission.final_path
- mission.is_done
- mission.wait(timeout)
- mission.cancel()

## Listener API
- listener.start(targets, is_regex, method, res_type)
- listener.set_targets(targets, is_regex, method, res_type)
- listener.wait(count, timeout, fit_count)
- listener.steps(count, timeout, gap)
- listener.wait_silent(timeout, targets_only)
- listener.clear()
- listener.pause(clear)
- listener.resume()
- listener.stop()
- listener.listening

## Interceptor API
- interceptor.start(targets, is_regex, method, res_type)
- interceptor.wait(timeout)
- interceptor.stop()
- interceptor.listening

## InterceptedRequest API
- req.request_id
- req.frame_id
- req.url
- req.method
- req.headers
- req.resource_type
- req.has_post_data
- req.post_data_entries
- req.continue_request(url, method, headers, post_data)
- req.fail(reason)
- req.fulfill(response_code, body, headers, response_phrase, body_base64)

## ListenerPacket API
- pkt.target
- pkt.url
- pkt.method
- pkt.resource_type
- pkt.is_failed
- pkt.request
- pkt.response
- pkt.fail_info

## ListenerRequest API
- req.url
- req.method
- req.headers
- req.post_data
- req.extra_info

## ListenerResponse API
- resp.url
- resp.status
- resp.status_text
- resp.headers
- resp.mime_type
- resp.body
- resp.body_base64
- resp.extra_info

## ListenerResponseExtraInfo API
- info.headers
- info.status_code
- info.headers_text

## ListenerFailInfo API
- info.error_text
- info.canceled
- info.blocked_reason

## ChromiumOptions API
- opts.browser_path
- opts.download_path
- opts.download_file_exists_mode
- opts.load_mode
- opts.headless_mode
- opts.user_data_path
- opts.width
- opts.height
- opts.no_sandbox_mode
- opts.set_browser_path(path)
- opts.set_user_data_path(path)
- opts.set_download_path(path)
- opts.set_file_exists(mode)
- opts.set_load_mode(value)
- opts.headless(on_off)
- opts.set_window_size(width, height)
- opts.no_sandbox(on_off)

## SessionOptions API
- opts.timeout_secs
- opts.user_agent
- opts.set_timeout(timeout_secs)
- opts.set_user_agent(user_agent)
# OpenPage Python API 完整统计

## 文件结构

- python/openpage/__init__.py - 公开 API 导出
- python/openpage/_compat.py - 兼容层，所有 Python 包装类
- python/openpage/py.typed - PEP 561 类型标记
- python/examples/ - 示例代码
- python/tests/test_openpage.py - 测试代码

## __init__.py 导出的公开 API (共 17 个)

- **Browser** (class, 17 个公开成员)
- **ChromiumOptions** (class, 17 个公开成员)
- **ChromiumPage** (class, 38 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 13 个公开成员)
- **Listener** (class, 10 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 29 个公开成员)
- **SessionElement** (class, 22 个公开成员)
- **SessionOptions** (class, 4 个公开成员)
- **SessionPage** (class, 16 个公开成员)
- **WebPage** (class, 36 个公开成员)

## _compat.py 中的内部辅助类 (未在 __all__ 中导出)

- **Any** (class, 0 个公开成员)
- **BrowserSetter** (class, 1 个公开成员)
- **BrowserStates** (class, 4 个公开成员)
- **BrowserWait** (class, 3 个公开成员)
- **ChromiumPageSetter** (class, 13 个公开成员)
- **ElementStates** (class, 10 个公开成员)
- **ElementWait** (class, 10 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **LoadModeSetter** (class, 3 个公开成员)
- **PageSetter** (class, 13 个公开成员)
- **PageStates** (class, 7 个公开成员)
- **PageWait** (class, 14 个公开成员)
- **Path** (class, 70 个公开成员)
- **WebPageSetter** (class, 13 个公开成员)
- **WebPageStates** (class, 7 个公开成员)
- **WebPageWait** (class, 14 个公开成员)
- **WindowSetter** (class, 8 个公开成员)

## 底层 Rust 模块 openpage_rs 中的类

- **Browser** (class, 28 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 31 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **Listener** (class, 9 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 64 个公开成员)
- **SessionElement** (class, 20 个公开成员)
- **SessionPage** (class, 23 个公开成员)
- **WebPage** (class, 83 个公开成员)
- **openpage_rs** (module)
