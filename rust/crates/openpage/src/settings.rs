use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use crate::element_list::ElementsOneRuntimeConfig;
use crate::error::{OpenPageError, OpenPageResult};

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub raise_when_ele_not_found: bool,
    pub raise_when_click_failed: bool,
    pub raise_when_wait_failed: bool,
    pub singleton_tab_obj: bool,
    pub cdp_timeout: f64,
    pub browser_connect_timeout: f64,
    pub auto_handle_alert: Option<bool>,
    pub language: Option<String>,
    pub suffixes_list: Option<PathBuf>,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            raise_when_ele_not_found: false,
            raise_when_click_failed: false,
            raise_when_wait_failed: false,
            singleton_tab_obj: true,
            cdp_timeout: 30.0,
            browser_connect_timeout: 30.0,
            auto_handle_alert: None,
            language: None,
            suffixes_list: Some(default_suffixes_list_path()),
        }
    }
}

pub struct Settings;

impl Settings {
    pub fn snapshot() -> SettingsSnapshot {
        snapshot()
    }

    pub fn reset() -> Self {
        restore(SettingsSnapshot::default());
        Self
    }

    pub fn set_raise_when_ele_not_found(on_off: bool) -> Self {
        with_settings_write(|settings| settings.raise_when_ele_not_found = on_off);
        Self
    }

    pub fn set_raise_when_click_failed(on_off: bool) -> Self {
        with_settings_write(|settings| settings.raise_when_click_failed = on_off);
        Self
    }

    pub fn set_raise_when_wait_failed(on_off: bool) -> Self {
        with_settings_write(|settings| settings.raise_when_wait_failed = on_off);
        Self
    }

    pub fn set_singleton_tab_obj(on_off: bool) -> Self {
        with_settings_write(|settings| settings.singleton_tab_obj = on_off);
        Self
    }

    pub fn set_cdp_timeout(second: f64) -> Self {
        with_settings_write(|settings| settings.cdp_timeout = second);
        Self
    }

    pub fn set_browser_connect_timeout(second: f64) -> Self {
        with_settings_write(|settings| settings.browser_connect_timeout = second);
        Self
    }

    pub fn set_auto_handle_alert(accept: Option<bool>) -> Self {
        with_settings_write(|settings| settings.auto_handle_alert = accept);
        Self
    }

    pub fn set_language(code: impl Into<String>) -> Self {
        let code = code.into();
        with_settings_write(|settings| settings.language = Some(code.clone()));
        Self
    }

    pub fn set_suffixes_list(path: impl AsRef<Path>) -> Self {
        with_settings_write(|settings| settings.suffixes_list = Some(path.as_ref().to_path_buf()));
        Self
    }
}

static SETTINGS: OnceLock<RwLock<SettingsSnapshot>> = OnceLock::new();

fn settings_store() -> &'static RwLock<SettingsSnapshot> {
    SETTINGS.get_or_init(|| RwLock::new(SettingsSnapshot::default()))
}

fn snapshot() -> SettingsSnapshot {
    settings_store()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn with_settings_write<F>(mut update: F)
where
    F: FnMut(&mut SettingsSnapshot),
{
    let mut settings = settings_store()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    update(&mut settings);
}

pub(crate) fn restore(snapshot: SettingsSnapshot) {
    let mut settings = settings_store()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *settings = snapshot;
}

pub(crate) fn default_none_element_runtime_config() -> ElementsOneRuntimeConfig {
    let settings = snapshot();
    ElementsOneRuntimeConfig {
        raise_when_not_found: settings.raise_when_ele_not_found,
        ..ElementsOneRuntimeConfig::default()
    }
}

pub(crate) fn default_auto_handle_alert() -> Option<bool> {
    snapshot().auto_handle_alert
}

pub(crate) fn wait_failed_should_raise() -> bool {
    snapshot().raise_when_wait_failed
}

pub(crate) fn click_failed_should_raise() -> bool {
    snapshot().raise_when_click_failed
}

pub(crate) fn singleton_tab_obj_enabled() -> bool {
    snapshot().singleton_tab_obj
}

pub fn wait_timeout_result(operation: &str, timeout_ms: u64) -> OpenPageResult<bool> {
    if wait_failed_should_raise() {
        Err(timeout_error(operation, timeout_ms))
    } else {
        Ok(false)
    }
}

pub(crate) fn timeout_error(operation: &str, timeout_ms: u64) -> OpenPageError {
    OpenPageError::Timeout(localized_timeout_message(operation, timeout_ms))
}

pub(crate) fn cdp_timeout_duration() -> Duration {
    seconds_to_duration(snapshot().cdp_timeout)
}

pub(crate) fn browser_connect_timeout_duration() -> Duration {
    seconds_to_duration(snapshot().browser_connect_timeout)
}

pub(crate) fn suffixes_list_path() -> PathBuf {
    snapshot()
        .suffixes_list
        .unwrap_or_else(default_suffixes_list_path)
}

pub(crate) fn timeout_duration_millis(timeout: Duration) -> u64 {
    timeout.as_millis().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn click_failed_no_rect_message() -> String {
    localized_message(
        "simulated click failed because element has no rect",
        "模拟点击失败，因为元素没有位置及大小",
    )
}

pub(crate) fn click_failed_hidden_or_disabled_message() -> String {
    localized_message(
        "simulated click failed because element is hidden or disabled",
        "模拟点击失败，因为元素被隐藏或被禁用",
    )
}

pub(crate) fn element_no_visible_rect_message() -> String {
    localized_message(
        "element does not have a visible rect",
        "元素没有可见位置及大小",
    )
}

pub(crate) fn localized_error_with_detail(
    en_prefix: &str,
    zh_cn_prefix: &str,
    detail: &str,
) -> String {
    let prefix = localized_message(en_prefix, zh_cn_prefix);
    if detail.trim().is_empty() {
        prefix
    } else {
        format!("{prefix}: {detail}")
    }
}

pub(crate) fn cookie_name_empty_message() -> String {
    localized_message("cookie name cannot be empty", "cookie 名称不能为空")
}

pub(crate) fn cookie_value_empty_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("cookie `{name}` 的值不能为空"),
        _ => format!("cookie `{name}` value cannot be empty"),
    }
}

pub(crate) fn cookie_requires_url_or_domain_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("cookie `{name}` 必须设置 url 或 domain"),
        _ => format!("cookie `{name}` requires either url or domain"),
    }
}

pub(crate) fn session_cookie_requires_url_or_domain_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("session cookie `{name}` 必须设置 url 或 domain"),
        _ => format!("session cookie `{name}` requires either url or domain"),
    }
}

pub(crate) fn cookie_text_separator_conflict_message() -> String {
    localized_message(
        "cookie text cannot mix ';' and ',' separators",
        "cookie 文本不能同时混用 ';' 和 ',' 分隔符",
    )
}

pub(crate) fn invalid_cookie_same_site_message(same_site: &str, name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("cookie `{name}` 的 same_site `{same_site}` 无效"),
        _ => format!("invalid cookie same_site `{same_site}` for `{name}`"),
    }
}

pub(crate) fn invalid_auto_port_scope_message(start: u16, end: u16) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("auto_port 范围必须满足 0 < start < end，当前为 ({start}, {end})")
        }
        _ => format!("auto_port scope must satisfy 0 < start < end, got ({start}, {end})"),
    }
}

pub(crate) fn no_free_port_in_auto_port_scope_message(start: u16, end: u16) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("未能在 auto_port 范围 [{start}, {end}) 内找到空闲端口"),
        _ => format!("failed to find free port in auto_port scope [{start}, {end})"),
    }
}

pub(crate) fn invalid_tab_index_message() -> String {
    localized_message(
        "tab index must start from 1 or use negative indices from -1",
        "标签页序号必须从 1 开始，或使用从 -1 开始的负序号",
    )
}

