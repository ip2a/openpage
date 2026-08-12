use super::*;
use chromiumoxide::cdp::browser_protocol::accessibility::{AxNode, AxValue, GetFullAxTreeParams};
use chromiumoxide::cdp::browser_protocol::page::FrameId;

const MAX_SNAPSHOT_ENTRIES: usize = 200;

const INTERACTIVE_AX_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "treeitem",
    "iframe",
];

const CONTENT_AX_ROLES: &[&str] = &[
    "heading",
    "cell",
    "gridcell",
    "columnheader",
    "rowheader",
    "listitem",
    "article",
    "region",
    "main",
    "navigation",
    "paragraph",
    "term",
    "definition",
    "status",
    "alert",
    "note",
    "figure",
    "caption",
];

#[derive(Debug)]
struct AxSnapshotNode {
    parent_id: Option<String>,
    role: String,
    name: String,
    value: Option<String>,
    backend_node_id: Option<i64>,
    frame_id: Option<String>,
    state: Map<String, Value>,
    ignored: bool,
    children: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSnapshotMode {
    Interactive,
    Semantic,
    All,
}

impl AgentSnapshotMode {
    fn parse(value: Option<&str>) -> OpenPageResult<Self> {
        match value.unwrap_or("interactive") {
            "interactive" => Ok(Self::Interactive),
            "semantic" => Ok(Self::Semantic),
            "all" => Ok(Self::All),
            other => Err(OpenPageError::UnsupportedOperation(format!(
                "unsupported snapshot mode: {other}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Semantic => "semantic",
            Self::All => "all",
        }
    }

    fn default_depth(self) -> usize {
        match self {
            Self::Interactive => 10,
            Self::Semantic => 8,
            Self::All => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSnapshotFormat {
    Text,
    Json,
}

impl AgentSnapshotFormat {
    fn parse(value: Option<&str>) -> OpenPageResult<Self> {
        match value.unwrap_or("text") {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(OpenPageError::UnsupportedOperation(format!(
                "unsupported snapshot format: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
struct AgentSnapshotOptions {
    mode: AgentSnapshotMode,
    format: AgentSnapshotFormat,
    raw: bool,
    compact: bool,
    depth: usize,
    selector: Option<String>,
}

impl AgentSnapshotOptions {
    fn from_params(params: &Value) -> OpenPageResult<Self> {
        let mode = AgentSnapshotMode::parse(optional_str(params, "mode"))?;
        let format = AgentSnapshotFormat::parse(optional_str(params, "format"))?;
        let depth = optional_u64(params, "depth")
            .map(|value| value as usize)
            .unwrap_or_else(|| mode.default_depth());

        Ok(Self {
            mode,
            format,
            raw: optional_bool(params, "raw").unwrap_or(false),
            compact: optional_bool(params, "compact").unwrap_or(false),
            depth,
            selector: optional_string(params, "selector"),
        })
    }
}

fn agent_snapshot_script(options: &AgentSnapshotOptions) -> OpenPageResult<String> {
    let options_json = serde_json::to_string(&json!({
        "mode": options.mode.as_str(),
        "depth": options.depth,
        "selector": options.selector,
        "maxEntries": MAX_SNAPSHOT_ENTRIES,
    }))
    .map_err(|err| OpenPageError::Serialization(err.to_string()))?;

    Ok(format!(
        r#"
        (() => {{
            const options = {options_json};
            const interactiveTags = new Set(['a', 'button', 'input', 'textarea', 'select', 'option', 'summary']);
            const interactiveRoles = new Set(['button', 'link', 'checkbox', 'radio', 'switch', 'tab', 'menuitem', 'option', 'textbox', 'combobox', 'searchbox']);
            const semanticTags = new Set(['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'main', 'nav', 'article', 'section', 'aside', 'label']);
            const semanticRoles = new Set(['heading', 'main', 'navigation', 'article', 'region', 'cell', 'gridcell', 'columnheader', 'rowheader', 'listitem']);
            const cleanText = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const clipText = (value, limit = 80) => cleanText(value).slice(0, limit);
            const cssEscape = (value) => {{
                if (globalThis.CSS && typeof globalThis.CSS.escape === 'function') return globalThis.CSS.escape(value);
                return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => `\\${{ch}}`);
            }};
            const roleOf = (el) => {{
                const explicit = cleanText(el.getAttribute('role')).toLowerCase();
                if (explicit) return explicit;
                const tag = el.tagName.toLowerCase();
                if (tag === 'a' && el.hasAttribute('href')) return 'link';
                if (tag === 'button') return 'button';
                if (tag === 'textarea') return 'textbox';
                if (tag === 'select') return 'combobox';
                if (tag === 'option') return 'option';
                if (tag === 'input') {{
                    const type = cleanText(el.getAttribute('type')).toLowerCase();
                    if (type === 'checkbox') return 'checkbox';
                    if (type === 'radio') return 'radio';
                    if (type === 'button' || type === 'submit' || type === 'reset') return 'button';
                    if (type === 'search') return 'searchbox';
                    return 'textbox';
                }}
                if (/^h[1-6]$/.test(tag)) return 'heading';
                return tag;
            }};
            const labelText = (el) => {{
                if (!el.labels || el.labels.length === 0) return '';
                return clipText(Array.from(el.labels)
                    .map(label => label.innerText || label.textContent || '')
                    .join(' '));
            }};
            const accessibleName = (el) => {{
                const aria = clipText(el.getAttribute('aria-label') || '');
                if (aria) return aria;
                const title = clipText(el.getAttribute('title') || '');
                if (title) return title;
                const alt = clipText(el.getAttribute('alt') || '');
                if (alt) return alt;
                const label = labelText(el);
                if (label) return label;
                const value = clipText(el.getAttribute('value') || '');
                if (value && ['input', 'option'].includes(el.tagName.toLowerCase())) return value;
                return clipText(el.innerText || el.textContent || '');
            }};
            const isVisible = (el) => {{
                const rect = el.getBoundingClientRect();
                if (rect.width === 0 || rect.height === 0) return false;
                const style = getComputedStyle(el);
                return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity || 1) !== 0;
            }};
            const isInteractive = (el) => {{
                const tag = el.tagName.toLowerCase();
                if (interactiveTags.has(tag)) return true;
                if (el.onclick || el.hasAttribute('onclick')) return true;
                if (el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1') return true;
                if (getComputedStyle(el).cursor === 'pointer') return true;
                if (el.isContentEditable) return true;
                return interactiveRoles.has(roleOf(el));
            }};
            const isSemantic = (el) => {{
                const tag = el.tagName.toLowerCase();
                if (semanticTags.has(tag)) return true;
                return semanticRoles.has(roleOf(el));
            }};
            const includeElement = (el) => {{
                if (!(el instanceof HTMLElement) || !isVisible(el)) return false;
                if (options.mode === 'interactive') return isInteractive(el);
                if (options.mode === 'semantic') return isInteractive(el) || isSemantic(el);
                return isInteractive(el) || isSemantic(el) || !!accessibleName(el);
            }};
            const nearestHeading = (el) => {{
                let node = el.previousElementSibling;
                while (node) {{
                    if (/^H[1-6]$/.test(node.tagName)) return clipText(node.innerText || node.textContent || '');
                    node = node.previousElementSibling;
                }}
                const parent = el.parentElement;
                if (!parent) return '';
                const heading = parent.querySelector('h1,h2,h3,h4,h5,h6');
                return heading ? clipText(heading.innerText || heading.textContent || '') : '';
            }};
            const cssPathOf = (el) => {{
                if (!(el instanceof Element)) return '';
                const parts = [];
                let node = el;
                while (node && node.nodeType === Node.ELEMENT_NODE) {{
                    const tag = node.tagName.toLowerCase();
                    if (node.id) {{
                        parts.unshift(`${{tag}}#${{cssEscape(node.id)}}`);
                        break;
                    }}
                    let nth = 1;
                    let sib = node;
                    while ((sib = sib.previousElementSibling)) nth += 1;
                    parts.unshift(`${{tag}}:nth-child(${{nth}})`);
                    node = node.parentElement;
                }}
                return parts.join(' > ');
            }};
            const xpathOf = (el) => {{
                if (!(el instanceof Element)) return '';
                const parts = [];
                let node = el;
                while (node && node.nodeType === Node.ELEMENT_NODE) {{
                    const tag = node.tagName.toLowerCase();
                    let index = 1;
                    let sib = node;
                    while ((sib = sib.previousElementSibling)) {{
                        if (sib.tagName.toLowerCase() === tag) index += 1;
                    }}
                    parts.unshift(`${{tag}}[${{index}}]`);
                    node = node.parentElement;
                }}
                return '/' + parts.join('/');
            }};
            const root = options.selector ? document.querySelector(options.selector) : document.body;
            if (!root) return {{ entries: [], truncated: false, error: options.selector ? `selector not found: ${{options.selector}}` : null, options }};
            const snapshot = [];
            const visit = (el, depth) => {{
                if (!el || snapshot.length >= options.maxEntries || depth > options.depth) return;
                if (includeElement(el)) snapshot.push({{ el, depth }});
                Array.from(el.children || []).forEach(child => visit(child, depth + 1));
            }};
            visit(root, 0);
            const entries = [];
            snapshot.forEach((item, i) => {{
                const el = item.el;
                const ref = 'e' + (i + 1);
                const attrs = {{}};
                for (const attr of ['id', 'name', 'type', 'placeholder', 'href', 'role', 'aria-label', 'title', 'alt', 'value']) {{
                    if (!el.hasAttribute(attr)) continue;
                    const value = cleanText(el.getAttribute(attr));
                    if (value) attrs[attr] = value;
                }}
                const rect = el.getBoundingClientRect();
                const entry = {{
                    ref,
                    role: roleOf(el),
                    tag: el.tagName.toLowerCase(),
                    name: accessibleName(el),
                    text: clipText(el.innerText || el.textContent || ''),
                    attrs,
                    depth: item.depth,
                    _cssPath: cssPathOf(el),
                    _xpath: xpathOf(el),
                    state: {{
                        visible: true,
                        disabled: !!el.disabled,
                        checked: !!el.checked,
                        selected: !!el.selected,
                        focused: document.activeElement === el,
                        inViewport: rect.bottom > 0 && rect.right > 0 && rect.top < window.innerHeight && rect.left < window.innerWidth,
                    }},
                }};
                const label = labelText(el);
                if (label) entry.label = label;
                const heading = nearestHeading(el);
                if (heading && heading !== entry.name && heading !== entry.text) entry.context = heading;
                entries.push(entry);
            }});
            return {{
                entries,
                truncated: snapshot.length >= options.maxEntries,
                error: null,
                options,
            }};
        }})()
    "#
    ))
}

fn ax_value_text(value: Option<&AxValue>) -> Option<String> {
    value
        .and_then(|value| value.value.as_ref())
        .map(|value| match value {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        })
        .map(|value| clip_agent_text(&normalize_agent_text(&value), 160))
        .filter(|value| !value.is_empty())
}

fn ax_snapshot_nodes(nodes: Vec<AxNode>) -> (Vec<AxSnapshotNode>, Vec<usize>) {
    let ids = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.node_id.as_ref().to_string(), index))
        .collect::<HashMap<_, _>>();
    let mut result = nodes
        .iter()
        .map(|node| {
            let mut state = Map::new();
            for property in node.properties.as_deref().unwrap_or_default() {
                let key = match property.name.as_ref() {
                    "disabled" => "disabled",
                    "checked" => "checked",
                    "selected" => "selected",
                    "focused" => "focused",
                    "expanded" => "expanded",
                    "required" => "required",
                    _ => continue,
                };
                if let Some(value) = property.value.value.clone() {
                    state.insert(key.to_string(), value);
                }
            }
            AxSnapshotNode {
                parent_id: node.parent_id.as_ref().map(|id| id.as_ref().to_string()),
                role: ax_value_text(node.role.as_ref())
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                name: ax_value_text(node.name.as_ref()).unwrap_or_default(),
                value: ax_value_text(node.value.as_ref()),
                backend_node_id: node.backend_dom_node_id.map(|id| *id.inner()),
                frame_id: node.frame_id.as_ref().map(|id| id.as_ref().to_string()),
                state,
                ignored: node.ignored,
                children: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut roots = Vec::new();
    for index in 0..result.len() {
        let parent = result[index]
            .parent_id
            .as_deref()
            .and_then(|id| ids.get(id))
            .copied();
        if let Some(parent) = parent {
            result[parent].children.push(index);
        } else {
            roots.push(index);
        }
    }
    (result, roots)
}

fn includes_ax_node(node: &AxSnapshotNode, mode: AgentSnapshotMode) -> bool {
    if node.ignored
        || node.role.is_empty()
        || node.role == "inlinetextbox"
        || node.backend_node_id.is_none()
    {
        return false;
    }
    let interactive = INTERACTIVE_AX_ROLES.contains(&node.role.as_str());
    let content = CONTENT_AX_ROLES.contains(&node.role.as_str());
    match mode {
        AgentSnapshotMode::Interactive => interactive,
        AgentSnapshotMode::Semantic => {
            interactive || content || (node.role == "statictext" && !node.name.is_empty())
        }
        AgentSnapshotMode::All => {
            interactive || content || !node.name.is_empty() || node.value.is_some()
        }
    }
}

fn render_ax_entries(
    nodes: &[AxSnapshotNode],
    roots: &[usize],
    options: &AgentSnapshotOptions,
) -> (Vec<Value>, bool) {
    fn visit(
        nodes: &[AxSnapshotNode],
        index: usize,
        options: &AgentSnapshotOptions,
        depth: usize,
        parent_display: Option<&str>,
        entries: &mut Vec<Value>,
        truncated: &mut bool,
    ) {
        if entries.len() >= MAX_SNAPSHOT_ENTRIES {
            *truncated = true;
            return;
        }
        let node = &nodes[index];
        let mut included = includes_ax_node(node, options.mode);
        if node.role == "statictext" && parent_display == Some(node.name.as_str()) {
            included = false;
        }
        if included && depth > options.depth {
            return;
        }

        let next_depth = if included { depth + 1 } else { depth };
        let next_display = if included && !node.name.is_empty() {
            Some(node.name.as_str())
        } else {
            parent_display
        };
        if included {
            let mut entry = Map::new();
            entry.insert("role".to_string(), json!(node.role));
            entry.insert("name".to_string(), json!(node.name));
            entry.insert("depth".to_string(), json!(depth));
            if let Some(value) = node.value.as_deref() {
                entry.insert("value".to_string(), json!(value));
            }
            if let Some(backend_node_id) = node.backend_node_id {
                entry.insert("_backendNodeId".to_string(), json!(backend_node_id));
            }
            if let Some(frame_id) = node.frame_id.as_deref() {
                entry.insert("_frameId".to_string(), json!(frame_id));
            }
            if !node.state.is_empty() {
                entry.insert("state".to_string(), Value::Object(node.state.clone()));
            }
            entries.push(Value::Object(entry));
        }

        for child in &node.children {
            visit(
                nodes,
                *child,
                options,
                next_depth,
                next_display,
                entries,
                truncated,
            );
            if *truncated {
                break;
            }
        }
    }

    let mut entries = Vec::new();
    let mut truncated = false;
    for root in roots {
        visit(nodes, *root, options, 0, None, &mut entries, &mut truncated);
        if truncated {
            break;
        }
    }
    (entries, truncated)
}

fn collect_ax_snapshot(
    state: &ServePage,
    options: &AgentSnapshotOptions,
) -> OpenPageResult<Option<(Vec<Value>, bool)>> {
    let frame = state.current_frame()?;
    let params = match frame.as_ref() {
        Some(frame) => GetFullAxTreeParams::builder()
            .frame_id(FrameId::new(frame.id().to_string()))
            .build(),
        None => GetFullAxTreeParams::default(),
    };
    let response = state.page.execute_cdp(params)?;
    let (nodes, default_roots) = ax_snapshot_nodes(response.nodes);
    let roots = if let Some(selector) = options.selector.as_deref() {
        let backend_node_id = match state.find_raw(selector) {
            Ok(element) => *element.backend_node_id().inner(),
            Err(_) => return Ok(None),
        };
        let Some(index) = nodes
            .iter()
            .position(|node| node.backend_node_id == Some(backend_node_id))
        else {
            return Ok(None);
        };
        vec![index]
    } else {
        default_roots
    };
    Ok(Some(render_ax_entries(&nodes, &roots, options)))
}

pub(super) fn snapshot_payload(state: &mut ServePage, params: &Value) -> OpenPageResult<Value> {
    let options = AgentSnapshotOptions::from_params(params)?;
    let origin = current_page_origin(state);
    let title = current_page_title(state);
    let ax_snapshot = collect_ax_snapshot(state, &options).ok().flatten();
    let fallback = if ax_snapshot.is_none() {
        Some(state.run_js(&agent_snapshot_script(&options)?)?)
    } else {
        None
    };
    let (mut entries, truncated) = match ax_snapshot {
        Some(result) => result,
        None => {
            let snapshot = fallback.as_ref().expect("fallback snapshot");
            (
                snapshot
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                snapshot
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
        }
    };
    state.register_snapshot_entries(&mut entries);

    let mut payload = payload_object(
        "text",
        Value::String(format_snapshot_text(
            &entries,
            title.as_deref(),
            origin.as_deref(),
        )),
        origin.as_deref(),
        title.as_deref(),
    );
    if !options.compact {
        payload.insert("refs".to_string(), Value::Object(snapshot_refs(&entries)));
    }
    payload.insert("count".to_string(), json!(entries.len()));
    payload.insert("mode".to_string(), json!(options.mode.as_str()));
    payload.insert("depth".to_string(), json!(options.depth));
    if let Some(selector) = options.selector {
        payload.insert("selector".to_string(), json!(selector));
    }
    if truncated {
        payload.insert("truncated".to_string(), json!(true));
    }
    if let Some(error) = fallback
        .as_ref()
        .and_then(|snapshot| snapshot.get("error"))
        .and_then(Value::as_str)
    {
        if !error.is_empty() {
            payload.insert("warning".to_string(), json!(error));
        }
    }
    if options.raw || options.format == AgentSnapshotFormat::Json {
        payload.insert("snapshot".to_string(), Value::Array(entries));
    }
    if options.compact && options.format == AgentSnapshotFormat::Json {
        payload.remove("text");
    }

    Ok(Value::Object(payload))
}

fn format_snapshot_text(entries: &[Value], title: Option<&str>, origin: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(title) = title {
        lines.push(format!("Page: {title}"));
    }
    if let Some(origin) = origin {
        lines.push(format!("URL: {origin}"));
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }

    if entries.is_empty() {
        lines.push("No interactive elements found".to_string());
        return lines.join("\n");
    }

    for entry in entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let ref_id = obj.get("ref").and_then(Value::as_str);
        let role = obj.get("role").and_then(Value::as_str).unwrap_or("element");
        let tag = obj.get("tag").and_then(Value::as_str);
        let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
        let text = obj.get("text").and_then(Value::as_str).unwrap_or("");
        let attrs = obj.get("attrs").and_then(Value::as_object);
        let label = obj.get("label").and_then(Value::as_str).unwrap_or("");
        let context = obj.get("context").and_then(Value::as_str).unwrap_or("");
        let state = obj.get("state").and_then(Value::as_object);
        let disabled = state
            .and_then(|state| state.get("disabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let checked = state
            .and_then(|state| state.get("checked"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let selected = state
            .and_then(|state| state.get("selected"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let focused = state
            .and_then(|state| state.get("focused"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let in_viewport = state
            .and_then(|state| state.get("inViewport"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let indent = obj
            .get("depth")
            .and_then(Value::as_u64)
            .map(|depth| "  ".repeat(depth.min(6) as usize))
            .unwrap_or_default();
        let display = if !name.is_empty() { name } else { text };
        let mut line = match ref_id {
            Some(ref_id) => format!("{indent}@{ref_id} {role}"),
            None => format!("{indent}{role}"),
        };
        if let Some(tag) = tag {
            line.push_str(" [");
            line.push_str(tag);
            line.push(']');
        }
        if !display.is_empty() {
            line.push(' ');
            line.push('"');
            line.push_str(&escape_snapshot_value(display));
            line.push('"');
        }
        if !label.is_empty() {
            line.push(' ');
            line.push_str("label=\"");
            line.push_str(&escape_snapshot_value(label));
            line.push('"');
        }

        if let Some(attrs) = attrs {
            for key in [
                "type",
                "placeholder",
                "href",
                "role",
                "aria-label",
                "alt",
                "title",
                "value",
                "name",
                "id",
                "class",
            ] {
                if let Some(value) = attrs
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    line.push(' ');
                    line.push_str(key);
                    line.push_str("=\"");
                    line.push_str(&escape_snapshot_value(value));
                    line.push('"');
                }
            }
        }
        if checked {
            line.push_str(" checked");
        }
        if selected {
            line.push_str(" selected");
        }
        if disabled {
            line.push_str(" disabled");
        }
        if focused {
            line.push_str(" focused");
        }
        if in_viewport {
            line.push_str(" in_viewport");
        }
        if !context.is_empty() {
            line.push_str(" context=\"");
            line.push_str(&escape_snapshot_value(context));
            line.push('"');
        }

        lines.push(line);
    }

    lines.join("\n")
}

fn snapshot_refs(entries: &[Value]) -> Map<String, Value> {
    let mut refs = Map::new();
    for entry in entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some(ref_id) = obj.get("ref").and_then(Value::as_str) else {
            continue;
        };

        let mut ref_obj = Map::new();
        if let Some(role) = obj.get("role").and_then(Value::as_str) {
            ref_obj.insert("role".to_string(), Value::String(role.to_string()));
        }
        if let Some(tag) = obj.get("tag").and_then(Value::as_str) {
            ref_obj.insert("tag".to_string(), Value::String(tag.to_string()));
        }
        if let Some(name) = obj.get("name").and_then(Value::as_str) {
            ref_obj.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(text) = obj.get("text").and_then(Value::as_str) {
            ref_obj.insert("text".to_string(), Value::String(text.to_string()));
        }
        if let Some(label) = obj.get("label").and_then(Value::as_str) {
            ref_obj.insert("label".to_string(), Value::String(label.to_string()));
        }
        if let Some(attrs) = obj.get("attrs").and_then(Value::as_object) {
            ref_obj.insert("attrs".to_string(), Value::Object(attrs.clone()));
        }
        if let Some(state) = obj.get("state").and_then(Value::as_object) {
            ref_obj.insert("state".to_string(), Value::Object(state.clone()));
        }
        refs.insert(ref_id.to_string(), Value::Object(ref_obj));
    }
    refs
}

fn escape_snapshot_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(role: &str, name: &str, backend_node_id: i64) -> AxSnapshotNode {
        AxSnapshotNode {
            parent_id: None,
            role: role.to_string(),
            name: name.to_string(),
            value: None,
            backend_node_id: Some(backend_node_id),
            frame_id: None,
            state: Map::new(),
            ignored: false,
            children: Vec::new(),
        }
    }

    fn options(mode: AgentSnapshotMode, depth: usize) -> AgentSnapshotOptions {
        AgentSnapshotOptions {
            mode,
            format: AgentSnapshotFormat::Json,
            raw: false,
            compact: false,
            depth,
            selector: None,
        }
    }

    #[test]
    fn snapshot_modes_keep_only_relevant_ax_nodes() {
        let button = node("button", "Sign in", 1);
        let heading = node("heading", "Products", 2);
        let text = node("statictext", "Six products", 3);

        assert!(includes_ax_node(&button, AgentSnapshotMode::Interactive));
        assert!(!includes_ax_node(&heading, AgentSnapshotMode::Interactive));
        assert!(includes_ax_node(&heading, AgentSnapshotMode::Semantic));
        assert!(includes_ax_node(&text, AgentSnapshotMode::Semantic));
    }

    #[test]
    fn snapshot_depth_ignores_unincluded_wrapper_nodes() {
        let mut root = node("rootwebarea", "Page", 1);
        root.children = vec![1];
        let mut wrapper = node("generic", "", 2);
        wrapper.children = vec![2];
        let button = node("button", "Deep button", 3);
        let nodes = vec![root, wrapper, button];

        let (entries, truncated) =
            render_ax_entries(&nodes, &[0], &options(AgentSnapshotMode::Interactive, 0));

        assert!(!truncated);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["name"], "Deep button");
        assert_eq!(entries[0]["depth"], 0);
    }

    #[test]
    fn snapshot_omits_duplicate_static_text_under_named_parent() {
        let mut button = node("button", "Sign in", 1);
        button.children = vec![1];
        let text = node("statictext", "Sign in", 2);
        let nodes = vec![button, text];

        let (entries, _) =
            render_ax_entries(&nodes, &[0], &options(AgentSnapshotMode::Semantic, 2));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["role"], "button");
    }

    #[test]
    fn ax_text_format_does_not_invent_unknown_tag() {
        let text = format_snapshot_text(
            &[json!({"ref": "e1", "role": "button", "name": "Sign in", "depth": 0})],
            None,
            None,
        );

        assert_eq!(text, "@e1 button \"Sign in\"");
    }
}
