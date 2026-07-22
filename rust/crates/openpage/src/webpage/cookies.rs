use super::*;

impl WebPage {
    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        match self.mode()? {
            WebMode::Driver => self.driver.cookies(),
            WebMode::Session => self.session.cookies(),
        }
    }

    pub fn cookie_header(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => self.driver.cookie_header(),
            WebMode::Session => {
                let Some(url) = self.session.url()? else {
                    return Ok(None);
                };
                self.session.cookie_header(&url)
            }
        }
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookies(cookies),
            WebMode::Session => self.session.set_cookies(cookies),
        }
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookie_header(url, cookie_header),
            WebMode::Session => self.session.set_cookie_header(url, cookie_header),
        }
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.set_cookie(name, value, url, domain, path),
            WebMode::Session => self.session.set_cookie(name, value, url, domain, path),
        }
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.remove_cookie(name, url, domain, path),
            WebMode::Session => self.session.remove_cookie(name, url),
        }
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        match self.mode()? {
            WebMode::Driver => self.driver.clear_cookies(),
            WebMode::Session => self.session.clear_cookies(),
        }
    }

    pub fn cookies_to_session(&self, copy_user_agent: bool) -> OpenPageResult<()> {
        let url = self.driver.url()?;
        if url.is_empty() {
            return Ok(());
        }
        if let Some(cookie_header) = self.driver.cookie_header()? {
            self.session.set_cookie_header(&url, &cookie_header)?;
        }
        if copy_user_agent {
            self.session
                .set_user_agent(Some(self.driver.user_agent()?))?;
        }
        Ok(())
    }

    pub fn cookies_to_browser(&self) -> OpenPageResult<()> {
        let Some(url) = self.session.url()? else {
            return Ok(());
        };

        let driver_url = self.driver.url()?;
        if !driver_url.starts_with("http://") && !driver_url.starts_with("https://") {
            self.driver.goto(&url)?;
        }

        if let Some(cookie_header) = self.session.cookie_header(&url)? {
            self.driver.set_cookie_header(&url, &cookie_header)?;
        }
        Ok(())
    }
}