pub(crate) fn invalid_download_file_exists_mode_message(value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("下载文件已存在策略必须是 rename/overwrite/skip 之一，当前为 {value}")
        }
        _ => format!("download file-exists mode must be one of rename/overwrite/skip, got {value}"),
    }
}

pub(crate) fn invalid_load_mode_message(value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("加载模式必须是 normal/eager/none 之一，当前为 {value}"),
        _ => format!("load mode must be one of normal/eager/none, got {value}"),
    }
}

pub(crate) fn unsupported_mouse_button_message(button: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("不支持的鼠标按钮: {button}"),
        _ => format!("unsupported mouse button: {button}"),
    }
}

pub(crate) fn click_at_count_must_be_positive_message() -> String {
    localized_message(
        "click_at() count must be >= 1",
        "click_at() 次数必须大于等于 1",
    )
}

pub(crate) fn download_not_found_message(guid: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("没有找到下载任务 `{guid}`"),
        _ => format!("download `{guid}` was not found"),
    }
}

pub(crate) fn invalid_options_manager_ini_literal_message(detail: &str) -> String {
    localized_error_with_detail(
        "invalid options manager ini literal",
        "无效的 OptionsManager ini 字面量",
        detail,
    )
}

pub(crate) fn invalid_xpath_html_message(detail: &str) -> String {
    localized_error_with_detail("invalid xpath html", "无效的 xpath HTML", detail)
}

pub(crate) fn invalid_xpath_query_message(query: &str, detail: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("无效的 xpath `{query}`: {detail}"),
        _ => format!("invalid xpath `{query}`: {detail}"),
    }
}

pub(crate) fn xpath_node_no_longer_exists_message() -> String {
    localized_message("xpath node no longer exists", "xpath 节点已不存在")
}

pub(crate) fn invalid_xpath_segment_index_message(tag: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("xpath 片段 `{tag}` 的序号无效"),
        _ => format!("invalid xpath segment index for `{tag}`"),
    }
}

pub(crate) fn xpath_segment_not_found_message(tag: &str, index: usize) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("没有找到 xpath 片段 `{tag}[{index}]`"),
        _ => format!("xpath segment `{tag}[{index}]` not found"),
    }
}

pub(crate) fn xpath_path_not_found_message(path: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("没有找到 xpath 路径 `{path}`"),
        _ => format!("xpath path `{path}` not found"),
    }
}

pub(crate) fn unsupported_xpath_path_message(path: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("不支持的 xpath 路径 `{path}`"),
        _ => format!("unsupported xpath path `{path}`"),
    }
}

pub(crate) fn unsupported_key_message(key: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("不支持的按键: {key}"),
        _ => format!("unsupported key: {key}"),
    }
}

pub(crate) fn driver_mode_only_message(operation: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{operation} 仅在 driver 模式可用"),
        _ => format!("{operation} is only available in driver mode"),
    }
}

pub(crate) fn web_element_list_driver_filter_message(operation: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("{operation} 过滤器仅适用于 driver-backed WebElement 列表")
        }
        _ => format!("{operation} filters are only available for driver-backed WebElement lists"),
    }
}

pub(crate) fn browser_backed_page_only_message(operation: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{operation} 仅适用于 browser-backed 页面"),
        _ => format!("{operation} is only available on browser-backed pages"),
    }
}

pub(crate) fn zoom_factor_must_be_positive_message(factor: f64) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("zoom factor 必须是有限正数，当前为 {factor}"),
        _ => format!("zoom factor must be a positive finite number: {factor}"),
    }
}

pub(crate) fn permission_setting_invalid_message(setting: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("permission setting 必须是 granted/denied/prompt 之一，当前为 {setting}")
        }
        _ => format!("permission setting must be one of granted/denied/prompt, got {setting}"),
    }
}

pub(crate) fn permission_origin_scheme_message(value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("permission origin 必须使用 http 或 https，当前为 {value}"),
        _ => format!("permission origin must use http or https, got {value}"),
    }
}

pub(crate) fn drag_in_requires_file_path_message() -> String {
    localized_message(
        "drag_in() requires at least one file path",
        "drag_in() 至少需要一个文件路径",
    )
}

pub(crate) fn drag_in_file_path_empty_message() -> String {
    localized_message(
        "drag_in() file path must not be empty",
        "drag_in() 文件路径不能为空",
    )
}

pub(crate) fn screenshot_clip_order_message() -> String {
    localized_message(
        "screenshot clip requires right_bottom to be greater than left_top",
        "截图裁剪要求 right_bottom 大于 left_top",
    )
}

pub(crate) fn screenshot_clip_complete_message() -> String {
    localized_message(
        "screenshot clip requires both left_top and right_bottom",
        "截图裁剪需要同时提供 left_top 和 right_bottom",
    )
}

pub(crate) fn value_did_not_return_message(
    name: &str,
    expected_en: &str,
    expected_zh_cn: &str,
    value: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回{expected_zh_cn}: {value}"),
        _ => format!("{name} did not return {expected_en}: {value}"),
    }
}

pub(crate) fn value_returned_non_string_entry_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 返回了非字符串条目: {value}"),
        _ => format!("{name} returned a non-string entry: {value}"),
    }
}

pub(crate) fn value_pair_entry_not_number_message(name: &str, entry: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} {entry} 条目不是数字"),
        _ => format!("{name} {entry} entry is not a number"),
    }
}

pub(crate) fn value_state_bool_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 状态脚本未返回布尔值: {value}"),
        _ => format!("{name} state script did not return a bool: {value}"),
    }
}

pub(crate) fn value_coordinate_pair_parse_failed_message(name: &str, detail: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("解析 {name} 坐标对失败: {detail}"),
        _ => format!("failed to parse {name} coordinate pair: {detail}"),
    }
}

pub(crate) fn value_coordinate_pair_required_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回坐标对"),
        _ => format!("{name} did not return a coordinate pair"),
    }
}

pub(crate) fn value_coordinate_pair_exactly_two_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回恰好两个坐标"),
        _ => format!("{name} did not return exactly two coordinates"),
    }
}

pub(crate) fn value_coordinate_not_numeric_message(name: &str, axis: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} {axis} 坐标不是数字: {value}"),
        _ => format!("{name} {axis} coordinate was not numeric: {value}"),
    }
}

pub(crate) fn value_string_compatible_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回可转为字符串的值: {value}"),
        _ => format!("{name} did not return a string-compatible value: {value}"),
    }
}

pub(crate) fn value_non_negative_integer_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回非负整数: {value}"),
        _ => format!("{name} did not return a non-negative integer: {value}"),
    }
}

pub(crate) fn value_number_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回数字: {value}"),
        _ => format!("{name} did not return a number: {value}"),
    }
}

pub(crate) fn value_unavailable_message(name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 不可用"),
        _ => format!("{name} is unavailable"),
    }
}

pub(crate) fn value_string_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 未返回字符串: {value}"),
        _ => format!("{name} did not return a string: {value}"),
    }
}

pub(crate) fn value_string_vec_entry_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 包含不可转为字符串的值: {value}"),
        _ => format!("{name} contained a non-string-compatible value: {value}"),
    }
}

pub(crate) fn value_string_vec_array_required_message(name: &str, value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{name} 脚本未返回数组: {value}"),
        _ => format!("{name} script did not return an array: {value}"),
    }
}

pub(crate) fn blob_src_data_url_required_message(value: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("blob src 未返回 data URL 字符串: {value}"),
        _ => format!("blob src did not return a data URL string: {value}"),
    }
}

pub(crate) fn action_wait_seconds_non_negative_message() -> String {
    localized_message("wait() seconds must be >= 0", "wait() 秒数必须 >= 0")
}

pub(crate) fn action_type_interval_non_negative_message() -> String {
    localized_message(
        "type_with_interval() seconds must be >= 0",
        "type_with_interval() 秒数必须 >= 0",
    )
}

pub(crate) fn action_click_times_positive_message() -> String {
    localized_message("click() times must be >= 1", "click() 次数必须 >= 1")
}

