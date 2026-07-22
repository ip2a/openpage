use super::*;

impl WebPage {
    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_screenshot()"),
            ));
        }
        self.driver.save_screenshot(path, full_page)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
    ) -> OpenPageResult<PageSaveContent> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save()"),
            ));
        }
        self.driver.save(path, name, as_pdf)
    }

    pub fn save_with_options(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
        pdf_options: Option<chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams>,
    ) -> OpenPageResult<PageSaveContent> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_with_options()"),
            ));
        }
        self.driver
            .save_with_options(path, name, as_pdf, pdf_options)
    }

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("save_pdf()"),
            ));
        }
        self.driver.save_pdf(path)
    }

    pub fn screenshot_bytes(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<Vec<u8>> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_bytes()"),
            ));
        }
        self.driver
            .screenshot_bytes(full_page, left_top, right_bottom)
    }

    pub fn screenshot_base64(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<String> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("screenshot_base64()"),
            ));
        }
        self.driver
            .screenshot_base64(full_page, left_top, right_bottom)
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<std::path::PathBuf> {
        if self.mode()? != WebMode::Driver {
            return Err(OpenPageError::UnsupportedOperation(
                driver_mode_only_message("get_screenshot()"),
            ));
        }
        self.driver
            .get_screenshot(path, name, full_page, left_top, right_bottom)
    }
}
