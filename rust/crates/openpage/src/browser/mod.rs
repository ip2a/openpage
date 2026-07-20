use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

mod launch;
mod operations;
use chromiumoxide::Command;
use chromiumoxide::browser::{Browser as OxBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    CancelDownloadParams, PermissionDescriptor, PermissionSetting, ResetPermissionsParams,
    SetDownloadBehaviorBehavior, SetDownloadBehaviorParams, SetPermissionParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ClearBrowserCookiesParams, CookieParam, DeleteCookiesParams, SetCookiesParams,
};
use chromiumoxide::cdp::browser_protocol::target::{
    ActivateTargetParams, CloseTargetParams, CreateBrowserContextParams, CreateTargetParams,
    EventTargetCreated, EventTargetDestroyed, GetTargetsParams, TargetId,
};
use futures::StreamExt;
use launch::*;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::timeout as tokio_timeout;
use url::Url;

use crate::download::{
    DownloadInfo, DownloadMission, DownloadState, DownloadStore, attach_download_store,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::Page;
use crate::settings::{
    browser_command_failed_message, browser_config_path_failed_message,
    browser_connect_timeout_duration, browser_launch_operation_failed_message,
    browser_setup_operation_failed_message, browser_temp_dir_create_failed_message,
    browser_user_data_dir_reset_failed_message, cdp_timeout_duration,
    component_state_lock_poisoned_message, download_canceled_message,
    download_did_not_complete_in_time_message, download_directory_create_failed_message,
    download_file_operation_failed_message, download_frame_not_mapped_to_tab_message,
    download_path_not_configured_message, download_skipped_without_final_path_message,
    invalid_auto_port_scope_message, invalid_download_file_exists_mode_message,
    invalid_launch_options_ini_boolean_message, invalid_launch_options_ini_field_expected_message,
    invalid_launch_options_ini_field_message, invalid_launch_options_ini_python_string_message,
    invalid_load_mode_message, invalid_tab_index_message, invalid_url_message,
    no_free_port_in_auto_port_scope_message, singleton_tab_obj_enabled,
    target_tab_not_found_message, timeout_duration_millis, timeout_error,
    unterminated_launch_options_ini_python_string_message, wait_failed_should_raise,
};
use crate::webpage::WebPage;

fn browser_newest_tab_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser newest tab",
        "浏览器最新标签页",
    ))
}

fn browser_timeouts_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser timeouts",
        "浏览器超时设置",
    ))
}

fn browser_retry_times_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser retry times",
        "浏览器重试次数",
    ))
}

fn browser_retry_interval_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser retry interval",
        "浏览器重试间隔",
    ))
}

fn browser_download_path_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser download path",
        "浏览器下载路径",
    ))
}

fn browser_download_file_exists_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser download file-exists",
        "浏览器下载文件存在策略",
    ))
}

fn browser_download_naming_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser download naming",
        "浏览器下载命名设置",
    ))
}

fn browser_load_mode_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "browser load mode",
        "浏览器加载模式",
    ))
}

fn page_download_settings_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "page download settings",
        "页面下载设置",
    ))
}

fn mission_download_settings_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "mission download settings",
        "下载任务设置",
    ))
}

fn isolated_context_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "isolated context",
        "隔离上下文",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadFileExistsMode {
    Rename,
    Overwrite,
    Skip,
}

impl DownloadFileExistsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::Overwrite => "overwrite",
            Self::Skip => "skip",
        }
    }

    pub fn parse(value: &str) -> OpenPageResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rename" | "r" => Ok(Self::Rename),
            "overwrite" | "o" => Ok(Self::Overwrite),
            "skip" | "s" => Ok(Self::Skip),
            _ => Err(OpenPageError::BrowserOperation(
                invalid_download_file_exists_mode_message(value),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadMode {
    #[default]
    Normal,
    Eager,
    None,
}

impl LoadMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Eager => "eager",
            Self::None => "none",
        }
    }

    pub fn parse(value: &str) -> OpenPageResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" | "n" => Ok(Self::Normal),
            "eager" | "e" => Ok(Self::Eager),
            "none" => Ok(Self::None),
            _ => Err(OpenPageError::BrowserOperation(invalid_load_mode_message(
                value,
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub page_load: u64,
    pub script: u64,
    pub implicit_wait: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            page_load: 30000,
            script: 30000,
            implicit_wait: 10000,
        }
    }
}

