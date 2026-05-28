use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use chromiumoxide::browser::{Browser as OxBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    CancelDownloadParams, SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ClearBrowserCookiesParams, CookieParam, DeleteCookiesParams, SetCookiesParams,
};
use chromiumoxide::cdp::browser_protocol::target::{
    ActivateTargetParams, CloseTargetParams, CreateTargetParams, GetTargetsParams, TargetId,
};
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use url::Url;

use crate::download::{
    DownloadInfo, DownloadMission, DownloadState, DownloadStore, attach_download_store,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::Page;

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
            _ => Err(OpenPageError::BrowserOperation(format!(
                "download file-exists mode must be one of rename/overwrite/skip, got {value}"
            ))),
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
            _ => Err(OpenPageError::BrowserOperation(format!(
                "load mode must be one of normal/eager/none, got {value}"
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

    pub fn download_path(&self) -> String {
        option_path_string(self.download_path.as_deref())
    }

    pub fn load_mode(&self) -> &'static str {
        self.load_mode.as_str()
    }

    pub fn set_retry(&mut self, retry_times: Option<usize>, retry_interval_millis: Option<u64>) {
        if let Some(retry_times) = retry_times {
            self.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            self.retry_interval_millis = retry_interval_millis;
        }
    }

    pub fn set_timeouts(
        &mut self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) {
        if let Some(base_secs) = base_secs {
            self.timeouts.implicit_wait = seconds_to_millis(base_secs);
        }
        if let Some(page_load_secs) = page_load_secs {
            self.timeouts.page_load = seconds_to_millis(page_load_secs);
        }
        if let Some(script_secs) = script_secs {
            self.timeouts.script = seconds_to_millis(script_secs);
        }
    }

    pub fn set_load_mode(&mut self, value: &str) -> OpenPageResult<()> {
        self.load_mode = LoadMode::parse(value)?;
        Ok(())
    }

    pub fn set_browser_path(&mut self, path: impl AsRef<Path>) {
        self.browser_path = Some(path.as_ref().to_path_buf());
    }

    pub fn set_download_path(&mut self, path: impl AsRef<Path>) {
        self.download_path = Some(path.as_ref().to_path_buf());
    }

    pub fn set_tmp_path(&mut self, path: impl AsRef<Path>) {
        self.tmp_path = Some(path.as_ref().to_path_buf());
    }

    pub fn set_cache_path(&mut self, path: impl AsRef<Path>) {
        self.cache_path = Some(path.as_ref().to_path_buf());
    }

    pub fn set_proxy(&mut self, proxy: impl Into<String>) {
        self.proxy = Some(proxy.into());
    }

    pub fn set_user_agent(&mut self, user_agent: impl Into<String>) {
        self.user_agent = Some(user_agent.into());
    }

    pub fn ignore_certificate_errors(&mut self, on_off: bool) {
        self.ignore_https_errors = on_off;
    }

    pub fn incognito(&mut self, on_off: bool) {
        self.incognito = on_off;
    }

    pub fn headless(&mut self, on_off: bool) {
        self.headless = on_off;
    }

    pub fn no_imgs(&mut self, on_off: bool) {
        self.no_imgs = on_off;
    }

    pub fn no_js(&mut self, on_off: bool) {
        self.no_js = on_off;
    }

    pub fn mute(&mut self, on_off: bool) {
        self.mute = on_off;
    }

    pub fn existing_only(&mut self, on_off: bool) {
        self.existing_only = on_off;
    }

    pub fn auto_port(&mut self, on_off: bool) {
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
    }

    pub fn auto_port_with_scope(
        &mut self,
        on_off: bool,
        scope: Option<(u16, u16)>,
    ) -> OpenPageResult<()> {
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
        Ok(())
    }

    pub fn set_user_data_path(&mut self, path: impl AsRef<Path>) {
        self.user_data_dir = Some(path.as_ref().to_path_buf());
        self.system_user_path = false;
        self.auto_port = false;
        self.auto_port_scope = None;
    }

    pub fn set_local_port(&mut self, port: u16) {
        self.remote_debugging_port = Some(port);
        self.address = Some(format!("127.0.0.1:{port}"));
        self.ws_address = None;
        self.auto_port = false;
        self.auto_port_scope = None;
    }

    pub fn set_address(&mut self, address: &str) {
        let (address, ws_address, local_port) = normalize_debugger_address(address);
        self.address = Some(address);
        self.ws_address = ws_address;
        self.remote_debugging_port = local_port;
        self.auto_port = false;
        self.auto_port_scope = None;
    }

    pub fn new_env(&mut self, on_off: bool) {
        self.new_env = on_off;
    }

    pub fn use_system_user_path(&mut self, on_off: bool) {
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

    pub fn set_argument(&mut self, arg: impl Into<String>) {
        let arg = arg.into();
        if !self.args.contains(&arg) {
            self.args.push(arg);
        }
    }

    pub fn set_user(&mut self, user: &str) {
        self.remove_argument("--profile-directory");
        self.set_argument(format!("--profile-directory={user}"));
    }

    pub fn remove_argument(&mut self, arg: &str) {
        self.args
            .retain(|a| a != arg && !a.starts_with(&format!("{arg}=")));
    }

    pub fn clear_arguments(&mut self) {
        self.args.clear();
    }

    pub fn add_extension(&mut self, path: impl AsRef<Path>) {
        self.extensions.push(path.as_ref().to_path_buf());
    }

    pub fn remove_extensions(&mut self) {
        self.extensions.clear();
    }

    pub fn set_pref(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.prefs.insert(key.into(), value);
    }

    pub fn remove_pref(&mut self, key: &str) {
        self.prefs.remove(key);
    }

    pub fn remove_pref_from_file(&mut self, key: impl Into<String>) {
        self.prefs_to_remove.push(key.into());
    }

    pub fn clear_prefs(&mut self) {
        self.prefs.clear();
    }

    pub fn set_flag(&mut self, flag: impl Into<String>) {
        let flag = flag.into();
        if !self.flags.contains(&flag) {
            self.flags.push(flag);
        }
    }

    pub fn clear_flags(&mut self) {
        self.flags.clear();
    }

    pub fn clear_flags_in_file(&mut self) {
        self.clear_file_flags = true;
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
    browser_pid: Option<u32>,
    downloads: DownloadStore,
    download_path: StdMutex<Option<PathBuf>>,
    download_file_exists: StdMutex<DownloadFileExistsMode>,
    browser_download_naming: StdMutex<BrowserDownloadNaming>,
    load_mode: StdMutex<LoadMode>,
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

impl Browser {
    pub fn launch(options: LaunchOptions) -> OpenPageResult<Self> {
        let runtime =
            Arc::new(Runtime::new().map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?);

        let mut options = options;

        if options.auto_port && options.remote_debugging_port.is_none() {
            options.remote_debugging_port = Some(find_free_port(options.auto_port_scope())?);
        }

        if let Some(ws_address) = options.ws_address.as_deref() {
            return Self::connect(ws_address);
        }

        if let Some(address) = options.address.as_deref() {
            if options.existing_only
                || !is_local_debugger_address(address)
                || debugger_address_port(address).is_none()
                || local_debugger_address_is_open(address)
            {
                let debugger_url = format!("http://{address}");
                return Self::connect(&debugger_url);
            }
        }

        if options.existing_only {
            let port = options.remote_debugging_port.unwrap_or(9222);
            let debugger_url = format!("http://127.0.0.1:{port}");
            return Self::connect(&debugger_url);
        }

        let (resolved_user_data_dir, use_temp_user_data_dir) =
            resolve_launch_user_data_dir(&options)?;
        if options.new_env {
            if let Some(user_data_dir) = resolved_user_data_dir.as_deref() {
                reset_browser_user_data_dir(user_data_dir)?;
            }
        }
        let base_tmp = options.tmp_path.as_deref();
        let download_spool_dir = make_temp_download_dir(base_tmp)?;
        if let Some(user_data_dir) = &resolved_user_data_dir {
            if !options.prefs.is_empty() || !options.prefs_to_remove.is_empty() {
                write_chrome_prefs(
                    user_data_dir,
                    &options.args,
                    &options.prefs,
                    &options.prefs_to_remove,
                )?;
            }
            if options.clear_file_flags || !options.flags.is_empty() {
                write_chrome_flags(user_data_dir, &options.flags, options.clear_file_flags)?;
            }
        }
        let config = build_browser_config(&options, resolved_user_data_dir.as_deref())?;
        let configured_download_path = options.download_path.clone();
        let (mut browser, mut handler) = runtime
            .block_on(async move { OxBrowser::launch(config).await })
            .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;

        let handler_task = runtime.spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    eprintln!("openpage handler error: {err}");
                }
            }
        });
        let (downloads, download_task) = attach_download_store(Arc::clone(&runtime), &browser)?;
        configure_download_behavior(&runtime, &browser, &download_spool_dir)?;
        let launched_browser_pid = browser_pid(&mut browser);

        let browser = Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                browser_pid: launched_browser_pid,
                downloads,
                download_path: StdMutex::new(configured_download_path.clone()),
                download_file_exists: StdMutex::new(options.download_file_exists),
                browser_download_naming: StdMutex::new(BrowserDownloadNaming::default()),
                load_mode: StdMutex::new(options.load_mode),
                page_download_settings: StdMutex::new(HashMap::new()),
                mission_download_settings: StdMutex::new(HashMap::new()),
                download_spool_dir: download_spool_dir.clone(),
                temp_user_data_dir: if use_temp_user_data_dir {
                    resolved_user_data_dir
                } else {
                    None
                },
                temp_download_dir: Some(download_spool_dir),
                headless: options.headless,
                timeouts: StdMutex::new(options.timeouts),
                retry_times: StdMutex::new(options.retry_times),
                retry_interval_millis: StdMutex::new(options.retry_interval_millis),
                _download_task: download_task,
                _handler_task: handler_task,
            }),
        };

        if let Some(path) = configured_download_path {
            browser.set_download_path(path)?;
        }

        Ok(browser)
    }

    pub fn connect(debugger_url: &str) -> OpenPageResult<Self> {
        let runtime =
            Arc::new(Runtime::new().map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?);
        let download_spool_dir = make_temp_download_dir(None)?;
        let url = debugger_url.to_string();
        let (mut browser, mut handler) = runtime
            .block_on(async move { OxBrowser::connect(url).await })
            .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;

        let handler_task = runtime.spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    eprintln!("openpage handler error: {err}");
                }
            }
        });

        runtime.block_on(async {
            browser
                .fetch_targets()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;

        let (downloads, download_task) = attach_download_store(Arc::clone(&runtime), &browser)?;
        configure_download_behavior(&runtime, &browser, &download_spool_dir)?;

        Ok(Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                browser_pid: None,
                downloads,
                download_path: StdMutex::new(None),
                download_file_exists: StdMutex::new(DownloadFileExistsMode::Rename),
                browser_download_naming: StdMutex::new(BrowserDownloadNaming::default()),
                load_mode: StdMutex::new(LoadMode::Normal),
                page_download_settings: StdMutex::new(HashMap::new()),
                mission_download_settings: StdMutex::new(HashMap::new()),
                download_spool_dir: download_spool_dir.clone(),
                temp_user_data_dir: None,
                temp_download_dir: Some(download_spool_dir),
                headless: false,
                timeouts: StdMutex::new(TimeoutConfig::default()),
                retry_times: StdMutex::new(3),
                retry_interval_millis: StdMutex::new(2_000),
                _download_task: download_task,
                _handler_task: handler_task,
            }),
        })
    }

    pub fn new_page(&self, url: Option<&str>) -> OpenPageResult<Page> {
        let target_url = url.unwrap_or("about:blank").to_string();
        let runtime = Arc::clone(&self.inner.runtime);
        let load_mode = self.load_mode_value()?;
        let page = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .new_page(target_url)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;

        Ok(Page::new_with_load_mode(runtime, page, load_mode)
            .with_browser(self.clone())
            .with_browser_pid(self.inner.browser_pid))
    }

    pub fn new_tab(
        &self,
        url: Option<&str>,
        new_window: bool,
        background: bool,
    ) -> OpenPageResult<Page> {
        let params = CreateTargetParams::builder()
            .url(url.unwrap_or("about:blank"))
            .new_window(new_window)
            .background(background)
            .build()
            .map_err(OpenPageError::BrowserOperation)?;
        let runtime = Arc::clone(&self.inner.runtime);
        let load_mode = self.load_mode_value()?;
        let page = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .new_page(params)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;
        Ok(Page::new_with_load_mode(runtime, page, load_mode)
            .with_browser(self.clone())
            .with_browser_pid(self.inner.browser_pid))
    }

    pub fn pages(&self) -> OpenPageResult<Vec<Page>> {
        let runtime = Arc::clone(&self.inner.runtime);
        let pages = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .pages()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;

        Ok(pages
            .into_iter()
            .map(|page| {
                Page::new(Arc::clone(&runtime), page)
                    .with_browser(self.clone())
                    .with_browser_pid(self.inner.browser_pid)
            })
            .collect())
    }

    pub fn get_page(&self, target_id: &str) -> OpenPageResult<Page> {
        let runtime = Arc::clone(&self.inner.runtime);
        let load_mode = self.load_mode_value()?;
        let page = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .get_page(TargetId::new(target_id))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;
        Ok(Page::new_with_load_mode(runtime, page, load_mode)
            .with_browser(self.clone())
            .with_browser_pid(self.inner.browser_pid))
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        Ok(self.pages()?.len())
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        Ok(self
            .pages()?
            .into_iter()
            .map(|page| page.target_id())
            .collect())
    }

    pub fn tab_infos(&self) -> OpenPageResult<Vec<TabInfo>> {
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            let targets = browser
                .execute(GetTargetsParams::default())
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(targets
                .result
                .target_infos
                .into_iter()
                .filter(|target| is_tab_like_type(&target.r#type))
                .map(|target| TabInfo {
                    target_id: target.target_id.as_ref().to_string(),
                    tab_type: target.r#type,
                    title: target.title,
                    url: target.url,
                    attached: target.attached,
                })
                .collect())
        })
    }

    pub fn latest_tab(&self) -> OpenPageResult<Option<Page>> {
        let Some(target_id) = self.tab_infos()?.last().map(|info| info.target_id.clone()) else {
            return Ok(None);
        };
        self.get_page(&target_id).map(Some)
    }

    pub fn activate_tab(&self, target_id: &str) -> OpenPageResult<()> {
        let params = ActivateTargetParams::new(TargetId::new(target_id));
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(params)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn close_tabs(&self, target_ids: &[String], others: bool) -> OpenPageResult<usize> {
        let closing_ids = if others {
            let keep = target_ids
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            self.tab_infos()?
                .into_iter()
                .map(|info| info.target_id)
                .filter(|target_id| !keep.contains(target_id))
                .collect::<Vec<_>>()
        } else {
            target_ids.to_vec()
        };
        if closing_ids.is_empty() {
            return Ok(0);
        }
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            for target_id in &closing_ids {
                browser
                    .execute(CloseTargetParams::new(TargetId::new(target_id)))
                    .await
                    .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            }
            Ok::<usize, OpenPageError>(closing_ids.len())
        })
    }

    pub fn version(&self) -> OpenPageResult<String> {
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            let version = browser
                .version()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(version.product)
        })
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let cookie = browser_cookie_param(name, value, url, domain, path);
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(SetCookiesParams::new(vec![cookie]))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url =
            Url::parse(url).map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        let cookies = browser_cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(SetCookiesParams::new(cookies))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let params = browser_delete_cookie_params(name, url, domain, path);
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(params)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(ClearBrowserCookiesParams::default())
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(self.version().is_ok())
    }

    pub fn is_headless(&self) -> bool {
        self.inner.headless
    }

    pub fn is_existed(&self) -> OpenPageResult<bool> {
        self.is_alive()
    }

    pub fn is_incognito(&self) -> OpenPageResult<bool> {
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            Ok(browser.is_incognito())
        })
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid
    }

    pub fn timeouts(&self) -> OpenPageResult<TimeoutConfig> {
        self.inner.timeouts.lock().map(|t| t.clone()).map_err(|_| {
            OpenPageError::BrowserOperation("browser timeouts lock poisoned".to_string())
        })
    }

    pub fn set_timeouts(&self, timeouts: TimeoutConfig) -> OpenPageResult<()> {
        *self.inner.timeouts.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser timeouts lock poisoned".to_string())
        })? = timeouts;
        Ok(())
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        self.inner
            .retry_times
            .lock()
            .map(|retry_times| *retry_times)
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser retry times lock poisoned".to_string())
            })
    }

    pub fn retry_interval_millis(&self) -> OpenPageResult<u64> {
        self.inner
            .retry_interval_millis
            .lock()
            .map(|retry_interval_millis| *retry_interval_millis)
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser retry interval lock poisoned".to_string())
            })
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> OpenPageResult<()> {
        if let Some(retry_times) = retry_times {
            *self.inner.retry_times.lock().map_err(|_| {
                OpenPageError::BrowserOperation("browser retry times lock poisoned".to_string())
            })? = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            *self.inner.retry_interval_millis.lock().map_err(|_| {
                OpenPageError::BrowserOperation("browser retry interval lock poisoned".to_string())
            })? = retry_interval_millis;
        }
        Ok(())
    }

    pub fn wait_for_new_tab(
        &self,
        _current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        let baseline = self.tab_ids()?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let current = self.tab_ids()?;
            if let Some(new_id) = current
                .iter()
                .find(|id| !baseline.iter().any(|seen| seen == *id))
            {
                return Ok(Some(new_id.clone()));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .download_path
            .lock()
            .map(|path| {
                path.as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
            })
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;

        self.inner
            .download_path
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
            })?
            .replace(path);
        Ok(())
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.inner
            .download_file_exists
            .lock()
            .map(|mode| mode.as_str().to_string())
            .map_err(|_| {
                OpenPageError::BrowserOperation(
                    "browser download file-exists lock poisoned".to_string(),
                )
            })
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        *self.inner.download_file_exists.lock().map_err(|_| {
            OpenPageError::BrowserOperation(
                "browser download file-exists lock poisoned".to_string(),
            )
        })? = mode;
        Ok(())
    }

    pub(crate) fn snapshot_browser_download_settings(
        &self,
    ) -> OpenPageResult<BrowserDownloadSettingsSnapshot> {
        let path = self
            .inner
            .download_path
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
            })?
            .clone();
        let file_exists = *self.inner.download_file_exists.lock().map_err(|_| {
            OpenPageError::BrowserOperation(
                "browser download file-exists lock poisoned".to_string(),
            )
        })?;
        let naming = self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download naming lock poisoned".to_string())
            })?
            .clone();
        Ok(BrowserDownloadSettingsSnapshot {
            path,
            file_exists,
            rename: naming.rename,
            suffix: naming.suffix,
        })
    }

    pub(crate) fn restore_browser_download_settings(
        &self,
        settings: BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        *self.inner.download_path.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
        })? = settings.path;
        *self.inner.download_file_exists.lock().map_err(|_| {
            OpenPageError::BrowserOperation(
                "browser download file-exists lock poisoned".to_string(),
            )
        })? = settings.file_exists;
        *self.inner.browser_download_naming.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser download naming lock poisoned".to_string())
        })? = BrowserDownloadNaming {
            rename: settings.rename,
            suffix: settings.suffix,
        };
        Ok(())
    }

    pub(crate) fn apply_browser_download_settings(
        &self,
        settings: &BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        if let Some(path) = &settings.path {
            std::fs::create_dir_all(path)
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        }
        *self.inner.download_path.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
        })? = settings.path.clone();
        *self.inner.download_file_exists.lock().map_err(|_| {
            OpenPageError::BrowserOperation(
                "browser download file-exists lock poisoned".to_string(),
            )
        })? = settings.file_exists;
        *self.inner.browser_download_naming.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser download naming lock poisoned".to_string())
        })? = BrowserDownloadNaming {
            rename: settings.rename.clone(),
            suffix: settings.suffix.clone(),
        };
        Ok(())
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_download_file_exists_mode(DownloadFileExistsMode::parse(mode)?)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self.inner.load_mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser load mode lock poisoned".to_string())
        })? = mode;
        Ok(())
    }

    pub fn page_download_path(&self, target_id: &str) -> OpenPageResult<Option<String>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        if let Some(path) = settings
            .get(&frame_id)
            .and_then(|settings| settings.path.as_ref())
        {
            return Ok(Some(path.to_string_lossy().into_owned()));
        }
        drop(settings);
        self.download_path()
    }

    pub fn set_page_download_path(
        &self,
        target_id: &str,
        path: impl AsRef<Path>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        let mut settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        settings.entry(frame_id).or_default().path = Some(path);
        Ok(())
    }

    pub fn page_download_file_exists_mode(&self, target_id: &str) -> OpenPageResult<String> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        if let Some(mode) = settings
            .get(&frame_id)
            .and_then(|settings| settings.file_exists)
        {
            return Ok(mode.as_str().to_string());
        }
        drop(settings);
        self.download_file_exists_mode()
    }

    pub fn set_page_download_file_exists_mode(
        &self,
        target_id: &str,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        settings.entry(frame_id).or_default().file_exists = Some(mode);
        Ok(())
    }

    pub fn when_page_download_file_exists(
        &self,
        target_id: &str,
        mode: &str,
    ) -> OpenPageResult<()> {
        self.set_page_download_file_exists_mode(target_id, DownloadFileExistsMode::parse(mode)?)
    }

    pub fn set_page_download_filename(
        &self,
        target_id: &str,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        let entry = settings.entry(frame_id).or_default();
        entry.rename = rename.map(str::to_string);
        entry.suffix = if suffix_specified {
            Some(suffix.map(str::to_string))
        } else {
            None
        };
        Ok(())
    }

    pub(crate) fn snapshot_page_download_settings(
        &self,
        target_id: &str,
    ) -> OpenPageResult<Option<PageDownloadSettings>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        Ok(settings.get(&frame_id).cloned())
    }

    pub(crate) fn restore_page_download_settings(
        &self,
        target_id: &str,
        settings: Option<PageDownloadSettings>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut current = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        if let Some(settings) = settings {
            current.insert(frame_id, settings);
        } else {
            current.remove(&frame_id);
        }
        Ok(())
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .mission_ids()?
            .into_iter()
            .map(|guid| DownloadMission::new(self.clone(), guid))
            .collect())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .last_guid()?
            .map(|guid| DownloadMission::new(self.clone(), guid)))
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match filename {
            Some(filename) => {
                let info = self.inner.downloads.wait_for_name(filename, timeout_ms)?;
                self.finalize_download(&info, None)
            }
            None => {
                let completed_before = self.inner.downloads.completed_len()?;
                let info = self
                    .inner
                    .downloads
                    .wait_for_next_after(completed_before, timeout_ms)?;
                self.finalize_download(&info, None)
            }
        }
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after(started_before, timeout_ms, cancel_it)
    }

    pub fn wait_for_download_begin_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
            cancel_it,
        )
    }

    pub(crate) fn download_started_len(&self) -> OpenPageResult<usize> {
        self.inner.downloads.started_len()
    }

    pub(crate) fn wait_for_download_begin_after(
        &self,
        started_before: usize,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self
            .inner
            .downloads
            .wait_for_begin_after(started_before, timeout_ms)
        {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub(crate) fn wait_for_download_begin_after_in_frames(
        &self,
        started_before: usize,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self.inner.downloads.wait_for_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
        ) {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self.inner.downloads.wait_until_idle(timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids()? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn wait_for_downloads_done_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self
            .inner
            .downloads
            .wait_until_idle_in_frames(frame_ids, timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids_in_frames(frame_ids)? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        let guid = guid.to_string();
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(CancelDownloadParams::new(guid))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
        })
    }

    pub fn close(&self) -> OpenPageResult<()> {
        self.inner.runtime.block_on(async {
            let mut browser = self.inner.browser.lock().await;
            browser
                .close()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            let _ = browser.wait().await;
            Ok::<(), OpenPageError>(())
        })?;
        if let Some(path) = &self.inner.temp_user_data_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        if let Some(path) = &self.inner.temp_download_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        Ok(())
    }

    pub(crate) fn download_info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.inner.downloads.info(guid)
    }

    pub(crate) fn download_folder(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        let settings = self.resolve_download_settings(&info)?;
        let path = settings.path;
        Ok(path.to_string_lossy().into_owned())
    }

    pub(crate) fn download_tab_id(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        self.resolve_frame_target_id(&info.frame_id)
    }

    pub(crate) fn download_tmp_path(&self, guid: &str) -> OpenPageResult<String> {
        Ok(self
            .inner
            .download_spool_dir
            .join(guid)
            .to_string_lossy()
            .into_owned())
    }

    pub(crate) fn wait_for_download_guid(
        &self,
        guid: &str,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<Option<String>> {
        let info = match timeout_ms {
            Some(timeout_ms) => match self.inner.downloads.wait_for_guid(guid, timeout_ms) {
                Ok(info) => info,
                Err(OpenPageError::Timeout(_)) => {
                    if let Ok(info) = self.download_info(guid) {
                        if info.state != DownloadState::Running {
                            return self.finish_waited_download(&info);
                        }
                    }
                    if cancel_if_timeout {
                        let _ = self.cancel_download(guid);
                    }
                    return Ok(None);
                }
                Err(err) => return Err(err),
            },
            None => self.inner.downloads.wait_for_guid_forever(guid)?,
        };
        self.finish_waited_download(&info)
    }

    fn finalize_download(
        &self,
        info: &DownloadInfo,
        filename: Option<&str>,
    ) -> OpenPageResult<String> {
        if info.state == DownloadState::Canceled {
            return Err(OpenPageError::BrowserOperation(format!(
                "download `{}` was canceled",
                info.guid
            )));
        }

        if info.state == DownloadState::Skipped {
            return info.final_path.clone().ok_or_else(|| {
                OpenPageError::BrowserOperation(format!(
                    "download `{}` was skipped without a final path",
                    info.guid
                ))
            });
        }

        let source_path = download_source_path(info, &self.inner.download_spool_dir)?;
        let settings = self.resolve_download_settings(info)?;
        let download_dir = settings.path;
        let mode = settings.file_exists;
        let rename = settings.rename;
        let suffix = settings.suffix;
        let preferred_name = filename.map(str::to_string).unwrap_or_else(|| {
            resolved_download_name(
                &info.suggested_filename,
                rename.as_deref(),
                suffix.as_ref().map(|value| value.as_deref()),
            )
        });
        let preferred_path = download_dir.join(&preferred_name);
        let (state, final_path) = finalize_download_path(&source_path, &preferred_path, mode)?;

        self.inner
            .downloads
            .set_finalized(&info.guid, state, final_path.clone())?;
        Ok(final_path)
    }

    fn finish_waited_download(&self, info: &DownloadInfo) -> OpenPageResult<Option<String>> {
        match info.state {
            DownloadState::Canceled => Ok(None),
            _ => self.finalize_download(info, None).map(Some),
        }
    }

    fn capture_mission_download_settings(&self, info: &DownloadInfo) -> OpenPageResult<()> {
        let settings = self.resolve_download_settings(info)?;
        let mut mission_settings = self.inner.mission_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("mission download settings lock poisoned".to_string())
        })?;
        mission_settings.insert(info.guid.clone(), settings);
        Ok(())
    }

    fn resolve_page_frame_id(&self, target_id: &str) -> OpenPageResult<String> {
        self.get_page(target_id)?.main_frame_id()
    }

    fn resolve_frame_target_id(&self, frame_id: &str) -> OpenPageResult<String> {
        for target_id in self.tab_ids()? {
            let page = self.get_page(&target_id)?;
            if page
                .download_scope_frame_ids()?
                .iter()
                .any(|current| current == frame_id)
            {
                return Ok(target_id);
            }
        }

        Err(OpenPageError::BrowserOperation(format!(
            "download frame `{frame_id}` was not mapped to a tab"
        )))
    }

    fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.inner.load_mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation("browser load mode lock poisoned".to_string())
        })
    }

    fn resolve_download_settings(
        &self,
        info: &DownloadInfo,
    ) -> OpenPageResult<ResolvedDownloadSettings> {
        if let Some(settings) = self
            .inner
            .mission_download_settings
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation(
                    "mission download settings lock poisoned".to_string(),
                )
            })?
            .get(&info.guid)
            .cloned()
        {
            return Ok(settings);
        }

        let page_settings = self.inner.page_download_settings.lock().map_err(|_| {
            OpenPageError::BrowserOperation("page download settings lock poisoned".to_string())
        })?;
        let page_settings = page_settings.get(&info.frame_id);
        let path = if let Some(path) = page_settings.and_then(|settings| settings.path.clone()) {
            path
        } else {
            self.inner
                .download_path
                .lock()
                .map_err(|_| {
                    OpenPageError::BrowserOperation(
                        "browser download path lock poisoned".to_string(),
                    )
                })?
                .clone()
                .ok_or_else(|| {
                    OpenPageError::UnsupportedOperation(
                        "download path is not configured".to_string(),
                    )
                })?
        };
        let mode = if let Some(mode) = page_settings.and_then(|settings| settings.file_exists) {
            mode
        } else {
            *self.inner.download_file_exists.lock().map_err(|_| {
                OpenPageError::BrowserOperation(
                    "browser download file-exists lock poisoned".to_string(),
                )
            })?
        };
        let rename = page_settings.and_then(|settings| settings.rename.clone());
        let browser_naming = self.inner.browser_download_naming.lock().map_err(|_| {
            OpenPageError::BrowserOperation("browser download naming lock poisoned".to_string())
        })?;
        let rename = rename.or_else(|| browser_naming.rename.clone());
        let suffix = page_settings
            .and_then(|settings| settings.suffix.clone())
            .or_else(|| browser_naming.suffix.clone());
        Ok(ResolvedDownloadSettings {
            path,
            file_exists: mode,
            rename,
            suffix,
        })
    }
}

