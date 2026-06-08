use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::Duration;

use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
use chromiumoxide::cdp::browser_protocol::page::{
    EventFileChooserOpened, FileChooserOpenedMode, SetInterceptFileChooserDialogParams,
};
use chromiumoxide::page::Page as OxPage;
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio::time::timeout as tokio_timeout;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::{execute_page_command_async, execute_page_command_blocking};
use crate::settings::{
    cdp_timeout_duration, component_not_running_message, component_state_lock_poisoned_message,
    file_chooser_backend_node_missing_message, timeout_duration_millis, timeout_error,
    upload_requires_at_least_one_file_message,
};

#[derive(Debug, Default)]
struct UploadState {
    pending_files: Option<Vec<String>>,
    active_request_id: Option<u64>,
    next_request_id: u64,
    completed_request_id: u64,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

#[derive(Debug)]
struct UploadShared {
    state: StdMutex<UploadState>,
    condvar: Condvar,
}

impl UploadShared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(UploadState::default()),
            condvar: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UploadTracker {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<UploadShared>,
}

impl UploadTracker {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        let shared = Arc::new(UploadShared::new());
        let tracker = Self {
            runtime: Arc::clone(&runtime),
            page: page.clone(),
            shared: Arc::clone(&shared),
        };

        let task_shared = Arc::clone(&shared);
        let task_page = page.clone();
        let handle = runtime.spawn(async move {
            let mut events = match register_upload_listener_with_cdp_timeout(
                task_page.event_listener::<EventFileChooserOpened>(),
                "register upload file chooser listener",
            )
            .await
            {
                Ok(events) => events,
                Err(err) => {
                    set_last_error(&task_shared, err.to_string());
                    return;
                }
            };

            while let Some(event) = events.next().await {
                let (files, request_id) = {
                    let mut state = match task_shared.state.lock() {
                        Ok(state) => state,
                        Err(_) => return,
                    };
                    (state.pending_files.take(), state.active_request_id)
                };

                let (Some(files), Some(request_id)) = (files, request_id) else {
                    continue;
                };

                let result = async {
                    apply_upload_files(&task_page, event, files).await?;
                    execute_page_command_async(
                        &task_page,
                        SetInterceptFileChooserDialogParams::new(false),
                        "UploadTracker::new()",
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                    Ok::<(), String>(())
                }
                .await;

                finish_request(&task_shared, request_id, result.err());
            }
        });

        if let Ok(mut state) = tracker.shared.state.lock() {
            state.task = Some(handle);
        }

        tracker
    }

    pub fn set_files(&self, files: &[String]) -> OpenPageResult<()> {
        let normalized = prepare_upload_file_paths(files)?;

        {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| upload_state_lock_poisoned_error())?;
            if state.task.is_none() {
                return Err(upload_tracker_not_running_error());
            }
            state.next_request_id = state.next_request_id.saturating_add(1).max(1);
            state.pending_files = Some(normalized);
            state.active_request_id = Some(state.next_request_id);
            state.last_error = None;
            self.shared.condvar.notify_all();
        }

        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.page,
            SetInterceptFileChooserDialogParams::new(true),
            "UploadTracker::set_files()",
        )?;
        Ok(())
    }

    pub fn wait_until_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| upload_state_lock_poisoned_error())?;

        let target_request_id = match state.active_request_id {
            Some(request_id) => request_id,
            None if state.next_request_id == 0 => return Ok(false),
            None if state.completed_request_id >= state.next_request_id => {
                if let Some(error) = &state.last_error {
                    return Err(OpenPageError::PageOperation(error.clone()));
                }
                return Ok(true);
            }
            None => state.next_request_id,
        };

        let timeout = Duration::from_millis(timeout_ms.max(1));
        let (state, _timeout_result) = self
            .shared
            .condvar
            .wait_timeout_while(state, timeout, |state| {
                state.active_request_id == Some(target_request_id)
                    && state.completed_request_id < target_request_id
                    && state.last_error.is_none()
            })
            .map_err(|_| upload_state_lock_poisoned_error())?;

        if let Some(error) = &state.last_error {
            return Err(OpenPageError::PageOperation(error.clone()));
        }

        Ok(state.completed_request_id >= target_request_id)
    }
}