const DEFAULT_AUTO_PORT_SCOPE: (u16, u16) = (9600, 59_600);
pub const OPENPAGE_BROWSER_PATH_ENV: &str = "OPENPAGE_BROWSER_PATH";

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub browser_path: Option<PathBuf>,
    pub download_path: Option<PathBuf>,
    pub download_file_exists: DownloadFileExistsMode,
    pub load_mode: LoadMode,
    pub retry_times: usize,
    pub retry_interval_millis: u64,
    pub user_data_dir: Option<PathBuf>,
    pub remote_debugging_port: Option<u16>,
    pub address: Option<String>,
    pub ws_address: Option<String>,
    pub headless: bool,
    pub width: u32,
    pub height: u32,
    pub no_sandbox: bool,
    /// Arbitrary Chrome command-line arguments.
    pub args: Vec<String>,
    pub incognito: bool,
    pub ignore_https_errors: bool,
    pub extensions: Vec<PathBuf>,
    pub disable_default_args: bool,
    pub proxy: Option<String>,
    pub mute: bool,
    pub no_js: bool,
    pub no_imgs: bool,
    pub user_agent: Option<String>,
    /// Temporary directory path (replaces system temp dir for browser internals).
    pub tmp_path: Option<PathBuf>,
    /// Disk cache directory path.
    pub cache_path: Option<PathBuf>,
    /// Automatically find an available port for remote debugging.
    pub auto_port: bool,
    /// Optional inclusive-exclusive port range used by auto_port.
    pub auto_port_scope: Option<(u16, u16)>,
    /// Only connect to an existing browser; fail if none is found.
    pub existing_only: bool,
    /// Whether to use the system-installed browser profile directory.
    pub system_user_path: bool,
    /// Whether to reset the target user-data directory before launching.
    pub new_env: bool,
    /// Timeouts configuration.
    pub timeouts: TimeoutConfig,
    /// Chrome preferences (written to Preferences file on launch).
    pub prefs: HashMap<String, serde_json::Value>,
    /// Preference keys removed from the on-disk Preferences file on launch.
    pub prefs_to_remove: Vec<String>,
    /// Whether to clear on-disk flags from Local State before applying current flags.
    pub clear_file_flags: bool,
    /// Chrome experiments/flags (written to Local State file on launch).
    pub flags: Vec<String>,
    /// Source ini path remembered from `from_ini()`, used by `save(None)`.
    pub source_ini_path: Option<PathBuf>,
}

impl LaunchOptions {
    pub fn new(read_file: bool, ini_path: Option<&Path>) -> OpenPageResult<Self> {
        Self::from_ini_options(read_file, ini_path)
    }

    pub fn from_ini_options(read_file: bool, ini_path: Option<&Path>) -> OpenPageResult<Self> {
        if read_file {
            Self::from_ini(ini_path)
        } else {
            built_in_launch_options_defaults()
        }
    }

    pub fn address(&self) -> String {
        resolved_launch_options_address(self)
    }

    pub fn remote_debugging_port(&self) -> Option<u16> {
        self.remote_debugging_port
    }

    pub fn ws_address(&self) -> Option<&str> {
        self.ws_address.as_deref()
    }

    pub fn user(&self) -> &str {
        chrome_profile_directory(&self.args)
    }

