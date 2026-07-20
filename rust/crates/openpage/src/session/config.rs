use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct IniDocument {
    section_order: Vec<String>,
    key_order: HashMap<String, Vec<String>>,
    sections: HashMap<String, HashMap<String, String>>,
}

pub(super) fn default_session_options_ini_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini")
}

pub(super) fn project_session_options_ini_path() -> OpenPageResult<PathBuf> {
    Ok(env::current_dir()?.join("dp_configs.ini"))
}

pub(super) fn built_in_session_options_defaults() -> OpenPageResult<SessionOptions> {
    parse_session_options_ini(include_str!("../../configs.ini"))
}

pub(super) fn resolve_session_options_ini_path(path: Option<&Path>) -> OpenPageResult<PathBuf> {
    match path {
        Some(path) if path.is_dir() => Ok(path.join("config.ini")),
        Some(path) => Ok(path.to_path_buf()),
        None => {
            let project_path = project_session_options_ini_path()?;
            if project_path.is_file() {
                Ok(project_path)
            } else {
                Ok(default_session_options_ini_path())
            }
        }
    }
}

pub(super) fn load_session_options_ini_template(
    target_path: &Path,
    source_ini_path: Option<&Path>,
) -> Option<String> {
    read_session_options_ini_template(target_path)
        .or_else(|| {
            source_ini_path
                .filter(|source_path| *source_path != target_path)
                .and_then(|source_path| read_session_options_ini_template(source_path))
        })
        .or_else(|| {
            let default_path = default_session_options_ini_path();
            (default_path.as_path() != target_path)
                .then_some(default_path)
                .and_then(|path| read_session_options_ini_template(path.as_path()))
        })
}

