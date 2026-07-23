use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::browser::{LaunchOptions, OPENPAGE_BROWSER_PATH_ENV};
use crate::error::{OpenPageError, OpenPageResult};
use crate::session::SessionOptions;
use crate::settings::{
    config_path_empty_message, config_root_table_required_message,
    config_section_table_required_message, invalid_config_file_message, invalid_toml_file_message,
};

pub const OPENPAGE_CONFIG_ENV: &str = "OPENPAGE_CONFIG";
pub const OPENPAGE_BROWSER_USER_DATA_DIR_ENV: &str = "OPENPAGE_USER_DATA_DIR";
pub const OPENPAGE_BROWSER_HEADLESS_ENV: &str = "OPENPAGE_HEADLESS";
pub const OPENPAGE_BROWSER_WIDTH_ENV: &str = "OPENPAGE_WIDTH";
pub const OPENPAGE_BROWSER_HEIGHT_ENV: &str = "OPENPAGE_HEIGHT";
pub const OPENPAGE_BROWSER_NO_SANDBOX_ENV: &str = "OPENPAGE_NO_SANDBOX";
pub const OPENPAGE_SESSION_TIMEOUT_SECS_ENV: &str = "OPENPAGE_SESSION_TIMEOUT_SECS";
pub const OPENPAGE_SESSION_USER_AGENT_ENV: &str = "OPENPAGE_SESSION_USER_AGENT";
const DEFAULT_DEBUGGER_ADDRESS: &str = "127.0.0.1:9222";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValueSource {
    BuiltInDefault,
    UserConfig,
    WorkspaceConfig,
    Environment,
}

impl ConfigValueSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltInDefault => "default",
            Self::UserConfig => "user_config",
            Self::WorkspaceConfig => "workspace_config",
            Self::Environment => "environment",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub launch: LaunchOptions,
    pub session: SessionOptions,
    pub user_config_path: PathBuf,
    pub workspace_config_path: PathBuf,
    pub loaded_user_config: bool,
    pub loaded_workspace_config: bool,
    pub browser_path_source: ConfigValueSource,
    pub debugger_source: ConfigValueSource,
    pub user_data_dir_source: ConfigValueSource,
}

#[derive(Debug, Clone)]
pub struct ConfigOverrides {
    pub browser_path: Option<PathBuf>,
    pub user_data_dir: Option<PathBuf>,
    pub local_port: Option<u16>,
    pub headless: Option<bool>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub no_sandbox: Option<bool>,
    pub incognito: Option<bool>,
    pub mute: Option<bool>,
}

impl ConfigOverrides {
    pub fn apply_to_launch(&self, launch: &mut LaunchOptions) {
        if let Some(path) = self.browser_path.as_ref() {
            launch.browser_path = Some(path.clone());
        }
        if let Some(path) = self.user_data_dir.as_ref() {
            launch.user_data_dir = Some(path.clone());
        }
        if let Some(port) = self.local_port {
            launch.set_local_port(port);
        }
        if let Some(value) = self.headless {
            launch.headless = value;
        }
        if let Some(value) = self.width {
            launch.width = value;
        }
        if let Some(value) = self.height {
            launch.height = value;
        }
        if let Some(value) = self.no_sandbox {
            launch.no_sandbox = value;
        }
        if let Some(value) = self.incognito {
            launch.incognito = value;
        }
        if let Some(value) = self.mute {
            launch.mute = value;
        }
    }
}