    pub fn timeouts(&self) -> HashMap<&'static str, f64> {
        HashMap::from([
            ("base", millis_to_seconds_f64(self.timeouts.implicit_wait)),
            ("page_load", millis_to_seconds_f64(self.timeouts.page_load)),
            ("script", millis_to_seconds_f64(self.timeouts.script)),
        ])
    }

    pub fn retry_interval(&self) -> f64 {
        millis_to_seconds_f64(self.retry_interval_millis)
    }

    pub fn proxy(&self) -> Option<&str> {
        self.proxy.as_deref()
    }

    pub fn arguments(&self) -> &[String] {
        &self.args
    }

    pub fn extensions(&self) -> &[PathBuf] {
        &self.extensions
    }

    pub fn preferences(&self) -> &HashMap<String, serde_json::Value> {
        &self.prefs
    }

    pub fn preferences_to_remove(&self) -> &[String] {
        &self.prefs_to_remove
    }

    pub fn flags(&self) -> &[String] {
        &self.flags
    }

    pub fn source_ini_path(&self) -> Option<&Path> {
        self.source_ini_path.as_deref()
    }

    pub fn system_user_path(&self) -> bool {
        self.system_user_path
    }

    pub fn is_existing_only(&self) -> bool {
        self.existing_only
    }

    pub fn is_auto_port(&self) -> bool {
        self.auto_port
    }

    pub fn auto_port_scope(&self) -> Option<(u16, u16)> {
        if self.auto_port {
            Some(self.auto_port_scope.unwrap_or(DEFAULT_AUTO_PORT_SCOPE))
        } else {
            None
        }
    }

    pub fn is_headless(&self) -> bool {
        self.headless
    }

    pub fn window_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_no_sandbox(&self) -> bool {
        self.no_sandbox
    }

    pub fn retry_times(&self) -> usize {
        self.retry_times
    }

    pub fn browser_path(&self) -> String {
        option_path_string(self.browser_path.as_deref())
    }

    pub fn user_data_path(&self) -> String {
        option_path_string(self.user_data_dir.as_deref())
    }

    pub fn tmp_path(&self) -> String {
        option_path_string(self.tmp_path.as_deref())
    }

    pub fn cache_path(&self) -> String {
        option_path_string(self.cache_path.as_deref())
    }

    pub fn download_path(&self) -> String {
        option_path_string(self.download_path.as_deref())
    }

    pub fn load_mode(&self) -> &'static str {
        self.load_mode.as_str()
    }

    pub fn download_file_exists(&self) -> &'static str {
        self.download_file_exists.as_str()
    }

    pub fn download_file_exists_mode(&self) -> DownloadFileExistsMode {
        self.download_file_exists
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn is_incognito(&self) -> bool {
        self.incognito
    }

    pub fn is_ignore_certificate_errors(&self) -> bool {
        self.ignore_https_errors
    }

    pub fn is_disable_default_args(&self) -> bool {
        self.disable_default_args
    }

    pub fn is_no_imgs(&self) -> bool {
        self.no_imgs
    }

    pub fn is_no_js(&self) -> bool {
        self.no_js
    }

    pub fn is_mute(&self) -> bool {
        self.mute
    }

    pub fn is_new_env(&self) -> bool {
        self.new_env
    }

    pub fn is_clear_file_flags(&self) -> bool {
        self.clear_file_flags
    }

    pub fn set_retry(
        &mut self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> &mut Self {
        if let Some(retry_times) = retry_times {
            self.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            self.retry_interval_millis = retry_interval_millis;
        }
        self
    }

    pub fn set_retry_seconds(
        &mut self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> &mut Self {
        self.set_retry(retry_times, retry_interval_secs.map(seconds_to_millis))
    }

    pub fn set_timeouts(
        &mut self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> &mut Self {
        if let Some(base_secs) = base_secs {
            self.timeouts.implicit_wait = seconds_to_millis(base_secs);
        }
        if let Some(page_load_secs) = page_load_secs {
            self.timeouts.page_load = seconds_to_millis(page_load_secs);
        }
        if let Some(script_secs) = script_secs {
            self.timeouts.script = seconds_to_millis(script_secs);
        }
        self
    }

    pub fn set_load_mode(&mut self, value: &str) -> OpenPageResult<&mut Self> {
        self.load_mode = LoadMode::parse(value)?;
        Ok(self)
    }

    pub fn set_download_file_exists_mode(&mut self, mode: DownloadFileExistsMode) -> &mut Self {
        self.download_file_exists = mode;
        self
    }

    pub fn when_download_file_exists(&mut self, mode: &str) -> OpenPageResult<&mut Self> {
        self.download_file_exists = DownloadFileExistsMode::parse(mode)?;
        Ok(self)
    }

    pub fn set_browser_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.browser_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn set_download_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.download_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn set_tmp_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.tmp_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn set_cache_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.cache_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn set_proxy(&mut self, proxy: impl Into<String>) -> &mut Self {
        self.proxy = Some(proxy.into());
        self
    }

    pub fn set_user_agent(&mut self, user_agent: impl Into<String>) -> &mut Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn ignore_certificate_errors(&mut self, on_off: bool) -> &mut Self {
        self.ignore_https_errors = on_off;
        self
    }

    pub fn incognito(&mut self, on_off: bool) -> &mut Self {
        self.incognito = on_off;
        self
    }

    pub fn headless(&mut self, on_off: bool) -> &mut Self {
        self.headless = on_off;
        self
    }

    pub fn no_imgs(&mut self, on_off: bool) -> &mut Self {
        self.no_imgs = on_off;
        self
    }

    pub fn no_js(&mut self, on_off: bool) -> &mut Self {
        self.no_js = on_off;
        self
    }

    pub fn mute(&mut self, on_off: bool) -> &mut Self {
        self.mute = on_off;
        self
    }

    pub fn existing_only(&mut self, on_off: bool) -> &mut Self {
        self.existing_only = on_off;
        self
    }

    pub fn auto_port(&mut self, on_off: bool) -> &mut Self {
        self.auto_port = on_off;
        if on_off {
            self.auto_port_scope = Some(DEFAULT_AUTO_PORT_SCOPE);
            self.remote_debugging_port = None;
            self.address = None;
            self.ws_address = None;
            self.user_data_dir = None;
            self.system_user_path = false;
        } else {
            self.auto_port_scope = None;
        }
        self
    }

    pub fn auto_port_with_scope(
        &mut self,
        on_off: bool,
        scope: Option<(u16, u16)>,
    ) -> OpenPageResult<&mut Self> {
        if on_off {
            let scope = scope.unwrap_or(DEFAULT_AUTO_PORT_SCOPE);
            validate_auto_port_scope(scope)?;
            self.auto_port = true;
            self.auto_port_scope = Some(scope);
            self.remote_debugging_port = None;
            self.address = None;
            self.ws_address = None;
            self.user_data_dir = None;
            self.system_user_path = false;
        } else {
            self.auto_port(false);
        }
        Ok(self)
    }

    pub fn set_user_data_path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.user_data_dir = Some(path.as_ref().to_path_buf());
        self.system_user_path = false;
        self.auto_port = false;
        self.auto_port_scope = None;
        self
    }

    pub fn set_local_port(&mut self, port: u16) -> &mut Self {
        self.remote_debugging_port = Some(port);
        self.address = Some(format!("127.0.0.1:{port}"));
        self.ws_address = None;
        self.auto_port = false;
        self.auto_port_scope = None;
        self
    }

    pub fn set_address(&mut self, address: &str) -> &mut Self {
        let (address, ws_address, local_port) = normalize_debugger_address(address);
        self.address = Some(address);
        self.ws_address = ws_address;
        self.remote_debugging_port = local_port;
        self.auto_port = false;
        self.auto_port_scope = None;
        self
    }

    pub fn new_env(&mut self, on_off: bool) -> &mut Self {
        self.new_env = on_off;
        self
    }

    pub fn use_system_user_path(&mut self, on_off: bool) -> &mut Self {
        let system_path = system_user_data_dir();
        self.system_user_path = on_off;
        if on_off {
            self.user_data_dir = system_path;
        } else if let (Some(current), Some(system_path)) =
            (self.user_data_dir.as_ref(), system_path.as_ref())
        {
            if current == system_path {
                self.user_data_dir = None;
            }
        }
        self
    }

    pub fn save(&self, path: Option<&Path>) -> OpenPageResult<PathBuf> {
        let path = resolve_launch_options_ini_path(path.or(self.source_ini_path.as_deref()))?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let template = load_launch_options_ini_template(&path, self.source_ini_path.as_deref());
        std::fs::write(
            &path,
            serialize_launch_options_ini(self, template.as_deref()),
        )?;
        Ok(path)
    }

    pub fn save_to_default(&self) -> OpenPageResult<PathBuf> {
        let path = default_launch_options_ini_path();
        self.save(Some(path.as_path()))
    }

    pub fn from_ini(path: Option<&Path>) -> OpenPageResult<Self> {
        let path = resolve_launch_options_ini_path(path)?;
        let content = std::fs::read_to_string(&path)?;
        let mut options = parse_launch_options_ini(&content)?;
        options.source_ini_path = Some(path);
        Ok(options)
    }

    pub fn set_argument(&mut self, arg: impl Into<String>) -> &mut Self {
        let arg = arg.into();
        if !self.args.contains(&arg) {
            self.args.push(arg);
        }
        self
    }

    pub fn set_argument_value(&mut self, arg: &str, value: Option<&str>) -> &mut Self {
        self.remove_argument(arg);
        match value {
            Some(value) => self.set_argument(format!("{arg}={value}")),
            None => self.set_argument(arg),
        }
    }

    pub fn set_user(&mut self, user: &str) -> &mut Self {
        self.remove_argument("--profile-directory");
        self.set_argument(format!("--profile-directory={user}"));
        self
    }

    pub fn remove_argument(&mut self, arg: &str) -> &mut Self {
        self.args
            .retain(|a| a != arg && !a.starts_with(&format!("{arg}=")));
        self
    }

    pub fn clear_arguments(&mut self) -> &mut Self {
        self.args.clear();
        self
    }

    pub fn add_extension(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.extensions.push(path.as_ref().to_path_buf());
        self
    }

    pub fn remove_extensions(&mut self) -> &mut Self {
        self.extensions.clear();
        self
    }

    pub fn set_pref(&mut self, key: impl Into<String>, value: serde_json::Value) -> &mut Self {
        self.prefs.insert(key.into(), value);
        self
    }

    pub fn remove_pref(&mut self, key: &str) -> &mut Self {
        self.prefs.remove(key);
        self
    }

    pub fn remove_pref_from_file(&mut self, key: impl Into<String>) -> &mut Self {
        self.prefs_to_remove.push(key.into());
        self
    }

    pub fn clear_prefs(&mut self) -> &mut Self {
        self.prefs.clear();
        self
    }

    pub fn set_flag(&mut self, flag: impl Into<String>) -> &mut Self {
        let flag = flag.into();
        if !self.flags.contains(&flag) {
            self.flags.push(flag);
        }
        self
    }

    pub fn set_flag_value(&mut self, flag: &str, value: Option<&str>) -> &mut Self {
        self.flags
            .retain(|item| item != flag && !item.starts_with(&format!("{flag}@")));
        match value {
            Some(value) => self.set_flag(format!("{flag}@{value}")),
            None => self.set_flag(flag),
        }
    }

    pub fn clear_flags(&mut self) -> &mut Self {
        self.flags.clear();
        self
    }

    pub fn clear_flags_in_file(&mut self) -> &mut Self {
        self.clear_file_flags = true;
        self
    }
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            browser_path: None,
            download_path: None,
            download_file_exists: DownloadFileExistsMode::Rename,
            load_mode: LoadMode::Normal,
            retry_times: 3,
            retry_interval_millis: 2_000,
            user_data_dir: None,
            remote_debugging_port: None,
            address: None,
            ws_address: None,
            headless: false,
            width: 1280,
            height: 900,
            no_sandbox: false,
            args: Vec::new(),
            incognito: false,
            ignore_https_errors: false,
            extensions: Vec::new(),
            disable_default_args: false,
            proxy: None,
            mute: false,
            no_js: false,
            no_imgs: false,
            user_agent: None,
            tmp_path: None,
            cache_path: None,
            auto_port: false,
            auto_port_scope: None,
            existing_only: false,
            system_user_path: false,
            new_env: false,
            timeouts: TimeoutConfig::default(),
            prefs: HashMap::new(),
            prefs_to_remove: Vec::new(),
            clear_file_flags: false,
            flags: Vec::new(),
            source_ini_path: None,
        }
    }
}

