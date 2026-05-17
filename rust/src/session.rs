use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::cookie::{CookieStore, Jar};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::USER_AGENT;
use scraper::{Html, Selector};
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
    selector: String,
    index: usize,
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
        let selector = selector_from_locator(locator)?;
        let body = self.body_arc()?;
        let html = Html::parse_document(&body);
        let selector_obj =
            Selector::parse(&selector).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
        if html.select(&selector_obj).next().is_none() {
            return Err(OpenPageError::ElementNotFound(locator.to_string()));
        }
        Ok(SessionElement {
            html: body,
            selector,
            index: 0,
        })
    }

    pub fn find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        let selector = selector_from_locator(locator)?;
        let body = self.body_arc()?;
        let html = Html::parse_document(&body);
        let selector_obj =
            Selector::parse(&selector).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
        let count = html.select(&selector_obj).count();
        Ok((0..count)
            .map(|index| SessionElement {
                html: Arc::clone(&body),
                selector: selector.clone(),
                index,
            })
            .collect())
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
    pub fn text(&self) -> OpenPageResult<Option<String>> {
        let html = Html::parse_document(&self.html);
        let selector = self.selector()?;
        Ok(html
            .select(&selector)
            .nth(self.index)
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|text| !text.is_empty()))
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        let html = Html::parse_document(&self.html);
        let selector = self.selector()?;
        Ok(html.select(&selector).nth(self.index).map(|node| node.html()))
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        let html = Html::parse_document(&self.html);
        let selector = self.selector()?;
        Ok(html
            .select(&selector)
            .nth(self.index)
            .and_then(|node| node.value().attr(name).map(ToString::to_string)))
    }

    fn selector(&self) -> OpenPageResult<Selector> {
        Selector::parse(&self.selector).map_err(|err| OpenPageError::ElementNotFound(err.to_string()))
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