fn browser_pid(browser: &mut OxBrowser) -> Option<u32> {
    browser
        .get_mut_child()
        .and_then(|child| child.as_mut_inner().id())
}

fn build_browser_config(
    options: &LaunchOptions,
    user_data_dir: Option<&Path>,
) -> OpenPageResult<BrowserConfig> {
    let mut builder = BrowserConfig::builder().window_size(options.width, options.height);
    builder = builder.enable_request_intercept().disable_cache();

    if options.headless {
        builder = builder.new_headless_mode();
    } else {
        builder = builder.with_head();
    }

    if options.no_sandbox {
        builder = builder.no_sandbox();
    }

    if let Some(path) = &options.browser_path {
        builder = builder.chrome_executable(path);
    }

    if let Some(path) = user_data_dir {
        builder = builder.user_data_dir(path);
    }

    if let Some(port) = options.remote_debugging_port {
        builder = builder.port(port);
    }

    if options.incognito {
        builder = builder.incognito();
    }

    if options.ignore_https_errors {
        builder = builder.respect_https_errors();
    }

    for path in &options.extensions {
        builder = builder.extension(path.to_string_lossy());
    }

    if options.disable_default_args {
        builder = builder.disable_default_args();
    }

    for arg in &options.args {
        builder = builder.arg(arg.as_str());
    }

    if options.mute {
        builder = builder.arg("--mute-audio");
    }

    if options.no_js {
        builder = builder.arg("--disable-javascript");
    }

    if options.no_imgs {
        builder = builder.arg("--blink-settings=imagesEnabled=false");
    }

    if let Some(proxy) = &options.proxy {
        builder = builder.arg(("proxy-server", proxy.as_str()));
    }

    if let Some(user_agent) = &options.user_agent {
        builder = builder.arg(("user-agent", user_agent.as_str()));
    }

    if let Some(cache_path) = &options.cache_path {
        builder = builder.arg(("disk-cache-dir", cache_path.to_string_lossy().as_ref()));
    }

    builder
        .build()
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))
}

