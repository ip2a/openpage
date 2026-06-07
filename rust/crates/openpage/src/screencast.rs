use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, EventScreencastFrame, ScreencastFrameAckParams, StartScreencastFormat,
    StartScreencastParams, StopScreencastParams,
};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::page::{Page as OxPage, ScreenshotParams};
use futures::StreamExt;
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame as AnimationFrame, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::error::{OpenPageError, OpenPageResult};
use crate::page::execute_page_command_async;
use crate::settings::{
    component_not_running_message, component_state_lock_poisoned_message,
    invalid_screencast_data_url_message, screencast_already_running_message,
    screencast_capture_path_unavailable_message, screencast_empty_mime_type_message,
    screencast_encode_output_failed_message, screencast_ffmpeg_encode_failed_message,
    screencast_ffmpeg_spawn_failed_message, screencast_mode_change_while_running_message,
    screencast_mode_output_suffix_message, screencast_no_frames_message,
    screencast_output_path_unavailable_message, screencast_requires_save_path_message,
    screencast_save_path_must_be_directory_message, unsupported_screencast_output_suffix_message,
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScreencastMode {
    #[default]
    Video,
    FrugalVideo,
    JsVideo,
    Imgs,
    FrugalImgs,
}

#[derive(Debug, Default)]
struct ScreencastState {
    mode: ScreencastMode,
    save_path: Option<PathBuf>,
    active_path: Option<PathBuf>,
    capture_path: Option<PathBuf>,
    running: bool,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

#[derive(Debug, Default)]
struct ScreencastShared {
    state: StdMutex<ScreencastState>,
}

#[derive(Clone, Debug)]
pub struct Screencast {
    runtime: Arc<Runtime>,
    page: OxPage,
    shared: Arc<ScreencastShared>,
}

#[derive(Debug, Deserialize)]
struct JsVideoPayload {
    mime_type: String,
    data_url: String,
}

impl Screencast {
    pub fn new(runtime: Arc<Runtime>, page: OxPage) -> Self {
        Self {
            runtime,
            page,
            shared: Arc::new(ScreencastShared::default()),
        }
    }

    pub fn set_mode(&self, mode: ScreencastMode) -> OpenPageResult<()> {
        let mut state = lock_state(&self.shared)?;
        if state.running {
            return Err(OpenPageError::BrowserOperation(
                screencast_mode_change_while_running_message(),
            ));
        }
        state.mode = mode;
        Ok(())
    }

    pub fn mode(&self) -> OpenPageResult<ScreencastMode> {
        Ok(lock_state(&self.shared)?.mode)
    }

    pub fn set_save_path(&self, save_path: impl AsRef<Path>) -> OpenPageResult<PathBuf> {
        let path = prepare_output_dir(save_path.as_ref())?;
        let mut state = lock_state(&self.shared)?;
        state.save_path = Some(path.clone());
        Ok(path)
    }

    pub fn start(&self, save_path: Option<&Path>) -> OpenPageResult<PathBuf> {
        let (mode, output_dir, capture_path) = {
            let mut state = lock_state(&self.shared)?;
            if state.running {
                return Err(OpenPageError::BrowserOperation(
                    screencast_already_running_message(),
                ));
            }

            let output_dir = match save_path {
                Some(path) => {
                    let resolved = prepare_output_dir(path)?;
                    state.save_path = Some(resolved.clone());
                    resolved
                }
                None => state.save_path.clone().ok_or_else(|| {
                    OpenPageError::BrowserOperation(screencast_requires_save_path_message())
                })?,
            };

            let capture_path = match state.mode {
                ScreencastMode::Video | ScreencastMode::FrugalVideo => {
                    Some(prepare_temp_output_dir()?)
                }
                ScreencastMode::Imgs | ScreencastMode::FrugalImgs => Some(output_dir.clone()),
                ScreencastMode::JsVideo => None,
            };

            state.active_path = Some(output_dir.clone());
            state.capture_path = capture_path.clone();
            state.running = true;
            state.last_error = None;
            (state.mode, output_dir, capture_path)
        };

        if mode == ScreencastMode::JsVideo {
            if let Err(err) = self
                .runtime
                .block_on(start_js_screencast(self.page.clone()))
            {
                let mut state = lock_state(&self.shared)?;
                state.active_path = None;
                state.capture_path = None;
                state.running = false;
                state.last_error = None;
                return Err(err);
            }
            return Ok(output_dir);
        }

        let page = self.page.clone();
        let shared = Arc::clone(&self.shared);
        let capture_dir = capture_path.ok_or_else(|| {
            OpenPageError::BrowserOperation(screencast_capture_path_unavailable_message())
        })?;
        let handle = self.runtime.spawn(async move {
            let result = match mode {
                ScreencastMode::Video | ScreencastMode::Imgs => {
                    run_imgs_screencast(page, shared.clone(), capture_dir).await
                }
                ScreencastMode::FrugalVideo | ScreencastMode::FrugalImgs => {
                    run_frugal_imgs_screencast(page, shared.clone(), capture_dir).await
                }
                ScreencastMode::JsVideo => Ok(()),
            };
            finish_screencast(&shared, result.err().map(|err| err.to_string()));
        });

        let mut state = lock_state(&self.shared)?;
        state.task = Some(handle);
        Ok(output_dir)
    }

    pub fn stop(&self) -> OpenPageResult<PathBuf> {
        self.stop_with_options(None, None)
    }

    pub fn stop_with_options(
        &self,
        video_name: Option<&str>,
        suffix: Option<&str>,
    ) -> OpenPageResult<PathBuf> {
        self.stop_internal(video_name, suffix, None)
    }

    pub fn stop_with_encoding(
        &self,
        video_name: Option<&str>,
        suffix: Option<&str>,
        codec: Option<&str>,
    ) -> OpenPageResult<PathBuf> {
        self.stop_internal(video_name, suffix, codec)
    }

    fn stop_internal(
        &self,
        video_name: Option<&str>,
        suffix: Option<&str>,
        codec: Option<&str>,
    ) -> OpenPageResult<PathBuf> {
        let (mode, output_dir, capture_path, handle) = {
            let mut state = lock_state(&self.shared)?;
            if !state.running {
                return Err(OpenPageError::BrowserOperation(
                    component_not_running_message("screencast", "录屏"),
                ));
            }
            state.running = false;
            let output_dir = state.active_path.clone().ok_or_else(|| {
                OpenPageError::BrowserOperation(screencast_output_path_unavailable_message())
            })?;
            (
                state.mode,
                output_dir,
                state.capture_path.clone(),
                state.task.take(),
            )
        };

        let result = match mode {
            ScreencastMode::Imgs => {
                wait_for_task(&self.runtime, handle);
                Ok(output_dir.clone())
            }
            ScreencastMode::FrugalImgs => {
                self.runtime
                    .block_on(stop_cdp_screencast(self.page.clone()))?;
                wait_for_task(&self.runtime, handle);
                Ok(output_dir.clone())
            }
            ScreencastMode::Video | ScreencastMode::FrugalVideo => {
                if mode == ScreencastMode::FrugalVideo {
                    self.runtime
                        .block_on(stop_cdp_screencast(self.page.clone()))?;
                }
                wait_for_task(&self.runtime, handle);
                let capture_dir = capture_path.ok_or_else(|| {
                    OpenPageError::BrowserOperation(screencast_capture_path_unavailable_message())
                })?;
                let video_path = build_video_output_path(mode, &output_dir, video_name, suffix)?;
                let encode_result = encode_frames_output(&capture_dir, &video_path, codec);
                cleanup_temp_dir(&capture_dir, &output_dir);
                encode_result?;
                Ok(video_path)
            }
            ScreencastMode::JsVideo => {
                let payload = self
                    .runtime
                    .block_on(stop_js_screencast(self.page.clone()))?;
                let video_path = build_video_output_path(mode, &output_dir, video_name, suffix)?;
                fs::write(&video_path, decode_data_url(&payload.data_url)?)?;
                Ok(video_path)
            }
        };

        let mut state = lock_state(&self.shared)?;
        state.active_path = None;
        state.capture_path = None;
        if let Some(error) = state.last_error.take() {
            return Err(OpenPageError::BrowserOperation(error));
        }
        result
    }

    pub fn is_running(&self) -> OpenPageResult<bool> {
        Ok(lock_state(&self.shared)?.running)
    }
}

impl ScreencastMode {
    fn default_suffix(self) -> &'static str {
        match self {
            ScreencastMode::Video | ScreencastMode::FrugalVideo => "mp4",
            ScreencastMode::JsVideo => "webm",
            ScreencastMode::Imgs | ScreencastMode::FrugalImgs => "",
        }
    }

    fn supports_suffix(self, suffix: &str) -> bool {
        match self {
            ScreencastMode::Video | ScreencastMode::FrugalVideo => {
                suffix.eq_ignore_ascii_case("gif") || suffix.eq_ignore_ascii_case("mp4")
            }
            ScreencastMode::JsVideo => suffix.eq_ignore_ascii_case("webm"),
            ScreencastMode::Imgs | ScreencastMode::FrugalImgs => false,
        }
    }
}

async fn start_js_screencast(page: OxPage) -> OpenPageResult<()> {
    let script = r#"
        (async () => {
            if (window.__openpageScreencast?.mediaRecorder?.state === "recording") {
                throw new Error("screencast is already running");
            }
            const stream = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: true });
            const mimeType = MediaRecorder.isTypeSupported("video/webm; codecs=vp9")
                ? "video/webm; codecs=vp9"
                : "video/webm";
            const holder = {
                stream,
                chunks: [],
                mimeType,
                mediaRecorder: null,
                blob: null,
            };
            const recorder = new MediaRecorder(stream, { mimeType });
            holder.mediaRecorder = recorder;
            recorder.addEventListener("dataavailable", event => {
                if (event.data && event.data.size) {
                    holder.chunks.push(event.data);
                }
            });
            recorder.addEventListener("stop", () => {
                holder.blob = new Blob(holder.chunks, { type: holder.mimeType });
            });
            recorder.start();
            window.__openpageScreencast = holder;
            return true;
        })()
    "#;
    evaluate_with_user_gesture(&page, script).await.map(|_| ())
}

