use super::*;

impl WebPage {
    pub fn html(&self) -> OpenPageResult<String> {
        match self.mode()? {
            WebMode::Driver => self.driver.html(),
            WebMode::Session => self.session.html(),
        }
    }
}
