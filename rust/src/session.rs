use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ego_tree::NodeId;
use encoding_rs::Encoding;
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{CONTENT_TYPE, USER_AGENT};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use url::Url;

use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{Locator, LocatorKind};

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub timeout_secs: u64,
    pub user_agent: Option<String>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            timeout_secs: 15,
            user_agent: None,
        }
    }
}

#[derive(Debug)]
struct SessionState {
    client: Client,
    cookie_jar: Arc<Jar>,
    user_agent: Option<String>,
    url: Option<String>,
    status_code: Option<u16>,
    encoding: Option<String>,
    body: Option<Arc<String>>,
    raw_data: Option<Arc<Vec<u8>>>,
    json: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct SessionPage {
    inner: Arc<Mutex<SessionState>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SessionElement {
    html: Arc<String>,
    node_id: NodeId,
}

impl SessionPage {
    pub fn new(options: SessionOptions) -> OpenPageResult<Self> {
        let cookie_jar = Arc::new(Jar::default());

        let client = ClientBuilder::new()
            .cookie_provider(Arc::clone(&cookie_jar))
            .no_proxy()
            .timeout(Duration::from_secs(options.timeout_secs))
            .build()
            .map_err(|err| OpenPageError::Http(format!("{err:?}")))?;

        Ok(Self {
            inner: Arc::new(Mutex::new(SessionState {
                client,
                cookie_jar,
                user_agent: options.user_agent,
                url: None,
                status_code: None,
                encoding: None,
                body: None,
                raw_data: None,
                json: None,
            })),
        })
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        let (client, user_agent) = self.request_parts()?;
        let mut request = client.get(url);
        if let Some(user_agent) = user_agent {
            request = request.header(USER_AGENT, user_agent);
        }

        let response = request
            .send()
            .map_err(|err| OpenPageError::Http(format!("{err:?}")))?;
        self.store_response(url, response)
    }

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<bool> {
        let (client, user_agent) = self.request_parts()?;
        let mut request = client.post(url);
        if let Some(user_agent) = user_agent {
            request = request.header(USER_AGENT, user_agent);
        }

        let response = match payload {
            Some(payload) => request
                .json(&payload)
                .send()
                .map_err(|err| OpenPageError::Http(format!("{err:?}")))?,
            None => request
                .send()
                .map_err(|err| OpenPageError::Http(format!("{err:?}")))?,
        };
        self.store_response(url, response)
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.url.clone())
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        Ok(self.lock_state()?.status_code)
    }

    pub fn html(&self) -> OpenPageResult<String> {
        Ok(self
            .lock_state()?
            .body
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        Ok(self
            .lock_state()?
            .raw_data
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.encoding.clone())
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        Ok(self.lock_state()?.json.clone())
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        let body = self.body_arc()?;
        Ok(self.first_text(&body, "title")?)
    }

    pub fn user_agent(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.user_agent.clone())
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(true)
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(false)
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        Ok(None)
    }

    pub fn is_headless(&self) -> bool {
        false
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        let Some(url) = self.url()? else {
            return Ok(Vec::new());
        };
        let Some(cookie_header) = self.cookie_header(&url)? else {
            return Ok(Vec::new());
        };
        cookies_from_header(&url, &cookie_header)
    }

    pub fn root(&self) -> OpenPageResult<SessionElement> {
        let body = self.body_arc()?;
        snapshot_root_arc(body)
    }

    pub fn set_user_agent(&self, user_agent: Option<String>) -> OpenPageResult<()> {
        self.lock_state()?.user_agent = user_agent;
        Ok(())
    }

    pub fn cookie_header(&self, url: &str) -> OpenPageResult<Option<String>> {
        let url = Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string()))?;
        let jar = self.lock_state()?.cookie_jar.clone();
        jar.cookies(&url)
            .map(|value| {
                value
                    .to_str()
                    .map(|text| text.to_string())
                    .map_err(|err| OpenPageError::Http(err.to_string()))
            })
            .transpose()
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string()))?;
        let jar = self.lock_state()?.cookie_jar.clone();
        for cookie in cookie_header
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            jar.add_cookie_str(cookie, &url);
        }
        Ok(())
    }

    pub fn find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        let body = self.body_arc()?;
        snapshot_find_arc(body, locator)
    }

    pub fn find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        let body = self.body_arc()?;
        snapshot_find_all_arc(body, locator)
    }

    fn first_text(&self, body: &Arc<String>, selector: &str) -> OpenPageResult<Option<String>> {
        let html = Html::parse_document(body);
        let selector_obj = Selector::parse(selector)
            .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
        Ok(html
            .select(&selector_obj)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|text| !text.is_empty()))
    }

    fn body_arc(&self) -> OpenPageResult<Arc<String>> {
        self.lock_state()?
            .body
            .as_ref()
            .cloned()
            .ok_or_else(|| OpenPageError::Http("session page has no loaded document".to_string()))
    }

    fn lock_state(&self) -> OpenPageResult<std::sync::MutexGuard<'_, SessionState>> {
        self.inner
            .lock()
            .map_err(|_| OpenPageError::Http("session state lock poisoned".to_string()))
    }

    fn request_parts(&self) -> OpenPageResult<(Client, Option<String>)> {
        let state = self.lock_state()?;
        Ok((state.client.clone(), state.user_agent.clone()))
    }

    fn store_response(
        &self,
        requested_url: &str,
        response: reqwest::blocking::Response,
    ) -> OpenPageResult<bool> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let raw_data = response
            .bytes()
            .map_err(|err| OpenPageError::Http(format!("{err:?}")))?
            .to_vec();
        let encoding = detect_body_encoding(content_type.as_deref(), &raw_data);
        let text = decode_body(&raw_data, encoding.as_deref());
        let parsed_json = serde_json::from_str::<Value>(&text).ok();

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.encoding = encoding;
        state.body = Some(Arc::new(text));
        state.raw_data = Some(Arc::new(raw_data));
        state.json = parsed_json;
        Ok((200..400).contains(&status))
    }
}