#[derive(Debug)]
struct BrowserState {
    runtime: Arc<Runtime>,
    browser: Mutex<OxBrowser>,
    debugger_address: String,
    browser_pid: Option<u32>,
    downloads: DownloadStore,
    newest_tab_id: Arc<StdMutex<Option<String>>>,
    download_path: StdMutex<Option<PathBuf>>,
    download_file_exists: StdMutex<DownloadFileExistsMode>,
    browser_download_naming: StdMutex<BrowserDownloadNaming>,
    load_mode: StdMutex<LoadMode>,
    page_cache: StdMutex<HashMap<String, Page>>,
    isolated_contexts: StdMutex<HashMap<String, String>>,
    page_download_settings: StdMutex<HashMap<String, PageDownloadSettings>>,
    mission_download_settings: StdMutex<HashMap<String, ResolvedDownloadSettings>>,
    download_spool_dir: PathBuf,
    temp_user_data_dir: Option<PathBuf>,
    temp_download_dir: Option<PathBuf>,
    headless: bool,
    timeouts: StdMutex<TimeoutConfig>,
    retry_times: StdMutex<usize>,
    retry_interval_millis: StdMutex<u64>,
    _download_task: tokio::task::JoinHandle<()>,
    _handler_task: tokio::task::JoinHandle<()>,
    _target_created_task: tokio::task::JoinHandle<()>,
    _target_destroyed_task: tokio::task::JoinHandle<()>,
}

