use super::*;

pub(super) fn parse_browser_cookie_header_url(url: &str) -> OpenPageResult<Url> {
    Url::parse(url).map_err(|err| {
        OpenPageError::BrowserOperation(invalid_url_message(url, Some(&err.to_string())))
    })
}

pub(super) fn browser_pid(browser: &mut OxBrowser) -> Option<u32> {
    browser
        .get_mut_child()
        .and_then(|child| child.as_mut_inner().id())
}

pub(super) fn build_browser_config(
    options: &LaunchOptions,
    user_data_dir: Option<&Path>,
) -> OpenPageResult<BrowserConfig> {
    let mut builder = BrowserConfig::builder()
        .window_size(options.width, options.height)
        .viewport(None);
    builder = builder.disable_cache();

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
        let arg = arg.strip_prefix("--").unwrap_or(arg);
        builder = match arg.split_once('=') {
            Some((key, value)) => builder.arg((key, value)),
            None => builder.arg(arg),
        };
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
        .map_err(|err| browser_launch_error("build browser config", err))
}

pub(super) fn validate_auto_port_scope(scope: (u16, u16)) -> OpenPageResult<()> {
    let (start, end) = scope;
    if start == 0 || start >= end {
        return Err(OpenPageError::BrowserOperation(
            invalid_auto_port_scope_message(start, end),
        ));
    }
    Ok(())
}

pub(super) fn find_free_port(scope: Option<(u16, u16)>) -> OpenPageResult<u16> {
    use std::net::TcpListener;
    let scope = scope.unwrap_or(DEFAULT_AUTO_PORT_SCOPE);
    validate_auto_port_scope(scope)?;

    for port in scope.0..scope.1 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            drop(listener);
            return Ok(port);
        }
    }

    Err(OpenPageError::BrowserLaunch(
        no_free_port_in_auto_port_scope_message(scope.0, scope.1),
    ))
}

pub(super) fn default_launch_options_ini_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini")
}

pub(super) fn project_launch_options_ini_path() -> OpenPageResult<PathBuf> {
    Ok(std::env::current_dir()?.join("dp_configs.ini"))
}

pub(super) fn built_in_launch_options_defaults() -> OpenPageResult<LaunchOptions> {
    parse_launch_options_ini(include_str!("../../configs.ini"))
}

pub(crate) fn browser_path_env_override() -> Option<PathBuf> {
    let value = std::env::var_os(OPENPAGE_BROWSER_PATH_ENV)?;
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

pub(super) fn resolve_launch_options_ini_path(path: Option<&Path>) -> OpenPageResult<PathBuf> {
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

pub(super) fn parse_launch_options_ini(content: &str) -> OpenPageResult<LaunchOptions> {
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
        options.retry_times = retry_times.parse::<usize>().map_err(|err| {
            OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
                "retry_times",
                &err.to_string(),
            ))
        })?;
    }
    if let Some(retry_interval) =
        ini_non_empty(ini_section_value(&sections, "others", "retry_interval"))
    {
        let retry_interval = retry_interval.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
                "retry_interval",
                &err.to_string(),
            ))
        })?;
        options.retry_interval_millis = seconds_to_millis(retry_interval);
    }
    if let Some(base) = ini_non_empty(ini_section_value(&sections, "timeouts", "base")) {
        options.timeouts.implicit_wait = seconds_to_millis(base.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
                "base timeout",
                &err.to_string(),
            ))
        })?);
    }
    if let Some(page_load) = ini_non_empty(ini_section_value(&sections, "timeouts", "page_load")) {
        options.timeouts.page_load =
            seconds_to_millis(page_load.parse::<f64>().map_err(|err| {
                OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
                    "page_load timeout",
                    &err.to_string(),
                ))
            })?);
    }
    if let Some(script) = ini_non_empty(ini_section_value(&sections, "timeouts", "script")) {
        options.timeouts.script = seconds_to_millis(script.parse::<f64>().map_err(|err| {
            OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
                "script timeout",
                &err.to_string(),
            ))
        })?);
    }

    Ok(options)
}

pub(super) fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
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

pub(super) fn ini_section_value<'a>(
    sections: &'a HashMap<String, HashMap<String, String>>,
    section: &str,
    key: &str,
) -> Option<&'a str> {
    sections.get(section)?.get(key).map(String::as_str)
}

pub(super) fn ini_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn parse_ini_bool(value: &str) -> OpenPageResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(OpenPageError::BrowserOperation(
            invalid_launch_options_ini_boolean_message(value),
        )),
    }
}