impl SessionElement {
    pub fn find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        find_in_scope(Arc::clone(&self.html), self.node_id, locator)
    }

    pub fn find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        find_all_in_scope(Arc::clone(&self.html), self.node_id, locator)
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(element.value().name().to_string()))
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>().trim().to_string();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.html())))
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.inner_html())))
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.with_element(|element| {
            Ok(element
                .value()
                .attrs
                .iter()
                .map(|(name, value)| (name.local.to_string(), value.to_string()))
                .collect())
        })
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(element.attr(name).map(ToString::to_string)))
    }

    pub fn parent(&self) -> OpenPageResult<SessionElement> {
        self.with_element(|element| {
            nearest_parent_element(element)
                .map(|parent| session_element_from_ref(&self.html, parent))
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound("parent element not found".to_string())
                })
        })
    }

    pub fn child(&self) -> OpenPageResult<SessionElement> {
        self.child_with(None, 1)
    }

    pub fn child_with(
        &self,
        locator: Option<&str>,
        index: usize,
    ) -> OpenPageResult<SessionElement> {
        nth_from_start(
            self.children_with(locator)?,
            index,
            "child element not found",
        )
    }

    pub fn children(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.children_with(None)
    }

    pub fn children_with(&self, locator: Option<&str>) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            collect_matching_elements(&self.html, element.child_elements(), locator)
        })
    }

    pub fn prev(&self) -> OpenPageResult<SessionElement> {
        self.prev_with(None, 1)
    }

    pub fn prev_with(&self, locator: Option<&str>, index: usize) -> OpenPageResult<SessionElement> {
        nth_from_end(
            self.prevs_with(locator)?,
            index,
            "previous element not found",
        )
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.prevs_with(None)
    }

    pub fn prevs_with(&self, locator: Option<&str>) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            let mut items = collect_matching_elements(
                &self.html,
                element.prev_siblings().filter_map(ElementRef::wrap),
                locator,
            )?;
            items.reverse();
            Ok(items)
        })
    }

    pub fn next(&self) -> OpenPageResult<SessionElement> {
        self.next_with(None, 1)
    }

    pub fn next_with(&self, locator: Option<&str>, index: usize) -> OpenPageResult<SessionElement> {
        nth_from_start(self.nexts_with(locator)?, index, "next element not found")
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.nexts_with(None)
    }

    pub fn nexts_with(&self, locator: Option<&str>) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            collect_matching_elements(
                &self.html,
                element.next_siblings().filter_map(ElementRef::wrap),
                locator,
            )
        })
    }

    pub fn before(&self) -> OpenPageResult<SessionElement> {
        self.before_with(None, 1)
    }

    pub fn before_with(
        &self,
        locator: Option<&str>,
        index: usize,
    ) -> OpenPageResult<SessionElement> {
        nth_from_end(
            self.befores_with(locator)?,
            index,
            "preceding element not found",
        )
    }

    pub fn befores(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.befores_with(None)
    }

    pub fn befores_with(&self, locator: Option<&str>) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            document_relatives(&self.html, element, RelativeDirection::Before, locator)
        })
    }

    pub fn after(&self) -> OpenPageResult<SessionElement> {
        self.after_with(None, 1)
    }

    pub fn after_with(
        &self,
        locator: Option<&str>,
        index: usize,
    ) -> OpenPageResult<SessionElement> {
        nth_from_start(
            self.afters_with(locator)?,
            index,
            "following element not found",
        )
    }

    pub fn afters(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.afters_with(None)
    }

    pub fn afters_with(&self, locator: Option<&str>) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            document_relatives(&self.html, element, RelativeDirection::After, locator)
        })
    }

    fn with_element<T, F>(&self, f: F) -> OpenPageResult<T>
    where
        F: FnOnce(ElementRef<'_>) -> OpenPageResult<T>,
    {
        let document = Html::parse_document(self.html.as_ref());
        let element = element_from_document(&document, self.node_id)?;
        f(element)
    }
}