async fn stop_js_screencast(page: OxPage) -> OpenPageResult<JsVideoPayload> {
    let script = r#"
        (async () => {
            const holder = window.__openpageScreencast;
            if (!holder || !holder.mediaRecorder) {
                throw new Error("screencast is not running");
            }
            if (holder.mediaRecorder.state !== "inactive") {
                await new Promise((resolve, reject) => {
                    holder.mediaRecorder.addEventListener("stop", resolve, { once: true });
                    holder.mediaRecorder.addEventListener("error", () => reject(new Error("media recorder error")), { once: true });
                    holder.mediaRecorder.stop();
                });
            }
            for (const track of holder.stream.getTracks()) {
                track.stop();
            }
            const blob = holder.blob || new Blob(holder.chunks, { type: holder.mimeType || "video/webm" });
            const dataUrl = await new Promise((resolve, reject) => {
                const reader = new FileReader();
                reader.onloadend = () => resolve(reader.result);
                reader.onerror = () => reject(reader.error || new Error("file reader error"));
                reader.readAsDataURL(blob);
            });
            const result = JSON.stringify({
                mime_type: blob.type || holder.mimeType || "video/webm",
                data_url: dataUrl,
            });
            window.__openpageScreencast = null;
            return result;
        })()
    "#;
    let value = evaluate_with_user_gesture(&page, script).await?;
    let raw = value_as_string(value, "js screencast payload")?;
    let payload = serde_json::from_str::<JsVideoPayload>(&raw)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    if payload.mime_type.is_empty() {
        return Err(OpenPageError::BrowserOperation(
            screencast_empty_mime_type_message(),
        ));
    }
    Ok(payload)
}