fn validate_auto_port_scope(scope: (u16, u16)) -> OpenPageResult<()> {
    let (start, end) = scope;
    if start == 0 || start >= end {
        return Err(OpenPageError::BrowserOperation(format!(
            "auto_port scope must satisfy 0 < start < end, got ({start}, {end})"
        )));
    }
    Ok(())
}

fn find_free_port(scope: Option<(u16, u16)>) -> OpenPageResult<u16> {
    use std::net::TcpListener;
    let scope = scope.unwrap_or(DEFAULT_AUTO_PORT_SCOPE);
    validate_auto_port_scope(scope)?;

    for port in scope.0..scope.1 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            drop(listener);
            return Ok(port);
        }
    }

    Err(OpenPageError::BrowserLaunch(format!(
        "failed to find free port in auto_port scope [{}, {})",
        scope.0, scope.1
    )))
}

fn default_launch_options_ini_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini")
}

fn project_launch_options_ini_path() -> OpenPageResult<PathBuf> {
    Ok(std::env::current_dir()?.join("dp_configs.ini"))
}

fn built_in_launch_options_defaults() -> OpenPageResult<LaunchOptions> {
    parse_launch_options_ini(include_str!("../configs.ini"))
}

fn resolve_launch_options_ini_path(path: Option<&Path>) -> OpenPageResult<PathBuf> {
    let path = match path {
        Some(path) => {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()?.join(path)
            }
        }
        None => {
            let project_path = project_launch_options_ini_path()?;
            if project_path.is_file() {
                project_path
            } else {
                default_launch_options_ini_path()
            }
        }
    };

    if path.is_dir() {
        Ok(path.join("config.ini"))
    } else {
        Ok(path)
    }
}

fn parse_launch_options_ini(content: &str) -> OpenPageResult<LaunchOptions> {
    let sections = parse_ini_sections(content);
    let mut options = LaunchOptions::default();

    if let Some(path) = ini_non_empty(ini_section_value(&sections, "paths", "download_path")) {
        options.set_download_path(path);
    }
    if let Some(path) = ini_non_empty(ini_section_value(&sections, "paths", "tmp_path")) {
        options.set_tmp_path(path);
    }
    if let Some(path) = ini_non_empty(ini_section_value(&sections, "paths", "cache_path")) {
        options.set_cache_path(path);
    }
    if let Some(path) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "browser_path",
    )) {
        options.set_browser_path(path);
    }
    if let Some(arguments) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "arguments",
    )) {
        options.args = parse_ini_string_list(arguments, "arguments")?;
    }
    if let Some(extensions) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "extensions",
    )) {
        options.extensions = parse_ini_string_list(extensions, "extensions")?
            .into_iter()
            .map(PathBuf::from)
            .collect();
    }
    if let Some(prefs) = ini_non_empty(ini_section_value(&sections, "chromium_options", "prefs")) {
        options.prefs = parse_ini_preferences(prefs)?;
    }
    if let Some(flags) = ini_non_empty(ini_section_value(&sections, "chromium_options", "flags")) {
        options.flags = parse_ini_flags(flags)?;
    }
    if let Some(load_mode) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "load_mode",
    )) {
        options.set_load_mode(load_mode)?;
    }
    if let Some(user) = ini_non_empty(ini_section_value(&sections, "chromium_options", "user")) {
        options.set_user(user);
    }
    if let Some(path) = ini_non_empty(ini_section_value(&sections, "paths", "user_data_path")) {
        options.set_user_data_path(path);
    }
    if let Some(system_user_path) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "system_user_path",
    )) {
        options.use_system_user_path(parse_ini_bool(system_user_path)?);
    }
    if let Some(address) =
        ini_non_empty(ini_section_value(&sections, "chromium_options", "address"))
    {
        options.set_address(address);
    }
    if let Some(auto_port) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "auto_port",
    )) {
        apply_loaded_auto_port_value(&mut options, auto_port)?;
    }

    if let Some(existing_only) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "existing_only",
    )) {
        options.existing_only(parse_ini_bool(existing_only)?);
    }
    if let Some(new_env) =
        ini_non_empty(ini_section_value(&sections, "chromium_options", "new_env"))
    {
        options.new_env(parse_ini_bool(new_env)?);
    }
    if let Some(headless) =
        ini_non_empty(ini_section_value(&sections, "chromium_options", "headless"))
    {
        options.headless(parse_ini_bool(headless)?);
    }
    if let Some(incognito) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "incognito",
    )) {
        options.incognito(parse_ini_bool(incognito)?);
    }
    if let Some(ignore_cert_errors) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "ignore_certificate_errors",
    )) {
        options.ignore_certificate_errors(parse_ini_bool(ignore_cert_errors)?);
    }
    if let Some(no_imgs) =
        ini_non_empty(ini_section_value(&sections, "chromium_options", "no_imgs"))
    {
        options.no_imgs(parse_ini_bool(no_imgs)?);
    }
    if let Some(no_js) = ini_non_empty(ini_section_value(&sections, "chromium_options", "no_js")) {
        options.no_js(parse_ini_bool(no_js)?);
    }
    if let Some(mute) = ini_non_empty(ini_section_value(&sections, "chromium_options", "mute")) {
        options.mute(parse_ini_bool(mute)?);
    }
    if let Some(user_agent) = ini_non_empty(ini_section_value(
        &sections,
        "chromium_options",
        "user_agent",
    )) {
        options.set_user_agent(user_agent);
    }
    if let Some(proxy) = ini_non_empty(ini_section_value(&sections, "proxies", "http"))
        .or_else(|| ini_non_empty(ini_section_value(&sections, "proxies", "https")))
    {
        options.set_proxy(proxy);
    }
    if let Some(retry_times) = ini_non_empty(ini_section_value(&sections, "others", "retry_times"))
    {
        options.retry_times = retry_times.parse().map_err(|err| {
            OpenPageError::BrowserOperation(format!(
                "invalid retry_times in launch options ini: {err}"
            ))
        })?;
    }
    if let Some(retry_interval) =
        ini_non_empty(ini_section_value(&sections, "others", "retry_interval"))
    {
        let retry_interval = retry_interval.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(format!(
                "invalid retry_interval in launch options ini: {err}"
            ))
        })?;
        options.retry_interval_millis = seconds_to_millis(retry_interval);
    }
    if let Some(base) = ini_non_empty(ini_section_value(&sections, "timeouts", "base")) {
        options.timeouts.implicit_wait = seconds_to_millis(base.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(format!(
                "invalid base timeout in launch options ini: {err}"
            ))
        })?);
    }
    if let Some(page_load) = ini_non_empty(ini_section_value(&sections, "timeouts", "page_load")) {
        options.timeouts.page_load =
            seconds_to_millis(page_load.parse::<f64>().map_err(|err| {
                OpenPageError::BrowserOperation(format!(
                    "invalid page_load timeout in launch options ini: {err}"
                ))
            })?);
    }
    if let Some(script) = ini_non_empty(ini_section_value(&sections, "timeouts", "script")) {
        options.timeouts.script = seconds_to_millis(script.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(format!(
                "invalid script timeout in launch options ini: {err}"
            ))
        })?);
    }

    Ok(options)
}

fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::new();
    let mut current_section: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim().to_string();
            sections.entry(section.clone()).or_insert_with(HashMap::new);
            current_section = Some(section);
            continue;
        }
        let Some(section) = current_section.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        sections
            .entry(section.clone())
            .or_insert_with(HashMap::new)
            .insert(key.trim().to_string(), value.trim().to_string());
    }

    sections
}

fn ini_section_value<'a>(
    sections: &'a HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Option<&'a str> {
    sections.get(section)?.get(key).map(String::as_str)
}

fn ini_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_ini_bool(value: &str) -> OpenPageResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(OpenPageError::BrowserOperation(format!(
            "invalid boolean in launch options ini: {value}"
        ))),
    }
}

fn parse_ini_string_list(value: &str, field: &str) -> OpenPageResult<Vec<String>> {
    let parsed = parse_ini_json_like_value(value, field)?;
    let items = parsed.as_array().ok_or_else(|| {
        OpenPageError::BrowserOperation(format!(
            "invalid {field} list in launch options ini: expected list"
        ))
    })?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                OpenPageError::BrowserOperation(format!(
                    "invalid {field} list in launch options ini: expected string items"
                ))
            })
        })
        .collect()
}

fn parse_ini_preferences(value: &str) -> OpenPageResult<HashMap<String, serde_json::Value>> {
    let parsed = parse_ini_json_like_value(value, "prefs")?;
    let object = parsed.as_object().ok_or_else(|| {
        OpenPageError::BrowserOperation(
            "invalid prefs object in launch options ini: expected object".to_string(),
        )
    })?;
    Ok(object
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

fn parse_ini_flags(value: &str) -> OpenPageResult<Vec<String>> {
    let parsed = parse_ini_json_like_value(value, "flags")?;
    match parsed {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| {
                item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    OpenPageError::BrowserOperation(
                        "invalid flags list in launch options ini: expected string items"
                            .to_string(),
                    )
                })
            })
            .collect(),
        serde_json::Value::Object(items) => items
            .into_iter()
            .map(|(key, value)| match value {
                serde_json::Value::Null => Ok(key),
                serde_json::Value::String(value) => Ok(format!("{key}@{value}")),
                serde_json::Value::Number(value) => Ok(format!("{key}@{value}")),
                serde_json::Value::Bool(value) => Ok(format!("{key}@{value}")),
                _ => Err(OpenPageError::BrowserOperation(
                    "invalid flags object in launch options ini: expected scalar values"
                        .to_string(),
                )),
            })
            .collect(),
        _ => Err(OpenPageError::BrowserOperation(
            "invalid flags in launch options ini: expected list or object".to_string(),
        )),
    }
}

fn parse_ini_json_like_value(value: &str, field: &str) -> OpenPageResult<serde_json::Value> {
    if let Ok(parsed) = serde_json::from_str(value) {
        return Ok(parsed);
    }
    let normalized = python_literal_to_json(value)?;
    serde_json::from_str(&normalized).map_err(|err| {
        OpenPageError::BrowserOperation(format!(
            "invalid {field} value in launch options ini: {err}"
        ))
    })
}

fn python_literal_to_json(value: &str) -> OpenPageResult<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut normalized = String::new();

    while index < chars.len() {
        match chars[index] {
            '\'' | '"' => {
                let quote = chars[index];
                index += 1;
                let mut content = String::new();
                let mut closed = false;

                while index < chars.len() {
                    let ch = chars[index];
                    if ch == '\\' {
                        index += 1;
                        if index >= chars.len() {
                            return Err(OpenPageError::BrowserOperation(
                                "invalid Python-style string in launch options ini".to_string(),
                            ));
                        }
                        let escaped = chars[index];
                        match escaped {
                            '\\' => content.push('\\'),
                            '\'' => content.push('\''),
                            '"' => content.push('"'),
                            'n' => content.push('\n'),
                            'r' => content.push('\r'),
                            't' => content.push('\t'),
                            other => content.push(other),
                        }
                        index += 1;
                        continue;
                    }
                    if ch == quote {
                        index += 1;
                        closed = true;
                        break;
                    }
                    content.push(ch);
                    index += 1;
                }

                if !closed {
                    return Err(OpenPageError::BrowserOperation(
                        "unterminated Python-style string in launch options ini".to_string(),
                    ));
                }

                normalized.push_str(&serde_json::to_string(&content).unwrap());
            }
            ch if ch.is_ascii_alphabetic() => {
                let start = index;
                index += 1;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
                let ident = chars[start..index].iter().collect::<String>();
                match ident.as_str() {
                    "True" => normalized.push_str("true"),
                    "False" => normalized.push_str("false"),
                    "None" => normalized.push_str("null"),
                    _ => normalized.push_str(&ident),
                }
            }
            ch => {
                normalized.push(ch);
                index += 1;
            }
        }
    }

    Ok(normalized)
}

fn parse_ini_u16_tuple(value: &str) -> OpenPageResult<(u16, u16)> {
    let trimmed = value.trim().trim_start_matches('(').trim_end_matches(')');
    let Some((start, end)) = trimmed.split_once(',') else {
        return Err(OpenPageError::BrowserOperation(format!(
            "invalid auto_port scope in launch options ini: {value}"
        )));
    };
    let start = start.trim().parse::<u16>().map_err(|err| {
        OpenPageError::BrowserOperation(format!(
            "invalid auto_port scope start in launch options ini: {err}"
        ))
    })?;
    let end = end.trim().parse::<u16>().map_err(|err| {
        OpenPageError::BrowserOperation(format!(
            "invalid auto_port scope end in launch options ini: {err}"
        ))
    })?;
    Ok((start, end))
}

fn apply_loaded_auto_port_value(options: &mut LaunchOptions, value: &str) -> OpenPageResult<()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => {
            options.auto_port(true);
            Ok(())
        }
        "false" | "0" | "no" | "off" => {
            options.auto_port(false);
            Ok(())
        }
        _ => {
            let scope = parse_ini_u16_tuple(value)?;
            options.auto_port_with_scope(true, Some(scope))
        }
    }
}

fn serialize_launch_options_ini(options: &LaunchOptions, template: Option<&str>) -> String {
    let rendered_sections = serialize_launch_options_ini_sections(options);
    let mut rendered_by_name = rendered_sections
        .iter()
        .cloned()
        .collect::<HashMap<String, String>>();
    let mut emitted = HashSet::new();
    let mut ordered_blocks = Vec::new();

    if let Some(template) = template {
        for section in parse_ini_section_blocks(template) {
            if let Some(rendered) = rendered_by_name.get(&section.name) {
                if emitted.insert(section.name.clone()) {
                    ordered_blocks.push(rendered.clone());
                }
            } else {
                let raw = trim_ini_block(&section.raw);
                if !raw.is_empty() {
                    ordered_blocks.push(raw.to_string());
                }
            }
        }
    }

    for (name, rendered) in rendered_sections {
        if emitted.insert(name.clone()) {
            ordered_blocks.push(rendered);
        }
        rendered_by_name.remove(&name);
    }

    if ordered_blocks.is_empty() {
        String::new()
    } else {
        format!("{}\n", ordered_blocks.join("\n\n"))
    }
}

fn serialize_launch_options_ini_sections(options: &LaunchOptions) -> Vec<(String, String)> {
    let address = options.address();
    let browser_path = option_path_string(options.browser_path.as_deref());
    let download_path = option_path_string(options.download_path.as_deref());
    let tmp_path = option_path_string(options.tmp_path.as_deref());
    let cache_path = option_path_string(options.cache_path.as_deref());
    let user_data_path = option_path_string(options.user_data_dir.as_deref());
    let arguments = serde_json::to_string(&options.args).unwrap_or_else(|_| "[]".to_string());
    let extensions = serde_json::to_string(
        &options
            .extensions
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    let prefs = serde_json::to_string(&options.prefs).unwrap_or_else(|_| "{}".to_string());
    let flags = serde_json::to_string(&options.flags).unwrap_or_else(|_| "[]".to_string());
    let proxy = options.proxy.clone().unwrap_or_default();
    let user_agent = options.user_agent.clone().unwrap_or_default();
    let user = options.user();
    let auto_port = ini_auto_port_value(options);

    vec![
        (
            "paths".to_string(),
            format!(
                "[paths]\n\
download_path = {download_path}\n\
tmp_path = {tmp_path}\n\
cache_path = {cache_path}\n\
user_data_path = {user_data_path}",
            ),
        ),
        (
            "chromium_options".to_string(),
            format!(
                "[chromium_options]\n\
address = {address}\n\
browser_path = {browser_path}\n\
arguments = {arguments}\n\
extensions = {extensions}\n\
prefs = {prefs}\n\
flags = {flags}\n\
load_mode = {load_mode}\n\
user = {user}\n\
auto_port = {auto_port}\n\
system_user_path = {system_user_path}\n\
existing_only = {existing_only}\n\
new_env = {new_env}\n\
headless = {headless}\n\
incognito = {incognito}\n\
ignore_certificate_errors = {ignore_certificate_errors}\n\
no_imgs = {no_imgs}\n\
no_js = {no_js}\n\
mute = {mute}\n\
user_agent = {user_agent}",
                load_mode = options.load_mode.as_str(),
                auto_port = auto_port,
                system_user_path = ini_bool(options.system_user_path),
                existing_only = ini_bool(options.existing_only),
                new_env = ini_bool(options.new_env),
                headless = ini_bool(options.headless),
                incognito = ini_bool(options.incognito),
                ignore_certificate_errors = ini_bool(options.ignore_https_errors),
                no_imgs = ini_bool(options.no_imgs),
                no_js = ini_bool(options.no_js),
                mute = ini_bool(options.mute),
            ),
        ),
        (
            "timeouts".to_string(),
            format!(
                "[timeouts]\n\
base = {base_timeout}\n\
page_load = {page_load_timeout}\n\
script = {script_timeout}",
                base_timeout = millis_to_ini_seconds(options.timeouts.implicit_wait),
                page_load_timeout = millis_to_ini_seconds(options.timeouts.page_load),
                script_timeout = millis_to_ini_seconds(options.timeouts.script),
            ),
        ),
        (
            "proxies".to_string(),
            format!(
                "[proxies]\n\
http = {proxy}\n\
https = {proxy}",
            ),
        ),
        (
            "others".to_string(),
            format!(
                "[others]\n\
retry_times = {retry_times}\n\
retry_interval = {retry_interval}",
                retry_times = options.retry_times,
                retry_interval = millis_to_ini_seconds(options.retry_interval_millis),
            ),
        ),
    ]
}

fn load_launch_options_ini_template(
    target_path: &Path,
    source_ini_path: Option<&Path>,
) -> Option<String> {
    read_launch_options_ini_template(target_path)
        .or_else(|| {
            source_ini_path
                .filter(|source_path| *source_path != target_path)
                .and_then(|source_path| read_launch_options_ini_template(source_path))
        })
        .or_else(|| {
            let default_path = default_launch_options_ini_path();
            (default_path.as_path() != target_path)
                .then_some(default_path)
                .and_then(|path| read_launch_options_ini_template(path.as_path()))
        })
}

fn read_launch_options_ini_template(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[derive(Debug, Clone)]
struct IniSectionBlock {
    name: String,
    raw: String,
}

fn parse_ini_section_blocks(content: &str) -> Vec<IniSectionBlock> {
    let mut blocks = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_lines = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(name) = current_name.take() {
                blocks.push(IniSectionBlock {
                    name,
                    raw: current_lines.join("\n"),
                });
            }
            current_name = Some(trimmed[1..trimmed.len() - 1].trim().to_string());
            current_lines.clear();
        }

        if current_name.is_some() {
            current_lines.push(raw_line.to_string());
        }
    }

    if let Some(name) = current_name {
        blocks.push(IniSectionBlock {
            name,
            raw: current_lines.join("\n"),
        });
    }

    blocks
}