pub(super) fn parse_ini_string_list(value: &str, field: &str) -> OpenPageResult<Vec<String>> {
    let parsed = parse_ini_json_like_value(value, field)?;
    let items = parsed.as_array().ok_or_else(|| {
        OpenPageError::BrowserOperation(invalid_launch_options_ini_field_expected_message(
            &format!("{field} list"),
            "list",
        ))
    })?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                OpenPageError::BrowserOperation(invalid_launch_options_ini_field_expected_message(
                    &format!("{field} list"),
                    "string items",
                ))
            })
        })
        .collect()
}

pub(super) fn parse_ini_preferences(
    value: &str,
) -> OpenPageResult<HashMap<String, serde_json::Value>> {
    let parsed = parse_ini_json_like_value(value, "prefs")?;
    let object = parsed.as_object().ok_or_else(|| {
        OpenPageError::BrowserOperation(invalid_launch_options_ini_field_expected_message(
            "prefs object",
            "object",
        ))
    })?;
    Ok(object
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

pub(super) fn parse_ini_flags(value: &str) -> OpenPageResult<Vec<String>> {
    let parsed = parse_ini_json_like_value(value, "flags")?;
    match parsed {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| {
                item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    OpenPageError::BrowserOperation(
                        invalid_launch_options_ini_field_expected_message(
                            "flags list",
                            "string items",
                        ),
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
                    invalid_launch_options_ini_field_expected_message(
                        "flags object",
                        "scalar values",
                    ),
                )),
            })
            .collect(),
        _ => Err(OpenPageError::BrowserOperation(
            invalid_launch_options_ini_field_expected_message("flags", "list or object"),
        )),
    }
}

pub(super) fn parse_ini_json_like_value(
    value: &str,
    field: &str,
) -> OpenPageResult<serde_json::Value> {
    if let Ok(parsed) = serde_json::from_str(value) {
        return Ok(parsed);
    }
    let normalized = python_literal_to_json(value)?;
    serde_json::from_str(&normalized).map_err(|err| {
        OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
            &format!("{field} value"),
            &err.to_string(),
        ))
    })
}

pub(super) fn python_literal_to_json(value: &str) -> OpenPageResult<String> {
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
                                invalid_launch_options_ini_python_string_message(),
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
                        unterminated_launch_options_ini_python_string_message(),
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

pub(super) fn parse_ini_u16_tuple(value: &str) -> OpenPageResult<(u16, u16)> {
    let trimmed = value.trim().trim_start_matches('(').trim_end_matches(')');
    let Some((start, end)) = trimmed.split_once(',') else {
        return Err(OpenPageError::BrowserOperation(
            invalid_launch_options_ini_field_message("auto_port scope", value),
        ));
    };
    let start = start.trim().parse::<u16>().map_err(|err| {
        OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
            "auto_port scope start",
            &err.to_string(),
        ))
    })?;
    let end = end.trim().parse::<u16>().map_err(|err| {
        OpenPageError::BrowserOperation(invalid_launch_options_ini_field_message(
            "auto_port scope end",
            &err.to_string(),
        ))
    })?;
    Ok((start, end))
}

pub(super) fn apply_loaded_auto_port_value(
    options: &mut LaunchOptions,
    value: &str,
) -> OpenPageResult<()> {
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
            options.auto_port_with_scope(true, Some(scope))?;
            Ok(())
        }
    }
}

