use super::*;

impl WebPage {
    pub fn get_tab<'a, I, T>(
        &self,
        id_or_num: Option<I>,
        title: Option<&str>,
        url: Option<&str>,
        tab_type: Option<T>,
        as_id: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>>
    where
        I: Into<BrowserTabSelector<'a>>,
        T: Into<BrowserTabTypeInput<'a>>,
    {
        self.browser
            .get_tab(id_or_num, title, url, tab_type, as_id)
            .map(|reference| reference.map(|reference| self.mix_tab_reference(reference)))
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        self.get(url).map(|_| ())
    }

    pub fn post(&self, url: &str) -> OpenPageResult<bool> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.post(url)
    }

    pub fn post_json(&self, url: &str, payload: Option<Value>) -> OpenPageResult<bool> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.post_json(url, payload)
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => self.browser.download_path(),
            WebMode::Session => self.session.download_path().map(Some),
        }
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        if self.mode()? == WebMode::Driver {
            self.cookies_to_session(true)?;
        }
        self.session.download_to(url, path)
    }
}
