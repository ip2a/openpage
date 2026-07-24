use super::*;
use chromiumoxide::cdp::browser_protocol::dom::BackendNodeId;

// --- ref types ---
#[derive(Default)]
pub(super) struct RefRegistry {
    next_id: u64,
    refs: HashMap<String, RefTarget>,
    by_key: HashMap<String, String>,
}

#[derive(Clone)]
struct RefTarget {
    target_id: String,
    frame_target: Option<String>,
    backend_node_id: Option<BackendNodeId>,
    css_path: Option<String>,
    xpath: Option<String>,
    role: Option<String>,
    tag: Option<String>,
    name: Option<String>,
    text: Option<String>,
}

impl RefRegistry {
    pub(super) fn clear(&mut self) {
        self.next_id = 0;
        self.refs.clear();
        self.by_key.clear();
    }

    fn get(&self, ref_id: &str) -> Option<&RefTarget> {
        self.refs.get(ref_id)
    }

    fn register(&mut self, target: RefTarget) -> String {
        let key = target.key();
        if let Some(ref_id) = self.by_key.get(&key) {
            self.refs.insert(ref_id.clone(), target);
            return ref_id.clone();
        }
        self.next_id += 1;
        let ref_id = format!("e{}", self.next_id);
        self.register_as(ref_id.clone(), target);
        ref_id
    }

    fn register_as(&mut self, ref_id: String, target: RefTarget) {
        if let Some(number) = ref_id
            .strip_prefix('e')
            .and_then(|value| value.parse::<u64>().ok())
        {
            self.next_id = self.next_id.max(number);
        }
        self.by_key.insert(target.key(), ref_id.clone());
        self.refs.insert(ref_id, target);
    }
}

impl RefTarget {
    fn key(&self) -> String {
        // Identity for dedup: a CSS path uniquely addresses a single element, so
        // it alone is a stable identity. Fall back to xpath only when css_path is
        // absent. Metadata (role/tag/name/text) and the position-volatile xpath
        // are deliberately excluded so the same node keeps its ref_id across
        // minor text or sibling-order changes.
        let locator = self
            .css_path
            .as_deref()
            .filter(|value| !value.is_empty())
            .or_else(|| self.xpath.as_deref().filter(|value| !value.is_empty()))
            .unwrap_or("");
        format!(
            "{}|{}|{}",
            self.target_id,
            self.frame_target.as_deref().unwrap_or(""),
            locator
        )
    }
}

// --- ref parsing ---
pub(super) fn parse_ref(input: &str) -> Option<&str> {
    if let Some(stripped) = input.strip_prefix('@') {
        return parse_plain_ref(stripped);
    }
    if let Some(stripped) = input.strip_prefix("ref=") {
        return parse_plain_ref(stripped);
    }
    parse_plain_ref(input)
}

fn parse_plain_ref(input: &str) -> Option<&str> {
    if input.len() > 1 && input.starts_with('e') && input[1..].chars().all(|c| c.is_ascii_digit()) {
        Some(input)
    } else {
        None
    }
}

