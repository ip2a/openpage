use std::sync::{Arc, Mutex};
use std::time::Duration;

use ego_tree::NodeId;
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::USER_AGENT;
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
    body: Option<Arc<String>>,
    json: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct SessionPage {
    inner: Arc<Mutex<SessionState>>,
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
                body: None,
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

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        Ok(self.lock_state()?.json.clone())
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        let body = self.body_arc()?;
        Ok(self.first_text(&body, "title")?)
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
        for cookie in cookie_header.split(';').map(str::trim).filter(|item| !item.is_empty()) {
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
        let selector_obj =
            Selector::parse(selector).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
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

    fn store_response(&self, requested_url: &str, response: reqwest::blocking::Response) -> OpenPageResult<bool> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let text = response
            .text()
            .map_err(|err| OpenPageError::Http(format!("{err:?}")))?;
        let parsed_json = serde_json::from_str::<Value>(&text).ok();

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.body = Some(Arc::new(text));
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

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(element.attr(name).map(ToString::to_string)))
    }

    pub fn parent(&self) -> OpenPageResult<SessionElement> {
        self.with_element(|element| {
            nearest_parent_element(element)
                .map(|parent| session_element_from_ref(&self.html, parent))
                .ok_or_else(|| OpenPageError::ElementNotFound("parent element not found".to_string()))
        })
    }

    pub fn children(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.with_element(|element| {
            Ok(element
                .child_elements()
                .map(|child| session_element_from_ref(&self.html, child))
                .collect())
        })
    }

    pub fn prev(&self) -> OpenPageResult<SessionElement> {
        self.with_element(|element| {
            previous_element(element)
                .map(|prev| session_element_from_ref(&self.html, prev))
                .ok_or_else(|| OpenPageError::ElementNotFound("previous element not found".to_string()))
        })
    }

    pub fn next(&self) -> OpenPageResult<SessionElement> {
        self.with_element(|element| {
            next_element(element)
                .map(|next| session_element_from_ref(&self.html, next))
                .ok_or_else(|| OpenPageError::ElementNotFound("next element not found".to_string()))
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

fn find_in_scope(html: Arc<String>, scope_id: NodeId, locator: &str) -> OpenPageResult<SessionElement> {
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

fn element_from_document<'a>(document: &'a Html, node_id: NodeId) -> OpenPageResult<ElementRef<'a>> {
    document
        .tree
        .get(node_id)
        .and_then(ElementRef::wrap)
        .ok_or_else(|| OpenPageError::ElementNotFound("snapshot node no longer exists".to_string()))
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

fn previous_element(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut current = element.prev_sibling();
    while let Some(node) = current {
        if let Some(prev) = ElementRef::wrap(node) {
            return Some(prev);
        }
        current = node.prev_sibling();
    }
    None
}

fn next_element(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut current = element.next_sibling();
    while let Some(node) = current {
        if let Some(next) = ElementRef::wrap(node) {
            return Some(next);
        }
        current = node.next_sibling();
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

        assert_eq!(heading.text().expect("heading text"), Some("OpenPage".to_string()));
        assert_eq!(heading.tag().expect("heading tag"), "h1".to_string());
        assert_eq!(
            first_item.attr("data-kind").expect("item attr"),
            Some("a".to_string())
        );
    }

    #[test]
    fn snapshot_find_all_keeps_match_order() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let items = root.find_all(".item").expect("items should exist");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text().expect("first item text"), Some("alpha".to_string()));
        assert_eq!(items[1].text().expect("second item text"), Some("beta".to_string()));

        let top_level = snapshot_find_all(HTML, ".item").expect("top-level items should exist");
        assert_eq!(top_level.len(), 2);
    }

    #[test]
    fn snapshot_traversal_supports_root_parent_siblings_and_children() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items list should exist");

        assert_eq!(root.tag().expect("root tag"), "html".to_string());
        assert_eq!(submit.parent().expect("submit parent").tag().expect("parent tag"), "section");
        assert_eq!(submit.next().expect("next sibling").attr("id").expect("next id"), Some("out".to_string()));
        assert_eq!(submit.prev().expect("prev sibling").attr("id").expect("prev id"), Some("name".to_string()));

        let children = items.children().expect("direct children");
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].text().expect("first child text"), Some("alpha".to_string()));
        assert_eq!(children[1].text().expect("second child text"), Some("beta".to_string()));
    }
}