impl Drop for BrowserState {
    fn drop(&mut self) {
        self._download_task.abort();
        self._handler_task.abort();
        self._target_created_task.abort();
        self._target_destroyed_task.abort();
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PageDownloadSettings {
    pub(crate) path: Option<PathBuf>,
    pub(crate) file_exists: Option<DownloadFileExistsMode>,
    pub(crate) rename: Option<String>,
    pub(crate) suffix: Option<Option<String>>,
}

#[derive(Debug, Clone)]
struct ResolvedDownloadSettings {
    path: PathBuf,
    file_exists: DownloadFileExistsMode,
    rename: Option<String>,
    suffix: Option<Option<String>>,
}

#[derive(Debug, Clone, Default)]
struct BrowserDownloadNaming {
    rename: Option<String>,
    suffix: Option<Option<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct BrowserDownloadSettingsSnapshot {
    pub(crate) path: Option<PathBuf>,
    pub(crate) file_exists: DownloadFileExistsMode,
    pub(crate) rename: Option<String>,
    pub(crate) suffix: Option<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct Browser {
    inner: Arc<BrowserState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub target_id: String,
    pub tab_type: String,
    pub title: String,
    pub url: String,
    pub attached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserPageUrlInput<'a> {
    None,
    Url(Cow<'a, str>),
}

impl<'a> From<&'a str> for BrowserPageUrlInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Url(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for BrowserPageUrlInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Url(Cow::Borrowed(value.as_str()))
    }
}

impl From<String> for BrowserPageUrlInput<'_> {
    fn from(value: String) -> Self {
        Self::Url(Cow::Owned(value))
    }
}

impl<'a> From<Option<&'a str>> for BrowserPageUrlInput<'a> {
    fn from(value: Option<&'a str>) -> Self {
        match value {
            Some(url) => Self::Url(Cow::Borrowed(url)),
            None => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserTabSelector<'a> {
    Id(Cow<'a, str>),
    Index(isize),
}

impl<'a> From<&'a str> for BrowserTabSelector<'a> {
    fn from(value: &'a str) -> Self {
        Self::Id(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for BrowserTabSelector<'a> {
    fn from(value: &'a String) -> Self {
        Self::Id(Cow::Borrowed(value.as_str()))
    }
}

impl From<isize> for BrowserTabSelector<'_> {
    fn from(value: isize) -> Self {
        Self::Index(value)
    }
}

impl From<i32> for BrowserTabSelector<'_> {
    fn from(value: i32) -> Self {
        Self::Index(value as isize)
    }
}

impl From<i64> for BrowserTabSelector<'_> {
    fn from(value: i64) -> Self {
        Self::Index(value as isize)
    }
}

impl From<usize> for BrowserTabSelector<'_> {
    fn from(value: usize) -> Self {
        Self::Index(value as isize)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserTabTypeInput<'a> {
    Single(Cow<'a, str>),
    Many(Vec<Cow<'a, str>>),
}

impl<'a> From<&'a str> for BrowserTabTypeInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Single(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for BrowserTabTypeInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Single(Cow::Borrowed(value.as_str()))
    }
}

impl<'a> From<&'a [&'a str]> for BrowserTabTypeInput<'a> {
    fn from(value: &'a [&'a str]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| Cow::Borrowed(*item))
                .collect::<Vec<_>>(),
        )
    }
}

impl<'a> From<&'a Vec<&'a str>> for BrowserTabTypeInput<'a> {
    fn from(value: &'a Vec<&'a str>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a [String]> for BrowserTabTypeInput<'a> {
    fn from(value: &'a [String]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| Cow::Borrowed(item.as_str()))
                .collect::<Vec<_>>(),
        )
    }
}

impl<'a> From<&'a Vec<String>> for BrowserTabTypeInput<'a> {
    fn from(value: &'a Vec<String>) -> Self {
        Self::from(value.as_slice())
    }
}

#[derive(Clone, Debug)]
pub enum BrowserTabReference {
    Page(Page),
    WebPage(WebPage),
    Id(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserTabTargetsInput<'a> {
    Single(BrowserTabSelector<'a>),
    Many(Vec<BrowserTabSelector<'a>>),
}

impl<'a> From<BrowserTabSelector<'a>> for BrowserTabTargetsInput<'a> {
    fn from(value: BrowserTabSelector<'a>) -> Self {
        Self::Single(value)
    }
}

impl<'a> From<&'a str> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl<'a> From<&'a String> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<usize> for BrowserTabTargetsInput<'_> {
    fn from(value: usize) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<isize> for BrowserTabTargetsInput<'_> {
    fn from(value: isize) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<i32> for BrowserTabTargetsInput<'_> {
    fn from(value: i32) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl From<i64> for BrowserTabTargetsInput<'_> {
    fn from(value: i64) -> Self {
        Self::Single(BrowserTabSelector::from(value))
    }
}

impl<'a> From<&'a [BrowserTabSelector<'a>]> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a [BrowserTabSelector<'a>]) -> Self {
        Self::Many(value.to_vec())
    }
}