fn selector_from_locator(locator: &str) -> OpenPageResult<String> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => Ok(locator.query().to_string()),
        LocatorKind::XPath => Err(OpenPageError::UnsupportedLocator(
            "xpath is not implemented for SessionPage".to_string(),
        )),
    }
}

pub fn cookies_from_header(url: &str, cookie_header: &str) -> OpenPageResult<Vec<CookieEntry>> {
    let parsed = Url::parse(url).map_err(|err| OpenPageError::Http(err.to_string()))?;
    let domain = parsed.domain().map(ToString::to_string);
    Ok(cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let (name, value) = item.split_once('=')?;
            Some(CookieEntry {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
                domain: domain.clone(),
            })
        })
        .collect())
}

fn detect_body_encoding(content_type: Option<&str>, body: &[u8]) -> Option<String> {
    if let Some(content_type) = content_type {
        for part in content_type.split(';').skip(1) {
            let trimmed = part.trim();
            let Some((name, value)) = trimmed.split_once('=') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("charset") {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_ascii_lowercase());
                }
            }
        }
    }

    if std::str::from_utf8(body).is_ok() {
        return Some("utf-8".to_string());
    }

    None
}

fn decode_body(body: &[u8], encoding: Option<&str>) -> String {
    if let Some(encoding) = encoding {
        if let Some(decoder) = Encoding::for_label(encoding.as_bytes()) {
            let (text, _, _) = decoder.decode(body);
            return text.into_owned();
        }
    }

    String::from_utf8_lossy(body).into_owned()
}

pub fn snapshot_root(html: &str) -> OpenPageResult<SessionElement> {
    snapshot_root_arc(Arc::new(html.to_string()))
}

pub fn snapshot_find(html: &str, locator: &str) -> OpenPageResult<SessionElement> {
    snapshot_find_arc(Arc::new(html.to_string()), locator)
}

pub fn snapshot_find_all(html: &str, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
    snapshot_find_all_arc(Arc::new(html.to_string()), locator)
}

fn snapshot_root_arc(html: Arc<String>) -> OpenPageResult<SessionElement> {
    let parsed = Html::parse_document(html.as_ref());
    Ok(session_element_from_ref(&html, parsed.root_element()))
}

fn snapshot_find_arc(html: Arc<String>, locator: &str) -> OpenPageResult<SessionElement> {
    let parsed = Html::parse_document(html.as_ref());
    let selector = parse_selector(locator)?;
    parsed
        .select(&selector)
        .next()
        .map(|element| session_element_from_ref(&html, element))
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.to_string()))
}