pub(super) fn read_session_options_ini_template(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

pub(super) fn parse_session_options_ini(content: &str) -> OpenPageResult<SessionOptions> {
    let ini = parse_ini_document(content);
    let mut options = SessionOptions::default();

    if let Some(download_path) = ini_non_empty(ini_section_value(&ini, "paths", "download_path")) {
        options.download_path = PathBuf::from(download_path);
    }

    if let Some(timeout) = ini_non_empty(ini_section_value(&ini, "timeouts", "base")) {
        options.timeout_secs = parse_ini_timeout_secs(timeout)?;
    }

    if let Some(http_proxy) = ini_non_empty(ini_section_value(&ini, "proxies", "http")) {
        options.http_proxy = Some(http_proxy.to_string());
    }
    if let Some(https_proxy) = ini_non_empty(ini_section_value(&ini, "proxies", "https")) {
        options.https_proxy = Some(https_proxy.to_string());
    }

    if let Some(retry_times) = ini_non_empty(ini_section_value(&ini, "others", "retry_times")) {
        options.retry_times = retry_times.parse::<usize>().map_err(|err| {
            OpenPageError::Http(invalid_session_ini_field_message(
                "retry_times",
                &err.to_string(),
            ))
        })?;
    }
    if let Some(retry_interval) = ini_non_empty(ini_section_value(&ini, "others", "retry_interval"))
    {
        options.retry_interval_millis = parse_ini_retry_interval_millis(retry_interval)?;
    }

    if let Some(headers) = ini_section_value(&ini, "session_options", "headers") {
        options.set_headers(parse_session_headers(headers)?)?;
    }
    if let Some(cookies) = ini_section_value(&ini, "session_options", "cookies") {
        options.set_cookies(&parse_session_cookies(cookies)?)?;
    }
    if let Some(user_agent) = ini_section_value(&ini, "session_options", "user_agent") {
        options.user_agent = parse_optional_ini_string(user_agent, "user_agent")?;
    }
    if let Some(auth) = ini_section_value(&ini, "session_options", "auth") {
        options.auth = parse_optional_ini_string_pair(auth, "auth")?;
    }
    if let Some(params) = ini_section_value(&ini, "session_options", "params") {
        options.set_params(&parse_session_params(params)?);
    }
    if let Some(verify) = ini_section_value(&ini, "session_options", "verify") {
        if let Some(verify) = parse_optional_ini_bool(verify)? {
            options.verify = verify;
        }
    }
    if let Some(cert) = ini_section_value(&ini, "session_options", "cert") {
        options.cert = parse_optional_ini_cert(cert)?;
    }
    if let Some(stream) = ini_section_value(&ini, "session_options", "stream") {
        if let Some(stream) = parse_optional_ini_bool(stream)? {
            options.stream = stream;
        }
    }
    if let Some(trust_env) = ini_section_value(&ini, "session_options", "trust_env") {
        if let Some(trust_env) = parse_optional_ini_bool(trust_env)? {
            options.trust_env = trust_env;
        }
    }
    if let Some(max_redirects) = ini_section_value(&ini, "session_options", "max_redirects") {
        options.max_redirects = parse_optional_ini_usize(max_redirects, "max_redirects")?;
    }

    Ok(options)
}

pub(super) fn serialize_session_options_ini(
    options: &SessionOptions,
    template: Option<&str>,
) -> String {
    let mut ini = template.map(parse_ini_document).unwrap_or_default();

    set_ini_value(
        &mut ini,
        "paths",
        "download_path",
        path_to_ini_value(&options.download_path),
    );
    set_ini_value(
        &mut ini,
        "timeouts",
        "base",
        options.timeout_secs.to_string(),
    );
    set_ini_value(
        &mut ini,
        "proxies",
        "http",
        options.http_proxy.clone().unwrap_or_default(),
    );
    set_ini_value(
        &mut ini,
        "proxies",
        "https",
        options.https_proxy.clone().unwrap_or_default(),
    );
    set_ini_value(
        &mut ini,
        "others",
        "retry_times",
        options.retry_times.to_string(),
    );
    set_ini_value(
        &mut ini,
        "others",
        "retry_interval",
        millis_to_ini_seconds(options.retry_interval_millis),
    );

    set_ini_value(
        &mut ini,
        "session_options",
        "headers",
        serialize_string_map(&options.headers),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "cookies",
        serialize_session_cookies(&options.cookies),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "user_agent",
        serde_json::to_string(&options.user_agent).unwrap_or_else(|_| "null".to_string()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "auth",
        serialize_optional_string_pair(options.auth.as_ref()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "params",
        serialize_string_map(&options.params),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "verify",
        ini_bool(options.verify).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "cert",
        serialize_optional_cert(options.cert.as_ref()),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "stream",
        ini_bool(options.stream).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "trust_env",
        ini_bool(options.trust_env).to_string(),
    );
    set_ini_value(
        &mut ini,
        "session_options",
        "max_redirects",
        serialize_optional_usize(options.max_redirects),
    );

    serialize_ini_document(&ini)
}

pub(super) fn parse_ini_document(content: &str) -> IniDocument {
    let mut document = IniDocument::default();
    let mut current_section: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim().to_string();
            ensure_ini_section(&mut document, &section);
            current_section = Some(section);
            continue;
        }
        let Some(section) = current_section.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        set_ini_value(&mut document, section, key.trim(), value.trim().to_string());
    }

    document
}

pub(super) fn ensure_ini_section(document: &mut IniDocument, section: &str) {
    if !document.section_order.iter().any(|item| item == section) {
        document.section_order.push(section.to_string());
    }
    document.key_order.entry(section.to_string()).or_default();
    document.sections.entry(section.to_string()).or_default();
}

pub(super) fn set_ini_value(document: &mut IniDocument, section: &str, key: &str, value: String) {
    ensure_ini_section(document, section);
    let key = key.to_string();
    let key_order = document.key_order.entry(section.to_string()).or_default();
    if !key_order.iter().any(|item| item == &key) {
        key_order.push(key.clone());
    }
    document
        .sections
        .entry(section.to_string())
        .or_default()
        .insert(key, value);
}

pub(super) fn serialize_ini_document(document: &IniDocument) -> String {
    let mut blocks = Vec::new();
    let mut emitted = HashSet::new();

    for section in &document.section_order {
        if emitted.insert(section.clone()) {
            blocks.push(serialize_ini_section(document, section));
        }
    }

    let mut extra_sections = document.sections.keys().cloned().collect::<Vec<_>>();
    extra_sections.sort();
    for section in extra_sections {
        if emitted.insert(section.clone()) {
            blocks.push(serialize_ini_section(document, &section));
        }
    }

    if blocks.is_empty() {
        String::new()
    } else {
        format!("{}\n", blocks.join("\n\n"))
    }
}

pub(super) fn serialize_ini_section(document: &IniDocument, section: &str) -> String {
    let mut lines = vec![format!("[{section}]")];
    let mut emitted = HashSet::new();

    if let Some(keys) = document.key_order.get(section) {
        for key in keys {
            if let Some(value) = document
                .sections
                .get(section)
                .and_then(|values| values.get(key))
            {
                emitted.insert(key.clone());
                lines.push(format!("{key} = {value}"));
            }
        }
    }

    let mut extra_keys = document
        .sections
        .get(section)
        .map(|values| {
            values
                .keys()
                .filter(|key| !emitted.contains(*key))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    extra_keys.sort();
    for key in extra_keys {
        if let Some(value) = document
            .sections
            .get(section)
            .and_then(|values| values.get(&key))
        {
            lines.push(format!("{key} = {value}"));
        }
    }

    lines.join("\n")
}

pub(super) fn ini_section_value<'a>(
    document: &'a IniDocument,
    section: &str,
    key: &str,
) -> Option<&'a str> {
    document.sections.get(section)?.get(key).map(String::as_str)
}

pub(super) fn ini_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn parse_ini_timeout_secs(value: &str) -> OpenPageResult<u64> {
    let timeout = value.parse::<f64>().map_err(|err| {
        OpenPageError::Http(invalid_session_ini_field_message(
            "timeout",
            &err.to_string(),
        ))
    })?;
    if timeout.is_sign_negative() {
        return Err(OpenPageError::Http(invalid_session_ini_field_message(
            "timeout",
            "negative value",
        )));
    }
    Ok(timeout.ceil() as u64)
}

pub(super) fn parse_ini_retry_interval_millis(value: &str) -> OpenPageResult<u64> {
    let retry_interval = value.parse::<f64>().map_err(|err| {
        OpenPageError::Http(invalid_session_ini_field_message(
            "retry_interval",
            &err.to_string(),
        ))
    })?;
    if retry_interval.is_sign_negative() {
        return Err(OpenPageError::Http(invalid_session_ini_field_message(
            "retry_interval",
            "negative value",
        )));
    }
    Ok((retry_interval * 1000.0).round() as u64)
}