pub(crate) fn launched_browser_only_message(operation: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{operation} 仅适用于已启动的浏览器实例"),
        _ => format!("{operation} is only available for launched browser instances"),
    }
}

pub(crate) fn clipboard_secure_context_required_message(method_name: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("{method_name}() 需要 secure-context 页面并支持 navigator.clipboard")
        }
        _ => format!("{method_name}() requires a secure-context page with navigator.clipboard"),
    }
}

pub(crate) fn permission_origin_required_message() -> String {
    localized_message(
        "permission override requires an http(s) page or an explicit --origin",
        "permission override 需要 http(s) 页面或显式 --origin",
    )
}

pub(crate) fn session_backed_element_driver_target_message(
    element_type: &str,
    target_en: &str,
    target_zh_cn: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => {
            format!("session-backed {element_type} 不支持用于 driver {target_zh_cn}")
        }
        _ => format!(
            "session-backed {element_type} is not supported for driver {target_en} targeting"
        ),
    }
}

pub(crate) fn session_backed_web_element_driver_actions_message() -> String {
    localized_message(
        "session-backed WebElement is not supported for driver actions",
        "session-backed WebElement 不支持用于 driver actions",
    )
}

pub(crate) fn frame_element_missing_frame_id_message() -> String {
    localized_message("frame element has no frame id", "frame 元素没有 frame id")
}

pub(crate) fn resolved_frame_owner_missing_object_id_message() -> String {
    localized_message(
        "resolved frame owner has no object id",
        "解析出的 frame owner 没有 object id",
    )
}

pub(crate) fn action_element_missing_clickable_rect_message() -> String {
    localized_message(
        "element has no clickable rect for actions",
        "元素没有可用于 actions 的可点击位置及大小",
    )
}

pub(crate) fn action_element_missing_rect_location_message() -> String {
    localized_message(
        "element has no rect location for actions",
        "元素没有可用于 actions 的位置",
    )
}

pub(crate) fn set_file_input_requires_at_least_one_file_message() -> String {
    localized_message(
        "set_file_input_files() requires at least one file",
        "set_file_input_files() 至少需要一个文件",
    )
}

pub(crate) fn parent_element_level_must_start_message() -> String {
    localized_message(
        "parent element not found: level must be >= 1",
        "没有找到父元素: level 必须 >= 1",
    )
}

pub(crate) fn parent_element_index_must_start_message() -> String {
    localized_message(
        "parent element not found: index must be >= 1",
        "没有找到父元素: index 必须 >= 1",
    )
}

pub(crate) fn select_element_required_message() -> String {
    localized_message(
        "select() is only available for <select> elements",
        "select() 仅适用于 <select> 元素",
    )
}

pub(crate) fn multi_select_action_required_message(action: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("select.{action}() 仅适用于多选 select 元素"),
        _ => format!("select.{action}() is only available for multi-select elements"),
    }
}

pub(crate) fn relative_direction_index_must_start_message() -> String {
    localized_message(
        "relative direction index must be >= 1",
        "相对方向序号必须 >= 1",
    )
}

pub(crate) fn resolve_frame_viewport_offset_failed_message(detail: &str) -> String {
    localized_error_with_detail(
        "resolve frame viewport offset failed",
        "解析 frame 视口偏移失败",
        detail,
    )
}

pub(crate) fn element_frame_viewport_offset_unavailable_message() -> String {
    localized_message(
        "element frame viewport offset unavailable",
        "元素 frame 视口偏移不可用",
    )
}

pub(crate) fn resolve_element_frame_id_failed_message(detail: &str) -> String {
    localized_error_with_detail(
        "resolve element frame id failed",
        "解析元素 frame id 失败",
        detail,
    )
}

pub(crate) fn element_top_frame_check_failed_message(detail: &str) -> String {
    localized_error_with_detail(
        "element top-frame check failed",
        "元素顶层 frame 检查失败",
        detail,
    )
}

pub(crate) fn frame_index_must_start_message() -> String {
    localized_message(
        "frame index must start from 1 or use negative indices from -1",
        "frame 序号必须从 1 开始，或使用从 -1 开始的负序号",
    )
}

pub(crate) fn frame_index_out_of_range_message(index: isize) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("frame 序号超出范围: {index}"),
        _ => format!("frame index out of range: {index}"),
    }
}

pub(crate) fn component_state_lock_poisoned_message(
    component_en: &str,
    component_zh_cn: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{component_zh_cn}锁已损坏"),
        _ => format!("{component_en} lock poisoned"),
    }
}

pub(crate) fn component_not_running_message(component_en: &str, component_zh_cn: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{component_zh_cn}未运行"),
        _ => format!("{component_en} is not running"),
    }
}

pub(crate) fn component_not_running_with_error_message(
    component_en: &str,
    component_zh_cn: &str,
    error: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{component_zh_cn}未运行: {error}"),
        _ => format!("{component_en} is not running: {error}"),
    }
}

pub(crate) fn component_not_active_start_message(
    component_en: &str,
    component_zh_cn: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{component_zh_cn}未处于活动状态，请先调用 start()"),
        _ => format!("{component_en} is not active; call start() first"),
    }
}

pub(crate) fn component_stopped_while_waiting_message(
    component_en: &str,
    component_zh_cn: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("等待期间{component_zh_cn}已停止"),
        _ => format!("{component_en} stopped while waiting"),
    }
}

pub(crate) fn invalid_regex_message(
    feature_en: &str,
    feature_zh_cn: &str,
    pattern: &str,
    error: &str,
) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("无效的{feature_zh_cn}正则 `{pattern}`: {error}"),
        _ => format!("invalid {feature_en} regex `{pattern}`: {error}"),
    }
}

pub(crate) fn target_tab_not_found_message() -> String {
    localized_message("target tab not found", "没有找到指定标签页")
}

pub(crate) fn no_new_tab_message() -> String {
    localized_message("failed to wait for new tab", "没有等到新标签页")
}

pub(crate) fn invalid_url_message(input: &str, detail: Option<&str>) -> String {
    let prefix = match current_language_code() {
        Some("zh_cn") => format!("无效的 url `{input}`，也许要加上 `http://`？"),
        _ => format!("invalid url `{input}`, maybe add `http://`?"),
    };
    append_optional_detail(prefix, detail)
}

pub(crate) fn invalid_file_url_message(input: &str, detail: Option<&str>) -> String {
    let prefix = match current_language_code() {
        Some("zh_cn") => format!("无效的 file url: {input}"),
        _ => format!("invalid file url: {input}"),
    };
    append_optional_detail(prefix, detail)
}

pub(crate) fn page_connect_timed_out_message(url: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("页面连接超时: {url}"),
        _ => format!("page connect timed out: {url}"),
    }
}

pub(crate) fn frame_html_unavailable_message() -> String {
    localized_message("frame html is unavailable", "frame html 不可用")
}

pub(crate) fn element_html_unavailable_message() -> String {
    localized_message("element html is unavailable", "element html 不可用")
}

pub(crate) fn element_tag_name_unavailable_message() -> String {
    localized_message("element tagName is unavailable", "element tagName 不可用")
}

pub(crate) fn element_resource_unavailable_message() -> String {
    localized_message("element resource is unavailable", "element 资源不可用")
}

pub(crate) fn frame_execution_context_unavailable_message(frame_id: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("frame 执行上下文不可用: {frame_id}"),
        _ => format!("frame execution context is unavailable: {frame_id}"),
    }
}

pub(crate) fn session_page_no_loaded_document_message() -> String {
    localized_message(
        "session page has no loaded document",
        "session 页面还没有已加载文档",
    )
}

pub(crate) fn session_page_no_current_url_message() -> String {
    localized_message(
        "session page has no current url; provide url explicitly",
        "session 页面没有当前 url；请显式传入 url",
    )
}

pub(crate) fn timeout_must_be_non_negative_message(seconds: f64) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("timeout 必须是有限且非负的数字，当前为 {seconds}"),
        _ => format!("timeout must be a finite non-negative number, got {seconds}"),
    }
}

