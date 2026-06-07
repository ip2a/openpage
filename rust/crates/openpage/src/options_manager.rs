use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::{OpenPageError, OpenPageResult};

#[derive(Debug, Clone)]
pub struct OptionsManager {
    ini_path: Option<PathBuf>,
    file_exists: bool,
    document: IniDocument,
}

impl Default for OptionsManager {
    fn default() -> Self {
        Self::without_file()
    }
}

impl OptionsManager {
    pub fn new(path: Option<&Path>) -> OpenPageResult<Self> {
        let path = resolve_options_manager_ini_path(path)?;
        Self::from_resolved_path(path)
    }

    pub fn with_default_path() -> OpenPageResult<Self> {
        Self::from_resolved_path(default_options_manager_ini_path())
    }

    pub fn without_file() -> Self {
        Self {
            ini_path: None,
            file_exists: false,
            document: built_in_options_manager_document(),
        }
    }

    pub fn default_ini_path() -> PathBuf {
        default_options_manager_ini_path()
    }

    pub fn project_ini_path() -> OpenPageResult<PathBuf> {
        project_options_manager_ini_path()
    }

    pub fn ini_path(&self) -> Option<&Path> {
        self.ini_path.as_deref()
    }

    pub fn file_exists(&self) -> bool {
        self.file_exists
    }

    pub fn sections(&self) -> Vec<String> {
        ordered_section_names(&self.document)
    }

    pub fn get_value(&self, section: &str, item: &str) -> Option<Value> {
        self.document
            .sections
            .get(section)?
            .get(item)
            .map(|value| parse_ini_literal_or_string(value))
    }

    pub fn get_option(&self, section: &str) -> Option<HashMap<String, Value>> {
        let values = self.document.sections.get(section)?;
        let mut parsed = HashMap::with_capacity(values.len());
        for key in ordered_key_names(&self.document, section) {
            if let Some(value) = values.get(&key) {
                parsed.insert(key, parse_ini_literal_or_string(value));
            }
        }
        Some(parsed)
    }

    pub fn set_item<T>(&mut self, section: &str, item: &str, value: T) -> OpenPageResult<&mut Self>
    where
        T: Serialize,
    {
        let value = serde_json::to_value(value).map_err(|err| {
            OpenPageError::Serialization(format!(
                "failed to serialize ini value for `{section}.{item}`: {err}"
            ))
        })?;
        set_ini_value(
            &mut self.document,
            section,
            item,
            serialize_top_level_ini_value(&value),
        );
        Ok(self)
    }

    pub fn set_item_literal(
        &mut self,
        section: &str,
        item: &str,
        value: impl Into<String>,
    ) -> &mut Self {
        set_ini_value(&mut self.document, section, item, value.into());
        self
    }

    pub fn remove_item(&mut self, section: &str, item: &str) -> &mut Self {
        remove_ini_value(&mut self.document, section, item);
        self
    }

    pub fn save(&mut self, path: Option<&Path>) -> OpenPageResult<PathBuf> {
        let path = resolve_options_manager_save_path(path, self.ini_path.as_deref())?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serialize_ini_document(&self.document))?;
        self.ini_path = Some(path.clone());
        self.file_exists = true;
        Ok(path)
    }

    pub fn save_to_default(&mut self) -> OpenPageResult<PathBuf> {
        let path = default_options_manager_ini_path();
        self.save(Some(path.as_path()))
    }

    pub fn show(&self) -> String {
        serialize_ini_document(&self.document)
    }

    fn from_resolved_path(path: PathBuf) -> OpenPageResult<Self> {
        if path.is_file() {
            let content = std::fs::read_to_string(&path)?;
            Ok(Self {
                ini_path: Some(path),
                file_exists: true,
                document: parse_ini_document(&content),
            })
        } else {
            Ok(Self {
                ini_path: Some(path),
                file_exists: false,
                document: built_in_options_manager_document(),
            })
        }
    }
}

#[derive(Debug, Clone, Default)]
struct IniDocument {
    section_order: Vec<String>,
    key_order: HashMap<String, Vec<String>>,
    sections: HashMap<String, HashMap<String, String>>,
}

fn default_options_manager_ini_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini")
}

fn project_options_manager_ini_path() -> OpenPageResult<PathBuf> {
    Ok(env::current_dir()?.join("dp_configs.ini"))
}

