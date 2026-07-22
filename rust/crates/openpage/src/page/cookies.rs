use super::*;

impl Page {
    pub fn cookie_header(&self) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            let cookies =
                run_page_future_with_cdp_timeout(self.inner.get_cookies(), "read cookies").await?;
            if cookies.is_empty() {
                return Ok(None);
            }

            Ok(Some(
                cookies
                    .into_iter()
                    .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                    .collect::<Vec<_>>()
                    .join("; "),
            ))
        })
    }

    pub fn cookies(&self) -> OpenPageResult<Vec<CookieEntry>> {
        let url = self.url()?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(Vec::new());
        }
        let Some(cookie_header) = self.cookie_header()? else {
            return Ok(Vec::new());
        };
        cookies_from_header(&url, &cookie_header)
    }
}