impl Default for ConfigOverrides {
    fn default() -> Self {
        Self {
            browser_path: None,
            user_data_dir: None,
            local_port: None,
            headless: None,
            width: None,
            height: None,
            no_sandbox: None,
            incognito: None,
            mute: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ConfigFile {
    browser: BrowserConfig,
    session: SessionConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct BrowserConfig {
    executable_path: Option<PathBuf>,
    address: Option<String>,
    local_port: Option<u16>,
    user_data_dir: Option<PathBuf>,
    download_path: Option<PathBuf>,
    existing_only: Option<bool>,
    disable_default_args: Option<bool>,
    arguments: Option<Vec<String>>,
    headless: Option<bool>,
    width: Option<u32>,
    height: Option<u32>,
    no_sandbox: Option<bool>,
    auto_port: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct SessionConfig {
    timeout_secs: Option<u64>,
    user_agent: Option<String>,
    download_path: Option<PathBuf>,
    retry_times: Option<usize>,
    retry_interval_millis: Option<u64>,
}

pub fn openpage_home() -> OpenPageResult<PathBuf> {
    if let Some(value) = env::var_os("OPENPAGE_HOME") {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = env::var_os("HOME") {
        return Ok(PathBuf::from(value).join(".openpage"));
    }
    if let Some(value) = env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(value).join(".openpage"));
    }
    if let (Some(drive), Some(path)) = (env::var_os("HOMEDRIVE"), env::var_os("HOMEPATH")) {
        let mut home = PathBuf::from(drive);
        home.push(path);
        return Ok(home.join(".openpage"));
    }
    Err(OpenPageError::Io(
        "OPENPAGE_HOME or HOME (or USERPROFILE on Windows) must be set".to_string(),
    ))
}

pub fn user_config_path() -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?.join("config.toml"))
}

pub fn workspace_config_path() -> OpenPageResult<PathBuf> {
    Ok(env::current_dir()?.join(".openpage").join("config.toml"))
}

pub fn resolve_config_path(path: &str) -> OpenPageResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(OpenPageError::BrowserOperation(config_path_empty_message(
            OPENPAGE_CONFIG_ENV,
        )));
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

pub fn load_resolved_config() -> OpenPageResult<ResolvedConfig> {
    let home = openpage_home()?;
    let mut launch = default_launch_options(&home);
    let mut session = SessionOptions::default();

    let user_path = user_config_path()?;
    let workspace_path = workspace_config_path()?;
    let mut loaded_user = false;
    let mut loaded_workspace = false;
    let mut browser_path_source = ConfigValueSource::BuiltInDefault;
    let mut debugger_source = ConfigValueSource::BuiltInDefault;
    let mut user_data_dir_source = ConfigValueSource::BuiltInDefault;

    if let Ok(custom_path) = env::var(OPENPAGE_CONFIG_ENV) {
        let path = resolve_config_path(&custom_path)?;
        let config = load_config_file(path.as_path())?;
        apply_config_file(
            &config,
            &mut launch,
            &mut session,
            ConfigValueSource::WorkspaceConfig,
            &mut browser_path_source,
            &mut debugger_source,
            &mut user_data_dir_source,
        );
        return apply_env_overrides(ResolvedConfig {
            launch,
            session,
            user_config_path: user_path,
            workspace_config_path: path,
            loaded_user_config: false,
            loaded_workspace_config: true,
            browser_path_source,
            debugger_source,
            user_data_dir_source,
        });
    }

    if user_path.is_file() {
        let config = load_config_file(user_path.as_path())?;
        apply_config_file(
            &config,
            &mut launch,
            &mut session,
            ConfigValueSource::UserConfig,
            &mut browser_path_source,
            &mut debugger_source,
            &mut user_data_dir_source,
        );
        loaded_user = true;
    }

    if workspace_path.is_file() {
        let config = load_config_file(workspace_path.as_path())?;
        apply_config_file(
            &config,
            &mut launch,
            &mut session,
            ConfigValueSource::WorkspaceConfig,
            &mut browser_path_source,
            &mut debugger_source,
            &mut user_data_dir_source,
        );
        loaded_workspace = true;
    }

    apply_env_overrides(ResolvedConfig {
        launch,
        session,
        user_config_path: user_path,
        workspace_config_path: workspace_path,
        loaded_user_config: loaded_user,
        loaded_workspace_config: loaded_workspace,
        browser_path_source,
        debugger_source,
        user_data_dir_source,
    })
}

pub fn browser_exec_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            PathBuf::from(
                "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            ),
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
        ]);
    }
    #[cfg(target_os = "linux")]
    {
        for name in [
            "google-chrome",
            "google-chrome-stable",
            "chromium-browser",
            "chromium",
            "brave-browser",
            "brave-browser-stable",
        ] {
            if let Some(path) = find_in_path(name) {
                candidates.push(path);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            candidates.extend([
                base.join(r"Google\Chrome\Application\chrome.exe"),
                base.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"),
                base.join(r"Chromium\Application\chrome.exe"),
            ]);
        }
        candidates.extend([
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
            PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"),
        ]);
    }
    dedup_existing_paths(candidates)
}