fn snapshot_find_all_arc(html: Arc<String>, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
    let parsed = Html::parse_document(html.as_ref());
    let selector = parse_selector(locator)?;
    Ok(parsed
        .select(&selector)
        .map(|element| session_element_from_ref(&html, element))
        .collect())
}

fn find_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
) -> OpenPageResult<SessionElement> {
    let parsed = Html::parse_document(html.as_ref());
    let scope = element_from_document(&parsed, scope_id)?;
    let selector = parse_selector(locator)?;
    scope
        .select(&selector)
        .next()
        .map(|element| session_element_from_ref(&html, element))
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.to_string()))
}

fn find_all_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
) -> OpenPageResult<Vec<SessionElement>> {
    let parsed = Html::parse_document(html.as_ref());
    let scope = element_from_document(&parsed, scope_id)?;
    let selector = parse_selector(locator)?;
    Ok(scope
        .select(&selector)
        .map(|element| session_element_from_ref(&html, element))
        .collect())
}

fn parse_selector(locator: &str) -> OpenPageResult<Selector> {
    let selector = selector_from_locator(locator)?;
    Selector::parse(&selector).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))
}

fn session_element_from_ref(html: &Arc<String>, element: ElementRef<'_>) -> SessionElement {
    SessionElement {
        html: Arc::clone(html),
        node_id: element.id(),
    }
}

fn element_from_document<'a>(
    document: &'a Html,
    node_id: NodeId,
) -> OpenPageResult<ElementRef<'a>> {
    document
        .tree
        .get(node_id)
        .and_then(ElementRef::wrap)
        .ok_or_else(|| OpenPageError::ElementNotFound("snapshot node no longer exists".to_string()))
}

#[derive(Clone, Copy)]
enum RelativeDirection {
    Before,
    After,
}

fn collect_matching_elements<'a, I>(
    html: &Arc<String>,
    elements: I,
    locator: Option<&str>,
) -> OpenPageResult<Vec<SessionElement>>
where
    I: IntoIterator<Item = ElementRef<'a>>,
{
    let selector = parse_optional_selector(locator)?;
    Ok(elements
        .into_iter()
        .filter(|element| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(element))
        })
        .map(|element| session_element_from_ref(html, element))
        .collect())
}

fn document_relatives(
    html: &Arc<String>,
    element: ElementRef<'_>,
    direction: RelativeDirection,
    locator: Option<&str>,
) -> OpenPageResult<Vec<SessionElement>> {
    let selector = parse_optional_selector(locator)?;
    let root = element.tree().root();
    let elements: Vec<_> = root.descendants().filter_map(ElementRef::wrap).collect();
    let current_id = element.id();
    let current_index = elements
        .iter()
        .position(|candidate| candidate.id() == current_id)
        .ok_or_else(|| {
            OpenPageError::ElementNotFound("snapshot node no longer exists".to_string())
        })?;

    let ancestor_ids: HashSet<_> = element.ancestors().map(|node| node.id()).collect();
    let descendant_ids: HashSet<_> = element
        .descendants()
        .skip(1)
        .map(|node| node.id())
        .collect();

    let iter: Box<dyn Iterator<Item = ElementRef<'_>> + '_> = match direction {
        RelativeDirection::Before => Box::new(
            elements[..current_index]
                .iter()
                .copied()
                .filter(|candidate| !ancestor_ids.contains(&candidate.id())),
        ),
        RelativeDirection::After => Box::new(
            elements[current_index + 1..]
                .iter()
                .copied()
                .filter(|candidate| !descendant_ids.contains(&candidate.id())),
        ),
    };

    Ok(iter
        .filter(|candidate| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(candidate))
        })
        .map(|candidate| session_element_from_ref(html, candidate))
        .collect())
}