impl<'a> From<&'a Vec<BrowserTabSelector<'a>>> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Vec<BrowserTabSelector<'a>>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a [&'a str]> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a [&'a str]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| BrowserTabSelector::from(*item))
                .collect(),
        )
    }
}

impl<'a> From<&'a Vec<&'a str>> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Vec<&'a str>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a [String]> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a [String]) -> Self {
        Self::Many(value.iter().map(BrowserTabSelector::from).collect())
    }
}

impl<'a> From<&'a Vec<String>> for BrowserTabTargetsInput<'a> {
    fn from(value: &'a Vec<String>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<&[usize]> for BrowserTabTargetsInput<'_> {
    fn from(value: &[usize]) -> Self {
        Self::Many(
            value
                .iter()
                .copied()
                .map(BrowserTabSelector::from)
                .collect(),
        )
    }
}

impl From<&Vec<usize>> for BrowserTabTargetsInput<'_> {
    fn from(value: &Vec<usize>) -> Self {
        Self::from(value.as_slice())
    }
}

impl From<&[isize]> for BrowserTabTargetsInput<'_> {
    fn from(value: &[isize]) -> Self {
        Self::Many(
            value
                .iter()
                .copied()
                .map(BrowserTabSelector::from)
                .collect(),
        )
    }
}

