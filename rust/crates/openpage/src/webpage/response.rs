use super::*;

impl WebPage {
    pub fn raw_data(&self) -> OpenPageResult<Vec<u8>> {
        match self.mode()? {
            WebMode::Driver => Ok(Vec::new()),
            WebMode::Session => self.session.raw_data(),
        }
    }

    pub fn encoding(&self) -> OpenPageResult<Option<String>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.encoding(),
        }
    }

    pub fn set_encoding<E>(&self, encoding: E) -> OpenPageResult<()>
    where
        E: Into<SessionEncodingInput>,
    {
        match self.mode()? {
            WebMode::Driver => Ok(()),
            WebMode::Session => self.session.set_encoding(encoding),
        }
    }

    pub fn status_code(&self) -> OpenPageResult<Option<u16>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.status_code(),
        }
    }

    pub fn json(&self) -> OpenPageResult<Option<Value>> {
        match self.mode()? {
            WebMode::Driver => Ok(None),
            WebMode::Session => self.session.json(),
        }
    }
}