pub(crate) fn upload_requires_at_least_one_file_message() -> String {
    localized_message(
        "upload_files() requires at least one file",
        "upload_files() 至少需要一个文件",
    )
}

pub(crate) fn file_chooser_backend_node_missing_message() -> String {
    localized_message(
        "file chooser did not expose a backend node id",
        "文件选择框没有提供 backend node id",
    )
}

pub(crate) fn screencast_mode_change_while_running_message() -> String {
    localized_message(
        "cannot change screencast mode while recording",
        "录屏进行中，不能切换录屏模式",
    )
}

pub(crate) fn screencast_already_running_message() -> String {
    localized_message("screencast is already running", "录屏已在运行")
}

pub(crate) fn screencast_requires_save_path_message() -> String {
    localized_message(
        "screencast requires a save path; call start(Some(path)) or set_save_path() first",
        "录屏需要保存路径；请先调用 start(Some(path)) 或 set_save_path()",
    )
}

pub(crate) fn screencast_capture_path_unavailable_message() -> String {
    localized_message(
        "screencast capture path is unavailable",
        "录屏捕获路径不可用",
    )
}

pub(crate) fn screencast_output_path_unavailable_message() -> String {
    localized_message(
        "screencast output path is unavailable",
        "录屏输出路径不可用",
    )
}

pub(crate) fn screencast_empty_mime_type_message() -> String {
    localized_message(
        "js screencast returned an empty mime type",
        "JS 录屏返回了空的 mime type",
    )
}

pub(crate) fn screencast_mode_output_suffix_message(mode: &str, suffix: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("录屏模式 {mode} 仅支持 .{suffix} 输出"),
        _ => format!("screencast mode {mode} only supports .{suffix} output"),
    }
}

pub(crate) fn screencast_no_frames_message() -> String {
    localized_message(
        "screencast did not capture any frames",
        "录屏没有捕获到任何帧",
    )
}

pub(crate) fn unsupported_screencast_output_suffix_message(suffix: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("不支持的录屏输出后缀: .{suffix}"),
        _ => format!("unsupported screencast output suffix: .{suffix}"),
    }
}

pub(crate) fn screencast_ffmpeg_spawn_failed_message(detail: &str) -> String {
    localized_error_with_detail(
        "failed to run ffmpeg for screencast",
        "运行 ffmpeg 编码录屏失败",
        detail,
    )
}

pub(crate) fn screencast_ffmpeg_encode_failed_message(status: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("ffmpeg 编码录屏输出失败，状态为 {status}"),
        _ => format!("ffmpeg failed to encode screencast output with status {status}"),
    }
}

pub(crate) fn screencast_encode_output_failed_message(detail: &str) -> String {
    localized_error_with_detail(
        "failed to encode screencast output",
        "编码录屏输出失败",
        detail,
    )
}

pub(crate) fn invalid_screencast_data_url_message() -> String {
    localized_message("invalid screencast data URL", "无效的录屏 data URL")
}

pub(crate) fn screencast_save_path_must_be_directory_message() -> String {
    localized_message(
        "screencast save path must be a directory",
        "录屏保存路径必须是目录",
    )
}

pub(crate) fn shadow_root_object_id_unavailable_message() -> String {
    localized_message(
        "shadow root object id is unavailable",
        "shadow root 的 object id 不可用",
    )
}

pub(crate) fn shadow_root_xpath_traversal_not_implemented_message() -> String {
    localized_message(
        "xpath shadow root traversal is not implemented yet",
        "暂未实现 shadow root 的 xpath 遍历",
    )
}

pub(crate) fn javascript_execution_timed_out_message() -> String {
    localized_message("javascript execution timed out", "JavaScript 执行超时")
}

fn localized_timeout_message(operation: &str, timeout_ms: u64) -> String {
    match current_language_code() {
        Some("zh_cn") => format!("{operation} 等待超时（{timeout_ms} ms）"),
        _ => format!("{operation} timed out after {timeout_ms} ms"),
    }
}

pub(crate) fn localized_message(en: &str, zh_cn: &str) -> String {
    match current_language_code() {
        Some("zh_cn") => zh_cn.to_string(),
        _ => en.to_string(),
    }
}

fn append_optional_detail(prefix: String, detail: Option<&str>) -> String {
    match detail.map(str::trim).filter(|detail| !detail.is_empty()) {
        Some(detail) => format!("{prefix}: {detail}"),
        None => prefix,
    }
}

fn current_language_code() -> Option<&'static str> {
    snapshot()
        .language
        .as_deref()
        .and_then(normalize_language_code)
}

fn default_suffixes_list_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("suffixes.dat")
}

fn normalize_language_code(code: &str) -> Option<&'static str> {
    match code.trim().to_ascii_lowercase().as_str() {
        "zh_cn" | "cn" => Some("zh_cn"),
        "en" => Some("en"),
        _ => None,
    }
}

fn seconds_to_duration(seconds: f64) -> Duration {
    if !seconds.is_finite() || seconds <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(seconds)
    }
}