// --- ref candidate matching ---
fn candidate_matches_ref_target(element: &Element, target: &RefTarget) -> OpenPageResult<bool> {
    let tag = element.tag()?;
    if let Some(expected) = target.tag.as_deref()
        && tag != expected
    {
        return Ok(false);
    }

    let text = element.text()?.unwrap_or_default();
    let normalized_text = normalize_agent_text(&text);
    let attrs = compact_element_attrs(element.attrs()?);
    let role = element_role(&tag, &attrs);
    if let Some(expected) = target.role.as_deref()
        && role != expected
    {
        return Ok(false);
    }

    let name = element_name(Some(&normalized_text), &attrs).unwrap_or_default();
    if let Some(expected) = target.name.as_deref() {
        let expected = normalize_agent_text(expected);
        if !expected.is_empty() && name != expected && !name.starts_with(&expected) {
            return Ok(false);
        }
    }
    if let Some(expected) = target.text.as_deref() {
        let expected = normalize_agent_text(expected);
        if !expected.is_empty()
            && normalized_text != expected
            && !normalized_text.starts_with(&expected)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

// --- ref resolution on the live page ---
impl ServePage {
    pub(super) fn find_ref(&self, ref_id: &str) -> OpenPageResult<Element> {
        let target = self.refs.borrow().get(ref_id).cloned().ok_or_else(|| {
            OpenPageError::ElementNotFound(format!(
                "unknown ref @{ref_id}; run `openpage snapshot` or `openpage find` to refresh refs"
            ))
        })?;
        if target.target_id != self.current_target_id()
            || target.frame_target != self.active_frame_target
        {
            return Err(OpenPageError::ElementNotFound(format!(
                "ref @{ref_id} belongs to another page or frame; run `openpage snapshot` again"
            )));
        }
        if let Some(backend_node_id) = target.backend_node_id
            && let Ok(element) = self.page.resolve_dom_backend_node_id(backend_node_id)
        {
            self.refresh_ref_target(ref_id, &element)?;
            return Ok(element);
        }
        if let Some(element) = self.find_ref_by_locator_hints(&target) {
            self.refresh_ref_target(ref_id, &element)?;
            return Ok(element);
        }
        if let Some(element) = self.reresolve_ref_target(&target)? {
            self.refresh_ref_target(ref_id, &element)?;
            return Ok(element);
        }
        Err(OpenPageError::ElementNotFound(format!(
            "ref @{ref_id} is stale and could not be re-resolved; run `openpage snapshot` again"
        )))
    }

    pub(super) fn register_element(&self, element: &Element) -> OpenPageResult<String> {
        let css_path = element.css_path().ok().filter(|value| !value.is_empty());
        let xpath = element.xpath().ok().filter(|value| !value.is_empty());
        if css_path.is_none() && xpath.is_none() {
            return Err(OpenPageError::ElementNotFound(
                "element has no stable locator hints".to_string(),
            ));
        }
        let tag = element.tag().ok();
        let attrs = element.attrs().ok().map(compact_element_attrs);
        let text = element
            .text()
            .ok()
            .flatten()
            .map(|value| clip_agent_text(&value, 120));
        let role = tag
            .as_deref()
            .zip(attrs.as_ref())
            .map(|(tag, attrs)| element_role(tag, attrs));
        let name = attrs
            .as_ref()
            .and_then(|attrs| element_name(text.as_deref(), attrs));
        Ok(self.refs.borrow_mut().register(RefTarget {
            target_id: self.current_target_id(),
            frame_target: self.active_frame_target.clone(),
            backend_node_id: Some(element.backend_node_id()),
            css_path,
            xpath,
            role,
            tag,
            name,
            text,
        }))
    }

    fn find_ref_by_locator_hints(&self, target: &RefTarget) -> Option<Element> {
        if let Some(css_path) = target.css_path.as_deref().filter(|value| !value.is_empty())
            && let Ok(element) = self.find_raw(&format!("css:{css_path}"))
        {
            return Some(element);
        }
        if let Some(xpath) = target.xpath.as_deref().filter(|value| !value.is_empty())
            && let Ok(element) = self.find_raw(&format!("xpath:{xpath}"))
        {
            return Some(element);
        }
        None
    }

    fn reresolve_ref_target(&self, target: &RefTarget) -> OpenPageResult<Option<Element>> {
        let mut queries = Vec::new();
        for value in [target.name.as_deref(), target.text.as_deref()] {
            let Some(value) = value
                .map(normalize_agent_text)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if !queries.contains(&value) {
                queries.push(value);
            }
        }

        for query in queries {
            let locator = format!("text={query}");
            let elements = match self.find_all_raw(&locator) {
                Ok(elements) => elements,
                Err(_) => continue,
            };
            let mut matches = Vec::new();
            for element in elements {
                if candidate_matches_ref_target(&element, target)? {
                    matches.push(element);
                }
            }
            if matches.len() == 1 {
                return Ok(matches.into_iter().next());
            }
        }

        Ok(None)
    }

    fn refresh_ref_target(&self, ref_id: &str, element: &Element) -> OpenPageResult<()> {
        let css_path = element.css_path().ok().filter(|value| !value.is_empty());
        let xpath = element.xpath().ok().filter(|value| !value.is_empty());
        let tag = element.tag().ok();
        let attrs = element.attrs().ok().map(compact_element_attrs);
        let text = element
            .text()
            .ok()
            .flatten()
            .map(|value| clip_agent_text(&value, 120));
        let role = tag
            .as_deref()
            .zip(attrs.as_ref())
            .map(|(tag, attrs)| element_role(tag, attrs));
        let name = attrs
            .as_ref()
            .and_then(|attrs| element_name(text.as_deref(), attrs));

        self.refs.borrow_mut().register_as(
            ref_id.to_string(),
            RefTarget {
                target_id: self.current_target_id(),
                frame_target: self.active_frame_target.clone(),
                backend_node_id: Some(element.backend_node_id()),
                css_path,
                xpath,
                role,
                tag,
                name,
                text,
            },
        );
        Ok(())
    }

    pub(super) fn register_snapshot_entries(&self, entries: &mut [Value]) {
        // Refs persist across snapshots: register() reuses the existing ref_id
        // when an element's identity key matches, and assigns the next continuing
        // id to new elements, so `e3` keeps meaning the same element across calls.
        let target_id = self.current_target_id();
        let frame_target = self.active_frame_target.clone();
        for entry in entries {
            let Some(obj) = entry.as_object_mut() else {
                continue;
            };
            let css_path = obj
                .remove("_cssPath")
                .and_then(|value| value.as_str().map(ToString::to_string))
                .filter(|value| !value.is_empty());
            let xpath = obj
                .remove("_xpath")
                .and_then(|value| value.as_str().map(ToString::to_string))
                .filter(|value| !value.is_empty());
            let target = RefTarget {
                target_id: target_id.clone(),
                frame_target: frame_target.clone(),
                backend_node_id: None,
                css_path,
                xpath,
                role: obj
                    .get("role")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                tag: obj
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                name: obj
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                text: obj
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            };
            let ref_id = self.refs.borrow_mut().register(target);
            obj["ref"] = Value::String(ref_id);
        }
    }
}