fn trim_ini_block(block: &str) -> &str {
    block.trim_matches('\n')
}

fn ini_auto_port_value(options: &LaunchOptions) -> String {
    match options.auto_port_scope() {
        Some((start, end)) => format!("({start}, {end})"),
        None => ini_bool(false).to_string(),
    }
}

fn option_path_string(path: Option<&Path>) -> String {
    path.map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn resolved_launch_options_address(options: &LaunchOptions) -> String {
    options
        .address
        .clone()
        .or_else(|| {
            options
                .remote_debugging_port
                .map(|port| format!("127.0.0.1:{port}"))
        })
        .unwrap_or_else(|| "127.0.0.1:9222".to_string())
}

fn ini_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn millis_to_ini_seconds(millis: u64) -> String {
    if millis % 1000 == 0 {
        (millis / 1000).to_string()
    } else {
        let seconds = millis as f64 / 1000.0;
        let mut value = format!("{seconds:.3}");
        while value.contains('.') && value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
        value
    }
}

fn millis_to_seconds_f64(millis: u64) -> f64 {
    millis as f64 / 1000.0
}

fn system_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Google")
                .join("Chrome")
        });
    }
    #[cfg(target_os = "linux")]
    {
        return std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".config").join("google-chrome"));
    }
    #[cfg(target_os = "windows")]
    {
        return std::env::var("LOCALAPPDATA").ok().map(|local_app_data| {
            PathBuf::from(local_app_data)
                .join("Google")
                .join("Chrome")
                .join("User Data")
        });
    }
    #[allow(unreachable_code)]
    None
}

fn normalize_debugger_address(address: &str) -> (String, Option<String>, Option<u16>) {
    let normalized = address.trim().replace("localhost", "127.0.0.1");

    if normalized.starts_with("ws://") || normalized.starts_with("wss://") {
        if let Ok(url) = Url::parse(&normalized) {
            if let Some(host) = url.host_str() {
                let address = match url.port_or_known_default() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_string(),
                };
                let local_port = if host == "127.0.0.1" {
                    url.port_or_known_default()
                } else {
                    None
                };
                return (address, Some(normalized), local_port);
            }
        }

        return (normalized.clone(), Some(normalized), None);
    }

    if normalized.starts_with("http://") || normalized.starts_with("https://") {
        if let Ok(url) = Url::parse(&normalized) {
            if let Some(host) = url.host_str() {
                let address = match url.port_or_known_default() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_string(),
                };
                let local_port = if host == "127.0.0.1" {
                    url.port_or_known_default()
                } else {
                    None
                };
                return (address, None, local_port);
            }
        }

        let address = normalized
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string();
        return (
            address.clone(),
            None,
            local_debugger_port_from_host_port(&address),
        );
    }

    let local_port = local_debugger_port_from_host_port(&normalized);
    (normalized, None, local_port)
}

fn is_local_debugger_address(address: &str) -> bool {
    debugger_address_host(address) == Some("127.0.0.1")
}

fn debugger_address_host(address: &str) -> Option<&str> {
    address.split_once(':').map(|(host, _)| host)
}

fn debugger_address_port(address: &str) -> Option<u16> {
    let (_, rest) = address.split_once(':')?;
    rest.split('/').next()?.parse().ok()
}

fn local_debugger_port_from_host_port(address: &str) -> Option<u16> {
    if is_local_debugger_address(address) {
        debugger_address_port(address)
    } else {
        None
    }
}

fn local_debugger_address_is_open(address: &str) -> bool {
    let port = match local_debugger_port_from_host_port(address) {
        Some(port) => port,
        None => return false,
    };
    let socket = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&socket, Duration::from_millis(200)).is_ok()
}

fn seconds_to_millis(seconds: f64) -> u64 {
    if seconds <= 0.0 || !seconds.is_finite() {
        0
    } else {
        (seconds * 1000.0) as u64
    }
}

fn reset_browser_user_data_dir(path: &Path) -> OpenPageResult<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(path).map_err(|err| {
        OpenPageError::BrowserLaunch(format!(
            "failed to reset browser user data dir {}: {err}",
            path.display()
        ))
    })
}

fn write_chrome_prefs(
    user_data_dir: &Path,
    args: &[String],
    prefs: &HashMap<String, serde_json::Value>,
    prefs_to_remove: &[String],
) -> OpenPageResult<()> {
    let prefs_dir = user_data_dir.join(chrome_profile_directory(args));
    std::fs::create_dir_all(&prefs_dir)
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
    let prefs_path = prefs_dir.join("Preferences");
    let mut existing: serde_json::Value = if prefs_path.exists() {
        let content = std::fs::read_to_string(&prefs_path)
            .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    for (key, value) in prefs {
        set_nested_json_value(&mut existing, key, value.clone());
    }
    for key in prefs_to_remove {
        remove_nested_json_value(&mut existing, key);
    }
    std::fs::write(&prefs_path, serde_json::to_string(&existing).unwrap())
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
    Ok(())
}

fn chrome_profile_directory(args: &[String]) -> &str {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--profile-directory="))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Default")
}

fn set_nested_json_value(target: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let mut current = target;
    let mut parts = path.split('.').peekable();

    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            ensure_json_object(current).insert(part.to_string(), value);
            return;
        }

        let entry = ensure_json_object(current)
            .entry(part.to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !entry.is_object() {
            *entry = serde_json::json!({});
        }
        current = entry;
    }
}

fn remove_nested_json_value(target: &mut serde_json::Value, path: &str) {
    let mut current = target;
    let mut parts = path.split('.').peekable();

    while let Some(part) = parts.next() {
        let Some(map) = current.as_object_mut() else {
            return;
        };
        if parts.peek().is_none() {
            map.remove(part);
            return;
        }
        let Some(next) = map.get_mut(part) else {
            return;
        };
        current = next;
    }
}

fn ensure_json_object(
    value: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    value.as_object_mut().expect("json object")
}

