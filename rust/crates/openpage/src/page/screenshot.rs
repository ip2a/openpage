use super::*;

impl Page {
    pub fn save_screenshot(&self, path: impl AsRef<Path>, full_page: bool) -> OpenPageResult<()> {
        let params = ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .full_page(full_page)
            .build();

        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.save_screenshot(params, path),
                "save screenshot",
            )
            .await?;
            Ok(())
        })
    }

    pub fn screenshot_bytes(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<Vec<u8>> {
        let params = page_screenshot_params(full_page, left_top, right_bottom)?;
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.screenshot(params), "capture screenshot")
                .await
        })
    }

    pub fn screenshot_base64(
        &self,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<String> {
        Ok(BASE64_STANDARD.encode(self.screenshot_bytes(full_page, left_top, right_bottom)?))
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        full_page: bool,
        left_top: Option<(f64, f64)>,
        right_bottom: Option<(f64, f64)>,
    ) -> OpenPageResult<PathBuf> {
        let title = self.title()?;
        let target = resolve_page_screenshot_target_path(path, name, Some(title.as_str()))?;
        let bytes = self.screenshot_bytes(full_page, left_top, right_bottom)?;
        fs::write(&target, bytes)?;
        Ok(target)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
    ) -> OpenPageResult<PageSaveContent> {
        self.save_with_options(path, name, as_pdf, None)
    }

    pub fn save_with_options(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        as_pdf: bool,
        pdf_options: Option<PrintToPdfParams>,
    ) -> OpenPageResult<PageSaveContent> {
        let save_target = match (path, name) {
            (None, None) => None,
            _ => Some(resolve_page_save_target_path(
                path,
                name,
                resolve_page_save_title(self, path, name)?.as_deref(),
                if as_pdf { "pdf" } else { "mhtml" },
            )?),
        };

        let content = if as_pdf {
            let pdf = self.runtime.block_on(async {
                run_page_future_with_cdp_timeout(
                    self.inner.pdf(pdf_options.unwrap_or_default()),
                    "print pdf",
                )
                .await
            })?;
            PageSaveContent::Pdf(pdf)
        } else {
            let mhtml = self.runtime.block_on(async {
                execute_page_command_async(
                    &self.inner,
                    CaptureSnapshotParams::builder()
                        .format(CaptureSnapshotFormat::Mhtml)
                        .build(),
                    "Page::save_with_options()",
                )
                .await
                .map(|result| result.data.clone())
            })?;
            PageSaveContent::Mhtml(mhtml)
        };

        if let Some(target) = save_target {
            match &content {
                PageSaveContent::Mhtml(mhtml) => fs::write(&target, mhtml.as_bytes())?,
                PageSaveContent::Pdf(pdf) => fs::write(&target, pdf)?,
            }
        }

        Ok(content)
    }

    pub fn save_pdf(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.save_pdf(PrintToPdfParams::default(), path),
                "save pdf",
            )
            .await?;
            Ok(())
        })
    }
}