async fn register_upload_listener_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| OpenPageError::PageOperation(err.to_string()))
}

async fn apply_upload_files(
    page: &OxPage,
    event: Arc<EventFileChooserOpened>,
    files: Vec<String>,
) -> Result<(), String> {
    let Some(backend_node_id) = event.backend_node_id else {
        return Err(file_chooser_backend_node_missing_message());
    };

    let selected_files = match event.mode {
        FileChooserOpenedMode::SelectSingle => files.into_iter().take(1).collect::<Vec<_>>(),
        FileChooserOpenedMode::SelectMultiple => files,
    };

    let params = SetFileInputFilesParams::builder()
        .files(selected_files)
        .backend_node_id(backend_node_id)
        .build()
        .map_err(|err| err.to_string())?;
    execute_page_command_async(page, params, "apply_upload_files()")
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn normalize_file_paths(files: &[String]) -> OpenPageResult<Vec<String>> {
    files
        .iter()
        .map(|file| {
            let path = PathBuf::from(file);
            let absolute = if path.is_absolute() {
                path
            } else {
                env::current_dir()?.join(path)
            };
            Ok(absolute.to_string_lossy().into_owned())
        })
        .collect()
}

fn prepare_upload_file_paths(files: &[String]) -> OpenPageResult<Vec<String>> {
    let normalized = normalize_file_paths(files)?;
    if normalized.is_empty() {
        return Err(OpenPageError::PageOperation(
            upload_requires_at_least_one_file_message(),
        ));
    }
    Ok(normalized)
}

fn upload_state_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "upload state",
        "上传状态",
    ))
}

fn upload_tracker_not_running_error() -> OpenPageError {
    OpenPageError::PageOperation(component_not_running_message(
        "upload tracker",
        "上传跟踪器",
    ))
}

fn set_last_error(shared: &Arc<UploadShared>, error: String) {
    if let Ok(mut state) = shared.state.lock() {
        state.pending_files = None;
        if let Some(request_id) = state.active_request_id.take() {
            state.completed_request_id = state.completed_request_id.max(request_id);
        }
        state.last_error = Some(error);
        shared.condvar.notify_all();
    }
}

fn finish_request(shared: &Arc<UploadShared>, request_id: u64, error: Option<String>) {
    if let Ok(mut state) = shared.state.lock() {
        if state.active_request_id == Some(request_id) {
            state.active_request_id = None;
        }
        state.completed_request_id = state.completed_request_id.max(request_id);
        state.last_error = error;
        shared.condvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::runtime::Runtime;

    use crate::error::OpenPageError;
    use crate::settings::{Settings, scoped_test_settings};

    use super::{prepare_upload_file_paths, register_upload_listener_with_cdp_timeout};

    #[test]
    fn upload_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english = prepare_upload_file_paths(&[])
            .expect_err("empty upload list should fail")
            .to_string();
        assert!(english.contains("upload_files() requires at least one file"));

        Settings::set_language("cn");

        let chinese = prepare_upload_file_paths(&[])
            .expect_err("empty upload list should fail in Chinese")
            .to_string();
        assert!(chinese.contains("upload_files() 至少需要一个文件"));
    }

    #[test]
    fn upload_listener_registration_respects_global_timeout_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("create tokio runtime");
        let result = runtime.block_on(async {
            register_upload_listener_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "register upload file chooser listener",
            )
            .await
        });

        Settings::reset();

        let error = result.expect_err("upload listener registration should time out");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("register upload file chooser listener")),
            "unexpected upload registration timeout error: {error}"
        );
    }
}
