use super::*;

pub(super) fn rebuild_session_client(state: &mut SessionState) -> OpenPageResult<()> {
    let client_options = SessionClientOptions::from(&*state);
    state.client = build_session_client(&client_options, Arc::clone(&state.cookie_jar))?;
    Ok(())
}

pub(super) fn session_client_for_url(state: &SessionState, _requested_url: &str) -> Client {
    state.client.clone()
}

pub(super) fn build_session_client(
    options: &SessionClientOptions,
    cookie_jar: Arc<SessionCookieJar>,
) -> OpenPageResult<Client> {
    let mut builder = ClientBuilder::new()
        .cookie_provider(cookie_jar)
        .timeout(Duration::from_secs(options.timeout_secs));

    if !options.trust_env {
        builder = builder.no_proxy();
    }
    if let Some(http_proxy) = options.http_proxy.as_deref() {
        builder = builder.proxy(Proxy::http(http_proxy).map_err(|err| {
            OpenPageError::Http(invalid_session_proxy_message(
                "http",
                http_proxy,
                &format!("{err:?}"),
            ))
        })?);
    }
    if let Some(https_proxy) = options.https_proxy.as_deref() {
        builder = builder.proxy(Proxy::https(https_proxy).map_err(|err| {
            OpenPageError::Http(invalid_session_proxy_message(
                "https",
                https_proxy,
                &format!("{err:?}"),
            ))
        })?);
    }
    if !options.verify {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if let Some(cert) = options.cert.as_ref() {
        builder = builder.identity(load_session_identity(cert)?);
    }
    if let Some(max_redirects) = options.max_redirects {
        builder = builder.redirect(if max_redirects == 0 {
            Policy::none()
        } else {
            Policy::limited(max_redirects)
        });
    }

    builder.build().map_err(|err| {
        OpenPageError::Http(session_client_build_failed_message(&format!("{err:?}")))
    })
}

pub(super) fn load_session_identity(cert: &SessionCert) -> OpenPageResult<Identity> {
    let pem = match cert {
        SessionCert::Pem(path) => std::fs::read(path).map_err(|err| {
            OpenPageError::Io(session_cert_read_failed_message(
                "cert",
                &path.display().to_string(),
                &err.to_string(),
            ))
        })?,
        SessionCert::PemPair { cert, key } => {
            let mut pem = std::fs::read(cert).map_err(|err| {
                OpenPageError::Io(session_cert_read_failed_message(
                    "cert",
                    &cert.display().to_string(),
                    &err.to_string(),
                ))
            })?;
            if !pem.ends_with(b"\n") {
                pem.push(b'\n');
            }
            pem.extend(std::fs::read(key).map_err(|err| {
                OpenPageError::Io(session_cert_read_failed_message(
                    "key",
                    &key.display().to_string(),
                    &err.to_string(),
                ))
            })?);
            pem
        }
    };

    Identity::from_pem(&pem).map_err(|err| {
        OpenPageError::Http(session_identity_parse_failed_message(&format!("{err:?}")))
    })
}

pub(super) fn apply_request_options(
    mut request: RequestBuilder,
    user_agent: Option<&str>,
    headers: &[(String, String)],
    auth: Option<&(String, String)>,
    timeout_secs: Option<u64>,
) -> RequestBuilder {
    if let Some(user_agent) = user_agent {
        request = request.header(USER_AGENT, user_agent);
    }
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some((username, password)) = auth {
        request = request.basic_auth(username, Some(password));
    }
    if let Some(timeout_secs) = timeout_secs {
        request = request.timeout(Duration::from_secs(timeout_secs));
    }
    request
}

pub(super) fn merge_request_headers(
    base: &HashMap<String, String>,
    overrides: &[(String, String)],
) -> Vec<(String, String)> {
    let mut merged: Vec<_> = base
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();

    for (name, value) in overrides {
        if let Some(existing) = merged
            .iter_mut()
            .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(name))
        {
            *existing = (name.clone(), value.clone());
        } else {
            merged.push((name.clone(), value.clone()));
        }
    }

    merged
}

pub(super) fn run_response_hooks(hooks: &SessionHooks, event: SessionHookEvent) {
    if hooks.is_empty() {
        return;
    }
    for hook in hooks.response_hooks() {
        hook(event.clone());
    }
}

pub(super) fn effective_request_headers(
    headers: &[(String, String)],
    current_url: Option<&str>,
    request_url: &str,
) -> OpenPageResult<Vec<(String, String)>> {
    let mut resolved = Vec::with_capacity(headers.len() + 1);
    let mut has_referer = false;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("referer") {
            has_referer = true;
            if !value.is_empty() {
                resolved.push((name.clone(), value.clone()));
            }
            continue;
        }
        resolved.push((name.clone(), value.clone()));
    }

    if !has_referer {
        if let Some(value) = default_referer_header(current_url, request_url)? {
            resolved.push(("Referer".to_string(), value));
        }
    }

    Ok(resolved)
}