pub fn resolve_browser_executable_path(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        if path.is_absolute() || path.components().count() > 1 {
            return path.exists().then(|| path.to_path_buf());
        }
        if let Some(found) = find_in_path(path.to_string_lossy().as_ref()) {
            return Some(found);
        }
    }
    browser_exec_candidates().into_iter().next()
}

pub fn update_user_browser_paths(
    browser_path: Option<&Path>,
    user_data_dir: Option<&Path>,
) -> OpenPageResult<PathBuf> {
    let path = match env::var(OPENPAGE_CONFIG_ENV) {
        Ok(custom_path) => resolve_config_path(&custom_path)?,
        Err(_) => user_config_path()?,
    };
    let mut root = load_toml_value(path.as_path())?;
    let browser = ensure_table_entry(&mut root, "browser")?;
    if let Some(value) = browser_path {
        browser.insert(
            "executable_path".to_string(),
            toml::Value::String(value.to_string_lossy().to_string()),
        );
    }
    if let Some(value) = user_data_dir {
        browser.insert(
            "user_data_dir".to_string(),
            toml::Value::String(value.to_string_lossy().to_string()),
        );
    }
    save_toml_value(path.as_path(), &root)?;
    Ok(path)
}

pub fn ensure_workspace_config_file() -> OpenPageResult<PathBuf> {
    let path = workspace_config_path()?;
    if path.is_file() {
        return Ok(path);
    }
    let mut root = load_toml_value(path.as_path())?;
    let _ = ensure_table_entry(&mut root, "browser")?;
    save_toml_value(path.as_path(), &root)?;
    Ok(path)
}

fn apply_env_overrides(mut resolved: ResolvedConfig) -> OpenPageResult<ResolvedConfig> {
    if let Some(value) = env::var_os(OPENPAGE_BROWSER_PATH_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            resolved.launch.browser_path = Some(path);
            resolved.browser_path_source = ConfigValueSource::Environment;
        }
    }
    if let Some(path) = env::var_os(OPENPAGE_BROWSER_USER_DATA_DIR_ENV) {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            resolved.launch.user_data_dir = Some(path);
            resolved.user_data_dir_source = ConfigValueSource::Environment;
        }
    }
    if let Some(value) = env::var(OPENPAGE_BROWSER_HEADLESS_ENV)
        .ok()
        .and_then(|raw| parse_bool(raw.as_str()))
    {
        resolved.launch.headless = value;
    }
    if let Some(value) = env::var(OPENPAGE_BROWSER_WIDTH_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        resolved.launch.width = value;
    }
    if let Some(value) = env::var(OPENPAGE_BROWSER_HEIGHT_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        resolved.launch.height = value;
    }
    if let Some(value) = env::var(OPENPAGE_BROWSER_NO_SANDBOX_ENV)
        .ok()
        .and_then(|raw| parse_bool(raw.as_str()))
    {
        resolved.launch.no_sandbox = value;
    }
    if let Some(value) = env::var(OPENPAGE_SESSION_TIMEOUT_SECS_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
    {
        resolved.session.timeout_secs = value;
    }
    if let Some(value) = env::var(OPENPAGE_SESSION_USER_AGENT_ENV).ok() {
        resolved.session.user_agent = Some(value);
    }
    Ok(resolved)
}