pub(super) fn serialize_launch_options_ini(
    options: &LaunchOptions,
    template: Option<&str>,
) -> String {
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

pub(super) fn serialize_launch_options_ini_sections(
    options: &LaunchOptions,
) -> Vec<(String, String)> {
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

pub(super) fn load_launch_options_ini_template(
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

pub(super) fn read_launch_options_ini_template(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[derive(Debug, Clone)]
pub(super) struct IniSectionBlock {
    name: String,
    raw: String,
}

pub(super) fn parse_ini_section_blocks(content: &str) -> Vec<IniSectionBlock> {
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

pub(super) fn trim_ini_block(block: &str) -> &str {
    block.trim_matches('\n')
}

pub(super) fn ini_auto_port_value(options: &LaunchOptions) -> String {
    match options.auto_port_scope() {
        Some((start, end)) => format!("({start}, {end})"),
        None => ini_bool(false).to_string(),
    }
}

pub(super) fn option_path_string(path: Option<&Path>) -> String {
    path.map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub(super) fn resolved_launch_options_address(options: &LaunchOptions) -> String {
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

pub(super) fn ini_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

pub(super) fn millis_to_ini_seconds(millis: u64) -> String {
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

pub(super) fn millis_to_seconds_f64(millis: u64) -> f64 {
    millis as f64 / 1000.0
}

pub(super) fn system_user_data_dir() -> Option<PathBuf> {
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

pub(super) fn normalize_debugger_address(address: &str) -> (String, Option<String>, Option<u16>) {
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

pub(super) fn is_local_debugger_address(address: &str) -> bool {
    debugger_address_host(address) == Some("127.0.0.1")
}

pub(super) fn debugger_address_host(address: &str) -> Option<&str> {
    address.split_once(':').map(|(host, _)| host)
}

pub(super) fn debugger_address_port(address: &str) -> Option<u16> {
    let (_, rest) = address.split_once(':')?;
    rest.split('/').next()?.parse().ok()
}

pub(super) fn local_debugger_port_from_host_port(address: &str) -> Option<u16> {
    if is_local_debugger_address(address) {
        debugger_address_port(address)
    } else {
        None
    }
}

pub(super) fn local_debugger_address_is_open(address: &str) -> bool {
    let port = match local_debugger_port_from_host_port(address) {
        Some(port) => port,
        None => return false,
    };
    let socket = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&socket, Duration::from_millis(200)).is_ok()
}

pub(super) fn seconds_to_millis(seconds: f64) -> u64 {
    if seconds <= 0.0 || !seconds.is_finite() {
        0
    } else {
        (seconds * 1000.0) as u64
    }
}

pub(super) fn reset_browser_user_data_dir(path: &Path) -> OpenPageResult<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(path).map_err(|err| {
        OpenPageError::BrowserLaunch(browser_user_data_dir_reset_failed_message(
            path,
            &err.to_string(),
        ))
    })
}

pub(super) fn write_chrome_prefs(
    user_data_dir: &Path,
    args: &[String],
    prefs: &HashMap<String, serde_json::Value>,
    prefs_to_remove: &[String],
) -> OpenPageResult<()> {
    let prefs_dir = user_data_dir.join(chrome_profile_directory(args));
    std::fs::create_dir_all(&prefs_dir).map_err(|err| {
        OpenPageError::BrowserLaunch(browser_config_path_failed_message(
            "create Chrome profile directory",
            "创建 Chrome profile 目录",
            &prefs_dir,
            &err.to_string(),
        ))
    })?;
    let prefs_path = prefs_dir.join("Preferences");
    let mut existing: serde_json::Value = if prefs_path.exists() {
        let content = std::fs::read_to_string(&prefs_path).map_err(|err| {
            OpenPageError::BrowserLaunch(browser_config_path_failed_message(
                "read Chrome Preferences",
                "读取 Chrome Preferences",
                &prefs_path,
                &err.to_string(),
            ))
        })?;
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
    std::fs::write(&prefs_path, serde_json::to_string(&existing).unwrap()).map_err(|err| {
        OpenPageError::BrowserLaunch(browser_config_path_failed_message(
            "write Chrome Preferences",
            "写入 Chrome Preferences",
            &prefs_path,
            &err.to_string(),
        ))
    })?;
    Ok(())
}

pub(super) fn chrome_profile_directory(args: &[String]) -> &str {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--profile-directory="))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Default")
}

pub(super) fn set_nested_json_value(
    target: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) {
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

pub(super) fn remove_nested_json_value(target: &mut serde_json::Value, path: &str) {
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

pub(super) fn ensure_json_object(
    value: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    value.as_object_mut().expect("json object")
}

pub(super) fn write_chrome_flags(
    user_data_dir: &Path,
    flags: &[String],
    clear_file_flags: bool,
) -> OpenPageResult<()> {
    let local_state_path = user_data_dir.join("Local State");
    let mut existing: serde_json::Value = if local_state_path.exists() {
        let content = std::fs::read_to_string(&local_state_path).map_err(|err| {
            OpenPageError::BrowserLaunch(browser_config_path_failed_message(
                "read Chrome Local State",
                "读取 Chrome Local State",
                &local_state_path,
                &err.to_string(),
            ))
        })?;
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
    std::fs::write(&local_state_path, serde_json::to_string(&existing).unwrap()).map_err(
        |err| {
            OpenPageError::BrowserLaunch(browser_config_path_failed_message(
                "write Chrome Local State",
                "写入 Chrome Local State",
                &local_state_path,
                &err.to_string(),
            ))
        },
    )?;
    Ok(())
}

pub(super) fn make_temp_user_data_dir(base: Option<&Path>) -> OpenPageResult<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| browser_launch_error("create user data temp suffix", err))?
        .as_nanos();
    let default_base = std::env::temp_dir();
    let base = base.unwrap_or_else(|| default_base.as_path());
    let path = base.join(format!("openpage-browser-{suffix}"));
    std::fs::create_dir_all(&path).map_err(|err| {
        OpenPageError::BrowserLaunch(browser_temp_dir_create_failed_message(
            "user data",
            "用户数据",
            &path,
            &err.to_string(),
        ))
    })?;
    Ok(path)
}

pub(super) fn resolve_launch_user_data_dir(
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

pub(super) fn make_temp_download_dir(base: Option<&Path>) -> OpenPageResult<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| browser_launch_error("create download temp suffix", err))?
        .as_nanos();
    let default_base = std::env::temp_dir();
    let base = base.unwrap_or_else(|| default_base.as_path());
    let path = base.join(format!("openpage-downloads-{suffix}"));
    std::fs::create_dir_all(&path).map_err(|err| {
        OpenPageError::BrowserLaunch(browser_temp_dir_create_failed_message(
            "download",
            "下载",
            &path,
            &err.to_string(),
        ))
    })?;
    Ok(path)
}

pub(super) fn configure_download_behavior(
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
            .map_err(|err| browser_operation_error("build download behavior params", err))?;
        execute_browser_handle_command_async(
            browser,
            params,
            "Browser::configure_download_behavior()",
        )
        .await?;
        Ok::<(), OpenPageError>(())
    })
}

pub(super) fn create_download_directory(path: &Path) -> OpenPageResult<()> {
    std::fs::create_dir_all(path).map_err(|err| {
        OpenPageError::BrowserOperation(download_directory_create_failed_message(
            path,
            &err.to_string(),
        ))
    })
}

pub(super) fn browser_launch_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::BrowserLaunch(browser_launch_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

pub(super) fn browser_operation_error(operation: &str, err: impl ToString) -> OpenPageError {
    OpenPageError::BrowserOperation(browser_setup_operation_failed_message(
        operation,
        &err.to_string(),
    ))
}

pub(super) async fn execute_browser_handle_command_async<T>(
    browser: &OxBrowser,
    command: T,
    operation: &str,
) -> OpenPageResult<T::Response>
where
    T: Command,
{
    run_browser_future_with_cdp_timeout(browser.execute(command), operation)
        .await
        .map(|response| response.result)
}

pub(super) fn execute_browser_command_blocking<T>(
    runtime: &Runtime,
    browser: &Mutex<OxBrowser>,
    command: T,
    operation: &str,
) -> OpenPageResult<T::Response>
where
    T: Command,
{
    runtime.block_on(async {
        let lock_operation = format!("{operation}.lock()");
        let browser = lock_with_cdp_timeout(browser, &lock_operation).await?;
        execute_browser_handle_command_async(&browser, command, operation).await
    })
}

pub(super) async fn lock_with_cdp_timeout<'a, T>(
    mutex: &'a Mutex<T>,
    operation: &str,
) -> OpenPageResult<MutexGuard<'a, T>> {
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, mutex.lock())
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))
}

pub(super) async fn run_browser_future_with_cdp_timeout<Fut, T, E>(
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
        .map_err(|err| {
            OpenPageError::BrowserOperation(browser_command_failed_message(
                operation,
                &err.to_string(),
            ))
        })
}

pub(super) fn download_source_path(
    info: &DownloadInfo,
    download_dir: &Path,
) -> OpenPageResult<PathBuf> {
    if let Some(path) = &info.final_path {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    let discovered_path = download_dir.join(&info.guid);
    if discovered_path.exists() {
        return Ok(discovered_path);
    }

    Err(OpenPageError::Timeout(
        download_did_not_complete_in_time_message(),
    ))
}

pub(super) fn finalize_download_path(
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
                std::fs::remove_file(preferred_path).map_err(|err| {
                    OpenPageError::BrowserOperation(download_file_operation_failed_message(
                        "remove existing target",
                        "删除已存在目标",
                        preferred_path,
                        &err.to_string(),
                    ))
                })?;
            }
            preferred_path.to_path_buf()
        }
        DownloadFileExistsMode::Skip => {
            if preferred_path.exists() {
                std::fs::remove_file(source_path).map_err(|err| {
                    OpenPageError::BrowserOperation(download_file_operation_failed_message(
                        "remove temporary source",
                        "删除临时源文件",
                        source_path,
                        &err.to_string(),
                    ))
                })?;
                return Ok((
                    DownloadState::Skipped,
                    preferred_path.to_string_lossy().into_owned(),
                ));
            }
            preferred_path.to_path_buf()
        }
    };

    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            OpenPageError::BrowserOperation(download_file_operation_failed_message(
                "create target directory",
                "创建目标目录",
                parent,
                &err.to_string(),
            ))
        })?;
    }
    std::fs::rename(source_path, &final_path).map_err(|err| {
        OpenPageError::BrowserOperation(download_file_operation_failed_message(
            "move final file",
            "移动最终文件",
            &final_path,
            &err.to_string(),
        ))
    })?;
    Ok((
        DownloadState::Completed,
        final_path.to_string_lossy().into_owned(),
    ))
}