async fn stop_cdp_screencast(page: OxPage) -> OpenPageResult<()> {
    execute_page_command_async(
        &page,
        StopScreencastParams::default(),
        "Screencast::stop_cdp_screencast()",
    )
    .await?;
    Ok(())
}

async fn evaluate_with_user_gesture(page: &OxPage, expression: &str) -> OpenPageResult<Value> {
    let params = EvaluateParams::builder()
        .expression(expression)
        .user_gesture(true)
        .build()
        .map_err(OpenPageError::PageOperation)?;
    let result = page
        .evaluate(params)
        .await
        .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
    result
        .into_value::<Value>()
        .map_err(|err| OpenPageError::JavaScript(err.to_string()))
}

async fn run_imgs_screencast(
    page: OxPage,
    shared: Arc<ScreencastShared>,
    capture_dir: PathBuf,
) -> OpenPageResult<()> {
    let mut index = 0_u64;

    while is_running(&shared)? {
        let params = ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Jpeg)
            .build();
        let bytes = page
            .screenshot(params)
            .await
            .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;
        fs::write(frame_output_path(&capture_dir, index), bytes)?;
        index += 1;
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    Ok(())
}

async fn run_frugal_imgs_screencast(
    page: OxPage,
    shared: Arc<ScreencastShared>,
    capture_dir: PathBuf,
) -> OpenPageResult<()> {
    let mut events = page
        .event_listener::<EventScreencastFrame>()
        .await
        .map_err(|err| OpenPageError::PageOperation(err.to_string()))?;

    execute_page_command_async(
        &page,
        StartScreencastParams::builder()
            .format(StartScreencastFormat::Jpeg)
            .quality(100)
            .every_nth_frame(1)
            .build(),
        "Screencast::run_frugal_imgs_screencast()",
    )
    .await?;

    let mut index = 0_u64;

    loop {
        if !is_running(&shared)? {
            break;
        }

        let event = match tokio::time::timeout(Duration::from_millis(100), events.next()).await {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(_) => continue,
        };

        let encoded: &str = event.data.as_ref();
        let bytes = BASE64_STANDARD
            .decode(encoded)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        fs::write(frame_output_path(&capture_dir, index), bytes)?;
        index += 1;

        execute_page_command_async(
            &page,
            ScreencastFrameAckParams::new(event.session_id),
            "Screencast::run_frugal_imgs_screencast()",
        )
        .await?;
    }

    Ok(())
}