fn write_chrome_flags(
    user_data_dir: &Path,
    flags: &[String],
    clear_file_flags: bool,
) -> OpenPageResult<()> {
    let local_state_path = user_data_dir.join("Local State");
    let mut existing: serde_json::Value = if local_state_path.exists() {
        let content = std::fs::read_to_string(&local_state_path)
            .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let experiments = ensure_json_object(&mut existing)
        .entry("browser".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let browser = ensure_json_object(experiments);
    let mut merged_flags = if clear_file_flags {
        Vec::new()
    } else {
        browser
            .get("enabled_labs_experiments")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    for flag in flags {
        if !merged_flags.contains(flag) {
            merged_flags.push(flag.clone());
        }
    }
    browser.insert(
        "enabled_labs_experiments".to_string(),
        serde_json::Value::Array(
            merged_flags
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    std::fs::write(&local_state_path, serde_json::to_string(&existing).unwrap())
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
    Ok(())
}

fn make_temp_user_data_dir(base: Option<&Path>) -> OpenPageResult<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?
        .as_nanos();
    let fallback = std::env::temp_dir();
    let base = base.unwrap_or_else(|| fallback.as_path());
    let path = base.join(format!("openpage-browser-{suffix}"));
    std::fs::create_dir_all(&path).map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
    Ok(path)
}

fn resolve_launch_user_data_dir(
    options: &LaunchOptions,
) -> OpenPageResult<(Option<PathBuf>, bool)> {
    let base_tmp = options.tmp_path.as_deref();
    let use_temp_user_data_dir = options.auto_port || options.user_data_dir.is_none();
    let resolved_user_data_dir = if use_temp_user_data_dir {
        Some(make_temp_user_data_dir(base_tmp)?)
    } else {
        options.user_data_dir.clone()
    };
    Ok((resolved_user_data_dir, use_temp_user_data_dir))
}

fn make_temp_download_dir(base: Option<&Path>) -> OpenPageResult<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?
        .as_nanos();
    let fallback = std::env::temp_dir();
    let base = base.unwrap_or_else(|| fallback.as_path());
    let path = base.join(format!("openpage-downloads-{suffix}"));
    std::fs::create_dir_all(&path).map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;
    Ok(path)
}

fn configure_download_behavior(
    runtime: &Arc<Runtime>,
    browser: &OxBrowser,
    download_path: &Path,
) -> OpenPageResult<()> {
    let download_path = download_path.to_string_lossy().into_owned();
    runtime.block_on(async {
        let params = SetDownloadBehaviorParams::builder()
            .behavior(SetDownloadBehaviorBehavior::AllowAndName)
            .download_path(download_path)
            .events_enabled(true)
            .build()
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        browser
            .execute(params)
            .await
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        Ok::<(), OpenPageError>(())
    })
}

fn download_source_path(info: &DownloadInfo, download_dir: &Path) -> OpenPageResult<PathBuf> {
    if let Some(path) = &info.final_path {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    let fallback = download_dir.join(&info.guid);
    if fallback.exists() {
        return Ok(fallback);
    }

    Err(OpenPageError::Timeout(
        "download did not complete in time".to_string(),
    ))
}

fn finalize_download_path(
    source_path: &Path,
    preferred_path: &Path,
    mode: DownloadFileExistsMode,
) -> OpenPageResult<(DownloadState, String)> {
    if source_path == preferred_path {
        return Ok((
            DownloadState::Completed,
            preferred_path.to_string_lossy().into_owned(),
        ));
    }

    let final_path = match mode {
        DownloadFileExistsMode::Rename => unique_download_path(preferred_path),
        DownloadFileExistsMode::Overwrite => {
            if preferred_path.exists() {
                std::fs::remove_file(preferred_path)
                    .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            }
            preferred_path.to_path_buf()
        }
        DownloadFileExistsMode::Skip => {
            if preferred_path.exists() {
                std::fs::remove_file(source_path)
                    .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
                return Ok((
                    DownloadState::Skipped,
                    preferred_path.to_string_lossy().into_owned(),
                ));
            }
            preferred_path.to_path_buf()
        }
    };

    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    }
    std::fs::rename(source_path, &final_path)
        .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
    Ok((
        DownloadState::Completed,
        final_path.to_string_lossy().into_owned(),
    ))
}

fn resolved_download_name(
    suggested_filename: &str,
    rename: Option<&str>,
    suffix: Option<Option<&str>>,
) -> String {
    match (rename, suffix) {
        (Some(rename), Some(Some(suffix))) => {
            if suffix.is_empty() {
                rename.to_string()
            } else {
                format!("{rename}.{suffix}")
            }
        }
        (Some(rename), Some(None)) => rename.to_string(),
        (Some(rename), None) => {
            let suggested_ext = Path::new(suggested_filename)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let rename_ext = Path::new(rename)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !suggested_ext.is_empty() && rename_ext != suggested_ext {
                format!("{rename}.{suggested_ext}")
            } else {
                rename.to_string()
            }
        }
        (None, Some(Some(suffix))) => {
            let stem = Path::new(suggested_filename)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(suggested_filename);
            if suffix.is_empty() {
                stem.to_string()
            } else {
                format!("{stem}.{suffix}")
            }
        }
        (None, Some(None)) => Path::new(suggested_filename)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(suggested_filename)
            .to_string(),
        (None, None) => suggested_filename.to_string(),
    }
}

fn is_tab_like_type(target_type: &str) -> bool {
    matches!(target_type, "page" | "tab")
}

fn browser_cookie_header_to_params(url: &Url, cookie_header: &str) -> Vec<CookieParam> {
    cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let (name, value) = item.split_once('=')?;
            Some(browser_cookie_param(
                name.trim(),
                value.trim(),
                Some(url.as_str()),
                None,
                None,
            ))
        })
        .collect()
}

fn browser_cookie_param(
    name: &str,
    value: &str,
    url: Option<&str>,
    domain: Option<&str>,
    path: Option<&str>,
) -> CookieParam {
    let mut cookie = CookieParam::new(name.trim(), value.trim());
    cookie.url = url
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie.domain = domain
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie.path = path
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    cookie
}

fn browser_delete_cookie_params(
    name: &str,
    url: Option<&str>,
    domain: Option<&str>,
    path: Option<&str>,
) -> DeleteCookiesParams {
    let mut params = DeleteCookiesParams::new(name.trim());
    params.url = url
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params.domain = domain
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params.path = path
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    params
}

fn unique_download_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let ext = path.extension().and_then(|value| value.to_str());

    for index in 1.. {
        let candidate = match ext {
            Some(ext) => parent.join(format!("{stem}_{index}.{ext}")),
            None => parent.join(format!("{stem}_{index}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use url::Url;

    use super::{
        DEFAULT_AUTO_PORT_SCOPE, DownloadFileExistsMode, LaunchOptions, LoadMode,
        browser_cookie_header_to_params, browser_cookie_param, browser_delete_cookie_params,
        default_launch_options_ini_path, finalize_download_path, find_free_port, is_tab_like_type,
        reset_browser_user_data_dir, resolve_launch_options_ini_path, resolve_launch_user_data_dir,
        resolved_download_name, system_user_data_dir, unique_download_path,
        validate_auto_port_scope, write_chrome_flags, write_chrome_prefs,
    };
    use crate::download::DownloadState;

    static CURRENT_DIR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn make_temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = env::temp_dir().join(format!("openpage-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    struct RestoreFileGuard {
        path: PathBuf,
        original: Option<Vec<u8>>,
    }

    impl RestoreFileGuard {
        fn new(path: PathBuf) -> Self {
            Self {
                original: fs::read(&path).ok(),
                path,
            }
        }
    }

    impl Drop for RestoreFileGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                let _ = fs::write(&self.path, original);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    struct CurrentDirGuard {
        original: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurrentDirGuard {
        fn change_to(path: &std::path::Path) -> Self {
            let lock = CURRENT_DIR_TEST_LOCK.lock().expect("lock current dir");
            let original = env::current_dir().expect("read current dir");
            env::set_current_dir(path).expect("set current dir");
            Self {
                original,
                _lock: lock,
            }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn unique_download_path_appends_counter() {
        let dir = make_temp_dir("rename");
        let path = dir.join("openpage.txt");
        fs::write(&path, "existing").expect("write existing");

        let unique = unique_download_path(&path);
        assert_eq!(
            unique.file_name().and_then(|name| name.to_str()),
            Some("openpage_1.txt")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finalize_download_path_skips_existing_target() {
        let dir = make_temp_dir("skip");
        let source = dir.join("guid-file");
        let target = dir.join("openpage.txt");
        fs::write(&source, "new").expect("write source");
        fs::write(&target, "existing").expect("write target");

        let (state, final_path) =
            finalize_download_path(&source, &target, DownloadFileExistsMode::Skip)
                .expect("skip finalize");
        assert_eq!(state, DownloadState::Skipped);
        assert_eq!(final_path, target.to_string_lossy());
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(&target).expect("read target"),
            "existing"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolved_download_name_keeps_suggested_extension_for_plain_rename() {
        assert_eq!(
            resolved_download_name("openpage.txt", Some("renamed"), None),
            "renamed.txt"
        );
    }

    #[test]
    fn resolved_download_name_prefers_explicit_suffix() {
        assert_eq!(
            resolved_download_name("openpage.txt", Some("renamed"), Some(Some("md"))),
            "renamed.md"
        );
    }

    #[test]
    fn resolved_download_name_can_strip_extension() {
        assert_eq!(
            resolved_download_name("openpage.txt", Some("renamed"), Some(None)),
            "renamed"
        );
    }

    #[test]
    fn tab_like_type_filters_non_page_targets() {
        assert!(is_tab_like_type("page"));
        assert!(is_tab_like_type("tab"));
        assert!(!is_tab_like_type("service_worker"));
    }

    #[test]
    fn browser_cookie_param_keeps_scope_fields() {
        let cookie = browser_cookie_param(
            "foo",
            "bar",
            Some("https://example.com/demo"),
            Some("example.com"),
            Some("/demo"),
        );
        assert_eq!(cookie.name, "foo");
        assert_eq!(cookie.value, "bar");
        assert_eq!(cookie.url.as_deref(), Some("https://example.com/demo"));
        assert_eq!(cookie.domain.as_deref(), Some("example.com"));
        assert_eq!(cookie.path.as_deref(), Some("/demo"));
    }

    #[test]
    fn browser_cookie_header_to_params_sets_url_scope() {
        let url = Url::parse("https://example.com").expect("url");
        let cookies = browser_cookie_header_to_params(&url, "foo=bar; baz=qux");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].url.as_deref(), Some("https://example.com/"));
        assert_eq!(cookies[1].name, "baz");
    }

    #[test]
    fn browser_delete_cookie_params_skip_blank_scope_fields() {
        let params = browser_delete_cookie_params("foo", Some(""), Some("example.com"), Some(" "));
        assert_eq!(params.name, "foo");
        assert!(params.url.is_none());
        assert_eq!(params.domain.as_deref(), Some("example.com"));
        assert!(params.path.is_none());
    }

    #[test]
    fn launch_options_retry_defaults_match_reference_behavior() {
        let options = LaunchOptions::default();
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_times(), 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert_eq!(options.retry_interval(), 2.0);
    }

    #[test]
    fn launch_options_set_retry_updates_values() {
        let mut options = LaunchOptions::default();
        options.set_retry(Some(5), Some(250));
        assert_eq!(options.retry_times, 5);
        assert_eq!(options.retry_times(), 5);
        assert_eq!(options.retry_interval_millis, 250);
        assert_eq!(options.retry_interval(), 0.25);

        options.set_retry(None, Some(0));
        assert_eq!(options.retry_times, 5);
        assert_eq!(options.retry_times(), 5);
        assert_eq!(options.retry_interval_millis, 0);
        assert_eq!(options.retry_interval(), 0.0);
    }

    #[test]
    fn launch_options_set_timeouts_updates_timeout_config_in_millis() {
        let mut options = LaunchOptions::default();
        options.set_timeouts(Some(1.5), Some(12.0), Some(0.25));

        assert_eq!(options.timeouts.implicit_wait, 1_500);
        assert_eq!(options.timeouts.page_load, 12_000);
        assert_eq!(options.timeouts.script, 250);
        assert_eq!(options.timeouts().get("base"), Some(&1.5));
        assert_eq!(options.timeouts().get("page_load"), Some(&12.0));
        assert_eq!(options.timeouts().get("script"), Some(&0.25));

        options.set_timeouts(None, Some(0.0), None);
        assert_eq!(options.timeouts.implicit_wait, 1_500);
        assert_eq!(options.timeouts.page_load, 0);
        assert_eq!(options.timeouts.script, 250);
        assert_eq!(options.timeouts().get("base"), Some(&1.5));
        assert_eq!(options.timeouts().get("page_load"), Some(&0.0));
        assert_eq!(options.timeouts().get("script"), Some(&0.25));
    }

    #[test]
    fn launch_options_set_load_mode_parses_reference_values() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.load_mode(), "normal");

        options.set_load_mode("eager").expect("set eager mode");
        assert_eq!(options.load_mode, LoadMode::Eager);
        assert_eq!(options.load_mode(), "eager");

        options.set_load_mode("none").expect("set none mode");
        assert_eq!(options.load_mode, LoadMode::None);
        assert_eq!(options.load_mode(), "none");

        let error = options
            .set_load_mode("invalid")
            .expect_err("invalid load mode should fail");
        assert!(error.to_string().contains("load mode"));
    }

    #[test]
    fn launch_options_set_browser_path_updates_executable_path() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.browser_path(), "");

        options.set_browser_path("/tmp/test-browser");

        assert_eq!(
            options.browser_path,
            Some(PathBuf::from("/tmp/test-browser"))
        );
        assert_eq!(options.browser_path(), "/tmp/test-browser");
    }

    #[test]
    fn launch_options_set_download_path_updates_download_directory() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.download_path(), "");

        options.set_download_path("/tmp/downloads");

        assert_eq!(options.download_path, Some(PathBuf::from("/tmp/downloads")));
        assert_eq!(options.download_path(), "/tmp/downloads");
    }

    #[test]
    fn launch_options_set_tmp_path_updates_temp_directory() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.tmp_path(), "");

        options.set_tmp_path("/tmp/openpage-tmp");

        assert_eq!(options.tmp_path, Some(PathBuf::from("/tmp/openpage-tmp")));
        assert_eq!(options.tmp_path(), "/tmp/openpage-tmp");
    }

    #[test]
    fn launch_options_set_cache_path_updates_cache_directory() {
        let mut options = LaunchOptions::default();
        options.set_cache_path("/tmp/openpage-cache");

        assert_eq!(
            options.cache_path,
            Some(PathBuf::from("/tmp/openpage-cache"))
        );
    }

    #[test]
    fn launch_options_set_proxy_updates_proxy_setting() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.proxy(), None);

        options.set_proxy("http://localhost:1080");

        assert_eq!(options.proxy, Some("http://localhost:1080".to_string()));
        assert_eq!(options.proxy(), Some("http://localhost:1080"));
    }

    #[test]
    fn launch_options_set_user_agent_updates_user_agent_setting() {
        let mut options = LaunchOptions::default();
        options.set_user_agent("Mozilla/5.0 OpenPage");

        assert_eq!(options.user_agent, Some("Mozilla/5.0 OpenPage".to_string()));
    }

    #[test]
    fn launch_options_ignore_certificate_errors_updates_https_error_setting() {
        let mut options = LaunchOptions::default();

        options.ignore_certificate_errors(true);
        assert!(options.ignore_https_errors);

        options.ignore_certificate_errors(false);
        assert!(!options.ignore_https_errors);
    }

    #[test]
    fn launch_options_incognito_updates_incognito_setting() {
        let mut options = LaunchOptions::default();

        options.incognito(true);
        assert!(options.incognito);

        options.incognito(false);
        assert!(!options.incognito);
    }

    #[test]
    fn launch_options_headless_updates_headless_setting() {
        let mut options = LaunchOptions::default();
        assert!(!options.is_headless());

        options.headless(false);
        assert!(!options.headless);
        assert!(!options.is_headless());

        options.headless(true);
        assert!(options.headless);
        assert!(options.is_headless());
    }

    #[test]
    fn launch_options_no_imgs_updates_image_loading_setting() {
        let mut options = LaunchOptions::default();

        options.no_imgs(true);
        assert!(options.no_imgs);

        options.no_imgs(false);
        assert!(!options.no_imgs);
    }

    #[test]
    fn launch_options_no_js_updates_javascript_setting() {
        let mut options = LaunchOptions::default();

        options.no_js(true);
        assert!(options.no_js);

        options.no_js(false);
        assert!(!options.no_js);
    }

    #[test]
    fn launch_options_mute_updates_audio_setting() {
        let mut options = LaunchOptions::default();

        options.mute(true);
        assert!(options.mute);

        options.mute(false);
        assert!(!options.mute);
    }

    #[test]
    fn launch_options_existing_only_updates_existing_only_setting() {
        let mut options = LaunchOptions::default();
        assert!(!options.is_existing_only());

        options.existing_only(true);
        assert!(options.existing_only);
        assert!(options.is_existing_only());

        options.existing_only(false);
        assert!(!options.existing_only);
        assert!(!options.is_existing_only());
    }

    #[test]
    fn launch_options_auto_port_enables_auto_port_and_clears_debugger_address() {
        let mut options = LaunchOptions::default();
        options.set_address("wss://127.0.0.1:9222/devtools/browser/abc");
        options.set_user_data_path("/tmp/openpage-user-data");
        options.system_user_path = true;
        assert!(!options.is_auto_port());

        options.auto_port(true);

        assert!(options.auto_port);
        assert!(options.is_auto_port());
        assert_eq!(options.auto_port_scope(), Some(DEFAULT_AUTO_PORT_SCOPE));
        assert!(options.remote_debugging_port.is_none());
        assert!(options.address.is_none());
        assert!(options.ws_address.is_none());
        assert!(options.user_data_dir.is_none());
        assert!(!options.system_user_path);

        options.auto_port(false);
        assert!(!options.auto_port);
        assert!(!options.is_auto_port());
        assert!(options.auto_port_scope().is_none());
    }

    #[test]
    fn launch_options_auto_port_with_scope_tracks_custom_scope() {
        let mut options = LaunchOptions::default();
        options.set_address("wss://127.0.0.1:9222/devtools/browser/abc");
        options.set_user_data_path("/tmp/openpage-user-data");
        options.system_user_path = true;

        options
            .auto_port_with_scope(true, Some((19600, 19616)))
            .expect("set auto port scope");

        assert!(options.auto_port);
        assert!(options.is_auto_port());
        assert_eq!(options.auto_port_scope(), Some((19600, 19616)));
        assert!(options.remote_debugging_port.is_none());
        assert!(options.address.is_none());
        assert!(options.ws_address.is_none());
        assert!(options.user_data_dir.is_none());
        assert!(!options.system_user_path);
    }

    #[test]
    fn launch_options_auto_port_with_scope_rejects_invalid_range() {
        let mut options = LaunchOptions::default();

        let err = options
            .auto_port_with_scope(true, Some((19616, 19616)))
            .expect_err("invalid auto port scope should fail");

        assert!(matches!(
            err,
            crate::error::OpenPageError::BrowserOperation(_)
        ));
    }

    #[test]
    fn launch_options_set_user_data_path_updates_user_data_dir_and_disables_auto_port() {
        let mut options = LaunchOptions::default();
        options.auto_port = true;
        options.auto_port_scope = Some((19600, 19616));
        options.system_user_path = true;
        assert_eq!(options.user_data_path(), "");

        options.set_user_data_path("/tmp/openpage-user-data");

        assert_eq!(
            options.user_data_dir,
            Some(PathBuf::from("/tmp/openpage-user-data"))
        );
        assert_eq!(options.user_data_path(), "/tmp/openpage-user-data");
        assert!(!options.system_user_path);
        assert!(!options.auto_port);
        assert!(options.auto_port_scope.is_none());
    }

    #[test]
    fn launch_options_use_system_user_path_sets_and_clears_system_user_data_dir() {
        let mut options = LaunchOptions::default();
        let expected = system_user_data_dir();
        assert!(!options.system_user_path());

        options.use_system_user_path(true);
        assert_eq!(options.user_data_dir, expected);
        assert!(options.system_user_path);
        assert!(options.system_user_path());

        options.use_system_user_path(false);
        assert!(options.user_data_dir.is_none());
        assert!(!options.system_user_path);
        assert!(!options.system_user_path());
    }

    #[test]
    fn launch_options_use_system_user_path_false_keeps_custom_user_data_dir() {
        let mut options = LaunchOptions::default();
        options.set_user_data_path("/tmp/custom-user-data");

        options.use_system_user_path(false);

        assert_eq!(
            options.user_data_dir,
            Some(PathBuf::from("/tmp/custom-user-data"))
        );
    }

    #[test]
    fn launch_options_save_writes_ini_snapshot_to_directory_target() {
        let dir = make_temp_dir("launch-options-save");
        let mut options = LaunchOptions::default();
        options.set_download_path(dir.join("downloads"));
        options.set_tmp_path(dir.join("tmp"));
        options.set_cache_path(dir.join("cache"));
        options.set_user_data_path(dir.join("user-data"));
        options.set_address("127.0.0.1:9333");
        options.set_load_mode("eager").expect("set eager mode");
        options.set_user("Profile 2");
        options.auto_port(false);
        options.existing_only(true);
        options.new_env(true);
        options.headless(false);
        options.incognito(true);
        options.ignore_certificate_errors(true);
        options.no_imgs(true);
        options.no_js(true);
        options.mute(true);
        options.set_user_agent("OpenPageTest/1.0");
        options.set_proxy("http://127.0.0.1:7890");
        options.add_extension(dir.join("extension"));
        options.set_retry(Some(5), Some(2500));
        options.set_timeouts(Some(1.5), Some(12.0), Some(0.25));

        let saved_path = options
            .save(Some(dir.as_path()))
            .expect("save launch options");
        assert_eq!(saved_path, dir.join("config.ini"));

        let saved = fs::read_to_string(&saved_path).expect("read saved ini");
        assert!(saved.contains("[chromium_options]"));
        assert!(saved.contains("address = 127.0.0.1:9333"));
        assert!(saved.contains("load_mode = eager"));
        assert!(saved.contains("auto_port = False"));
        assert!(saved.contains("existing_only = True"));
        assert!(saved.contains("headless = False"));
        assert!(saved.contains("no_js = True"));
        assert!(saved.contains("retry_interval = 2.5"));
        assert!(saved.contains("user = Profile 2"));
        assert!(saved.contains("extensions = ["));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_save_to_default_writes_default_configs_ini() {
        let saved_path = default_launch_options_ini_path();
        let _guard = RestoreFileGuard::new(saved_path.clone());
        let mut options = LaunchOptions::default();
        options.set_user_agent("OpenPageSaveDefault/1.0");

        let returned = options
            .save_to_default()
            .expect("save launch options to default ini");

        assert_eq!(returned, saved_path);

        let saved = fs::read_to_string(&saved_path).expect("read saved default ini");
        assert!(saved.contains("user_agent = OpenPageSaveDefault/1.0"));
    }

    #[test]
    fn launch_options_save_preserves_auto_port_scope_in_ini_snapshot() {
        let dir = make_temp_dir("launch-options-save-auto-port-scope");
        let mut options = LaunchOptions::default();
        options
            .auto_port_with_scope(true, Some((19600, 19616)))
            .expect("set auto port scope");

        let saved_path = options
            .save(Some(dir.as_path()))
            .expect("save launch options with auto port scope");
        let saved = fs::read_to_string(&saved_path).expect("read saved ini");
        let loaded = LaunchOptions::from_ini(Some(saved_path.as_path()))
            .expect("load launch options with auto port scope");

        assert!(saved.contains("auto_port = (19600, 19616)"));
        assert_eq!(loaded.auto_port_scope(), Some((19600, 19616)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_from_ini_loads_saved_snapshot() {
        let dir = make_temp_dir("launch-options-from-ini");
        let mut options = LaunchOptions::default();
        options.set_download_path(dir.join("downloads"));
        options.set_tmp_path(dir.join("tmp"));
        options.set_cache_path(dir.join("cache"));
        options.set_user_data_path(dir.join("user-data"));
        options.set_address("127.0.0.1:9333");
        options.set_load_mode("eager").expect("set eager mode");
        options.set_user("Profile 2");
        options.existing_only(true);
        options.new_env(true);
        options.headless(false);
        options.incognito(true);
        options.ignore_certificate_errors(true);
        options.no_imgs(true);
        options.no_js(true);
        options.mute(true);
        options.set_user_agent("OpenPageFromIni/1.0");
        options.set_proxy("http://127.0.0.1:7890");
        options.set_argument("--start-maximized");
        options.add_extension(dir.join("extension"));
        options.set_pref("profile.default_content_settings.popups", json!(0));
        options.set_flag("test-flag@1");
        options.set_retry(Some(5), Some(2500));
        options.set_timeouts(Some(1.5), Some(12.0), Some(0.25));

        let saved_path = options
            .save(Some(dir.as_path()))
            .expect("save launch options");
        let loaded = LaunchOptions::from_ini(Some(saved_path.as_path()))
            .expect("load launch options from ini");

        assert_eq!(
            loaded.download_path(),
            dir.join("downloads").to_string_lossy()
        );
        assert_eq!(loaded.tmp_path(), dir.join("tmp").to_string_lossy());
        assert_eq!(loaded.address(), "127.0.0.1:9333");
        assert_eq!(
            loaded.user_data_path(),
            dir.join("user-data").to_string_lossy()
        );
        assert_eq!(loaded.load_mode(), "eager");
        assert_eq!(loaded.user(), "Profile 2");
        assert!(loaded.is_existing_only());
        assert!(loaded.new_env);
        assert!(!loaded.is_headless());
        assert!(loaded.incognito);
        assert!(loaded.ignore_https_errors);
        assert!(loaded.no_imgs);
        assert!(loaded.no_js);
        assert!(loaded.mute);
        assert_eq!(loaded.proxy(), Some("http://127.0.0.1:7890"));
        assert_eq!(loaded.retry_times(), 5);
        assert_eq!(loaded.retry_interval(), 2.5);
        assert_eq!(loaded.timeouts().get("base"), Some(&1.5));
        assert_eq!(loaded.timeouts().get("page_load"), Some(&12.0));
        assert_eq!(loaded.timeouts().get("script"), Some(&0.25));
        assert!(
            loaded
                .arguments()
                .contains(&"--start-maximized".to_string())
        );
        assert_eq!(loaded.extensions().len(), 1);
        assert_eq!(
            loaded
                .preferences()
                .get("profile.default_content_settings.popups"),
            Some(&json!(0))
        );
        assert_eq!(loaded.flags, vec!["test-flag@1".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_from_ini_none_loads_default_configs_file() {
        let options = LaunchOptions::from_ini(None).expect("load default configs ini");

        assert_eq!(options.address(), "127.0.0.1:9222");
        assert_eq!(options.browser_path(), "chrome");
        assert_eq!(options.load_mode(), "normal");
        assert!(!options.is_headless());
        assert_eq!(options.user(), "Default");
        assert!(
            options
                .arguments()
                .contains(&"--no-default-browser-check".to_string())
        );
        assert_eq!(
            options
                .preferences()
                .get("profile.default_content_settings.popups"),
            Some(&json!(0))
        );
        assert_eq!(
            options
                .preferences()
                .get("profile.default_content_setting_values"),
            Some(&json!({"notifications": 2}))
        );
        assert!(options.flags.is_empty());
    }

    #[test]
    fn launch_options_from_ini_none_prefers_project_dp_configs_file() {
        let dir = make_temp_dir("launch-options-project-config");
        let project_ini = dir.join("dp_configs.ini");
        fs::write(
            &project_ini,
            "[chromium_options]\naddress = 127.0.0.1:9555\nbrowser_path = project-chrome\n",
        )
        .expect("write project configs ini");
        let _guard = CurrentDirGuard::change_to(dir.as_path());

        let resolved = resolve_launch_options_ini_path(None).expect("resolve project ini path");
        let options = LaunchOptions::from_ini(None).expect("load project configs ini");

        assert_eq!(
            fs::canonicalize(&resolved).expect("canonicalize resolved project ini"),
            fs::canonicalize(&project_ini).expect("canonicalize expected project ini")
        );
        assert_eq!(options.address(), "127.0.0.1:9555");
        assert_eq!(options.browser_path(), "project-chrome");
        assert_eq!(
            options
                .source_ini_path
                .as_ref()
                .and_then(|path| fs::canonicalize(path).ok()),
            Some(fs::canonicalize(&project_ini).expect("canonicalize source project ini"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_from_ini_options_false_returns_defaults_without_reading_file() {
        let dir = make_temp_dir("launch-options-from-ini-options-false");
        let config_path = dir.join("ignored.ini");
        let mut options = LaunchOptions::default();
        options.set_user_agent("IgnoredByReadFileFalse/1.0");
        options
            .save(Some(config_path.as_path()))
            .expect("write ignored ini");

        let loaded = LaunchOptions::from_ini_options(false, Some(config_path.as_path()))
            .expect("create default launch options");

        assert_eq!(loaded.address(), "127.0.0.1:9222");
        assert_eq!(loaded.browser_path(), "chrome");
        assert!(!loaded.is_headless());
        assert!(
            loaded
                .arguments()
                .contains(&"--no-default-browser-check".to_string())
        );
        assert_eq!(
            loaded
                .preferences()
                .get("profile.default_content_settings.popups"),
            Some(&json!(0))
        );
        assert!(loaded.user_agent.is_none());
        assert!(loaded.source_ini_path.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_from_ini_options_true_none_reads_default_ini() {
        let options = LaunchOptions::from_ini_options(true, None).expect("load default ini");

        assert_eq!(options.address(), "127.0.0.1:9222");
        assert_eq!(options.load_mode(), "normal");
        assert!(options.source_ini_path.is_some());
    }

    #[test]
    fn launch_options_from_ini_options_true_reads_specified_ini_path() {
        let dir = make_temp_dir("launch-options-from-ini-options-true");
        let config_path = dir.join("custom.ini");
        let mut options = LaunchOptions::default();
        options.set_user_agent("ReadSpecifiedIni/1.0");
        options.set_user("Profile 9");
        options
            .save(Some(config_path.as_path()))
            .expect("write custom ini");

        let loaded = LaunchOptions::from_ini_options(true, Some(config_path.as_path()))
            .expect("load specified ini");

        assert_eq!(loaded.user(), "Profile 9");
        assert_eq!(loaded.user_agent.as_deref(), Some("ReadSpecifiedIni/1.0"));
        assert_eq!(
            loaded.source_ini_path.as_deref(),
            Some(config_path.as_path())
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_save_without_path_reuses_loaded_ini_path() {
        let dir = make_temp_dir("launch-options-save-current-ini");
        let config_path = dir.join("current.ini");
        let mut options = LaunchOptions::default();
        options.set_user_agent("OpenPageBeforeSave/1.0");
        options
            .save(Some(config_path.as_path()))
            .expect("write current ini");

        let mut loaded =
            LaunchOptions::from_ini(Some(config_path.as_path())).expect("load current ini");
        loaded.set_user_agent("OpenPageAfterSave/2.0");

        let saved_path = loaded.save(None).expect("save back to current ini");

        assert_eq!(saved_path, config_path);
        let saved = fs::read_to_string(&saved_path).expect("read saved current ini");
        assert!(saved.contains("user_agent = OpenPageAfterSave/2.0"));
        assert!(!saved.contains("OpenPageBeforeSave/1.0"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_from_ini_loads_reference_drissionpage_configs_file() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let config_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");

        let options = LaunchOptions::from_ini(Some(config_path.as_path()))
            .expect("load DrissionPage reference configs ini");

        assert_eq!(options.address(), "127.0.0.1:9222");
        assert_eq!(options.browser_path(), "chrome");
        assert_eq!(options.load_mode(), "normal");
        assert_eq!(options.user(), "Default");
        assert_eq!(options.timeouts().get("base"), Some(&10.0));
        assert_eq!(options.timeouts().get("page_load"), Some(&30.0));
        assert_eq!(options.timeouts().get("script"), Some(&30.0));
        assert_eq!(options.retry_times(), 3);
        assert_eq!(options.retry_interval(), 2.0);
        assert!(
            options
                .arguments()
                .contains(&"--no-default-browser-check".to_string())
        );
        assert_eq!(
            options
                .preferences()
                .get("profile.default_content_settings.popups"),
            Some(&json!(0))
        );
        assert!(options.flags.is_empty());
    }

    #[test]
    fn launch_options_save_preserves_non_browser_sections_from_template_ini() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let source_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");
        let dir = make_temp_dir("launch-options-save-preserve-template-sections");
        let target_path = dir.join("copied.ini");

        let mut options = LaunchOptions::from_ini(Some(source_path.as_path()))
            .expect("load DrissionPage reference configs ini");
        options.set_user_agent("OpenPageTemplateSave/1.0");

        let saved_path = options
            .save(Some(target_path.as_path()))
            .expect("save copied ini");
        let saved = fs::read_to_string(&saved_path).expect("read copied ini");

        assert_eq!(saved_path, target_path);
        assert!(saved.contains("[session_options]"));
        assert!(saved.contains("headers = {'user-agent':"));
        assert!(saved.contains("user_agent = OpenPageTemplateSave/1.0"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_add_extension_tracks_extension_path() {
        let mut options = LaunchOptions::default();
        assert!(options.extensions().is_empty());

        options.add_extension("/tmp/openpage-extension");
        let expected = vec![PathBuf::from("/tmp/openpage-extension")];

        assert_eq!(options.extensions(), expected.as_slice());
        assert_eq!(options.extensions, expected);
    }

    #[test]
    fn launch_options_set_local_port_updates_debug_port_and_disables_auto_port() {
        let mut options = LaunchOptions::default();
        options.auto_port = true;
        options.auto_port_scope = Some((19600, 19616));

        options.set_local_port(9333);

        assert_eq!(options.remote_debugging_port, Some(9333));
        assert_eq!(options.address.as_deref(), Some("127.0.0.1:9333"));
        assert!(options.ws_address.is_none());
        assert!(!options.auto_port);
        assert!(options.auto_port_scope.is_none());
    }

    #[test]
    fn launch_options_address_returns_default_when_unset() {
        let options = LaunchOptions::default();

        assert_eq!(options.address(), "127.0.0.1:9222");
    }

    #[test]
    fn launch_options_address_returns_explicit_debugger_address() {
        let mut options = LaunchOptions::default();

        options.set_address("wss://localhost:9222/devtools/browser/abc");

        assert_eq!(options.address(), "127.0.0.1:9222");
    }

    #[test]
    fn launch_options_set_address_normalizes_local_host_and_disables_auto_port() {
        let mut options = LaunchOptions::default();
        options.auto_port = true;
        options.auto_port_scope = Some((19600, 19616));

        options.set_address("http://localhost:9222/");

        assert_eq!(options.address.as_deref(), Some("127.0.0.1:9222"));
        assert_eq!(options.remote_debugging_port, Some(9222));
        assert!(options.ws_address.is_none());
        assert!(!options.auto_port);
        assert!(options.auto_port_scope.is_none());
    }

    #[test]
    fn validate_auto_port_scope_rejects_zero_or_reversed_ranges() {
        assert!(validate_auto_port_scope((0, 9601)).is_err());
        assert!(validate_auto_port_scope((9601, 9601)).is_err());
        assert!(validate_auto_port_scope((9602, 9601)).is_err());
        assert!(validate_auto_port_scope((9600, 9601)).is_ok());
    }

    #[test]
    fn find_free_port_uses_requested_scope() {
        use std::net::TcpListener;

        for _ in 0..128 {
            let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupied port");
            let start = occupied.local_addr().expect("occupied addr").port();
            let Some(next_port) = start.checked_add(1) else {
                continue;
            };
            let Some(end_port) = next_port.checked_add(1) else {
                continue;
            };
            let Ok(candidate) = TcpListener::bind(("127.0.0.1", next_port)) else {
                continue;
            };
            drop(candidate);

            let found = find_free_port(Some((start, end_port))).expect("find scoped port");
            assert_eq!(found, next_port);
            return;
        }

        panic!("failed to find adjacent ports for scoped auto_port test");
    }

    #[test]
    fn resolve_launch_user_data_dir_prefers_temp_dir_when_auto_port_enabled() {
        let dir = make_temp_dir("auto-port-user-data");
        let mut options = LaunchOptions::default();
        options.set_tmp_path(&dir);
        options.set_user_data_path(dir.join("fixed-user-data"));
        options.auto_port(true);

        let (resolved, use_temp_dir) =
            resolve_launch_user_data_dir(&options).expect("resolve auto port user data dir");
        let resolved = resolved.expect("resolved user data dir");

        assert!(use_temp_dir);
        assert!(resolved.starts_with(&dir));
        assert_ne!(resolved, dir.join("fixed-user-data"));

        let _ = fs::remove_dir_all(&resolved);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_options_set_address_accepts_ws_url() {
        let mut options = LaunchOptions::default();

        options.set_address("wss://localhost:9222/devtools/browser/abc");

        assert_eq!(options.address.as_deref(), Some("127.0.0.1:9222"));
        assert_eq!(
            options.ws_address.as_deref(),
            Some("wss://127.0.0.1:9222/devtools/browser/abc")
        );
        assert_eq!(options.remote_debugging_port, Some(9222));
        assert!(!options.auto_port);
    }

    #[test]
    fn launch_options_new_env_sets_switch() {
        let mut options = LaunchOptions::default();
        options.new_env(true);
        assert!(options.new_env);

        options.new_env(false);
        assert!(!options.new_env);
    }

    #[test]
    fn reset_browser_user_data_dir_removes_existing_directory_contents() {
        let dir = make_temp_dir("new-env");
        let nested = dir.join("Default");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(nested.join("Preferences"), "{}").expect("write prefs");

        reset_browser_user_data_dir(&dir).expect("reset user data dir");

        assert!(!dir.exists());
    }

    #[test]
    fn launch_options_remove_argument_drops_matching_valued_argument() {
        let mut options = LaunchOptions::default();
        options.set_argument("--window-size=800,600");
        options.set_argument("--user-data-dir=/tmp/openpage");
        let expected_before_remove = vec![
            "--window-size=800,600".to_string(),
            "--user-data-dir=/tmp/openpage".to_string(),
        ];

        assert_eq!(options.arguments(), expected_before_remove.as_slice());

        options.remove_argument("--window-size");
        let expected_after_remove = vec!["--user-data-dir=/tmp/openpage".to_string()];

        assert!(!options.args.contains(&"--window-size=800,600".to_string()));
        assert!(
            options
                .args
                .contains(&"--user-data-dir=/tmp/openpage".to_string())
        );
        assert_eq!(options.arguments(), expected_after_remove.as_slice());
    }

    #[test]
    fn launch_options_set_user_replaces_profile_directory_argument() {
        let mut options = LaunchOptions::default();
        options.set_argument("--profile-directory=Profile 1");

        options.set_user("Default");

        assert_eq!(
            options
                .args
                .iter()
                .filter(|arg| arg.starts_with("--profile-directory="))
                .count(),
            1
        );
        assert!(
            options
                .args
                .contains(&"--profile-directory=Default".to_string())
        );
    }

    #[test]
    fn launch_options_user_returns_default_when_unset() {
        let options = LaunchOptions::default();

        assert_eq!(options.user(), "Default");
    }

    #[test]
    fn launch_options_user_returns_selected_profile_directory() {
        let mut options = LaunchOptions::default();

        options.set_user("Profile 3");

        assert_eq!(options.user(), "Profile 3");
    }

    #[test]
    fn launch_options_remove_pref_from_file_tracks_keys() {
        let mut options = LaunchOptions::default();
        options.remove_pref_from_file("profile.default_content_settings.popups");

        assert_eq!(
            options.prefs_to_remove,
            vec!["profile.default_content_settings.popups".to_string()]
        );
    }

    #[test]
    fn launch_options_preferences_reflect_pref_mutations() {
        let mut options = LaunchOptions::default();
        assert!(options.preferences().is_empty());

        options.set_pref("profile.default_content_settings.popups", json!(0));
        options.set_pref("credentials_enable_service", json!(false));
        assert_eq!(
            options
                .preferences()
                .get("profile.default_content_settings.popups"),
            Some(&json!(0))
        );
        assert_eq!(
            options.preferences().get("credentials_enable_service"),
            Some(&json!(false))
        );

        options.remove_pref("profile.default_content_settings.popups");
        assert!(
            options
                .preferences()
                .get("profile.default_content_settings.popups")
                .is_none()
        );
        assert_eq!(
            options.preferences().get("credentials_enable_service"),
            Some(&json!(false))
        );

        options.clear_prefs();
        assert!(options.preferences().is_empty());
    }

    #[test]
    fn launch_options_clear_flags_in_file_sets_switch() {
        let mut options = LaunchOptions::default();
        options.clear_flags_in_file();

        assert!(options.clear_file_flags);
    }

    #[test]
    fn write_chrome_prefs_uses_profile_directory_and_nested_pref_paths() {
        let dir = make_temp_dir("prefs");
        let profile_dir = dir.join("Profile 1");
        fs::create_dir_all(&profile_dir).expect("create profile dir");
        let prefs_path = profile_dir.join("Preferences");
        fs::write(
            &prefs_path,
            json!({
                "profile": {
                    "default_content_settings": {
                        "popups": 1,
                        "other": 2
                    }
                },
                "credentials_enable_service": true
            })
            .to_string(),
        )
        .expect("write prefs");

        let mut prefs = HashMap::new();
        prefs.insert(
            "profile.default_content_settings.notifications".to_string(),
            json!(2),
        );
        prefs.insert("credentials_enable_service".to_string(), json!(false));
        let prefs_to_remove = vec!["profile.default_content_settings.popups".to_string()];

        write_chrome_prefs(
            &dir,
            &["--profile-directory=Profile 1".to_string()],
            &prefs,
            &prefs_to_remove,
        )
        .expect("write chrome prefs");

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&prefs_path).expect("read updated prefs"))
                .expect("parse updated prefs");
        assert_eq!(saved["credentials_enable_service"], json!(false));
        assert_eq!(
            saved["profile"]["default_content_settings"]["notifications"],
            json!(2)
        );
        assert_eq!(
            saved["profile"]["default_content_settings"]["other"],
            json!(2)
        );
        assert!(
            saved["profile"]["default_content_settings"]
                .get("popups")
                .is_none()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_chrome_flags_merges_existing_flags_by_default() {
        let dir = make_temp_dir("flags-merge");
        let state_path = dir.join("Local State");
        fs::write(
            &state_path,
            json!({"browser": {"enabled_labs_experiments": ["existing", "kept"]}}).to_string(),
        )
        .expect("write state");

        write_chrome_flags(&dir, &["existing".to_string(), "new".to_string()], false)
            .expect("write chrome flags");

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&state_path).expect("read updated state"))
                .expect("parse updated state");
        assert_eq!(
            saved["browser"]["enabled_labs_experiments"],
            json!(["existing", "kept", "new"])
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_chrome_flags_can_clear_existing_file_flags() {
        let dir = make_temp_dir("flags-clear");
        let state_path = dir.join("Local State");
        fs::write(
            &state_path,
            json!({"browser": {"enabled_labs_experiments": ["existing", "kept"]}}).to_string(),
        )
        .expect("write state");

        write_chrome_flags(&dir, &["new".to_string()], true).expect("write chrome flags");

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&state_path).expect("read updated state"))
                .expect("parse updated state");
        assert_eq!(saved["browser"]["enabled_labs_experiments"], json!(["new"]));

        let _ = fs::remove_dir_all(&dir);
    }
}