#[cfg(test)]
pub(crate) struct SettingsTestGuard {
    snapshot: SettingsSnapshot,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for SettingsTestGuard {
    fn drop(&mut self) {
        restore(self.snapshot.clone());
    }
}

#[cfg(test)]
static SETTINGS_TEST_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn scoped_test_settings() -> SettingsTestGuard {
    let lock = SETTINGS_TEST_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    SettingsTestGuard {
        snapshot: snapshot(),
        _lock: lock,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Settings, SettingsSnapshot, action_click_times_positive_message,
        action_element_missing_clickable_rect_message,
        action_element_missing_rect_location_message, action_type_interval_non_negative_message,
        action_wait_seconds_non_negative_message, blob_src_data_url_required_message,
        browser_backed_page_only_message, browser_connect_timeout_duration,
        click_at_count_must_be_positive_message, click_failed_no_rect_message,
        clipboard_secure_context_required_message, component_not_active_start_message,
        component_not_running_message, component_not_running_with_error_message,
        component_state_lock_poisoned_message, component_stopped_while_waiting_message,
        cookie_name_empty_message, default_suffixes_list_path, download_not_found_message,
        drag_in_file_path_empty_message, drag_in_requires_file_path_message,
        driver_mode_only_message, element_frame_viewport_offset_unavailable_message,
        element_html_unavailable_message, element_no_visible_rect_message,
        element_resource_unavailable_message, element_tag_name_unavailable_message,
        element_top_frame_check_failed_message, file_chooser_backend_node_missing_message,
        frame_element_missing_frame_id_message, frame_execution_context_unavailable_message,
        frame_html_unavailable_message, frame_index_must_start_message,
        frame_index_out_of_range_message, invalid_auto_port_scope_message,
        invalid_cookie_same_site_message, invalid_download_file_exists_mode_message,
        invalid_file_url_message, invalid_load_mode_message,
        invalid_options_manager_ini_literal_message, invalid_regex_message,
        invalid_screencast_data_url_message, invalid_tab_index_message, invalid_url_message,
        invalid_xpath_html_message, invalid_xpath_query_message,
        invalid_xpath_segment_index_message, javascript_execution_timed_out_message,
        launched_browser_only_message, multi_select_action_required_message, no_new_tab_message,
        page_connect_timed_out_message, parent_element_index_must_start_message,
        parent_element_level_must_start_message, permission_origin_required_message,
        permission_origin_scheme_message, permission_setting_invalid_message,
        relative_direction_index_must_start_message, resolve_element_frame_id_failed_message,
        resolve_frame_viewport_offset_failed_message,
        resolved_frame_owner_missing_object_id_message, scoped_test_settings,
        screencast_already_running_message, screencast_capture_path_unavailable_message,
        screencast_empty_mime_type_message, screencast_encode_output_failed_message,
        screencast_ffmpeg_encode_failed_message, screencast_ffmpeg_spawn_failed_message,
        screencast_mode_change_while_running_message, screencast_mode_output_suffix_message,
        screencast_no_frames_message, screencast_output_path_unavailable_message,
        screencast_requires_save_path_message, screencast_save_path_must_be_directory_message,
        screenshot_clip_complete_message, screenshot_clip_order_message,
        select_element_required_message, session_backed_element_driver_target_message,
        session_backed_web_element_driver_actions_message, session_page_no_current_url_message,
        session_page_no_loaded_document_message, set_file_input_requires_at_least_one_file_message,
        shadow_root_object_id_unavailable_message,
        shadow_root_xpath_traversal_not_implemented_message, target_tab_not_found_message,
        timeout_error, timeout_must_be_non_negative_message, unsupported_key_message,
        unsupported_mouse_button_message, unsupported_screencast_output_suffix_message,
        unsupported_xpath_path_message, upload_requires_at_least_one_file_message,
        value_coordinate_not_numeric_message, value_coordinate_pair_exactly_two_message,
        value_coordinate_pair_parse_failed_message, value_coordinate_pair_required_message,
        value_did_not_return_message, value_non_negative_integer_required_message,
        value_number_required_message, value_pair_entry_not_number_message,
        value_returned_non_string_entry_message, value_state_bool_required_message,
        value_string_compatible_required_message, value_string_required_message,
        value_string_vec_array_required_message, value_string_vec_entry_required_message,
        value_unavailable_message, web_element_list_driver_filter_message,
        xpath_node_no_longer_exists_message, xpath_path_not_found_message,
        xpath_segment_not_found_message, zoom_factor_must_be_positive_message,
    };
    use crate::error::OpenPageError;
    use std::path::Path;

    #[test]
    fn settings_defaults_match_dp_runtime_defaults() {
        let _guard = scoped_test_settings();
        Settings::reset();
        let default_suffixes = default_suffixes_list_path();

        assert_eq!(
            Settings::snapshot(),
            SettingsSnapshot {
                raise_when_ele_not_found: false,
                raise_when_click_failed: false,
                raise_when_wait_failed: false,
                singleton_tab_obj: true,
                cdp_timeout: 30.0,
                browser_connect_timeout: 30.0,
                auto_handle_alert: None,
                language: None,
                suffixes_list: Some(default_suffixes.clone()),
            }
        );
        assert!(
            default_suffixes.is_file(),
            "bundled suffix list should exist"
        );
        assert_eq!(browser_connect_timeout_duration().as_secs_f64(), 30.0);
    }

    #[test]
    fn settings_setters_update_global_snapshot() {
        let _guard = scoped_test_settings();
        Settings::reset();

        Settings::set_raise_when_ele_not_found(true);
        Settings::set_raise_when_click_failed(true);
        Settings::set_raise_when_wait_failed(true);
        Settings::set_singleton_tab_obj(false);
        Settings::set_cdp_timeout(1.5);
        Settings::set_browser_connect_timeout(2.5);
        Settings::set_auto_handle_alert(Some(false));
        Settings::set_language("en");
        Settings::set_suffixes_list("/tmp/suffixes.dat");

        let snapshot = Settings::snapshot();
        assert!(snapshot.raise_when_ele_not_found);
        assert!(snapshot.raise_when_click_failed);
        assert!(snapshot.raise_when_wait_failed);
        assert!(!snapshot.singleton_tab_obj);
        assert_eq!(snapshot.cdp_timeout, 1.5);
        assert_eq!(snapshot.browser_connect_timeout, 2.5);
        assert_eq!(snapshot.auto_handle_alert, Some(false));
        assert_eq!(snapshot.language.as_deref(), Some("en"));
        assert_eq!(
            snapshot.suffixes_list.as_deref(),
            Some(Path::new("/tmp/suffixes.dat"))
        );
    }

    #[test]
    fn settings_language_localizes_core_error_messages() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english_timeout = timeout_error("Page::wait_for_doc_loaded()", 120);
        assert!(
            matches!(english_timeout, OpenPageError::Timeout(ref message) if message.contains("timed out after 120 ms"))
        );
        assert_eq!(
            click_failed_no_rect_message(),
            "simulated click failed because element has no rect"
        );
        assert_eq!(
            element_no_visible_rect_message(),
            "element does not have a visible rect"
        );
        assert_eq!(
            OpenPageError::BrowserOperation("detail".to_string()).to_string(),
            "browser operation failed: detail"
        );
        assert_eq!(
            OpenPageError::Http(cookie_name_empty_message()).to_string(),
            "http operation failed: cookie name cannot be empty"
        );
        assert_eq!(
            invalid_auto_port_scope_message(0, 9601),
            "auto_port scope must satisfy 0 < start < end, got (0, 9601)"
        );
        assert_eq!(
            invalid_cookie_same_site_message("Broken", "sid"),
            "invalid cookie same_site `Broken` for `sid`"
        );
        assert_eq!(
            invalid_url_message("example.test", Some("detail")),
            "invalid url `example.test`, maybe add `http://`?: detail"
        );
        assert_eq!(
            invalid_file_url_message("file://example.com/path", None),
            "invalid file url: file://example.com/path"
        );
        assert_eq!(
            invalid_tab_index_message(),
            "tab index must start from 1 or use negative indices from -1"
        );
        assert_eq!(
            invalid_download_file_exists_mode_message("bad"),
            "download file-exists mode must be one of rename/overwrite/skip, got bad"
        );
        assert_eq!(
            invalid_load_mode_message("fast"),
            "load mode must be one of normal/eager/none, got fast"
        );
        assert_eq!(
            invalid_options_manager_ini_literal_message("parse error"),
            "invalid options manager ini literal: parse error"
        );
        assert_eq!(
            invalid_xpath_html_message("parse error"),
            "invalid xpath html: parse error"
        );
        assert_eq!(
            invalid_xpath_query_message("//[", "parse error"),
            "invalid xpath `//[`: parse error"
        );
        assert_eq!(
            xpath_node_no_longer_exists_message(),
            "xpath node no longer exists"
        );
        assert_eq!(
            invalid_xpath_segment_index_message("div"),
            "invalid xpath segment index for `div`"
        );
        assert_eq!(
            xpath_segment_not_found_message("section", 2),
            "xpath segment `section[2]` not found"
        );
        assert_eq!(
            xpath_path_not_found_message("/html/body/main[1]"),
            "xpath path `/html/body/main[1]` not found"
        );
        assert_eq!(
            unsupported_xpath_path_message("broken"),
            "unsupported xpath path `broken`"
        );
        assert_eq!(unsupported_key_message("Hyper"), "unsupported key: Hyper");
        assert_eq!(
            driver_mode_only_message("get_frame()"),
            "get_frame() is only available in driver mode"
        );
        assert_eq!(
            web_element_list_driver_filter_message("displayed()"),
            "displayed() filters are only available for driver-backed WebElement lists"
        );
        assert_eq!(
            browser_backed_page_only_message("retry_times()"),
            "retry_times() is only available on browser-backed pages"
        );
        assert_eq!(
            zoom_factor_must_be_positive_message(0.0),
            "zoom factor must be a positive finite number: 0"
        );
        assert_eq!(
            permission_setting_invalid_message("maybe"),
            "permission setting must be one of granted/denied/prompt, got maybe"
        );
        assert_eq!(
            permission_origin_scheme_message("ftp://example.test"),
            "permission origin must use http or https, got ftp://example.test"
        );
        assert_eq!(
            drag_in_requires_file_path_message(),
            "drag_in() requires at least one file path"
        );
        assert_eq!(
            drag_in_file_path_empty_message(),
            "drag_in() file path must not be empty"
        );
        assert_eq!(
            screenshot_clip_order_message(),
            "screenshot clip requires right_bottom to be greater than left_top"
        );
        assert_eq!(
            screenshot_clip_complete_message(),
            "screenshot clip requires both left_top and right_bottom"
        );
        assert_eq!(
            value_did_not_return_message("demo", "a string", "字符串", "null"),
            "demo did not return a string: null"
        );
        assert_eq!(
            value_returned_non_string_entry_message("demo", "1"),
            "demo returned a non-string entry: 1"
        );
        assert_eq!(
            value_pair_entry_not_number_message("demo", "first"),
            "demo first entry is not a number"
        );
        assert_eq!(
            value_state_bool_required_message("visible", "null"),
            "visible state script did not return a bool: null"
        );
        assert_eq!(
            value_coordinate_pair_parse_failed_message("rect", "parse error"),
            "failed to parse rect coordinate pair: parse error"
        );
        assert_eq!(
            value_coordinate_pair_required_message("rect"),
            "rect did not return a coordinate pair"
        );
        assert_eq!(
            value_coordinate_pair_exactly_two_message("rect"),
            "rect did not return exactly two coordinates"
        );
        assert_eq!(
            value_coordinate_not_numeric_message("rect", "x", "null"),
            "rect x coordinate was not numeric: null"
        );
        assert_eq!(
            value_string_compatible_required_message("attr", "[]"),
            "attr did not return a string-compatible value: []"
        );
        assert_eq!(
            value_non_negative_integer_required_message("child_count", "-1"),
            "child_count did not return a non-negative integer: -1"
        );
        assert_eq!(
            value_number_required_message("child_count", "null"),
            "child_count did not return a number: null"
        );
        assert_eq!(value_unavailable_message("tag"), "tag is unavailable");
        assert_eq!(
            value_string_required_message("tag", "true"),
            "tag did not return a string: true"
        );
        assert_eq!(
            value_string_vec_entry_required_message("comments", "[]"),
            "comments contained a non-string-compatible value: []"
        );
        assert_eq!(
            value_string_vec_array_required_message("comments", "null"),
            "comments script did not return an array: null"
        );
        assert_eq!(
            blob_src_data_url_required_message("null"),
            "blob src did not return a data URL string: null"
        );
        assert_eq!(
            action_wait_seconds_non_negative_message(),
            "wait() seconds must be >= 0"
        );
        assert_eq!(
            action_type_interval_non_negative_message(),
            "type_with_interval() seconds must be >= 0"
        );
        assert_eq!(
            action_click_times_positive_message(),
            "click() times must be >= 1"
        );
        assert_eq!(
            launched_browser_only_message("window hide()"),
            "window hide() is only available for launched browser instances"
        );
        assert_eq!(
            clipboard_secure_context_required_message("clipboard_read_text"),
            "clipboard_read_text() requires a secure-context page with navigator.clipboard"
        );
        assert_eq!(
            permission_origin_required_message(),
            "permission override requires an http(s) page or an explicit --origin"
        );
        assert_eq!(
            session_backed_element_driver_target_message(
                "WebElement",
                "page frame",
                "页面 frame 定位"
            ),
            "session-backed WebElement is not supported for driver page frame targeting"
        );
        assert_eq!(
            session_backed_web_element_driver_actions_message(),
            "session-backed WebElement is not supported for driver actions"
        );
        assert_eq!(
            frame_element_missing_frame_id_message(),
            "frame element has no frame id"
        );
        assert_eq!(
            resolved_frame_owner_missing_object_id_message(),
            "resolved frame owner has no object id"
        );
        assert_eq!(
            action_element_missing_clickable_rect_message(),
            "element has no clickable rect for actions"
        );
        assert_eq!(
            action_element_missing_rect_location_message(),
            "element has no rect location for actions"
        );
        assert_eq!(
            set_file_input_requires_at_least_one_file_message(),
            "set_file_input_files() requires at least one file"
        );
        assert_eq!(
            parent_element_level_must_start_message(),
            "parent element not found: level must be >= 1"
        );
        assert_eq!(
            parent_element_index_must_start_message(),
            "parent element not found: index must be >= 1"
        );
        assert_eq!(
            select_element_required_message(),
            "select() is only available for <select> elements"
        );
        assert_eq!(
            multi_select_action_required_message("clear"),
            "select.clear() is only available for multi-select elements"
        );
        assert_eq!(
            relative_direction_index_must_start_message(),
            "relative direction index must be >= 1"
        );
        assert_eq!(
            resolve_frame_viewport_offset_failed_message("detail"),
            "resolve frame viewport offset failed: detail"
        );
        assert_eq!(
            element_frame_viewport_offset_unavailable_message(),
            "element frame viewport offset unavailable"
        );
        assert_eq!(
            resolve_element_frame_id_failed_message("detail"),
            "resolve element frame id failed: detail"
        );
        assert_eq!(
            element_top_frame_check_failed_message("detail"),
            "element top-frame check failed: detail"
        );
        assert_eq!(
            unsupported_mouse_button_message("side"),
            "unsupported mouse button: side"
        );
        assert_eq!(
            click_at_count_must_be_positive_message(),
            "click_at() count must be >= 1"
        );
        assert_eq!(
            download_not_found_message("abc"),
            "download `abc` was not found"
        );
        assert_eq!(
            component_state_lock_poisoned_message("console state", "控制台状态"),
            "console state lock poisoned"
        );
        assert_eq!(
            component_not_running_message("console", "控制台"),
            "console is not running"
        );
        assert_eq!(
            component_not_running_with_error_message("listener", "监听器", "boom"),
            "listener is not running: boom"
        );
        assert_eq!(
            component_not_active_start_message("interceptor", "拦截器"),
            "interceptor is not active; call start() first"
        );
        assert_eq!(
            component_stopped_while_waiting_message("console", "控制台"),
            "console stopped while waiting"
        );
        assert_eq!(
            invalid_regex_message("listener", "监听规则", "(", "regex parse error"),
            "invalid listener regex `(`: regex parse error"
        );
        assert_eq!(target_tab_not_found_message(), "target tab not found");
        assert_eq!(no_new_tab_message(), "failed to wait for new tab");
        assert_eq!(
            page_connect_timed_out_message("https://example.test/"),
            "page connect timed out: https://example.test/"
        );
        assert_eq!(
            timeout_must_be_non_negative_message(-0.5),
            "timeout must be a finite non-negative number, got -0.5"
        );
        assert_eq!(
            upload_requires_at_least_one_file_message(),
            "upload_files() requires at least one file"
        );
        assert_eq!(
            file_chooser_backend_node_missing_message(),
            "file chooser did not expose a backend node id"
        );
        assert_eq!(
            screencast_mode_change_while_running_message(),
            "cannot change screencast mode while recording"
        );
        assert_eq!(
            screencast_already_running_message(),
            "screencast is already running"
        );
        assert_eq!(
            screencast_requires_save_path_message(),
            "screencast requires a save path; call start(Some(path)) or set_save_path() first"
        );
        assert_eq!(
            screencast_capture_path_unavailable_message(),
            "screencast capture path is unavailable"
        );
        assert_eq!(
            screencast_output_path_unavailable_message(),
            "screencast output path is unavailable"
        );
        assert_eq!(
            screencast_empty_mime_type_message(),
            "js screencast returned an empty mime type"
        );
        assert_eq!(
            screencast_mode_output_suffix_message("Video", "mp4"),
            "screencast mode Video only supports .mp4 output"
        );
        assert_eq!(
            screencast_no_frames_message(),
            "screencast did not capture any frames"
        );
        assert_eq!(
            unsupported_screencast_output_suffix_message("avi"),
            "unsupported screencast output suffix: .avi"
        );
        assert_eq!(
            screencast_ffmpeg_spawn_failed_message("boom"),
            "failed to run ffmpeg for screencast: boom"
        );
        assert_eq!(
            screencast_ffmpeg_encode_failed_message("exit status: 1"),
            "ffmpeg failed to encode screencast output with status exit status: 1"
        );
        assert_eq!(
            screencast_encode_output_failed_message("image error"),
            "failed to encode screencast output: image error"
        );
        assert_eq!(
            invalid_screencast_data_url_message(),
            "invalid screencast data URL"
        );
        assert_eq!(
            screencast_save_path_must_be_directory_message(),
            "screencast save path must be a directory"
        );
        assert_eq!(
            shadow_root_object_id_unavailable_message(),
            "shadow root object id is unavailable"
        );
        assert_eq!(
            shadow_root_xpath_traversal_not_implemented_message(),
            "xpath shadow root traversal is not implemented yet"
        );
        assert_eq!(
            javascript_execution_timed_out_message(),
            "javascript execution timed out"
        );

        Settings::set_language("cn");

        let chinese_timeout = timeout_error("Page::wait_for_doc_loaded()", 120);
        assert!(
            matches!(chinese_timeout, OpenPageError::Timeout(ref message) if message.contains("等待超时"))
        );
        assert_eq!(
            click_failed_no_rect_message(),
            "模拟点击失败，因为元素没有位置及大小"
        );
        assert_eq!(element_no_visible_rect_message(), "元素没有可见位置及大小");
        assert_eq!(
            OpenPageError::BrowserOperation("detail".to_string()).to_string(),
            "浏览器操作失败: detail"
        );
        assert_eq!(
            OpenPageError::Http(cookie_name_empty_message()).to_string(),
            "HTTP 操作失败: cookie 名称不能为空"
        );
        assert_eq!(
            invalid_auto_port_scope_message(0, 9601),
            "auto_port 范围必须满足 0 < start < end，当前为 (0, 9601)"
        );
        assert_eq!(
            invalid_cookie_same_site_message("Broken", "sid"),
            "cookie `sid` 的 same_site `Broken` 无效"
        );
        assert_eq!(
            invalid_url_message("example.test", Some("detail")),
            "无效的 url `example.test`，也许要加上 `http://`？: detail"
        );
        assert_eq!(
            invalid_file_url_message("file://example.com/path", None),
            "无效的 file url: file://example.com/path"
        );
        assert_eq!(
            invalid_tab_index_message(),
            "标签页序号必须从 1 开始，或使用从 -1 开始的负序号"
        );
        assert_eq!(
            invalid_download_file_exists_mode_message("bad"),
            "下载文件已存在策略必须是 rename/overwrite/skip 之一，当前为 bad"
        );
        assert_eq!(
            invalid_load_mode_message("fast"),
            "加载模式必须是 normal/eager/none 之一，当前为 fast"
        );
        assert_eq!(
            invalid_options_manager_ini_literal_message("parse error"),
            "无效的 OptionsManager ini 字面量: parse error"
        );
        assert_eq!(
            invalid_xpath_html_message("parse error"),
            "无效的 xpath HTML: parse error"
        );
        assert_eq!(
            invalid_xpath_query_message("//[", "parse error"),
            "无效的 xpath `//[`: parse error"
        );
        assert_eq!(xpath_node_no_longer_exists_message(), "xpath 节点已不存在");
        assert_eq!(
            invalid_xpath_segment_index_message("div"),
            "xpath 片段 `div` 的序号无效"
        );
        assert_eq!(
            xpath_segment_not_found_message("section", 2),
            "没有找到 xpath 片段 `section[2]`"
        );
        assert_eq!(
            xpath_path_not_found_message("/html/body/main[1]"),
            "没有找到 xpath 路径 `/html/body/main[1]`"
        );
        assert_eq!(
            unsupported_xpath_path_message("broken"),
            "不支持的 xpath 路径 `broken`"
        );
        assert_eq!(unsupported_key_message("Hyper"), "不支持的按键: Hyper");
        assert_eq!(
            driver_mode_only_message("get_frame()"),
            "get_frame() 仅在 driver 模式可用"
        );
        assert_eq!(
            web_element_list_driver_filter_message("displayed()"),
            "displayed() 过滤器仅适用于 driver-backed WebElement 列表"
        );
        assert_eq!(
            browser_backed_page_only_message("retry_times()"),
            "retry_times() 仅适用于 browser-backed 页面"
        );
        assert_eq!(
            zoom_factor_must_be_positive_message(0.0),
            "zoom factor 必须是有限正数，当前为 0"
        );
        assert_eq!(
            permission_setting_invalid_message("maybe"),
            "permission setting 必须是 granted/denied/prompt 之一，当前为 maybe"
        );
        assert_eq!(
            permission_origin_scheme_message("ftp://example.test"),
            "permission origin 必须使用 http 或 https，当前为 ftp://example.test"
        );
        assert_eq!(
            drag_in_requires_file_path_message(),
            "drag_in() 至少需要一个文件路径"
        );
        assert_eq!(
            drag_in_file_path_empty_message(),
            "drag_in() 文件路径不能为空"
        );
        assert_eq!(
            screenshot_clip_order_message(),
            "截图裁剪要求 right_bottom 大于 left_top"
        );
        assert_eq!(
            screenshot_clip_complete_message(),
            "截图裁剪需要同时提供 left_top 和 right_bottom"
        );
        assert_eq!(
            value_did_not_return_message("demo", "a string", "字符串", "null"),
            "demo 未返回字符串: null"
        );
        assert_eq!(
            value_returned_non_string_entry_message("demo", "1"),
            "demo 返回了非字符串条目: 1"
        );
        assert_eq!(
            value_pair_entry_not_number_message("demo", "first"),
            "demo first 条目不是数字"
        );
        assert_eq!(
            value_state_bool_required_message("visible", "null"),
            "visible 状态脚本未返回布尔值: null"
        );
        assert_eq!(
            value_coordinate_pair_parse_failed_message("rect", "parse error"),
            "解析 rect 坐标对失败: parse error"
        );
        assert_eq!(
            value_coordinate_pair_required_message("rect"),
            "rect 未返回坐标对"
        );
        assert_eq!(
            value_coordinate_pair_exactly_two_message("rect"),
            "rect 未返回恰好两个坐标"
        );
        assert_eq!(
            value_coordinate_not_numeric_message("rect", "x", "null"),
            "rect x 坐标不是数字: null"
        );
        assert_eq!(
            value_string_compatible_required_message("attr", "[]"),
            "attr 未返回可转为字符串的值: []"
        );
        assert_eq!(
            value_non_negative_integer_required_message("child_count", "-1"),
            "child_count 未返回非负整数: -1"
        );
        assert_eq!(
            value_number_required_message("child_count", "null"),
            "child_count 未返回数字: null"
        );
        assert_eq!(value_unavailable_message("tag"), "tag 不可用");
        assert_eq!(
            value_string_required_message("tag", "true"),
            "tag 未返回字符串: true"
        );
        assert_eq!(
            value_string_vec_entry_required_message("comments", "[]"),
            "comments 包含不可转为字符串的值: []"
        );
        assert_eq!(
            value_string_vec_array_required_message("comments", "null"),
            "comments 脚本未返回数组: null"
        );
        assert_eq!(
            blob_src_data_url_required_message("null"),
            "blob src 未返回 data URL 字符串: null"
        );
        assert_eq!(
            action_wait_seconds_non_negative_message(),
            "wait() 秒数必须 >= 0"
        );
        assert_eq!(
            action_type_interval_non_negative_message(),
            "type_with_interval() 秒数必须 >= 0"
        );
        assert_eq!(
            action_click_times_positive_message(),
            "click() 次数必须 >= 1"
        );
        assert_eq!(
            launched_browser_only_message("window hide()"),
            "window hide() 仅适用于已启动的浏览器实例"
        );
        assert_eq!(
            clipboard_secure_context_required_message("clipboard_read_text"),
            "clipboard_read_text() 需要 secure-context 页面并支持 navigator.clipboard"
        );
        assert_eq!(
            permission_origin_required_message(),
            "permission override 需要 http(s) 页面或显式 --origin"
        );
        assert_eq!(
            session_backed_element_driver_target_message(
                "WebElement",
                "page frame",
                "页面 frame 定位"
            ),
            "session-backed WebElement 不支持用于 driver 页面 frame 定位"
        );
        assert_eq!(
            session_backed_web_element_driver_actions_message(),
            "session-backed WebElement 不支持用于 driver actions"
        );
        assert_eq!(
            frame_element_missing_frame_id_message(),
            "frame 元素没有 frame id"
        );
        assert_eq!(
            resolved_frame_owner_missing_object_id_message(),
            "解析出的 frame owner 没有 object id"
        );
        assert_eq!(
            action_element_missing_clickable_rect_message(),
            "元素没有可用于 actions 的可点击位置及大小"
        );
        assert_eq!(
            action_element_missing_rect_location_message(),
            "元素没有可用于 actions 的位置"
        );
        assert_eq!(
            set_file_input_requires_at_least_one_file_message(),
            "set_file_input_files() 至少需要一个文件"
        );
        assert_eq!(
            parent_element_level_must_start_message(),
            "没有找到父元素: level 必须 >= 1"
        );
        assert_eq!(
            parent_element_index_must_start_message(),
            "没有找到父元素: index 必须 >= 1"
        );
        assert_eq!(
            select_element_required_message(),
            "select() 仅适用于 <select> 元素"
        );
        assert_eq!(
            multi_select_action_required_message("clear"),
            "select.clear() 仅适用于多选 select 元素"
        );
        assert_eq!(
            relative_direction_index_must_start_message(),
            "相对方向序号必须 >= 1"
        );
        assert_eq!(
            resolve_frame_viewport_offset_failed_message("detail"),
            "解析 frame 视口偏移失败: detail"
        );
        assert_eq!(
            element_frame_viewport_offset_unavailable_message(),
            "元素 frame 视口偏移不可用"
        );
        assert_eq!(
            resolve_element_frame_id_failed_message("detail"),
            "解析元素 frame id 失败: detail"
        );
        assert_eq!(
            element_top_frame_check_failed_message("detail"),
            "元素顶层 frame 检查失败: detail"
        );
        assert_eq!(
            unsupported_mouse_button_message("side"),
            "不支持的鼠标按钮: side"
        );
        assert_eq!(
            click_at_count_must_be_positive_message(),
            "click_at() 次数必须大于等于 1"
        );
        assert_eq!(download_not_found_message("abc"), "没有找到下载任务 `abc`");
        assert_eq!(
            component_state_lock_poisoned_message("console state", "控制台状态"),
            "控制台状态锁已损坏"
        );
        assert_eq!(
            component_not_running_message("console", "控制台"),
            "控制台未运行"
        );
        assert_eq!(
            component_not_running_with_error_message("listener", "监听器", "boom"),
            "监听器未运行: boom"
        );
        assert_eq!(
            component_not_active_start_message("interceptor", "拦截器"),
            "拦截器未处于活动状态，请先调用 start()"
        );
        assert_eq!(
            component_stopped_while_waiting_message("console", "控制台"),
            "等待期间控制台已停止"
        );
        assert_eq!(
            invalid_regex_message("listener", "监听规则", "(", "regex parse error"),
            "无效的监听规则正则 `(`: regex parse error"
        );
        assert_eq!(target_tab_not_found_message(), "没有找到指定标签页");
        assert_eq!(no_new_tab_message(), "没有等到新标签页");
        assert_eq!(
            page_connect_timed_out_message("https://example.test/"),
            "页面连接超时: https://example.test/"
        );
        assert_eq!(
            timeout_must_be_non_negative_message(-0.5),
            "timeout 必须是有限且非负的数字，当前为 -0.5"
        );
        assert_eq!(
            upload_requires_at_least_one_file_message(),
            "upload_files() 至少需要一个文件"
        );
        assert_eq!(
            file_chooser_backend_node_missing_message(),
            "文件选择框没有提供 backend node id"
        );
        assert_eq!(
            screencast_mode_change_while_running_message(),
            "录屏进行中，不能切换录屏模式"
        );
        assert_eq!(screencast_already_running_message(), "录屏已在运行");
        assert_eq!(
            screencast_requires_save_path_message(),
            "录屏需要保存路径；请先调用 start(Some(path)) 或 set_save_path()"
        );
        assert_eq!(
            screencast_capture_path_unavailable_message(),
            "录屏捕获路径不可用"
        );
        assert_eq!(
            screencast_output_path_unavailable_message(),
            "录屏输出路径不可用"
        );
        assert_eq!(
            screencast_empty_mime_type_message(),
            "JS 录屏返回了空的 mime type"
        );
        assert_eq!(
            screencast_mode_output_suffix_message("Video", "mp4"),
            "录屏模式 Video 仅支持 .mp4 输出"
        );
        assert_eq!(screencast_no_frames_message(), "录屏没有捕获到任何帧");
        assert_eq!(
            unsupported_screencast_output_suffix_message("avi"),
            "不支持的录屏输出后缀: .avi"
        );
        assert_eq!(
            screencast_ffmpeg_spawn_failed_message("boom"),
            "运行 ffmpeg 编码录屏失败: boom"
        );
        assert_eq!(
            screencast_ffmpeg_encode_failed_message("exit status: 1"),
            "ffmpeg 编码录屏输出失败，状态为 exit status: 1"
        );
        assert_eq!(
            screencast_encode_output_failed_message("image error"),
            "编码录屏输出失败: image error"
        );
        assert_eq!(invalid_screencast_data_url_message(), "无效的录屏 data URL");
        assert_eq!(
            screencast_save_path_must_be_directory_message(),
            "录屏保存路径必须是目录"
        );
        assert_eq!(
            shadow_root_object_id_unavailable_message(),
            "shadow root 的 object id 不可用"
        );
        assert_eq!(
            shadow_root_xpath_traversal_not_implemented_message(),
            "暂未实现 shadow root 的 xpath 遍历"
        );
        assert_eq!(
            javascript_execution_timed_out_message(),
            "JavaScript 执行超时"
        );
    }