fn build_video_output_path(
    mode: ScreencastMode,
    output_dir: &Path,
    video_name: Option<&str>,
    suffix: Option<&str>,
) -> OpenPageResult<PathBuf> {
    let suffix = suffix
        .unwrap_or(mode.default_suffix())
        .trim_start_matches('.');
    if !mode.supports_suffix(suffix) {
        return Err(OpenPageError::UnsupportedOperation(
            screencast_mode_output_suffix_message(&format!("{mode:?}"), mode.default_suffix()),
        ));
    }

    let file_name = match video_name {
        Some(name) if !name.trim().is_empty() => {
            let candidate = Path::new(name);
            if suffix.is_empty() || candidate.extension().is_some() {
                candidate.to_path_buf()
            } else {
                PathBuf::from(format!("{name}.{suffix}"))
            }
        }
        _ => PathBuf::from(format!("{}.{}", timestamp_nanos(), suffix)),
    };

    Ok(output_dir.join(file_name))
}

fn encode_frames_as_gif(capture_dir: &Path, output_path: &Path) -> OpenPageResult<()> {
    let frame_paths = collect_frame_paths(capture_dir)?;
    if frame_paths.is_empty() {
        return Err(OpenPageError::BrowserOperation(
            screencast_no_frames_message(),
        ));
    }

    let file = File::create(output_path)?;
    let mut encoder = GifEncoder::new(file);
    encoder.set_repeat(Repeat::Infinite).map_err(image_error)?;

    for frame_path in frame_paths {
        let frame = ImageReader::open(&frame_path)
            .map_err(image_error)?
            .decode()
            .map_err(image_error)?
            .into_rgba8();
        encoder
            .encode_frame(AnimationFrame::from_parts(
                frame,
                0,
                0,
                Delay::from_numer_denom_ms(40, 1),
            ))
            .map_err(image_error)?;
    }

    Ok(())
}

fn encode_frames_output(
    capture_dir: &Path,
    output_path: &Path,
    codec: Option<&str>,
) -> OpenPageResult<()> {
    let suffix = output_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if suffix.eq_ignore_ascii_case("gif") {
        return encode_frames_as_gif(capture_dir, output_path);
    }
    if suffix.eq_ignore_ascii_case("mp4") {
        return encode_frames_as_mp4(capture_dir, output_path, codec);
    }
    Err(OpenPageError::UnsupportedOperation(
        unsupported_screencast_output_suffix_message(suffix),
    ))
}

fn encode_frames_as_mp4(
    capture_dir: &Path,
    output_path: &Path,
    codec: Option<&str>,
) -> OpenPageResult<()> {
    let frame_paths = collect_frame_paths(capture_dir)?;
    if frame_paths.is_empty() {
        return Err(OpenPageError::BrowserOperation(
            screencast_no_frames_message(),
        ));
    }

    let codec = codec
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("libx264");
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-framerate")
        .arg("25")
        .arg("-i")
        .arg("frame_%06d.jpg")
        .arg("-vf")
        .arg("pad=ceil(iw/2)*2:ceil(ih/2)*2")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:v")
        .arg(codec)
        .arg(output_path)
        .current_dir(capture_dir)
        .status()
        .map_err(|err| {
            OpenPageError::BrowserOperation(screencast_ffmpeg_spawn_failed_message(
                &err.to_string(),
            ))
        })?;

    if !status.success() {
        return Err(OpenPageError::BrowserOperation(
            screencast_ffmpeg_encode_failed_message(&status.to_string()),
        ));
    }

    Ok(())
}