pub(super) fn resolved_download_name(
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

pub(super) fn is_tab_like_type(target_type: &str) -> bool {
    matches!(target_type, "page" | "tab")
}

pub(super) fn normalize_browser_tab_types(input: BrowserTabTypeInput<'_>) -> Vec<String> {
    match input {
        BrowserTabTypeInput::Single(value) => vec![value.trim().to_ascii_lowercase()],
        BrowserTabTypeInput::Many(values) => values
            .into_iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect(),
    }
}

pub(super) fn browser_tab_info_matches(
    info: &TabInfo,
    title: Option<&str>,
    url: Option<&str>,
    tab_types: &[String],
) -> bool {
    if !tab_types.is_empty() && !tab_types.iter().any(|kind| kind == &info.tab_type) {
        return false;
    }
    if let Some(title) = title.filter(|title| !title.is_empty())
        && !info.title.contains(title)
    {
        return false;
    }
    if let Some(url) = url.filter(|url| !url.is_empty())
        && !info.url.contains(url)
    {
        return false;
    }
    true
}

pub(super) fn select_browser_tab_info_by_selector<'a>(
    infos: &'a [TabInfo],
    selector: BrowserTabSelector<'_>,
) -> OpenPageResult<Option<&'a TabInfo>> {
    match selector {
        BrowserTabSelector::Id(target_id) => {
            Ok(infos.iter().find(|info| info.target_id == target_id))
        }
        BrowserTabSelector::Index(index) => {
            if index == 0 {
                return Err(OpenPageError::BrowserOperation(invalid_tab_index_message()));
            }
            let resolved_index = if index > 0 {
                (index as usize).checked_sub(1)
            } else {
                infos.len().checked_sub(index.unsigned_abs())
            };
            Ok(resolved_index.and_then(|resolved_index| infos.get(resolved_index)))
        }
    }
}