impl From<&Vec<isize>> for BrowserTabTargetsInput<'_> {
    fn from(value: &Vec<isize>) -> Self {
        Self::from(value.as_slice())
    }
}

fn attach_newest_tab_tracker(
    runtime: &Arc<Runtime>,
    browser: &OxBrowser,
    newest_tab_id: Arc<StdMutex<Option<String>>>,
) -> OpenPageResult<(tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>)> {
    let (mut created_events, mut destroyed_events) = runtime.block_on(async {
        let created_events = run_browser_future_with_cdp_timeout(
            browser.event_listener::<EventTargetCreated>(),
            "Browser::attach_newest_tab_tracker().register_target_created_listener()",
        )
        .await?;
        let destroyed_events = run_browser_future_with_cdp_timeout(
            browser.event_listener::<EventTargetDestroyed>(),
            "Browser::attach_newest_tab_tracker().register_target_destroyed_listener()",
        )
        .await?;
        Ok::<_, OpenPageError>((created_events, destroyed_events))
    })?;

    let created_newest_tab_id = Arc::clone(&newest_tab_id);
    let target_created_task = runtime.spawn(async move {
        while let Some(event) = created_events.next().await {
            let target_info = &event.target_info;
            if is_tab_like_type(&target_info.r#type) && !target_info.url.starts_with("devtools://")
            {
                if let Ok(mut newest_tab_id) = created_newest_tab_id.lock() {
                    *newest_tab_id = Some(target_info.target_id.as_ref().to_string());
                }
            }
        }
    });

    let target_destroyed_task = runtime.spawn(async move {
        while let Some(event) = destroyed_events.next().await {
            if let Ok(mut tracked_newest) = newest_tab_id.lock()
                && tracked_newest.as_deref() == Some(event.target_id.as_ref())
            {
                *tracked_newest = None;
            }
        }
    });

    Ok((target_created_task, target_destroyed_task))
}

fn resolve_newest_tab_id(
    current_ids: &[String],
    tracked_newest_tab_id: Option<String>,
) -> Option<String> {
    if let Some(tracked_newest_tab_id) = tracked_newest_tab_id
        && current_ids
            .iter()
            .any(|target_id| target_id == &tracked_newest_tab_id)
    {
        return Some(tracked_newest_tab_id);
    }

    current_ids.first().cloned()
}

fn move_newest_tab_info_to_front(infos: &mut Vec<TabInfo>, tracked_newest_tab_id: Option<String>) {
    let current_ids = infos
        .iter()
        .map(|info| info.target_id.clone())
        .collect::<Vec<_>>();
    let Some(newest_tab_id) = resolve_newest_tab_id(&current_ids, tracked_newest_tab_id) else {
        return;
    };

    if let Some(index) = infos
        .iter()
        .position(|info| info.target_id == newest_tab_id)
        && index > 0
    {
        let newest = infos.remove(index);
        infos.insert(0, newest);
    }
}