fn collect_frame_paths(capture_dir: &Path) -> OpenPageResult<Vec<PathBuf>> {
    let mut frames = Vec::new();
    for entry in fs::read_dir(capture_dir)? {
        let path = entry?.path();
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if matches!(ext, "jpg" | "jpeg" | "JPG" | "JPEG") {
            frames.push(path);
        }
    }
    frames.sort();
    Ok(frames)
}

fn decode_data_url(data_url: &str) -> OpenPageResult<Vec<u8>> {
    let (_, payload) = data_url
        .split_once(',')
        .ok_or_else(|| OpenPageError::Serialization(invalid_screencast_data_url_message()))?;
    BASE64_STANDARD
        .decode(payload)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))
}

fn cleanup_temp_dir(capture_dir: &Path, output_dir: &Path) {
    if capture_dir != output_dir {
        let _ = fs::remove_dir_all(capture_dir);
    }
}

fn prepare_temp_output_dir() -> OpenPageResult<PathBuf> {
    let path = env::temp_dir().join("openpage").join(format!(
        "screencast_tmp_{}_{}",
        timestamp_nanos(),
        std::process::id()
    ));
    prepare_output_dir(&path)
}

fn value_as_string(value: Value, label: &str) -> OpenPageResult<String> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(OpenPageError::Serialization(format!(
            "expected string for {label}, got {other}"
        ))),
    }
}

fn image_error(err: impl std::fmt::Display) -> OpenPageError {
    OpenPageError::BrowserOperation(screencast_encode_output_failed_message(&err.to_string()))
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos()
}

fn lock_state(
    shared: &Arc<ScreencastShared>,
) -> OpenPageResult<std::sync::MutexGuard<'_, ScreencastState>> {
    shared.state.lock().map_err(|_| {
        OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
            "screencast state",
            "录屏状态",
        ))
    })
}

fn is_running(shared: &Arc<ScreencastShared>) -> OpenPageResult<bool> {
    Ok(lock_state(shared)?.running)
}

fn finish_screencast(shared: &Arc<ScreencastShared>, error: Option<String>) {
    if let Ok(mut state) = shared.state.lock() {
        state.running = false;
        if error.is_some() {
            state.last_error = error;
        }
    }
}

fn wait_for_task(runtime: &Arc<Runtime>, handle: Option<JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = runtime.block_on(async { handle.await });
    }
}

fn prepare_output_dir(path: &Path) -> OpenPageResult<PathBuf> {
    if path.exists() && path.is_file() {
        return Err(OpenPageError::BrowserOperation(
            screencast_save_path_must_be_directory_message(),
        ));
    }
    fs::create_dir_all(path)?;
    path.canonicalize().or_else(|_| Ok(path.to_path_buf()))
}