fn nth_from_start(
    elements: Vec<SessionElement>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<SessionElement> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    elements
        .into_iter()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn nth_from_end(
    elements: Vec<SessionElement>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<SessionElement> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
    }
    let len = elements.len();
    if index > len {
        return Err(OpenPageError::ElementNotFound(error_message.to_string()));
    }
    elements
        .into_iter()
        .nth(len - index)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn parse_optional_selector(locator: Option<&str>) -> OpenPageResult<Option<Selector>> {
    locator
        .map(str::trim)
        .filter(|locator| !locator.is_empty())
        .map(parse_selector)
        .transpose()
}

fn nearest_parent_element(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut current = element.parent();
    while let Some(node) = current {
        if let Some(parent) = ElementRef::wrap(node) {
            return Some(parent);
        }
        current = node.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{snapshot_find, snapshot_find_all, snapshot_root};

    const HTML: &str = r#"
<!doctype html>
<html>
  <body>
    <section id="root">
      <h1>OpenPage</h1>
      <input id="name" value="openpage" />
      <button id="submit">Go</button>
      <div id="out"></div>
      <ul class="items">
        <li class="item" data-kind="a">alpha</li>
        <li class="item" data-kind="b">beta</li>
      </ul>
    </section>
  </body>
</html>
"#;

    #[test]
    fn snapshot_find_supports_nested_queries() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let heading = root.find("h1").expect("nested heading should exist");
        let first_item = root.find(".item").expect("nested item should exist");

        assert_eq!(
            heading.text().expect("heading text"),
            Some("OpenPage".to_string())
        );
        assert_eq!(heading.tag().expect("heading tag"), "h1".to_string());
        assert_eq!(
            first_item.attr("data-kind").expect("item attr"),
            Some("a".to_string())
        );
        let attrs = first_item.attrs().expect("item attrs");
        assert!(attrs.contains(&("class".to_string(), "item".to_string())));
        assert!(attrs.contains(&("data-kind".to_string(), "a".to_string())));
    }

    #[test]
    fn snapshot_find_all_keeps_match_order() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let items = root.find_all(".item").expect("items should exist");

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].text().expect("first item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            items[1].text().expect("second item text"),
            Some("beta".to_string())
        );

        let top_level = snapshot_find_all(HTML, ".item").expect("top-level items should exist");
        assert_eq!(top_level.len(), 2);
    }

    #[test]
    fn snapshot_traversal_supports_root_parent_siblings_and_children() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items list should exist");

        assert_eq!(root.tag().expect("root tag"), "html".to_string());
        assert_eq!(
            submit
                .parent()
                .expect("submit parent")
                .tag()
                .expect("parent tag"),
            "section"
        );
        assert_eq!(
            submit
                .next()
                .expect("next sibling")
                .attr("id")
                .expect("next id"),
            Some("out".to_string())
        );
        assert_eq!(
            submit
                .prev()
                .expect("prev sibling")
                .attr("id")
                .expect("prev id"),
            Some("name".to_string())
        );

        let children = items.children().expect("direct children");
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0].text().expect("first child text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            children[1].text().expect("second child text"),
            Some("beta".to_string())
        );
        assert_eq!(
            submit.raw_text().expect("submit raw text"),
            Some("Go".to_string())
        );
    }

    #[test]
    fn snapshot_relative_lists_cover_before_after_and_filtered_siblings() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let prevs = submit.prevs().expect("previous siblings");
        assert_eq!(prevs.len(), 2);
        assert_eq!(prevs[0].tag().expect("first prev tag"), "h1".to_string());
        assert_eq!(
            prevs[1].attr("id").expect("second prev id"),
            Some("name".to_string())
        );

        let nexts = submit.nexts().expect("next siblings");
        assert_eq!(nexts.len(), 2);
        assert_eq!(
            nexts[0].attr("id").expect("first next id"),
            Some("out".to_string())
        );
        assert_eq!(nexts[1].tag().expect("second next tag"), "ul".to_string());

        let afters = submit.afters_with(Some(".item")).expect("following items");
        assert_eq!(afters.len(), 2);
        assert_eq!(
            afters[0].text().expect("first after text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            submit
                .after_with(Some(".item"), 2)
                .expect("second matching after")
                .text()
                .expect("second matching after text"),
            Some("beta".to_string())
        );

        let befores = second_item
            .befores_with(Some(".item"))
            .expect("preceding items");
        assert_eq!(befores.len(), 1);
        assert_eq!(
            befores[0].text().expect("preceding item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            second_item
                .before()
                .expect("nearest preceding element")
                .text()
                .expect("nearest preceding text"),
            Some("alpha".to_string())
        );
    }
}
