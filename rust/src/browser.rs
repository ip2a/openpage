use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use chromiumoxide::browser::{Browser as OxBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    CancelDownloadParams, SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use crate::download::{
    DownloadInfo, DownloadMission, DownloadState, DownloadStore, attach_download_store,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::Page;

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub browser_path: Option<PathBuf>,
    pub download_path: Option<PathBuf>,
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
            download_path: None,
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
    downloads: DownloadStore,
    download_path: StdMutex<Option<PathBuf>>,
    _download_task: tokio::task::JoinHandle<()>,
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
        let configured_download_path = options.download_path.clone();
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
        let (downloads, download_task) = attach_download_store(Arc::clone(&runtime), &browser)?;

        let browser = Self {
            inner: Arc::new(BrowserState {
                runtime,
                browser: Mutex::new(browser),
                downloads,
                download_path: StdMutex::new(configured_download_path.clone()),
                _download_task: download_task,
                _handler_task: handler_task,
            }),
        };

        if let Some(path) = configured_download_path {
            browser.set_download_path(path)?;
        }

        Ok(browser)
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

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .download_path
            .lock()
            .map(|path| {
                path.as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
            })
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        let download_path = path.to_string_lossy().into_owned();

        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            let params = SetDownloadBehaviorParams::builder()
                .behavior(SetDownloadBehaviorBehavior::Allow)
                .download_path(download_path)
                .events_enabled(true)
                .build()
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            browser
                .execute(params)
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok::<(), OpenPageError>(())
        })?;

        self.inner
            .download_path
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation("browser download path lock poisoned".to_string())
            })?
            .replace(path);
        Ok(())
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .mission_ids()?
            .into_iter()
            .map(|guid| DownloadMission::new(self.clone(), guid))
            .collect())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .last_guid()?
            .map(|guid| DownloadMission::new(self.clone(), guid)))
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        let download_dir = self.download_path()?.map(PathBuf::from).ok_or_else(|| {
            OpenPageError::UnsupportedOperation("download path is not configured".to_string())
        })?;
        match filename {
            Some(filename) => {
                let info = self.inner.downloads.wait_for_name(filename, timeout_ms)?;
                resolve_download_path(&info, &download_dir, Some(filename), timeout_ms, None)
            }
            None => {
                let completed_before = self.inner.downloads.completed_len()?;
                let baseline = read_visible_downloads(&download_dir)?;
                let info = self
                    .inner
                    .downloads
                    .wait_for_next_after(completed_before, timeout_ms)?;
                resolve_download_path(&info, &download_dir, None, timeout_ms, Some(&baseline))
            }
        }
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        let guid = guid.to_string();
        self.inner.runtime.block_on(async {
            let browser = self.inner.browser.lock().await;
            browser
                .execute(CancelDownloadParams::new(guid))
                .await
                .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
            Ok(())
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

    pub(crate) fn download_info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.inner.downloads.info(guid)
    }

    pub(crate) fn wait_for_download_guid(
        &self,
        guid: &str,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        let download_dir = self.download_path()?.map(PathBuf::from).ok_or_else(|| {
            OpenPageError::UnsupportedOperation("download path is not configured".to_string())
        })?;
        let info = self.inner.downloads.wait_for_guid(guid, timeout_ms)?;
        resolve_download_path(&info, &download_dir, None, timeout_ms, None)
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

fn resolve_download_path(
    info: &DownloadInfo,
    download_dir: &Path,
    filename: Option<&str>,
    timeout_ms: u64,
    baseline: Option<&[PathBuf]>,
) -> OpenPageResult<String> {
    if info.state == DownloadState::Canceled {
        return Err(OpenPageError::BrowserOperation(format!(
            "download `{}` was canceled",
            info.guid
        )));
    }

    if let Some(path) = &info.final_path {
        return Ok(path.clone());
    }

    let preferred_name = filename.unwrap_or(&info.suggested_filename);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let preferred_path = download_dir.join(preferred_name);
        if preferred_path.exists() {
            return Ok(preferred_path.to_string_lossy().into_owned());
        }

        if let Some(baseline) = baseline {
            let current = read_visible_downloads(download_dir)?;
            if let Some(path) = current
                .into_iter()
                .find(|path| !baseline.iter().any(|seen| seen == path))
            {
                return Ok(path.to_string_lossy().into_owned());
            }
        }

        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(
                "download did not complete in time".to_string(),
            ));
        }
        sleep(Duration::from_millis(100));
    }
}

fn read_visible_downloads(dir: &Path) -> OpenPageResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in
        std::fs::read_dir(dir).map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?
    {
        let entry = entry.map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("crdownload"))
        {
            continue;
        }
        files.push(path);
    }
    files.sort();
    Ok(files)
}