    #[test]
    fn settings_language_localizes_additional_runtime_messages() {
        let _guard = scoped_test_settings();
        Settings::reset();

        assert_eq!(
            frame_index_must_start_message(),
            "frame index must start from 1 or use negative indices from -1"
        );
        assert_eq!(
            frame_index_out_of_range_message(3),
            "frame index out of range: 3"
        );
        assert_eq!(
            frame_html_unavailable_message(),
            "frame html is unavailable"
        );
        assert_eq!(
            element_html_unavailable_message(),
            "element html is unavailable"
        );
        assert_eq!(
            element_tag_name_unavailable_message(),
            "element tagName is unavailable"
        );
        assert_eq!(
            element_resource_unavailable_message(),
            "element resource is unavailable"
        );
        assert_eq!(
            frame_execution_context_unavailable_message("frame-1"),
            "frame execution context is unavailable: frame-1"
        );
        assert_eq!(
            session_page_no_loaded_document_message(),
            "session page has no loaded document"
        );
        assert_eq!(
            session_page_no_current_url_message(),
            "session page has no current url; provide url explicitly"
        );

        Settings::set_language("cn");

        assert_eq!(
            frame_index_must_start_message(),
            "frame 序号必须从 1 开始，或使用从 -1 开始的负序号"
        );
        assert_eq!(frame_index_out_of_range_message(3), "frame 序号超出范围: 3");
        assert_eq!(frame_html_unavailable_message(), "frame html 不可用");
        assert_eq!(element_html_unavailable_message(), "element html 不可用");
        assert_eq!(
            element_tag_name_unavailable_message(),
            "element tagName 不可用"
        );
        assert_eq!(element_resource_unavailable_message(), "element 资源不可用");
        assert_eq!(
            frame_execution_context_unavailable_message("frame-1"),
            "frame 执行上下文不可用: frame-1"
        );
        assert_eq!(
            session_page_no_loaded_document_message(),
            "session 页面还没有已加载文档"
        );
        assert_eq!(
            session_page_no_current_url_message(),
            "session 页面没有当前 url；请显式传入 url"
        );
    }
}
