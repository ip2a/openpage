use super::*;

impl SessionCookieJar {
    pub(super) fn add_cookie_str(&self, cookie: &str, url: &Url) {
        let cookies = RawCookie::parse(cookie)
            .ok()
            .map(|cookie| cookie.into_owned())
            .into_iter();
        self.inner
            .write()
            .expect("session cookie jar lock poisoned")
            .store_response_cookies(cookies, url);
    }

    pub(super) fn cookie_header(&self, url: &Url) -> Option<HeaderValue> {
        let value = self
            .inner
            .read()
            .expect("session cookie jar lock poisoned")
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if value.is_empty() {
            return None;
        }
        HeaderValue::from_bytes(value.as_bytes()).ok()
    }

    pub(super) fn matching_cookies(&self, url: &Url) -> Vec<SessionCookie> {
        self.inner
            .read()
            .expect("session cookie jar lock poisoned")
            .matches(url)
            .into_iter()
            .map(SessionCookie::from_store_cookie)
            .collect()
    }

    pub(super) fn all_cookies(&self) -> Vec<SessionCookie> {
        self.inner
            .read()
            .expect("session cookie jar lock poisoned")
            .iter_unexpired()
            .map(SessionCookie::from_store_cookie)
            .collect()
    }
}

impl ReqwestCookieStore for SessionCookieJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let cookies = cookie_headers.filter_map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|text| RawCookie::parse(text).ok())
                .map(|cookie| cookie.into_owned())
        });
        self.inner
            .write()
            .expect("session cookie jar lock poisoned")
            .store_response_cookies(cookies, url);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.cookie_header(url)
    }
}

impl SessionCookie {
    pub(super) fn from_store_cookie(cookie: &StoredCookie<'_>) -> Self {
        Self {
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            domain: cookie.domain.as_cow().map(|value| value.into_owned()),
            path: Some(String::from(&cookie.path)),
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            same_site: cookie.same_site().map(|value| value.to_string()),
            host_only: matches!(cookie.domain, cookie_store::CookieDomain::HostOnly(_)),
            persistent: cookie.is_persistent(),
        }
    }
}

impl From<SessionCookie> for CookieEntry {
    fn from(cookie: SessionCookie) -> Self {
        Self {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
        }
    }
}

pub(super) fn initialize_session_cookies(
    cookie_jar: &SessionCookieJar,
    cookies: &[SessionCookieParam],
) -> OpenPageResult<()> {
    for cookie in cookies {
        let scope_url = cookie_scope_url_from_param(cookie)?;
        let cookie_str = cookie_param_to_set_cookie(cookie);
        cookie_jar.add_cookie_str(&cookie_str, &scope_url);
    }
    Ok(())
}

pub(super) fn normalize_session_download_path(path: &Path) -> OpenPageResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|current_dir| current_dir.join(path))
            .map_err(|err| {
                OpenPageError::Io(session_download_path_resolve_failed_message(
                    &path.display().to_string(),
                    &err.to_string(),
                ))
            })?
    };
    Ok(normalize_path_components(&absolute))
}

pub(super) fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

pub(super) fn suggested_session_download_filename(
    content_disposition: Option<&str>,
    request_url: &str,
    final_url: &str,
) -> String {
    content_disposition
        .and_then(content_disposition_filename)
        .or_else(|| filename_from_url(final_url))
        .or_else(|| filename_from_url(request_url))
        .unwrap_or_else(|| "download".to_string())
}

pub(super) fn content_disposition_filename(header: &str) -> Option<String> {
    for part in header.split(';').skip(1) {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("filename*=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            let value = value
                .split_once("''")
                .map(|(_, encoded)| encoded)
                .unwrap_or(value);
            if let Some(filename) = sanitize_download_filename(value) {
                return Some(filename);
            }
        } else if let Some(value) = part.strip_prefix("filename=") {
            if let Some(filename) =
                sanitize_download_filename(value.trim().trim_matches('"').trim_matches('\''))
            {
                return Some(filename);
            }
        }
    }
    None
}

pub(super) fn filename_from_url(target: &str) -> Option<String> {
    Url::parse(target).ok().and_then(|url| {
        url.path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
            .and_then(sanitize_download_filename)
    })
}

pub(super) fn sanitize_download_filename(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let value = value.replace(['/', '\\'], "_");
    Some(value)
}

pub(super) fn cookie_scope_url_from_param(cookie: &SessionCookieParam) -> OpenPageResult<Url> {
    if let Some(url) = cookie.url.as_deref() {
        return Url::parse(url)
            .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))));
    }

    let domain = cookie
        .domain
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenPageError::Http(session_cookie_requires_url_or_domain_message(&cookie.name))
        })?;
    let host = domain.trim_start_matches('.');
    let scheme = if cookie.secure { "https" } else { "http" };
    let path = cookie
        .path
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("/");
    let candidate = format!("{scheme}://{host}{path}");
    Url::parse(&candidate)
        .map_err(|err| OpenPageError::Http(invalid_url_message(&candidate, Some(&err.to_string()))))
}

pub(super) fn cookie_param_to_set_cookie(cookie: &SessionCookieParam) -> String {
    let mut value = cookie_assignment(
        &cookie.name,
        &cookie.value,
        cookie.domain.as_deref(),
        cookie.path.as_deref(),
    );
    if cookie.secure {
        value.push_str("; Secure");
    }
    if cookie.http_only {
        value.push_str("; HttpOnly");
    }
    if let Some(same_site) = cookie
        .same_site
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        value.push_str("; SameSite=");
        value.push_str(same_site);
    }
    value
}

pub fn cookies_from_header(url: &str, cookie_header: &str) -> OpenPageResult<Vec<CookieEntry>> {
    let parsed = Url::parse(url)
        .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))))?;
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