pub(super) fn default_referer_header(
    current_url: Option<&str>,
    request_url: &str,
) -> OpenPageResult<Option<String>> {
    if let Some(current_url) = current_url.filter(|url| !url.is_empty()) {
        return Ok(Some(current_url.to_string()));
    }

    let parsed = Url::parse(request_url).map_err(|err| {
        OpenPageError::Http(invalid_url_message(request_url, Some(&err.to_string())))
    })?;
    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    let mut authority = host.to_string();
    if let Some(port) = parsed.port() {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    Ok(Some(format!("{}://{}", parsed.scheme(), authority)))
}

pub(super) fn upsert_header_pair(headers: &mut Vec<(String, String)>, name: String, value: String) {
    if let Some(existing) = headers
        .iter_mut()
        .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(&name))
    {
        *existing = (name, value);
    } else {
        headers.push((name, value));
    }
}

pub(super) fn remove_header_pairs(headers: &mut Vec<(String, String)>, name: &str) {
    headers.retain(|(existing_name, _)| !existing_name.eq_ignore_ascii_case(name));
}

pub(super) fn upsert_header_map(
    headers: &mut HashMap<String, String>,
    name: String,
    value: String,
) {
    headers.retain(|existing_name, _| !existing_name.eq_ignore_ascii_case(&name));
    headers.insert(name, value);
}

pub(super) fn append_query_params(
    target: &str,
    params: &[(String, String)],
) -> OpenPageResult<String> {
    if params.is_empty() {
        return Ok(target.to_string());
    }

    let mut url = Url::parse(target)
        .map_err(|err| OpenPageError::Http(invalid_url_message(target, Some(&err.to_string()))))?;
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in params {
            query.append_pair(name, value);
        }
    }
    Ok(url.into())
}

pub(super) fn resolve_local_file_path(target: &str) -> OpenPageResult<Option<std::path::PathBuf>> {
    if target.starts_with("file://") {
        let url = Url::parse(target).map_err(|err| {
            OpenPageError::Io(invalid_file_url_message(target, Some(&err.to_string())))
        })?;
        match url.host_str() {
            None | Some("localhost") => {}
            Some(_) => return Err(OpenPageError::Io(invalid_file_url_message(target, None))),
        }
        return url
            .to_file_path()
            .map(Some)
            .map_err(|_| OpenPageError::Io(invalid_file_url_message(target, None)));
    }

    let path = Path::new(target);
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

pub(super) fn cookie_assignment(
    name: &str,
    value: &str,
    domain: Option<&str>,
    path: Option<&str>,
) -> String {
    let mut cookie = format!("{}={}", name.trim(), value.trim());
    if let Some(domain) = domain.filter(|item| !item.trim().is_empty()) {
        cookie.push_str("; Domain=");
        cookie.push_str(domain.trim());
    }
    if let Some(path) = path.filter(|item| !item.trim().is_empty()) {
        cookie.push_str("; Path=");
        cookie.push_str(path.trim());
    }
    cookie
}

pub(super) fn remove_cookie_from_header(cookie_header: &str, name: &str) -> String {
    cookie_header
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter(|item| {
            item.split_once('=')
                .map(|(cookie_name, _)| !cookie_name.trim().eq(name.trim()))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub(super) fn detect_body_encoding(content_type: Option<&str>, body: &[u8]) -> Option<String> {
    if let Some(encoding) = declared_content_type_encoding(content_type) {
        return Some(encoding);
    }

    if std::str::from_utf8(body).is_ok() {
        return Some("utf-8".to_string());
    }

    None
}

pub(super) fn declared_content_type_encoding(content_type: Option<&str>) -> Option<String> {
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
    None
}

pub(super) fn decode_body(body: &[u8], encoding: Option<&str>) -> String {
    if let Some(encoding) = encoding {
        if let Some(decoder) = Encoding::for_label(encoding.as_bytes()) {
            let (text, _, _) = decoder.decode(body);
            return text.into_owned();
        }
    }

    String::from_utf8_lossy(body).into_owned()
}

pub(super) fn resolve_effective_encoding(
    content_type: Option<&str>,
    body: &[u8],
    forced_encoding: Option<&str>,
) -> Option<String> {
    forced_encoding
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| detect_body_encoding(content_type, body))
}

pub(super) fn refresh_state_body_encoding(state: &mut SessionState) {
    let Some(raw_data) = state.raw_data.as_ref() else {
        state.encoding = state
            .forced_encoding
            .as_ref()
            .map(|value| value.to_ascii_lowercase())
            .or_else(|| declared_content_type_encoding(state.response_content_type.as_deref()));
        state.body = None;
        state.json = None;
        return;
    };

    let encoding = resolve_effective_encoding(
        state.response_content_type.as_deref(),
        raw_data.as_ref(),
        state.forced_encoding.as_deref(),
    );
    let text = decode_body(raw_data.as_ref(), encoding.as_deref());
    let parsed_json = serde_json::from_str::<Value>(&text).ok();

    state.encoding = encoding;
    state.body = Some(Arc::new(text));
    state.json = parsed_json;
}

pub(super) fn ensure_response_body_loaded(state: &mut SessionState) -> OpenPageResult<()> {
    if state.raw_data.is_some() || state.pending_response.is_none() {
        return Ok(());
    }

    let pending = state
        .pending_response
        .take()
        .expect("pending response checked");
    let raw_data = Arc::new(
        pending
            .response
            .bytes()
            .map_err(|err| {
                OpenPageError::Http(session_response_body_read_failed_message(
                    &pending.requested_url,
                    &format!("{err:?}"),
                ))
            })?
            .to_vec(),
    );
    state.raw_data = Some(raw_data);
    refresh_state_body_encoding(state);
    Ok(())
}
