use super::*;

impl Session {
    pub fn new(options: SessionOptions) -> OpenPageResult<Self> {
        let cookie_jar = Arc::new(SessionCookieJar::default());
        initialize_session_cookies(&cookie_jar, &options.cookies)?;
        let client_options = SessionClientOptions::from(&options);
        let client = build_session_client(&client_options, Arc::clone(&cookie_jar))?;
        let download_path = normalize_session_download_path(&options.download_path)?;

        let mut headers = HashMap::new();
        for (name, value) in options.headers {
            upsert_header_map(&mut headers, name, value);
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(SessionState {
                client,
                cookie_jar,
                timeout_secs: options.timeout_secs,
                user_agent: options.user_agent,
                download_path,
                last_download: None,
                headers,
                retry_times: options.retry_times,
                retry_interval_millis: options.retry_interval_millis,
                http_proxy: options.http_proxy,
                https_proxy: options.https_proxy,
                params: options.params,
                verify: options.verify,
                auth: options.auth,
                hooks: options.hooks,
                stream: options.stream,
                cert: options.cert,
                trust_env: options.trust_env,
                max_redirects: options.max_redirects,
                url: None,
                status_code: None,
                response_headers: Vec::new(),
                response_content_type: None,
                forced_encoding: None,
                encoding: None,
                body: None,
                raw_data: None,
                json: None,
                pending_response: None,
            })),
            none_element_config: Arc::new(Mutex::new(default_none_element_config())),
        })
    }

    pub fn settings(&self) -> SessionSettings<'_> {
        SessionSettings { session: self }
    }

    pub fn get(&self, url: &str) -> OpenPageResult<Response> {
        self.get_with_options(url, &SessionRequestOptions::default())
    }

    pub fn get_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        if let Some(path) = resolve_local_file_path(url)? {
            return self.load_local_file(&path);
        }
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.get(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "GET",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn head(&self, url: &str) -> OpenPageResult<Response> {
        self.head_with_options(url, &SessionRequestOptions::default())
    }

    pub fn head_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.head(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "HEAD",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn options(&self, url: &str) -> OpenPageResult<Response> {
        self.options_with_options(url, &SessionRequestOptions::default())
    }

    pub fn options_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context
                    .client
                    .request(reqwest::Method::OPTIONS, &request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "OPTIONS",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post(&self, url: &str) -> OpenPageResult<Response> {
        self.post_with_options(url, &SessionRequestOptions::default())
    }

    pub fn post_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "POST",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post_form<K, V>(&self, url: &str, form: &[(K, V)]) -> OpenPageResult<Response>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.post_form_with_options(url, form, &SessionRequestOptions::default())
    }

    pub fn post_form_with_options<K, V>(
        &self,
        url: &str,
        form: &[(K, V)],
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (name, value) in form {
                serializer.append_pair(name.as_ref(), value.as_ref());
            }
            let body = serializer.finish();
            apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "POST",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post_body(&self, url: &str, body: impl Into<String>) -> OpenPageResult<Response> {
        self.post_body_with_options(url, body, &SessionRequestOptions::default())
    }

    pub fn post_body_with_options(
        &self,
        url: &str,
        body: impl Into<String>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        let body = body.into();
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .body(body.clone())
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "POST",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn put(&self, url: &str) -> OpenPageResult<Response> {
        self.put_with_options(url, &SessionRequestOptions::default())
    }

    pub fn put_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.put(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "PUT",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn delete(&self, url: &str) -> OpenPageResult<Response> {
        self.delete_with_options(url, &SessionRequestOptions::default())
    }

    pub fn delete_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.delete(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "DELETE",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn patch(&self, url: &str) -> OpenPageResult<Response> {
        self.patch_with_options(url, &SessionRequestOptions::default())
    }

    pub fn patch_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.patch(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "PATCH",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<Response> {
        self.post_json_with_options(url, payload, &SessionRequestOptions::default())
    }

    pub fn post_json_with_options(
        &self,
        url: &str,
        payload: Option<Value>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            let request = apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            );
            match &payload {
                Some(payload) => request.json(payload).send().map_err(|err| {
                    OpenPageError::Http(session_request_failed_message(
                        "POST",
                        &request_url,
                        &format!("{err:?}"),
                    ))
                }),
                None => request.send().map_err(|err| {
                    OpenPageError::Http(session_request_failed_message(
                        "POST",
                        &request_url,
                        &format!("{err:?}"),
                    ))
                }),
            }
        })
    }

    pub fn post_json_body(&self, url: &str, body: impl Into<String>) -> OpenPageResult<Response> {
        self.post_json_body_with_options(url, body, &SessionRequestOptions::default())
    }

    pub fn post_json_body_with_options(
        &self,
        url: &str,
        body: impl Into<String>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<Response> {
        let body = body.into();
        self.send_request_with_retry(url, Some(options), |context| {
            let request_url = append_query_params(url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            apply_request_options(
                context.client.post(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .header(CONTENT_TYPE, "application/json")
            .body(body.clone())
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "POST",
                    &request_url,
                    &format!("{err:?}"),
                ))
            })
        })
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.url.clone())
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        Ok(self.lock_state()?.status_code)
    }

    pub fn session(&self) -> OpenPageResult<SessionInfo> {
        let state = self.lock_state()?;
        let cookie_jar = state.cookie_jar.clone();
        let mut headers = state
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        headers.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let snapshot = SessionInfo {
            timeout_secs: state.timeout_secs,
            user_agent: state.user_agent.clone(),
            headers,
            cookies: Vec::new(),
            download_path: state.download_path.display().to_string(),
            retry_times: state.retry_times,
            retry_interval_millis: state.retry_interval_millis,
            http_proxy: state.http_proxy.clone(),
            https_proxy: state.https_proxy.clone(),
            params: state.params.clone(),
            verify: state.verify,
            auth: state.auth.clone(),
            stream: state.stream,
            cert: state.cert.clone(),
            trust_env: state.trust_env,
            max_redirects: state.max_redirects,
            current_url: state.url.clone(),
        };
        drop(state);

        let mut cookies = cookie_jar.all_cookies();
        cookies.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.domain.cmp(&right.domain))
                .then(left.path.cmp(&right.path))
        });

        Ok(SessionInfo {
            cookies,
            ..snapshot
        })
    }

    pub fn session_snapshot(&self) -> OpenPageResult<SessionInfo> {
        self.session()
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn url_available(&self) -> OpenPageResult<bool> {
        Ok(self
            .lock_state()?
            .status_code
            .map(|status| (200..400).contains(&status))
            .unwrap_or(false))
    }

    pub fn html(&self) -> OpenPageResult<String> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state
            .body
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state
            .raw_data
            .as_ref()
            .map(|body| body.as_ref().clone())
            .unwrap_or_default())
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.encoding.clone())
    }

    pub fn forced_encoding(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.forced_encoding.clone())
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        Ok(state.json.clone())
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        let body = self.body_arc()?;
        Ok(self.first_text(&body, "title")?)
    }

    pub fn user_agent(&self) -> OpenPageResult<Option<String>> {
        Ok(self.lock_state()?.user_agent.clone())
    }

    pub fn download_path(&self) -> OpenPageResult<String> {
        Ok(self.lock_state()?.download_path.display().to_string())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<SessionDownload>> {
        Ok(self.lock_state()?.last_download.clone())
    }

    pub fn timeout_secs(&self) -> OpenPageResult<u64> {
        Ok(self.lock_state()?.timeout_secs)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        Ok(self.lock_state()?.retry_times)
    }

    pub fn retry_interval_millis(&self) -> OpenPageResult<u64> {
        Ok(self.lock_state()?.retry_interval_millis)
    }

    pub fn retry_interval(&self) -> OpenPageResult<f64> {
        Ok(self.retry_interval_millis()? as f64 / 1000.0)
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(self.lock_state()?.status_code.is_some())
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
        Ok(self
            .cookies_detailed(false)?
            .into_iter()
            .map(CookieEntry::from)
            .collect())
    }

    pub fn cookies_all_domains(&self) -> OpenPageResult<Vec<CookieEntry>> {
        Ok(self
            .cookies_detailed(true)?
            .into_iter()
            .map(CookieEntry::from)
            .collect())
    }

    pub fn cookies_detailed(&self, all_domains: bool) -> OpenPageResult<Vec<SessionCookie>> {
        let cookie_jar = self.lock_state()?.cookie_jar.clone();
        if all_domains {
            return Ok(cookie_jar.all_cookies());
        }

        let Some(url) = self.url()? else {
            return Ok(Vec::new());
        };
        let url = Url::parse(&url).map_err(|err| {
            OpenPageError::Http(invalid_url_message(&url, Some(&err.to_string())))
        })?;
        Ok(cookie_jar.matching_cookies(&url))
    }

    pub fn root(&self) -> OpenPageResult<DocumentElement> {
        let body = self.body_arc()?;
        snapshot_root_arc(body, self.base_url_arc()?, Some(&self.none_element_config))
    }

    pub fn set_user_agent<U>(&self, user_agent: U) -> OpenPageResult<()>
    where
        U: Into<SessionUserAgentInput>,
    {
        self.lock_state()?.user_agent = session_user_agent_input(user_agent);
        Ok(())
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.lock_state()?.download_path = normalize_session_download_path(path.as_ref())?;
        Ok(())
    }

    pub fn set_headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        let headers = parse_headers_input(headers)?;
        let mut state = self.lock_state()?;
        state.headers.clear();
        for (name, value) in headers {
            upsert_header_map(&mut state.headers, name, value);
        }
        Ok(())
    }

    pub fn set_header(&self, name: &str, value: &str) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        upsert_header_map(&mut state.headers, name.to_string(), value.to_string());
        Ok(())
    }

    pub fn set_timeout(&self, timeout_secs: u64) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.timeout_secs = timeout_secs;
        rebuild_session_client(&mut state)
    }

    pub fn set_retry<T, I>(&self, retry_times: T, retry_interval: I) -> OpenPageResult<()>
    where
        T: Into<SessionRetryTimesInput>,
        I: Into<SessionRetryIntervalInput>,
    {
        let mut state = self.lock_state()?;
        if let Some(retry_times) = session_retry_times_input(retry_times) {
            state.retry_times = retry_times;
        }
        if let Some(retry_interval_millis) = session_retry_interval_input(retry_interval) {
            state.retry_interval_millis = retry_interval_millis;
        }
        Ok(())
    }

    pub fn set_params<'a, P>(&self, params: P) -> OpenPageResult<()>
    where
        P: Into<ParamsInput<'a>>,
    {
        self.lock_state()?.params = params_input_pairs(params);
        Ok(())
    }

    pub fn set_auth<A>(&self, auth: A) -> OpenPageResult<()>
    where
        A: Into<SessionAuthInput>,
    {
        self.lock_state()?.auth = session_auth_input(auth);
        Ok(())
    }

    pub fn set_hooks(&self, hooks: SessionHooks) -> OpenPageResult<()> {
        self.lock_state()?.hooks = hooks;
        Ok(())
    }

    pub fn hooks(&self) -> OpenPageResult<SessionHooks> {
        Ok(self.lock_state()?.hooks.clone())
    }

    pub fn set_stream(&self, stream: bool) -> OpenPageResult<()> {
        self.lock_state()?.stream = stream;
        Ok(())
    }

    pub fn stream(&self) -> OpenPageResult<bool> {
        Ok(self.lock_state()?.stream)
    }

    pub fn set_proxies<H, S>(&self, http_proxy: H, https_proxy: S) -> OpenPageResult<()>
    where
        H: Into<SessionProxyInput>,
        S: Into<SessionProxyInput>,
    {
        let mut state = self.lock_state()?;
        state.http_proxy = session_proxy_input(http_proxy);
        state.https_proxy = session_proxy_input(https_proxy);
        rebuild_session_client(&mut state)
    }

    pub fn set_verify(&self, verify: bool) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.verify = verify;
        rebuild_session_client(&mut state)
    }

    pub fn set_cert<C>(&self, cert: C) -> OpenPageResult<()>
    where
        C: Into<SessionCertInput>,
    {
        let mut state = self.lock_state()?;
        state.cert = session_cert_input(cert);
        rebuild_session_client(&mut state)
    }

    pub fn set_trust_env(&self, trust_env: bool) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.trust_env = trust_env;
        rebuild_session_client(&mut state)
    }

    pub fn set_max_redirects<M>(&self, max_redirects: M) -> OpenPageResult<()>
    where
        M: Into<SessionMaxRedirectsInput>,
    {
        let mut state = self.lock_state()?;
        state.max_redirects = session_max_redirects_input(max_redirects);
        rebuild_session_client(&mut state)
    }

    pub fn set_encoding<E>(&self, encoding: E) -> OpenPageResult<()>
    where
        E: Into<SessionEncodingInput>,
    {
        let mut state = self.lock_state()?;
        state.forced_encoding = session_encoding_input(encoding);
        refresh_state_body_encoding(&mut state);
        Ok(())
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        self.download_with_options(url, &SessionRequestOptions::default())
    }

    pub fn download_with_options(
        &self,
        url: &str,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<String> {
        self.download_request(url, Some(options), None)
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        self.download_to_with_options(url, path, &SessionRequestOptions::default())
    }

    pub fn download_to_with_options(
        &self,
        url: &str,
        path: impl AsRef<Path>,
        options: &SessionRequestOptions,
    ) -> OpenPageResult<String> {
        self.download_request(
            url,
            Some(options),
            Some(normalize_session_download_path(path.as_ref())?),
        )
    }

    pub fn cookie_header(&self, url: &str) -> OpenPageResult<Option<String>> {
        let url = Url::parse(url)
            .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))))?;
        let jar = self.lock_state()?.cookie_jar.clone();
        jar.cookie_header(&url)
            .map(|value| {
                value
                    .to_str()
                    .map(|text| text.to_string())
                    .map_err(session_cookie_header_decode_error)
            })
            .transpose()
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = Url::parse(url)
            .map_err(|err| OpenPageError::Http(invalid_url_message(url, Some(&err.to_string()))))?;
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

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        let current_url = self
            .url()?
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
        let cookies = cookie_input_to_params(cookies.into(), current_url.as_deref())?;
        let jar = self.lock_state()?.cookie_jar.clone();
        for cookie in &cookies {
            let url = cookie_scope_url_from_param(cookie)?;
            jar.add_cookie_str(&cookie_param_to_set_cookie(cookie), &url);
        }
        Ok(())
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let url = self.cookie_scope_url(url)?;
        let cookie = cookie_assignment(name, value, domain, path);
        self.lock_state()?.cookie_jar.add_cookie_str(&cookie, &url);
        Ok(())
    }

    pub fn remove_cookie(&self, name: &str, url: Option<&str>) -> OpenPageResult<()> {
        let url = self.cookie_scope_url(url)?;
        let header = self.cookie_header(url.as_str())?.unwrap_or_default();
        let filtered = remove_cookie_from_header(&header, name);
        self.clear_cookies()?;
        if !filtered.is_empty() {
            self.set_cookie_header(url.as_str(), &filtered)?;
        }
        Ok(())
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        state.cookie_jar = Arc::new(SessionCookieJar::default());
        rebuild_session_client(&mut state)
    }

    pub fn close(&self) -> OpenPageResult<()> {
        let mut state = self.lock_state()?;
        rebuild_session_client(&mut state)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let body = self.body_arc()?;
        snapshot_find_arc(
            body,
            locator.raw(),
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let body = self.body_arc()?;
        snapshot_find_all_arc(
            body,
            locator.raw(),
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_by(&self, by: &str, value: &str) -> OpenPageResult<DocumentElement> {
        self.find((by, value))
    }

    pub fn find_all_by(&self, by: &str, value: &str) -> OpenPageResult<Vec<DocumentElement>> {
        self.find_all((by, value))
    }

    pub fn query_xpath(&self, expression: &str) -> OpenPageResult<Vec<SessionXPathResult>> {
        let body = self.body_arc()?;
        snapshot_query_xpath_arc(
            body,
            expression,
            self.base_url_arc()?,
            Some(&self.none_element_config),
        )
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<DocumentElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
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
        let mut state = self.lock_state()?;
        ensure_response_body_loaded(&mut state)?;
        state
            .body
            .as_ref()
            .cloned()
            .ok_or_else(|| OpenPageError::Http(session_page_no_loaded_document_message()))
    }

    fn base_url_arc(&self) -> OpenPageResult<Option<Arc<String>>> {
        Ok(self
            .lock_state()?
            .url
            .as_ref()
            .map(|url| Arc::new(url.clone())))
    }

    pub(super) fn lock_state(&self) -> OpenPageResult<std::sync::MutexGuard<'_, SessionState>> {
        self.inner.lock().map_err(|_| {
            OpenPageError::Http(component_state_lock_poisoned_message(
                "session state",
                "会话状态",
            ))
        })
    }

    fn cookie_scope_url(&self, url: Option<&str>) -> OpenPageResult<Url> {
        match url {
            Some(url) => Url::parse(url).map_err(|err| {
                OpenPageError::Http(invalid_url_message(url, Some(&err.to_string())))
            }),
            None => {
                let current_url =
                    self.lock_state()?.url.clone().ok_or_else(|| {
                        OpenPageError::Http(session_page_no_current_url_message())
                    })?;
                Url::parse(&current_url).map_err(|err| {
                    OpenPageError::Http(invalid_url_message(&current_url, Some(&err.to_string())))
                })
            }
        }
    }

    fn request_context(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
    ) -> OpenPageResult<SessionRequestContext> {
        let state = self.lock_state()?;
        let request_options = request_options.cloned().unwrap_or_default();
        let mut params = state.params.clone();
        params.extend(request_options.params);
        let mut hooks = state.hooks.clone();
        if let Some(request_hooks) = request_options.hooks.as_ref() {
            hooks.extend_response_hooks(request_hooks);
        }
        Ok(SessionRequestContext {
            client: session_client_for_url(&state, requested_url),
            user_agent: request_options
                .user_agent
                .or_else(|| state.user_agent.clone()),
            headers: merge_request_headers(&state.headers, &request_options.headers),
            current_url: state.url.clone(),
            params,
            auth: request_options.auth.or_else(|| state.auth.clone()),
            hooks,
            retry_times: request_options.retry_times.unwrap_or(state.retry_times),
            retry_interval_millis: request_options
                .retry_interval_millis
                .unwrap_or(state.retry_interval_millis),
            timeout_secs: request_options.timeout_secs,
            stream: request_options.stream.unwrap_or(state.stream),
        })
    }

    fn send_request_with_retry<F>(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
        mut send: F,
    ) -> OpenPageResult<Response>
    where
        F: FnMut(&SessionRequestContext) -> OpenPageResult<reqwest::blocking::Response>,
    {
        let context = self.request_context(requested_url, request_options)?;
        let retry_times = context.retry_times;
        let retry_interval_millis = context.retry_interval_millis;
        for attempt in 0..=retry_times {
            match send(&context) {
                Ok(response) => {
                    let response = if context.stream && context.hooks.is_empty() {
                        self.store_streaming_response(requested_url, response)?
                    } else {
                        self.store_response(requested_url, response, &context.hooks)?
                    };
                    if response.is_success() || attempt == retry_times {
                        return Ok(response);
                    }
                }
                Err(err) => {
                    if attempt == retry_times {
                        return Err(err);
                    }
                }
            }

            if retry_interval_millis > 0 {
                sleep(Duration::from_millis(retry_interval_millis));
            }
        }

        Err(OpenPageError::Http(
            session_request_retry_loop_exited_message(),
        ))
    }

    fn download_request(
        &self,
        requested_url: &str,
        request_options: Option<&SessionRequestOptions>,
        explicit_target: Option<PathBuf>,
    ) -> OpenPageResult<String> {
        let context = self.request_context(requested_url, request_options)?;
        let retry_times = context.retry_times;
        let retry_interval_millis = context.retry_interval_millis;

        for attempt in 0..=retry_times {
            let request_url = append_query_params(requested_url, &context.params)?;
            let headers = effective_request_headers(
                &context.headers,
                context.current_url.as_deref(),
                &request_url,
            )?;
            let response = apply_request_options(
                context.client.get(&request_url),
                context.user_agent.as_deref(),
                &headers,
                context.auth.as_ref(),
                context.timeout_secs,
            )
            .send()
            .map_err(|err| {
                OpenPageError::Http(session_request_failed_message(
                    "GET",
                    &request_url,
                    &format!("{err:?}"),
                ))
            });

            match response {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let final_url = response.url().to_string();
                    let response_headers = response
                        .headers()
                        .iter()
                        .map(|(name, value)| {
                            (
                                name.as_str().to_string(),
                                value.to_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let content_type = response
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let content_disposition = response
                        .headers()
                        .get(CONTENT_DISPOSITION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let raw_data = Arc::new(
                        response
                            .bytes()
                            .map_err(|err| {
                                OpenPageError::Http(session_response_body_read_failed_message(
                                    &request_url,
                                    &format!("{err:?}"),
                                ))
                            })?
                            .to_vec(),
                    );
                    run_response_hooks(
                        &context.hooks,
                        SessionHookEvent {
                            requested_url: request_url.clone(),
                            response: SessionResponseInfo {
                                url: Some(if final_url.is_empty() {
                                    request_url.clone()
                                } else {
                                    final_url.clone()
                                }),
                                status_code: Some(status_code),
                                headers: response_headers,
                                content_type: content_type.clone(),
                                encoding: None,
                            },
                            raw_data: Arc::clone(&raw_data),
                        },
                    );
                    if !(200..400).contains(&status_code) {
                        if attempt == retry_times {
                            return Err(OpenPageError::Http(session_download_status_message(
                                status_code,
                                &request_url,
                            )));
                        }
                    } else {
                        let filename = suggested_session_download_filename(
                            content_disposition.as_deref(),
                            &request_url,
                            &final_url,
                        );
                        let target_path = self
                            .resolve_session_download_target(explicit_target.as_ref(), &filename)?;
                        if let Some(parent) = target_path.parent() {
                            std::fs::create_dir_all(parent).map_err(|err| {
                                OpenPageError::Io(session_download_file_failed_message(
                                    "create parent directory",
                                    &parent.display().to_string(),
                                    &err.to_string(),
                                ))
                            })?;
                        }
                        std::fs::write(&target_path, raw_data.as_ref()).map_err(|err| {
                            OpenPageError::Io(session_download_file_failed_message(
                                "write",
                                &target_path.display().to_string(),
                                &err.to_string(),
                            ))
                        })?;

                        let path = target_path.display().to_string();
                        let filename = target_path
                            .file_name()
                            .and_then(|value| value.to_str())
                            .map(str::to_string)
                            .unwrap_or(filename);
                        let download = SessionDownload {
                            url: request_url,
                            final_url,
                            path: path.clone(),
                            filename,
                            content_type,
                            status_code,
                            total_bytes: raw_data.len() as u64,
                        };
                        self.lock_state()?.last_download = Some(download);
                        return Ok(path);
                    }
                }
                Err(err) => {
                    if attempt == retry_times {
                        return Err(err);
                    }
                }
            }

            if retry_interval_millis > 0 {
                sleep(Duration::from_millis(retry_interval_millis));
            }
        }

        Err(OpenPageError::Http(
            session_download_retry_loop_exited_message(),
        ))
    }

    fn response_from_state(&self, load_body: bool) -> OpenPageResult<Response> {
        let mut state = self.lock_state()?;
        if load_body {
            ensure_response_body_loaded(&mut state)?;
        }
        let body = state
            .raw_data
            .clone()
            .unwrap_or_else(|| Arc::new(Vec::new()));
        let text = state
            .body
            .clone()
            .unwrap_or_else(|| Arc::new(String::new()));
        Ok(Response {
            url: state.url.clone(),
            status_code: state.status_code,
            headers: state.response_headers.clone(),
            content_type: state.response_content_type.clone(),
            encoding: state.encoding.clone(),
            body,
            text,
        })
    }

    fn store_response(
        &self,
        requested_url: &str,
        response: reqwest::blocking::Response,
        hooks: &SessionHooks,
    ) -> OpenPageResult<Response> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<Vec<_>>();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let raw_data = Arc::new(
            response
                .bytes()
                .map_err(|err| {
                    OpenPageError::Http(session_response_body_read_failed_message(
                        requested_url,
                        &format!("{err:?}"),
                    ))
                })?
                .to_vec(),
        );

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.response_headers = response_headers;
        state.response_content_type = content_type;
        state.pending_response = None;
        state.raw_data = Some(Arc::clone(&raw_data));
        refresh_state_body_encoding(&mut state);
        let hook_event = SessionHookEvent {
            requested_url: requested_url.to_string(),
            response: SessionResponseInfo {
                url: state.url.clone(),
                status_code: state.status_code,
                headers: state.response_headers.clone(),
                content_type: state.response_content_type.clone(),
                encoding: state.encoding.clone(),
            },
            raw_data,
        };
        drop(state);
        run_response_hooks(hooks, hook_event);
        Ok(self.response_from_state(false)?)
    }

    fn store_streaming_response(
        &self,
        requested_url: &str,
        response: reqwest::blocking::Response,
    ) -> OpenPageResult<Response> {
        let final_url = response.url().to_string();
        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<Vec<_>>();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let mut state = self.lock_state()?;
        state.url = Some(if final_url.is_empty() {
            requested_url.to_string()
        } else {
            final_url
        });
        state.status_code = Some(status);
        state.response_headers = response_headers;
        state.response_content_type = content_type;
        state.raw_data = None;
        state.body = None;
        state.json = None;
        state.pending_response = Some(PendingSessionResponse {
            requested_url: requested_url.to_string(),
            response,
        });
        refresh_state_body_encoding(&mut state);
        drop(state);
        Ok(self.response_from_state(false)?)
    }

    fn load_local_file(&self, path: &Path) -> OpenPageResult<Response> {
        let canonical = path.canonicalize().map_err(|err| {
            OpenPageError::Io(session_local_file_failed_message(
                "resolve",
                &path.display().to_string(),
                &err.to_string(),
            ))
        })?;
        let raw_data = std::fs::read(&canonical).map_err(|err| {
            OpenPageError::Io(session_local_file_failed_message(
                "read",
                &canonical.display().to_string(),
                &err.to_string(),
            ))
        })?;
        let file_url = Url::from_file_path(&canonical)
            .map_err(|_| {
                OpenPageError::Io(session_local_file_failed_message(
                    "build file url for",
                    &canonical.display().to_string(),
                    "invalid file path",
                ))
            })?
            .to_string();

        let mut state = self.lock_state()?;
        state.url = Some(file_url);
        state.status_code = Some(200);
        state.response_headers = Vec::new();
        state.response_content_type = None;
        state.pending_response = None;
        state.raw_data = Some(Arc::new(raw_data));
        refresh_state_body_encoding(&mut state);
        drop(state);
        self.response_from_state(true)
    }

    fn resolve_session_download_target(
        &self,
        explicit_target: Option<&PathBuf>,
        filename: &str,
    ) -> OpenPageResult<PathBuf> {
        if let Some(path) = explicit_target {
            return Ok(path.clone());
        }
        Ok(self.lock_state()?.download_path.join(filename))
    }
}
