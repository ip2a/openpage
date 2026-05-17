use std::path::PathBuf;
use std::sync::Arc;

use chromiumoxide::browser::{Browser as OxBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::Page;

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub browser_path: Option<PathBuf>,
    pub user_data_dir: Option<PathBuf>,
    pub headless: bool,
    pub width: u32,
    pub height: u32,
    pub no_sandbox: bool,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            browser_path: None,
            user_data_dir: None,
            headless: true,
            width: 1280,
            height: 900,
            no_sandbox: false,
        }
    }
}

#[derive(Debug)]
struct BrowserState {
    runtime: Arc<Runtime>,
    browser: Mutex<OxBrowser>,
    _handler_task: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub struct Browser {
    inner: Arc<BrowserState>,
}

impl Browser {
    pub fn launch(options: LaunchOptions) -> OpenPageResult<Self> {
        let runtime =
            Arc::new(Runtime::new().map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?);

        let config = build_browser_config(&options)?;
        let (browser, mut handler) = runtime
            .block_on(async move { OxBrowser::launch(config).await })
            .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;

        let handler_task = runtime.spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    eprintln!("openpage handler error: {err}");
                }
            }
        });

        Ok(Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                _handler_task: handler_task,
            }),
        })
    }

    pub fn new_page(&self, url: Option<&str>) -> OpenPageResult<Page> {
        let target_url = url.unwrap_or("about:blank").to_string();
        let runtime = Arc::clone(&self.inner.runtime);
        let page = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .new_page(target_url)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;

        Ok(Page::new(runtime, page))
    }

    pub fn pages(&self) -> OpenPageResult<Vec<Page>> {
        let runtime = Arc::clone(&self.inner.runtime);
        let pages = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .pages()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;

        Ok(pages
            .into_iter()
            .map(|page| Page::new(Arc::clone(&runtime), page))
            .collect())
    }

    pub fn get_page(&self, target_id: &str) -> OpenPageResult<Page> {
        let runtime = Arc::clone(&self.inner.runtime);
        let page = self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .get_page(TargetId::new(target_id))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))
        })?;
        Ok(Page::new(runtime, page))
    }

    pub fn tabs_count(&self) -> OpenPageResult<usize> {
        Ok(self.pages()?.len())
    }

    pub fn tab_ids(&self) -> OpenPageResult<Vec<String>> {
        Ok(self
            .pages()?
            .into_iter()
            .map(|page| page.target_id())
            .collect())
    }

    pub fn version(&self) -> OpenPageResult<String> {
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            let version = browser
                .version()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(version.product)
        })
    }

    pub fn close(&self) -> OpenPageResult<()> {
        self.inner.runtime.block_on(async {
            let mut browser = self.inner.browser.lock().await;
            browser
                .close()
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            let _ = browser.wait().await;
            Ok(())
        })
    }
}

fn build_browser_config(options: &LaunchOptions) -> OpenPageResult<BrowserConfig> {
    let mut builder = BrowserConfig::builder().window_size(options.width, options.height);

    if options.headless {
        builder = builder.new_headless_mode();
    } else {
        builder = builder.with_head();
    }

    if options.no_sandbox {
        builder = builder.no_sandbox();
    }

    if let Some(path) = &options.browser_path {
        builder = builder.chrome_executable(path);
    }

    if let Some(path) = &options.user_data_dir {
        builder = builder.user_data_dir(path);
    }

    builder
        .build()
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))
}