fn frame_output_path(output_dir: &Path, index: u64) -> PathBuf {
    output_dir.join(format!("frame_{index:06}.jpg"))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ScreencastMode, build_video_output_path, decode_data_url, encode_frames_output,
        frame_output_path, image_error, prepare_output_dir,
    };
    use crate::settings::{Settings, scoped_test_settings};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        env::temp_dir().join(format!(
            "openpage-rust-{label}-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn prepare_output_dir_creates_directory() {
        let path = temp_path("screencast-dir");
        let resolved = prepare_output_dir(&path).expect("create dir");

        assert!(resolved.exists());
        assert!(resolved.is_dir());

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn prepare_output_dir_rejects_file_path() {
        let _guard = scoped_test_settings();
        Settings::reset();
        let path = temp_path("screencast-file");
        fs::write(&path, b"test").expect("write file");

        let error = prepare_output_dir(&path).expect_err("file path should fail");
        assert!(error.to_string().contains("directory"));

        Settings::set_language("cn");
        let error = prepare_output_dir(&path).expect_err("file path should fail");
        assert!(error.to_string().contains("目录"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn image_error_follows_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = image_error("png failed").to_string();
        assert_eq!(
            english,
            "browser operation failed: failed to encode screencast output: png failed"
        );

        Settings::set_language("cn");

        let chinese = image_error("png failed").to_string();
        assert_eq!(chinese, "浏览器操作失败: 编码录屏输出失败: png failed");
    }

    #[test]
    fn frame_output_path_uses_zero_padded_index() {
        let path = frame_output_path(Path::new("/tmp/demo"), 42);
        assert_eq!(path, Path::new("/tmp/demo/frame_000042.jpg"));
    }

    #[test]
    fn video_mode_defaults_to_mp4_output() {
        let output = build_video_output_path(
            ScreencastMode::Video,
            Path::new("/tmp/demo"),
            Some("capture"),
            None,
        )
        .expect("build output path");
        assert_eq!(output, Path::new("/tmp/demo/capture.mp4"));
    }

    #[test]
    fn js_video_mode_defaults_to_webm_output() {
        let output = build_video_output_path(
            ScreencastMode::JsVideo,
            Path::new("/tmp/demo"),
            Some("capture"),
            None,
        )
        .expect("build output path");
        assert_eq!(output, Path::new("/tmp/demo/capture.webm"));
    }

    #[test]
    fn gif_suffix_is_supported_for_frame_video_modes() {
        let output = build_video_output_path(
            ScreencastMode::Video,
            Path::new("/tmp/demo"),
            Some("capture"),
            Some("gif"),
        )
        .expect("gif should be supported");
        assert_eq!(output, Path::new("/tmp/demo/capture.gif"));
    }

    #[test]
    fn mp4_suffix_is_supported_for_frame_video_modes() {
        let output = build_video_output_path(
            ScreencastMode::Video,
            Path::new("/tmp/demo"),
            Some("capture"),
            Some("mp4"),
        )
        .expect("mp4 should be supported");
        assert_eq!(output, Path::new("/tmp/demo/capture.mp4"));
    }

    #[test]
    fn encode_frames_output_rejects_unknown_suffix() {
        let _guard = scoped_test_settings();
        Settings::reset();
        let capture_dir = temp_path("screencast-capture");
        fs::create_dir_all(&capture_dir).expect("create capture dir");

        let error = encode_frames_output(&capture_dir, Path::new("/tmp/demo/capture.avi"), None)
            .expect_err("unknown suffix should fail");
        assert!(
            error
                .to_string()
                .contains("unsupported screencast output suffix")
        );

        Settings::set_language("cn");
        let error = encode_frames_output(&capture_dir, Path::new("/tmp/demo/capture.avi"), None)
            .expect_err("unknown suffix should fail");
        assert!(error.to_string().contains("不支持的录屏输出后缀"));

        let _ = fs::remove_dir_all(&capture_dir);
    }

    #[test]
    fn build_video_output_path_localizes_mode_suffix_error() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let error = build_video_output_path(
            ScreencastMode::JsVideo,
            Path::new("/tmp/demo"),
            Some("capture"),
            Some("mp4"),
        )
        .expect_err("mp4 should not be supported for js video");
        assert!(error.to_string().contains("only supports .webm output"));

        Settings::set_language("cn");
        let error = build_video_output_path(
            ScreencastMode::JsVideo,
            Path::new("/tmp/demo"),
            Some("capture"),
            Some("mp4"),
        )
        .expect_err("mp4 should not be supported for js video");
        assert!(error.to_string().contains("仅支持 .webm 输出"));
    }

    #[test]
    fn encode_frames_output_localizes_missing_frames_error() {
        let _guard = scoped_test_settings();
        Settings::reset();
        let capture_dir = temp_path("screencast-no-frames");
        fs::create_dir_all(&capture_dir).expect("create capture dir");

        let error = encode_frames_output(&capture_dir, Path::new("/tmp/demo/capture.mp4"), None)
            .expect_err("empty capture dir should fail");
        assert!(error.to_string().contains("did not capture any frames"));

        Settings::set_language("cn");
        let error = encode_frames_output(&capture_dir, Path::new("/tmp/demo/capture.mp4"), None)
            .expect_err("empty capture dir should fail");
        assert!(error.to_string().contains("没有捕获到任何帧"));

        let _ = fs::remove_dir_all(&capture_dir);
    }

    #[test]
    fn decode_data_url_returns_payload_bytes() {
        let data = decode_data_url("data:video/webm;base64,aGVsbG8=").expect("decode data url");
        assert_eq!(data, b"hello");
    }

    #[test]
    fn decode_data_url_localizes_invalid_data_error() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let error = decode_data_url("bad-data").expect_err("invalid data url should fail");
        assert!(error.to_string().contains("invalid screencast data URL"));

        Settings::set_language("cn");
        let error = decode_data_url("bad-data").expect_err("invalid data url should fail");
        assert!(error.to_string().contains("无效的录屏 data URL"));
    }
}