fn resolve_options_manager_ini_path(path: Option<&Path>) -> OpenPageResult<PathBuf> {
    match path {
        Some(path) => Ok(path.to_path_buf()),
        None => {
            let project_path = project_options_manager_ini_path()?;
            if project_path.is_file() {
                Ok(project_path)
            } else {
                Ok(default_options_manager_ini_path())
            }
        }
    }
}

fn resolve_options_manager_save_path(
    path: Option<&Path>,
    current_ini_path: Option<&Path>,
) -> OpenPageResult<PathBuf> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => current_ini_path
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                OpenPageError::BrowserOperation(
                    "options manager ini path is not set; use save(Some(path)) or initialize with OptionsManager::new(...)"
                        .to_string(),
                )
            })?,
    };

    if path.is_dir() {
        Ok(path.join("config.ini"))
    } else {
        Ok(path)
    }
}

fn built_in_options_manager_document() -> IniDocument {
    parse_ini_document(include_str!("../configs.ini"))
}

fn parse_ini_document(content: &str) -> IniDocument {
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

fn ensure_ini_section(document: &mut IniDocument, section: &str) {
    if !document.section_order.iter().any(|item| item == section) {
        document.section_order.push(section.to_string());
    }
    document.key_order.entry(section.to_string()).or_default();
    document.sections.entry(section.to_string()).or_default();
}

fn set_ini_value(document: &mut IniDocument, section: &str, key: &str, value: String) {
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

fn remove_ini_value(document: &mut IniDocument, section: &str, key: &str) {
    if let Some(values) = document.sections.get_mut(section) {
        values.remove(key);
    }
    if let Some(keys) = document.key_order.get_mut(section) {
        keys.retain(|existing| existing != key);
    }
}

fn serialize_ini_document(document: &IniDocument) -> String {
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

fn serialize_ini_section(document: &IniDocument, section: &str) -> String {
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

fn ordered_section_names(document: &IniDocument) -> Vec<String> {
    let mut sections = document.section_order.clone();
    for section in document.sections.keys() {
        if !sections.iter().any(|existing| existing == section) {
            sections.push(section.clone());
        }
    }
    sections
}

fn ordered_key_names(document: &IniDocument, section: &str) -> Vec<String> {
    let mut keys = document.key_order.get(section).cloned().unwrap_or_default();
    if let Some(values) = document.sections.get(section) {
        for key in values.keys() {
            if !keys.iter().any(|existing| existing == key) {
                keys.push(key.clone());
            }
        }
    }
    keys
}

fn parse_ini_literal_or_string(value: &str) -> Value {
    parse_ini_json_like_value(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn parse_ini_json_like_value(value: &str) -> OpenPageResult<Value> {
    if let Ok(parsed) = serde_json::from_str(value) {
        return Ok(parsed);
    }
    let normalized = python_literal_to_json(value)?;
    serde_json::from_str(&normalized).map_err(|err| {
        OpenPageError::BrowserOperation(format!("invalid options manager ini literal: {err}"))
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
                                "invalid Python-style string in options manager ini".to_string(),
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
                        "unterminated Python-style string in options manager ini".to_string(),
                    ));
                }

                normalized.push_str(&serde_json::to_string(&content).unwrap_or_default());
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
            '(' => {
                normalized.push('[');
                index += 1;
            }
            ')' => {
                normalized.push(']');
                index += 1;
            }
            ch => {
                normalized.push(ch);
                index += 1;
            }
        }
    }

    Ok(normalized)
}

fn serialize_top_level_ini_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serialize_nested_ini_value(other),
    }
}

fn serialize_nested_ini_value(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(value) => {
            if *value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(text) => serialize_python_string(text),
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(serialize_nested_ini_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        serialize_python_string(key),
                        serialize_nested_ini_value(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn serialize_python_string(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::OptionsManager;

    static CURRENT_DIR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn make_temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = env::temp_dir().join(format!("openpage-options-manager-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    struct CurrentDirGuard {
        original: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurrentDirGuard {
        fn change_to(path: &PathBuf) -> Self {
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
    fn options_manager_new_none_prefers_project_dp_configs_file() {
        let dir = make_temp_dir("project-default");
        let project_ini = dir.join("dp_configs.ini");
        fs::write(
            &project_ini,
            "[chromium_options]\naddress = 127.0.0.1:9555\n",
        )
        .expect("write project ini");
        let _guard = CurrentDirGuard::change_to(&dir);

        let manager = OptionsManager::new(None).expect("load project options manager");

        assert!(manager.file_exists());
        assert_eq!(
            manager
                .ini_path()
                .and_then(|path| fs::canonicalize(path).ok()),
            Some(fs::canonicalize(&project_ini).expect("canonicalize project ini"))
        );
        assert_eq!(
            manager.get_value("chromium_options", "address"),
            Some(json!("127.0.0.1:9555"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_manager_without_file_starts_from_builtin_defaults() {
        let manager = OptionsManager::without_file();

        assert!(!manager.file_exists());
        assert!(manager.ini_path().is_none());
        assert_eq!(
            manager.get_value("chromium_options", "load_mode"),
            Some(json!("normal"))
        );
        assert_eq!(
            manager.get_value("chromium_options", "auto_port"),
            Some(json!(false))
        );
    }

    #[test]
    fn options_manager_get_set_and_remove_follow_reference_literal_semantics() {
        let mut manager = OptionsManager::without_file();
        manager
            .set_item("others", "retry_times", 7usize)
            .expect("set retry_times");
        manager
            .set_item("chromium_options", "flags", json!(["demo", "trace@1"]))
            .expect("set flags");
        manager
            .set_item(
                "session_options",
                "headers",
                json!({"user-agent": "OpenPage/Options"}),
            )
            .expect("set headers");
        manager.set_item_literal("custom", "mode", "manual");

        assert_eq!(manager.get_value("others", "retry_times"), Some(json!(7)));
        assert_eq!(
            manager.get_value("chromium_options", "flags"),
            Some(json!(["demo", "trace@1"]))
        );
        assert_eq!(
            manager.get_value("session_options", "headers"),
            Some(json!({"user-agent": "OpenPage/Options"}))
        );
        assert_eq!(manager.get_value("custom", "mode"), Some(json!("manual")));

        let headers = manager
            .get_option("session_options")
            .expect("session section should exist");
        assert_eq!(
            headers.get("headers"),
            Some(&json!({"user-agent": "OpenPage/Options"}))
        );

        manager.remove_item("custom", "mode");
        assert_eq!(manager.get_value("custom", "mode"), None);
    }

    #[test]
    fn options_manager_save_and_reload_round_trips_directory_targets() {
        let dir = make_temp_dir("save-roundtrip");
        let mut manager = OptionsManager::without_file();
        manager
            .set_item("chromium_options", "auto_port", true)
            .expect("set auto_port");
        manager
            .set_item("chromium_options", "arguments", json!(["--headless=new"]))
            .expect("set arguments");
        manager
            .set_item("others", "retry_interval", 2.5)
            .expect("set retry interval");

        let saved_path = manager
            .save(Some(dir.as_path()))
            .expect("save options manager into directory");
        let saved = fs::read_to_string(&saved_path).expect("read saved ini");
        let reloaded = OptionsManager::new(Some(saved_path.as_path())).expect("reload saved ini");

        assert_eq!(saved_path, dir.join("config.ini"));
        assert!(saved.contains("auto_port = True"));
        assert!(saved.contains("arguments = ['--headless=new']"));
        assert!(saved.contains("retry_interval = 2.5"));
        assert!(reloaded.file_exists());
        assert_eq!(
            reloaded.get_value("chromium_options", "auto_port"),
            Some(json!(true))
        );
        assert_eq!(
            reloaded.get_value("chromium_options", "arguments"),
            Some(json!(["--headless=new"]))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_manager_save_none_reuses_loaded_ini_path() {
        let dir = make_temp_dir("save-current");
        let config_path = dir.join("current.ini");
        fs::write(&config_path, "[others]\nretry_times = 3\n").expect("write current ini");

        let mut manager =
            OptionsManager::new(Some(config_path.as_path())).expect("load current options ini");
        manager
            .set_item("others", "retry_times", 9usize)
            .expect("update retry_times");

        let saved_path = manager.save(None).expect("save back to current ini");
        let saved = fs::read_to_string(&saved_path).expect("read saved current ini");

        assert_eq!(saved_path, config_path);
        assert!(saved.contains("retry_times = 9"));

        let _ = fs::remove_dir_all(&dir);
    }
}