fn apply_config_file(
    config: &ConfigFile,
    launch: &mut LaunchOptions,
    session: &mut SessionOptions,
    source: ConfigValueSource,
    browser_path_source: &mut ConfigValueSource,
    debugger_source: &mut ConfigValueSource,
    user_data_dir_source: &mut ConfigValueSource,
) {
    if let Some(path) = config.browser.executable_path.as_ref() {
        launch.browser_path = Some(path.clone());
        *browser_path_source = source;
    }
    if let Some(address) = config.browser.address.as_deref() {
        launch.set_address(address);
        *debugger_source = source;
    }
    if let Some(port) = config.browser.local_port {
        launch.set_local_port(port);
        *debugger_source = source;
    }
    if let Some(path) = config.browser.user_data_dir.as_ref() {
        launch.user_data_dir = Some(path.clone());
        *user_data_dir_source = source;
    }
    if let Some(path) = config.browser.download_path.as_ref() {
        launch.download_path = Some(path.clone());
    }
    if let Some(value) = config.browser.existing_only {
        launch.existing_only(value);
    }
    if let Some(value) = config.browser.disable_default_args {
        launch.disable_default_args = value;
    }
    if let Some(args) = config.browser.arguments.as_ref() {
        launch.args = args.clone();
    }
    if let Some(value) = config.browser.headless {
        launch.headless = value;
    }
    if let Some(value) = config.browser.width {
        launch.width = value;
    }
    if let Some(value) = config.browser.height {
        launch.height = value;
    }
    if let Some(value) = config.browser.no_sandbox {
        launch.no_sandbox = value;
    }
    if let Some(value) = config.browser.auto_port {
        launch.auto_port(value);
    }

    if let Some(value) = config.session.timeout_secs {
        session.timeout_secs = value;
    }
    if let Some(value) = config.session.user_agent.as_ref() {
        session.user_agent = Some(value.clone());
    }
    if let Some(path) = config.session.download_path.as_ref() {
        session.download_path = path.clone();
    }
    if let Some(value) = config.session.retry_times {
        session.retry_times = value;
    }
    if let Some(value) = config.session.retry_interval_millis {
        session.retry_interval_millis = value;
    }
}

fn load_config_file(path: &Path) -> OpenPageResult<ConfigFile> {
    let content = std::fs::read_to_string(path)?;
    toml::from_str::<ConfigFile>(&content).map_err(|err| {
        OpenPageError::Serialization(invalid_config_file_message(
            &path.display().to_string(),
            &err.to_string(),
        ))
    })
}

fn load_toml_value(path: &Path) -> OpenPageResult<toml::Value> {
    if path.is_file() {
        let content = std::fs::read_to_string(path)?;
        toml::from_str::<toml::Value>(&content).map_err(|err| {
            OpenPageError::Serialization(invalid_toml_file_message(
                &path.display().to_string(),
                &err.to_string(),
            ))
        })
    } else {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }
}

fn save_toml_value(path: &Path, value: &toml::Value) -> OpenPageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(value)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    std::fs::write(path, content)?;
    Ok(())
}

fn ensure_table_entry<'a>(
    value: &'a mut toml::Value,
    key: &str,
) -> OpenPageResult<&'a mut toml::map::Map<String, toml::Value>> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| OpenPageError::Serialization(config_root_table_required_message()))?;
    let entry = table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    entry
        .as_table_mut()
        .ok_or_else(|| OpenPageError::Serialization(config_section_table_required_message(key)))
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn default_launch_options(home: &Path) -> LaunchOptions {
    let mut launch = LaunchOptions::default();
    launch.set_address(DEFAULT_DEBUGGER_ADDRESS);
    launch.set_user_data_path(home.join("profiles").join("default"));
    launch.disable_default_args = true;
    launch.args = vec![
        "no-default-browser-check".to_string(),
        "disable-suggestions-ui".to_string(),
        "no-first-run".to_string(),
        "disable-popup-blocking".to_string(),
        "hide-crash-restore-bubble".to_string(),
        "disable-features=PrivacySandboxSettings4".to_string(),
    ];
    launch
}

fn dedup_existing_paths(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in candidates {
        if !path.exists() {
            continue;
        }
        if deduped.iter().any(|existing| existing == &path) {
            continue;
        }
        deduped.push(path);
    }
    deduped
}