pub(super) fn resolve_browser_tab_target_id(
    infos: &[TabInfo],
    selector: BrowserTabSelector<'_>,
) -> OpenPageResult<String> {
    select_browser_tab_info_by_selector(infos, selector)?
        .map(|info| info.target_id.clone())
        .ok_or_else(|| OpenPageError::BrowserOperation(target_tab_not_found_message()))
}

pub(super) fn find_new_tab_id(
    baseline_ids: &[String],
    current_ids: &[String],
    baseline_marker: Option<&str>,
    current_newest: Option<&str>,
) -> Option<String> {
    if let (Some(baseline_marker), Some(current_newest)) = (baseline_marker, current_newest)
        && current_ids
            .iter()
            .any(|target_id| target_id == current_newest)
        && !baseline_ids
            .iter()
            .any(|target_id| target_id == current_newest)
        && current_newest != baseline_marker
    {
        return Some(current_newest.to_string());
    }

    current_ids
        .iter()
        .find(|target_id| !baseline_ids.iter().any(|seen| seen == *target_id))
        .cloned()
}

pub(super) fn resolve_browser_tab_target_ids(
    infos: &[TabInfo],
    input: BrowserTabTargetsInput<'_>,
) -> OpenPageResult<Vec<String>> {
    let selectors = match input {
        BrowserTabTargetsInput::Single(selector) => vec![selector],
        BrowserTabTargetsInput::Many(selectors) => selectors,
    };
    let mut target_ids = Vec::with_capacity(selectors.len());
    let mut seen = HashSet::new();
    for selector in selectors {
        let target_id = resolve_browser_tab_target_id(infos, selector)?;
        if seen.insert(target_id.clone()) {
            target_ids.push(target_id);
        }
    }
    Ok(target_ids)
}

pub(super) fn browser_cookie_header_to_params(url: &Url, cookie_header: &str) -> Vec<CookieParam> {
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

pub(super) fn browser_cookie_param(
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

pub(super) fn browser_delete_cookie_params(
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

pub(super) fn unique_download_path(path: &Path) -> PathBuf {
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
