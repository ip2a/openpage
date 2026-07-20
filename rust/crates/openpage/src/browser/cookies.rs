use super::*;

impl Browser {
    pub fn version(&self) -> OpenPageResult<String> {
        self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::version().lock()").await?;
            let version =
                run_browser_future_with_cdp_timeout(browser.version(), "Browser::version()")
                    .await?;
            Ok(version.product)
        })
    }

    pub fn address(&self) -> String {
        self.inner.debugger_address.clone()
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let cookie = browser_cookie_param(name, value, url, domain, path);
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            SetCookiesParams::new(vec![cookie]),
            "Browser::set_cookie()",
        )?;
        Ok(())
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = parse_browser_cookie_header_url(url)?;
        let cookies = browser_cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            SetCookiesParams::new(cookies),
            "Browser::set_cookie_header()",
        )?;
        Ok(())
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let params = browser_delete_cookie_params(name, url, domain, path);
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::remove_cookie()",
        )?;
        Ok(())
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            ClearBrowserCookiesParams::default(),
            "Browser::clear_cookies()",
        )?;
        Ok(())
    }

    pub fn set_permission(
        &self,
        permission: PermissionDescriptor,
        setting: PermissionSetting,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
        browser_context_id: Option<&str>,
    ) -> OpenPageResult<()> {
        let mut params = SetPermissionParams::builder()
            .permission(permission)
            .setting(setting);
        if let Some(origin) = origin {
            params = params.origin(origin.to_string());
        }
        if let Some(embedded_origin) = embedded_origin {
            params = params.embedded_origin(embedded_origin.to_string());
        }
        if let Some(browser_context_id) = browser_context_id {
            params = params.browser_context_id(browser_context_id.to_string());
        }
        let params = params.build().map_err(OpenPageError::BrowserOperation)?;
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::set_permission()",
        )?;
        Ok(())
    }

    pub fn reset_permissions(&self, browser_context_id: Option<&str>) -> OpenPageResult<()> {
        let mut params = ResetPermissionsParams::builder();
        if let Some(browser_context_id) = browser_context_id {
            params = params.browser_context_id(browser_context_id.to_string());
        }
        let params = params.build();
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            params,
            "Browser::reset_permissions()",
        )?;
        Ok(())
    }
}