fn find_in_path(executable: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(executable);
        if candidate.exists() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            for suffix in [".exe", ".cmd", ".bat"] {
                let candidate = dir.join(format!("{executable}{suffix}"));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ConfigValueSource, OPENPAGE_BROWSER_PATH_ENV, OPENPAGE_CONFIG_ENV, ensure_table_entry,
        load_config_file, load_resolved_config, load_toml_value, resolve_config_path,
    };
    use crate::Settings;
    use crate::settings::scoped_test_settings;

    static CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.as_ref() {
                unsafe {
                    std::env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    struct CurrentDirGuard {
        previous: std::path::PathBuf,
    }

    impl CurrentDirGuard {
        fn set(path: &std::path::Path) -> Self {
            let previous = std::env::current_dir().expect("current dir");
            std::env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("openpage-config-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn resolved_config_uses_workspace_over_user() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let home = temp_dir("home");
        let cwd = temp_dir("cwd");
        let user_dir = home.join(".openpage");
        fs::create_dir_all(&user_dir).expect("create user dir");
        fs::write(
            user_dir.join("config.toml"),
            "[browser]\nexecutable_path = \"/tmp/user-browser\"\n",
        )
        .expect("write user config");
        let workspace_dir = cwd.join(".openpage");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");
        fs::write(
            workspace_dir.join("config.toml"),
            "[browser]\nexecutable_path = \"/tmp/workspace-browser\"\n",
        )
        .expect("write workspace config");

        let _home_guard = EnvGuard::set("HOME", home.to_string_lossy().as_ref());
        let _cwd_guard = CurrentDirGuard::set(&cwd);
        unsafe {
            std::env::remove_var(OPENPAGE_CONFIG_ENV);
            std::env::remove_var(OPENPAGE_BROWSER_PATH_ENV);
        }

        let config = load_resolved_config().expect("load resolved config");
        assert_eq!(
            config.launch.browser_path.as_deref(),
            Some(std::path::Path::new("/tmp/workspace-browser"))
        );
        assert_eq!(
            config.browser_path_source,
            ConfigValueSource::WorkspaceConfig
        );
        assert_eq!(
            config.user_data_dir_source,
            ConfigValueSource::BuiltInDefault
        );
    }

    #[test]
    fn resolved_config_uses_env_over_workspace_and_user() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let home = temp_dir("home-env");
        let cwd = temp_dir("cwd-env");
        let user_dir = home.join(".openpage");
        fs::create_dir_all(&user_dir).expect("create user dir");
        fs::write(
            user_dir.join("config.toml"),
            "[browser]\nexecutable_path = \"/tmp/user-browser\"\n",
        )
        .expect("write user config");
        let workspace_dir = cwd.join(".openpage");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");
        fs::write(
            workspace_dir.join("config.toml"),
            "[browser]\nexecutable_path = \"/tmp/workspace-browser\"\n",
        )
        .expect("write workspace config");

        let _home_guard = EnvGuard::set("HOME", home.to_string_lossy().as_ref());
        let _cwd_guard = CurrentDirGuard::set(&cwd);
        let _env_guard = EnvGuard::set(OPENPAGE_BROWSER_PATH_ENV, "/tmp/env-browser");
        unsafe {
            std::env::remove_var(OPENPAGE_CONFIG_ENV);
        }

        let config = load_resolved_config().expect("load resolved config");
        assert_eq!(
            config.launch.browser_path.as_deref(),
            Some(std::path::Path::new("/tmp/env-browser"))
        );
        assert_eq!(config.browser_path_source, ConfigValueSource::Environment);
        assert_eq!(
            config.user_data_dir_source,
            ConfigValueSource::BuiltInDefault
        );
    }

    #[test]
    fn resolved_config_tracks_user_data_dir_source() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let home = temp_dir("home-user-data-dir");
        let cwd = temp_dir("cwd-user-data-dir");
        let user_dir = home.join(".openpage");
        fs::create_dir_all(&user_dir).expect("create user dir");
        fs::write(
            user_dir.join("config.toml"),
            "[browser]\nuser_data_dir = \"/tmp/user-profile\"\n",
        )
        .expect("write user config");

        let _home_guard = EnvGuard::set("HOME", home.to_string_lossy().as_ref());
        let _cwd_guard = CurrentDirGuard::set(&cwd);
        unsafe {
            std::env::remove_var(OPENPAGE_CONFIG_ENV);
            std::env::remove_var(OPENPAGE_BROWSER_PATH_ENV);
        }

        let config = load_resolved_config().expect("load resolved config");
        assert_eq!(
            config.launch.user_data_dir.as_deref(),
            Some(std::path::Path::new("/tmp/user-profile"))
        );
        assert_eq!(config.user_data_dir_source, ConfigValueSource::UserConfig);
    }

    #[test]
    fn resolved_config_tracks_debugger_source() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let home = temp_dir("home-debugger-source");
        let cwd = temp_dir("cwd-debugger-source");
        let workspace_dir = cwd.join(".openpage");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");
        fs::write(
            workspace_dir.join("config.toml"),
            "[browser]\nlocal_port = 9555\n",
        )
        .expect("write workspace config");

        let _home_guard = EnvGuard::set("HOME", home.to_string_lossy().as_ref());
        let _cwd_guard = CurrentDirGuard::set(&cwd);
        unsafe {
            std::env::remove_var(OPENPAGE_CONFIG_ENV);
            std::env::remove_var(OPENPAGE_BROWSER_PATH_ENV);
        }

        let config = load_resolved_config().expect("load resolved config");
        assert_eq!(config.launch.remote_debugging_port, Some(9555));
        assert_eq!(config.launch.address.as_deref(), Some("127.0.0.1:9555"));
        assert_eq!(config.debugger_source, ConfigValueSource::WorkspaceConfig);
    }

    #[test]
    fn config_parse_errors_follow_language_setting() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let _settings = scoped_test_settings();
        Settings::reset();

        let dir = temp_dir("parse-errors");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "[browser\n").expect("write invalid config");

        let english_config = load_config_file(&config_path)
            .expect_err("invalid config should fail")
            .to_string();
        assert!(english_config.contains("invalid config file"));

        let english_toml = load_toml_value(&config_path)
            .expect_err("invalid TOML should fail")
            .to_string();
        assert!(english_toml.contains("invalid TOML file"));

        let english_empty_path = resolve_config_path(" ")
            .expect_err("empty config path should fail")
            .to_string();
        assert!(english_empty_path.contains("OPENPAGE_CONFIG cannot be empty"));

        Settings::set_language("cn");

        let chinese_config = load_config_file(&config_path)
            .expect_err("invalid config should localize")
            .to_string();
        assert!(chinese_config.contains("无效的配置文件"));

        let chinese_toml = load_toml_value(&config_path)
            .expect_err("invalid TOML should localize")
            .to_string();
        assert!(chinese_toml.contains("无效的 TOML 文件"));

        let chinese_empty_path = resolve_config_path(" ")
            .expect_err("empty config path should localize")
            .to_string();
        assert!(chinese_empty_path.contains("OPENPAGE_CONFIG 不能为空"));

        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn config_table_shape_errors_follow_language_setting() {
        let _lock = CONFIG_TEST_LOCK.lock().expect("config test lock");
        let _settings = scoped_test_settings();
        Settings::reset();

        let mut root = toml::Value::String("not-table".to_string());
        let english_root = ensure_table_entry(&mut root, "browser")
            .expect_err("non-table root should fail")
            .to_string();
        assert!(english_root.contains("config root must be a TOML table"));

        let mut section = toml::Value::Table(toml::map::Map::from_iter([(
            "browser".to_string(),
            toml::Value::String("not-table".to_string()),
        )]));
        let english_section = ensure_table_entry(&mut section, "browser")
            .expect_err("non-table section should fail")
            .to_string();
        assert!(english_section.contains("config `browser` section must be a TOML table"));

        Settings::set_language("cn");

        let mut root = toml::Value::String("not-table".to_string());
        let chinese_root = ensure_table_entry(&mut root, "browser")
            .expect_err("non-table root should localize")
            .to_string();
        assert!(chinese_root.contains("配置根节点必须是 TOML table"));

        let mut section = toml::Value::Table(toml::map::Map::from_iter([(
            "browser".to_string(),
            toml::Value::String("not-table".to_string()),
        )]));
        let chinese_section = ensure_table_entry(&mut section, "browser")
            .expect_err("non-table section should localize")
            .to_string();
        assert!(chinese_section.contains("配置 `browser` section 必须是 TOML table"));
    }
}
