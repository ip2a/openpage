use crate::WebFrame;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, ExecutionContextId};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use url::Url;

use super::{
    NavigationShared, NavigationTracker, Page, PageElementContent, PageElementInfo,
    PageNavigationSnapshot, PageSaveContent, action_drag_payload,
    browser_cookie_param_from_session_cookie, compose_frame_html, cookie_domain_candidates_for_url,
    cookie_param, default_frame_locator, delete_cookie_params, frame_locator, frame_locator_input,
    history_entry_index, is_explicit_locator, marker_xpath, optional_frame_locator_input,
    page_element_info_properties_json, page_operation_error, permission_origin_from_input,
    register_navigation_listener_with_cdp_timeout, remaining_timeout_ms,
    resolve_implicit_wait_timeout_ms, resolve_navigation_local_file_path,
    resolve_page_save_target_path, resolve_page_screenshot_target_path, resolve_permission_origin,
    run_page_future_with_cdp_timeout, run_page_lookup_future_with_cdp_timeout, run_with_timeout,
    runtime_timeout_seconds_to_millis, screenshot_clip, storage_lookup_script, value_as_f64_pair,
    value_as_optional_string, value_as_string, value_as_string_vec,
};
use crate::element_list::ElementsListExt;
use crate::error::OpenPageError;
use crate::session::SessionCookieParam;
use crate::settings::{
    cdp_timeout_duration, javascript_execution_timed_out_message, scoped_test_settings,
    timeout_duration_millis,
};
use crate::{
    Browser, BrowserTabReference, BrowserTabSelector, By, DisconnectedFrame, DisconnectedPage,
    Frame, Keys, LaunchOptions, OpenPageResult, Settings, WebElement, wait_until,
};

fn runtime_test_temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("openpage-{name}-{}-{unique}", std::process::id()))
}

fn poison_mutex<T: Send + 'static>(mutex: Arc<std::sync::Mutex<T>>) {
    let join = thread::spawn(move || {
        let _guard = mutex.lock().expect("lock poisoned test mutex");
        panic!("poison mutex");
    })
    .join();
    assert!(join.is_err(), "poison helper thread should panic");
}

fn launch_headless_test_browser_with_args(
    name: &str,
    extra_args: &[&str],
) -> crate::OpenPageResult<(Browser, PathBuf)> {
    let temp_dir = runtime_test_temp_dir(name);
    fs::create_dir_all(&temp_dir).expect("create runtime test temp dir");

    let mut options = LaunchOptions::default();
    options.headless(true);
    options.auto_port(true);
    options.new_env(true);
    options.set_tmp_path(&temp_dir);
    options.set_timeouts(Some(1.0), Some(5.0), Some(1.0));
    for arg in extra_args {
        options.set_argument(*arg);
    }

    Browser::launch(options).map(|browser| (browser, temp_dir))
}

fn launch_headless_test_browser(name: &str) -> crate::OpenPageResult<(Browser, PathBuf)> {
    launch_headless_test_browser_with_args(name, &[])
}

fn pair_from_value(value: Value, label: &str) -> crate::OpenPageResult<(f64, f64)> {
    let values = match value {
        Value::Array(values) => values,
        other => {
            return Err(OpenPageError::PageOperation(format!(
                "{label} did not return an array: {other}"
            )));
        }
    };
    if values.len() != 2 {
        return Err(OpenPageError::PageOperation(format!(
            "{label} did not return exactly two values"
        )));
    }
    let x = values[0].as_f64().ok_or_else(|| {
        OpenPageError::PageOperation(format!("{label} x was not numeric: {}", values[0]))
    })?;
    let y = values[1].as_f64().ok_or_else(|| {
        OpenPageError::PageOperation(format!("{label} y was not numeric: {}", values[1]))
    })?;
    Ok((x, y))
}

fn expected_dp_viewport_screen_origin(
    page: &super::Page,
) -> crate::OpenPageResult<(f64, f64, f64)> {
    let window_state = page.window_state()?;
    let (window_left, window_top) = page.window_location()?;
    let (window_width, window_height) = page.window_size()?;
    let (viewport_width, viewport_height) = pair_from_value(
        page.run_js("[window.innerWidth, window.innerHeight]")?,
        "top window viewport size with scrollbar",
    )?;
    let device_pixel_ratio = page
        .run_js("window.devicePixelRatio || 1")?
        .as_f64()
        .ok_or_else(|| {
            OpenPageError::PageOperation("devicePixelRatio was not numeric".to_string())
        })?;

    let (window_left, window_top) = if matches!(window_state.as_str(), "maximized" | "fullscreen") {
        (0.0, 0.0)
    } else {
        (window_left as f64 + 7.0, window_top as f64)
    };

    let (window_width, window_height) = match window_state.as_str() {
        "fullscreen" => (window_width as f64, window_height as f64),
        "maximized" => (window_width as f64 - 16.0, window_height as f64 - 16.0),
        _ => (window_width as f64 - 16.0, window_height as f64 - 7.0),
    };

    Ok((
        window_left + window_width - viewport_width,
        window_top + window_height - viewport_height,
        device_pixel_ratio,
    ))
}

fn assert_pair_close(actual: (f64, f64), expected: (f64, f64), label: &str) {
    assert!(
        (actual.0 - expected.0).abs() < 1.0,
        "{label} x mismatch: actual={:?}, expected={:?}",
        actual,
        expected
    );
    assert!(
        (actual.1 - expected.1).abs() < 1.0,
        "{label} y mismatch: actual={:?}, expected={:?}",
        actual,
        expected
    );
}

fn spawn_download_site() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind download server");
    listener
        .set_nonblocking(true)
        .expect("set download server nonblocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("download server addr")
    );
    let handle = thread::spawn(move || {
        let html = r#"<!doctype html>
<html>
<body>
  <a id="download" href="/download">Download</a>
</body>
</html>
"#;
        let payload = b"openpage-download";
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut served_download = false;
        while Instant::now() < deadline && !served_download {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                "/download" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"openpage.txt\"\r\nConnection: close\r\n\r\n",
                        payload.len()
                    );
                    let _ = stream.write_all(payload);
                    served_download = true;
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });
    (address, handle)
}

fn spawn_cookie_site() -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind cookie server");
    listener
        .set_nonblocking(true)
        .expect("set cookie server nonblocking");
    let port = listener.local_addr().expect("cookie server addr").port();
    let handle = thread::spawn(move || {
        let html = "<!doctype html><html><body id=\"root\">cookie</body></html>";
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut served_html = false;
        while Instant::now() < deadline && !served_html {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                    served_html = true;
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });
    (port, handle)
}

fn spawn_delayed_load_site(delay: Duration) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind delayed load server");
    listener
        .set_nonblocking(true)
        .expect("set delayed load server nonblocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("delayed load server addr")
    );
    let handle = thread::spawn(move || {
        let html = r#"<!doctype html>
<html>
<head>
  <script defer src="/slow.js"></script>
</head>
<body data-ready="pending">
  <div id="status">pending</div>
</body>
</html>
"#;
        let script = "document.body.dataset.ready = 'loaded'; document.getElementById('status').textContent = 'loaded'; window.__delayedScriptLoaded = true;";
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut served_html = false;
        let mut served_script = false;
        while Instant::now() < deadline && !(served_html && served_script) {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                    served_html = true;
                }
                "/slow.js" => {
                    thread::sleep(delay);
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/javascript; charset=utf-8\r\nConnection: close\r\n\r\n{script}",
                        script.len()
                    );
                    served_script = true;
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });
    (address, handle)
}

fn spawn_cross_origin_iframe_site() -> (String, thread::JoinHandle<()>, thread::JoinHandle<()>) {
    let child_listener = TcpListener::bind("127.0.0.1:0").expect("bind child iframe server");
    child_listener
        .set_nonblocking(true)
        .expect("set child iframe server nonblocking");
    let child_address = format!(
        "http://{}",
        child_listener
            .local_addr()
            .expect("child iframe server addr")
    );

    let parent_listener = TcpListener::bind("127.0.0.1:0").expect("bind parent iframe server");
    parent_listener
        .set_nonblocking(true)
        .expect("set parent iframe server nonblocking");
    let parent_address = format!(
        "http://{}",
        parent_listener
            .local_addr()
            .expect("parent iframe server addr")
    );

    let child_url = format!("{child_address}/child");
    let child_handle = thread::spawn(move || {
        let html = r#"<!doctype html>
<html>
<head><title>Cross Origin Child</title></head>
<body style="margin:0;height:1600px;">
  <div
    id="inner-box"
    style="position:absolute;left:56px;top:88px;width:96px;height:58px;border:3px solid #111;padding:5px;background:#eee;"
  ></div>
</body>
</html>
"#;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (mut stream, _) = match child_listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/child" | "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    let parent_handle = thread::spawn(move || {
        let html = format!(
            r#"<!doctype html>
<html>
<body style="margin:0;">
  <iframe
    id="cross-frame"
    style="position:absolute;left:170px;top:110px;width:430px;height:280px;border:0;"
    src="{child_url}"
  ></iframe>
</body>
</html>
"#
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (mut stream, _) = match parent_listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/parent" | "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    (
        format!("{parent_address}/parent"),
        parent_handle,
        child_handle,
    )
}

fn spawn_nested_cross_origin_iframe_site() -> (
    String,
    thread::JoinHandle<()>,
    thread::JoinHandle<()>,
    thread::JoinHandle<()>,
) {
    let grandchild_listener =
        TcpListener::bind("127.0.0.1:0").expect("bind grandchild iframe server");
    grandchild_listener
        .set_nonblocking(true)
        .expect("set grandchild iframe server nonblocking");
    let grandchild_address = format!(
        "http://{}",
        grandchild_listener
            .local_addr()
            .expect("grandchild iframe server addr")
    );
    let grandchild_url = format!("{grandchild_address}/grandchild");

    let child_listener = TcpListener::bind("127.0.0.1:0").expect("bind nested child server");
    child_listener
        .set_nonblocking(true)
        .expect("set nested child server nonblocking");
    let child_address = format!(
        "http://{}",
        child_listener
            .local_addr()
            .expect("nested child server addr")
    );
    let child_url = format!("{child_address}/child");

    let parent_listener = TcpListener::bind("127.0.0.1:0").expect("bind nested parent server");
    parent_listener
        .set_nonblocking(true)
        .expect("set nested parent server nonblocking");
    let parent_address = format!(
        "http://{}",
        parent_listener
            .local_addr()
            .expect("nested parent server addr")
    );

    let grandchild_handle = thread::spawn(move || {
        let html = r#"<!doctype html>
<html>
<head><title>Nested Cross Origin Grandchild</title></head>
<body style="margin:0;height:1400px;">
  <div
    id="deep-box"
    style="position:absolute;left:44px;top:70px;width:88px;height:52px;border:3px solid #111;padding:5px;background:#eee;"
  ></div>
</body>
</html>
"#;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (mut stream, _) = match grandchild_listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/grandchild" | "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    let child_handle = thread::spawn(move || {
        let html = format!(
            r#"<!doctype html>
<html>
<head><title>Nested Cross Origin Child</title></head>
<body style="margin:0;">
  <iframe
    id="inner-frame"
    style="position:absolute;left:90px;top:60px;width:240px;height:180px;border:0;"
    src="{grandchild_url}"
  ></iframe>
</body>
</html>
"#
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (mut stream, _) = match child_listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/child" | "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    let parent_handle = thread::spawn(move || {
        let html = format!(
            r#"<!doctype html>
<html>
<body style="margin:0;">
  <iframe
    id="outer-frame"
    style="position:absolute;left:170px;top:110px;width:430px;height:280px;border:0;"
    src="{child_url}"
  ></iframe>
</body>
</html>
"#
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (mut stream, _) = match parent_listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/parent" | "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    (
        format!("{parent_address}/parent"),
        parent_handle,
        child_handle,
        grandchild_handle,
    )
}

#[test]
fn page_operation_errors_follow_settings_language() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let english = page_operation_error("read title", "boom").to_string();
    assert_eq!(
        english,
        "page operation failed: page operation read title failed: boom"
    );

    Settings::set_language("cn");

    let chinese = page_operation_error("read title", "boom").to_string();
    assert_eq!(chinese, "页面操作失败: 页面操作 read title 失败: boom");
}

#[test]
fn history_entry_index_moves_backward() {
    assert_eq!(history_entry_index(3, 5, -2), Some(1));
}

#[test]
fn history_entry_index_returns_none_when_offset_leaves_bounds() {
    assert_eq!(history_entry_index(0, 5, -1), None);
    assert_eq!(history_entry_index(4, 5, 1), None);
}

#[test]
fn storage_lookup_script_returns_item_lookup() {
    let script = storage_lookup_script("sessionStorage", Some("token")).expect("script");
    assert!(script.contains("sessionStorage.getItem"));
    assert!(script.contains(&json!("token").to_string()));
}

#[test]
fn storage_lookup_script_returns_full_dump() {
    let script = storage_lookup_script("localStorage", None).expect("script");
    assert!(script.contains("localStorage.length"));
    assert!(script.contains("return result"));
}

#[test]
fn frame_locator_uses_name_or_id_lookup_for_plain_strings() {
    assert_eq!(
        frame_locator("demo-frame"),
        r#"xpath://*[(name()="iframe" or name()="frame") and (@name="demo-frame" or @id="demo-frame")]"#
    );
}

#[test]
fn frame_locator_keeps_explicit_locators() {
    assert!(is_explicit_locator("css:iframe.demo"));
    assert_eq!(frame_locator("css:iframe.demo"), "css:iframe.demo");
    assert_eq!(
        default_frame_locator(),
        r#"xpath://*[name()="iframe" or name()="frame"]"#
    );
}

#[test]
fn frame_locator_input_accepts_by_tuples() {
    assert_eq!(
        frame_locator_input((By::ID, "demo-frame")).expect("by id frame locator"),
        "@id=demo-frame"
    );
    assert_eq!(
        optional_frame_locator_input(Some((By::TAG_NAME, "iframe"))).expect("by tag frame locator"),
        "tag:iframe"
    );
    assert_eq!(
        optional_frame_locator_input(None::<&str>).expect("default frame locator"),
        default_frame_locator()
    );
}

#[test]
fn marker_xpath_targets_global_marker_attribute() {
    assert_eq!(
        marker_xpath("openpage-page-1"),
        r#"xpath://*[@data-openpage-page-marker="openpage-page-1"]"#
    );
}

#[test]
fn page_element_info_accepts_pairs_and_maps() {
    let pair_items = [
        ("innerText", "DrissionPage"),
        ("href", "https://drissionpage.cn"),
    ];
    let string_items = vec![
        ("innerText".to_string(), "OpenPage".to_string()),
        ("target".to_string(), "_blank".to_string()),
    ];
    let mut map_items = HashMap::new();
    map_items.insert("innerText".to_string(), "Detached".to_string());
    map_items.insert("href".to_string(), "https://example.test".to_string());

    let from_pairs = PageElementInfo::from(("a", &pair_items));
    let from_strings = PageElementInfo::from(("a", &string_items));
    let from_map = PageElementInfo::from(("a", &map_items));

    assert_eq!(from_pairs.tag(), "a");
    assert_eq!(from_pairs.properties.len(), 2);
    assert_eq!(from_strings.properties.len(), 2);
    assert_eq!(from_map.tag(), "a");
    assert_eq!(from_map.properties.len(), 2);
}

#[test]
fn page_element_info_accepts_json_value_maps() {
    let pair_items = [("tabIndex", json!(3)), ("hidden", json!(true))];
    let value_items = vec![
        ("innerText".to_string(), json!("OpenPage")),
        ("draggable".to_string(), json!(false)),
    ];
    let mut map_items = HashMap::new();
    map_items.insert("value".to_string(), json!(12));
    map_items.insert("disabled".to_string(), Value::Bool(true));

    let from_pairs = PageElementInfo::from(("button", &pair_items));
    let from_values = PageElementInfo::from(("button", &value_items));
    let from_map = PageElementInfo::from(("button", &map_items));

    assert_eq!(from_pairs.properties.len(), 2);
    assert!(
        from_pairs
            .properties
            .iter()
            .any(|(name, value)| { name == "tabIndex" && value == &json!(3) })
    );
    assert!(
        from_values
            .properties
            .iter()
            .any(|(name, value)| { name == "draggable" && value == &json!(false) })
    );
    assert!(
        from_map
            .properties
            .iter()
            .any(|(name, value)| { name == "disabled" && value == &Value::Bool(true) })
    );
}

#[test]
fn page_element_info_properties_json_serializes_json_scalars() {
    let info = PageElementInfo::from((
        "button",
        [
            ("innerText", json!("DrissionPage")),
            ("tabIndex", json!(3)),
            ("disabled", Value::Bool(false)),
        ],
    ));
    let properties = page_element_info_properties_json(&info).expect("properties json");

    assert!(properties.contains(r#""innerText":"DrissionPage""#));
    assert!(properties.contains(r#""tabIndex":3"#));
    assert!(properties.contains(r#""disabled":false"#));
}

#[test]
fn page_element_content_accepts_html_and_info_inputs() {
    let html = PageElementContent::from("<button>demo</button>");
    let info = PageElementContent::from(("button", [("disabled", json!(true))]));

    match html {
        PageElementContent::Html(value) => {
            assert_eq!(value.as_ref(), "<button>demo</button>");
        }
        other => panic!("expected html content, got {other:?}"),
    }

    match info {
        PageElementContent::Info(info) => {
            assert_eq!(info.tag(), "button");
            assert_eq!(info.properties.len(), 1);
        }
        other => panic!("expected info content, got {other:?}"),
    }
}

#[test]
fn compose_frame_html_reuses_opening_tag() {
    assert_eq!(
        compose_frame_html(
            "iframe",
            r#"<iframe id="demo"></iframe>"#,
            "<html>inner</html>"
        ),
        r#"<iframe id="demo"><html>inner</html></iframe>"#
    );
}

#[test]
fn remaining_timeout_ms_clamps_elapsed_deadlines() {
    let expired = Instant::now()
        .checked_sub(Duration::from_millis(10))
        .expect("expired instant");
    let future = Instant::now() + Duration::from_millis(50);

    assert_eq!(remaining_timeout_ms(expired), 0);
    assert!(remaining_timeout_ms(future) <= 50);
}

#[test]
fn resolve_implicit_wait_timeout_ms_prefers_configured_value() {
    assert_eq!(resolve_implicit_wait_timeout_ms(Some(2500)), 2500);
    assert_eq!(resolve_implicit_wait_timeout_ms(Some(0)), 0);
    assert_eq!(resolve_implicit_wait_timeout_ms(None), 10_000);
}

#[test]
fn run_with_timeout_times_out_slow_future() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let error = runtime
        .block_on(run_with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok::<_, OpenPageError>(())
            },
            1,
            "javascript execution timed out",
        ))
        .expect_err("future should time out");

    assert!(error.to_string().contains("javascript execution timed out"));
}

#[test]
fn run_with_timeout_accepts_localized_timeout_message() {
    let _settings = scoped_test_settings();
    Settings::set_language("cn");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let error = runtime
        .block_on(run_with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok::<_, OpenPageError>(())
            },
            1,
            javascript_execution_timed_out_message(),
        ))
        .expect_err("future should time out");

    assert!(error.to_string().contains("JavaScript 执行超时"));
}

#[test]
fn screenshot_clip_requires_complete_bounds() {
    assert!(screenshot_clip(None, None).expect("no clip").is_none());
    assert!(screenshot_clip(Some((0.0, 0.0)), None).is_err());
    assert!(
        screenshot_clip(Some((0.0, 0.0)), Some((10.0, 10.0)))
            .expect("clip")
            .is_some()
    );
}

#[test]
fn resolve_page_screenshot_target_path_defaults_to_title() {
    let path = resolve_page_screenshot_target_path(None, None, Some("Open:Page")).expect("path");
    assert!(path.is_absolute());
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some("Open_Page.png")
    );
}

#[test]
fn resolve_page_save_target_path_defaults_to_title_and_extension() {
    let path =
        resolve_page_save_target_path(None, None, Some("Open:Page"), "mhtml").expect("save path");
    assert!(path.is_absolute());
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some("Open_Page.mhtml")
    );
}

#[test]
fn cookie_param_keeps_optional_scope_fields() {
    let cookie = cookie_param(
        "foo",
        "bar",
        Some("https://example.com/demo"),
        Some("example.com"),
        Some("/demo"),
    );
    assert_eq!(cookie.name, "foo");
    assert_eq!(cookie.value, "bar");
    assert_eq!(cookie.url.as_deref(), Some("https://example.com/demo"));
    assert_eq!(cookie.domain.as_deref(), Some("example.com"));
    assert_eq!(cookie.path.as_deref(), Some("/demo"));
}

#[test]
fn delete_cookie_params_skip_blank_scope_fields() {
    let params = delete_cookie_params("foo", Some(" "), Some("example.com"), Some(""));
    assert_eq!(params.name, "foo");
    assert!(params.url.is_none());
    assert_eq!(params.domain.as_deref(), Some("example.com"));
    assert!(params.path.is_none());
}

#[test]
fn cookie_domain_candidates_follow_configured_suffix_list() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let temp_dir = runtime_test_temp_dir("suffixes-list");
    fs::create_dir_all(&temp_dir).expect("create suffixes temp dir");
    let suffix_path = temp_dir.join("suffixes.dat");
    fs::write(&suffix_path, "// BEGIN ICANN DOMAINS\nwild.test\nco.uk\n")
        .expect("write custom suffix list");
    Settings::set_suffixes_list(&suffix_path);

    let url =
        Url::parse("https://www.example.wild.test/path").expect("parse custom suffix list url");
    assert_eq!(
        cookie_domain_candidates_for_url(&url),
        vec![
            "www.example.wild.test".to_string(),
            ".example.wild.test".to_string(),
            "example.wild.test".to_string(),
        ]
    );

    let uk_url = Url::parse("https://shop.service.example.co.uk/path").expect("parse co.uk url");
    assert_eq!(
        cookie_domain_candidates_for_url(&uk_url),
        vec![
            "shop.service.example.co.uk".to_string(),
            ".service.example.co.uk".to_string(),
            "service.example.co.uk".to_string(),
            ".example.co.uk".to_string(),
            "example.co.uk".to_string(),
        ]
    );
}

#[test]
fn cookie_same_site_validation_follows_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let cookie = SessionCookieParam {
        name: "sid".to_string(),
        value: "1".to_string(),
        url: Some("https://example.test/".to_string()),
        domain: None,
        path: None,
        secure: false,
        http_only: false,
        same_site: Some("Broken".to_string()),
    };

    let english = browser_cookie_param_from_session_cookie(&cookie)
        .expect_err("english same_site validation should fail");
    assert!(matches!(
        english,
        OpenPageError::PageOperation(ref message)
            if message.contains("invalid cookie same_site `Broken` for `sid`")
    ));

    Settings::set_language("cn");

    let chinese = browser_cookie_param_from_session_cookie(&cookie)
        .expect_err("chinese same_site validation should fail");
    assert!(matches!(
        chinese,
        OpenPageError::PageOperation(ref message)
            if message.contains("cookie `sid` 的 same_site `Broken` 无效")
    ));
}

#[test]
fn page_navigation_validation_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let english_file = resolve_navigation_local_file_path("file://example.com/path")
        .expect_err("english file url validation should fail");
    assert!(matches!(
        english_file,
        OpenPageError::PageOperation(ref message)
            if message.contains("invalid file url: file://example.com/path")
    ));

    let english_timeout = runtime_timeout_seconds_to_millis(f64::NAN)
        .expect_err("english timeout validation should fail");
    assert!(matches!(
        english_timeout,
        OpenPageError::PageOperation(ref message)
            if message.contains("timeout must be a finite non-negative number")
    ));

    Settings::set_language("cn");

    let chinese_file = resolve_navigation_local_file_path("file://example.com/path")
        .expect_err("chinese file url validation should fail");
    assert!(matches!(
        chinese_file,
        OpenPageError::PageOperation(ref message)
            if message.contains("无效的 file url: file://example.com/path")
    ));

    let chinese_timeout = runtime_timeout_seconds_to_millis(f64::NAN)
        .expect_err("chinese timeout validation should fail");
    assert!(matches!(
        chinese_timeout,
        OpenPageError::PageOperation(ref message)
            if message.contains("timeout 必须是有限且非负的数字")
    ));
}

#[test]
fn page_host_validation_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let english_drag = action_drag_payload(crate::ActionsDragData::files(Vec::<String>::new()))
        .expect_err("empty drag file list should fail");
    assert!(matches!(
        english_drag,
        OpenPageError::PageOperation(ref message)
            if message.contains("drag_in() requires at least one file path")
    ));

    let english_clip = screenshot_clip(Some((10.0, 10.0)), Some((5.0, 20.0)))
        .expect_err("invalid screenshot clip order should fail");
    assert!(matches!(
        english_clip,
        OpenPageError::PageOperation(ref message)
            if message.contains(
                "screenshot clip requires right_bottom to be greater than left_top"
            )
    ));

    let english_origin = permission_origin_from_input("ftp://example.test")
        .expect_err("permission origin scheme should fail");
    assert!(matches!(
        english_origin,
        OpenPageError::BrowserOperation(ref message)
            if message.contains("permission origin must use http or https")
    ));

    Settings::set_language("cn");

    let chinese_drag = action_drag_payload(crate::ActionsDragData::files(vec![""]))
        .expect_err("empty drag file path should fail");
    assert!(matches!(
        chinese_drag,
        OpenPageError::PageOperation(ref message)
            if message.contains("drag_in() 文件路径不能为空")
    ));

    let chinese_clip =
        screenshot_clip(Some((10.0, 10.0)), None).expect_err("partial screenshot clip should fail");
    assert!(matches!(
        chinese_clip,
        OpenPageError::PageOperation(ref message)
            if message.contains("截图裁剪需要同时提供 left_top 和 right_bottom")
    ));

    let chinese_origin = permission_origin_from_input("ftp://example.test")
        .expect_err("permission origin scheme should localize");
    assert!(matches!(
        chinese_origin,
        OpenPageError::BrowserOperation(ref message)
            if message.contains("permission origin 必须使用 http 或 https")
    ));
}

#[test]
fn page_value_type_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let english_string =
        value_as_string(Value::Null, "demo").expect_err("string value conversion should fail");
    assert!(matches!(
        english_string,
        OpenPageError::JavaScript(ref message)
            if message.contains("demo did not return a string: null")
    ));

    let english_entry = value_as_string_vec(json!(["ok", 1]), "demo")
        .expect_err("string vector entry conversion should fail");
    assert!(matches!(
        english_entry,
        OpenPageError::JavaScript(ref message)
            if message.contains("demo returned a non-string entry: 1")
    ));

    Settings::set_language("cn");

    let chinese_optional = value_as_optional_string(json!(1), "demo")
        .expect_err("optional string value conversion should localize");
    assert!(matches!(
        chinese_optional,
        OpenPageError::JavaScript(ref message)
            if message.contains("demo 未返回字符串或 null: 1")
    ));

    let chinese_pair =
        value_as_f64_pair(json!([1, "x"]), "demo").expect_err("pair conversion should fail");
    assert!(matches!(
        chinese_pair,
        OpenPageError::JavaScript(ref message)
            if message.contains("demo second 条目不是数字")
    ));
}

#[test]
fn browser_backed_page_method_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    assert_eq!(
        super::browser_backed_page_method_message("tabs_count"),
        "tabs_count() is only available on browser-backed pages"
    );

    Settings::set_language("cn");

    assert_eq!(
        super::browser_backed_page_method_message("tabs_count"),
        "tabs_count() 仅适用于 browser-backed 页面"
    );
}

#[test]
fn page_browser_backed_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) =
        launch_headless_test_browser("page-browser-backed-l10n").expect("launch headless browser");

    let result = (|| -> OpenPageResult<()> {
        let page = browser.new_page(None)?;
        let english_zoom = page
            .set_zoom_factor(0.0)
            .expect_err("invalid zoom factor should fail");
        assert!(matches!(
            english_zoom,
            OpenPageError::BrowserOperation(ref message)
                if message.contains("zoom factor must be a positive finite number")
        ));
        let mut english_actions = page.new_actions();
        let english_action = match english_actions.wait(-0.1, None) {
            Err(error) => error,
            Ok(_) => panic!("negative action wait should fail"),
        };
        assert!(matches!(
            english_action,
            OpenPageError::PageOperation(ref message)
                if message.contains("wait() seconds must be >= 0")
        ));

        let detached = Page {
            browser: None,
            browser_pid: None,
            ..page.clone()
        };

        let english = detached
            .download_file_exists_mode()
            .expect_err("download_file_exists_mode() should require browser backing");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains(
                    "download_file_exists_mode() is only available on browser-backed pages"
            )
        ));
        let english_window = detached
            .window_hide()
            .expect_err("window_hide should require launched browser pid");
        assert!(matches!(
            english_window,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains(
                    "window hide() is only available for launched browser instances"
                )
        ));

        Settings::set_language("cn");

        let chinese_permission = page
            .set_permission(
                "clipboard-read",
                "maybe",
                Some("https://example.test"),
                None,
            )
            .expect_err("invalid permission setting should fail");
        assert!(matches!(
            chinese_permission,
            OpenPageError::BrowserOperation(ref message)
                if message.contains("permission setting 必须是 granted/denied/prompt 之一")
        ));
        let mut chinese_actions = page.new_actions();
        let chinese_type = match chinese_actions.type_with_interval("x", -0.1) {
            Err(error) => error,
            Ok(_) => panic!("negative action type interval should fail"),
        };
        assert!(matches!(
            chinese_type,
            OpenPageError::PageOperation(ref message)
                if message.contains("type_with_interval() 秒数必须 >= 0")
        ));
        let chinese_click = match chinese_actions.m_click(None::<&str>, 0) {
            Err(error) => error,
            Ok(_) => panic!("zero action click count should fail"),
        };
        assert!(matches!(
            chinese_click,
            OpenPageError::PageOperation(ref message)
                if message.contains("click() 次数必须 >= 1")
        ));
        let chinese_clipboard = page
            .clipboard_read_text()
            .expect_err("about:blank clipboard should require secure context");
        assert!(matches!(
            chinese_clipboard,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains(
                    "clipboard_read_text() 需要 secure-context 页面并支持 navigator.clipboard"
                )
        ));
        let chinese_origin = resolve_permission_origin(None, "about:blank")
            .expect_err("permission origin should require http(s)");
        assert!(matches!(
            chinese_origin,
            OpenPageError::BrowserOperation(ref message)
                if message.contains("permission override 需要 http(s) 页面或显式 --origin")
        ));

        let chinese_retry = detached
            .retry_times()
            .expect_err("retry_times() should require browser backing");
        assert!(matches!(
            chinese_retry,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("retry_times() 仅适用于 browser-backed 页面")
        ));
        let chinese_timeout = detached
            .set_timeouts(Some(1.0), None, None)
            .expect_err("set_timeouts() should require browser backing");
        assert!(matches!(
            chinese_timeout,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("set_timeouts() 仅适用于 browser-backed 页面")
        ));
        let chinese_wait = detached
            .wait_for_downloads_done(10, true)
            .expect_err("wait_for_downloads_done() should require browser backing");
        assert!(matches!(
            chinese_wait,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("wait_for_downloads_done() 仅适用于 browser-backed 页面")
        ));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page browser-backed errors should localize");
}

#[test]
fn page_lock_poisoned_runtime_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) =
        launch_headless_test_browser("page-lock-poisoned-l10n").expect("launch headless browser");

    let result = (|| -> OpenPageResult<()> {
        let page = browser.new_page(None)?;

        poison_mutex(Arc::clone(&page.none_element_config));
        let english = page
            .set_raise_when_ele_not_found(true)
            .expect_err("set_raise_when_ele_not_found() should surface poisoned config")
            .to_string();
        assert!(english.contains("none element runtime config lock poisoned"));

        Settings::set_language("cn");

        poison_mutex(Arc::clone(&page.init_scripts));
        let chinese = page
            .remove_init_js(None)
            .expect_err("remove_init_js(None) should localize poisoned init script state")
            .to_string();
        assert!(chinese.contains("页面初始化脚本锁已损坏"));

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page lock poisoned localization regression");
}

#[test]
fn page_set_cookies_accepts_scope_free_cookie_on_http_page() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (port, server) = spawn_cookie_site();
    let (browser, temp_dir) =
        launch_headless_test_browser("page-cookie-scope").expect("launch headless browser");

    let result = (|| -> OpenPageResult<()> {
        let url = format!("http://localhost:{port}/");
        let page = browser.new_page(Some(url.as_str()))?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        page.set_cookies("sid=abc")?;

        let cookie_header = page.cookie_header()?.unwrap_or_default();
        assert!(
            cookie_header.contains("sid=abc"),
            "cookie header should include sid=abc, got {cookie_header}"
        );
        let cookies = page.cookies()?;
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == "sid" && cookie.value == "abc"),
            "cookie list should include sid=abc, got {cookies:?}"
        );
        Ok(())
    })();

    if let Err(err) = browser.close() {
        panic!("close headless browser: {err}");
    }
    server.join().expect("join cookie server");
    let _ = fs::remove_dir_all(&temp_dir);

    result.expect("page set_cookies scope fallback regression");
}

#[test]
fn page_add_ele_info_returns_detached_element_without_dom_residue() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-add-ele-detached").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js("(() => { document.body.innerHTML = ''; return true; })()")?;

        let marker = format!(
            "detached-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        let info = [
            ("innerText", json!("Detached link")),
            ("title", json!("detached-title")),
            ("data-openpage-detached-test", json!(marker.clone())),
        ];

        let element = page.add_ele(("a", &info), None::<&str>, None::<&str>)?;

        assert_eq!(
            element.attr("data-openpage-detached-test")?,
            Some(marker.clone())
        );
        assert_eq!(
            element.run_js("return this.isConnected;")?,
            Value::Bool(false)
        );
        let selector = format!("css:[data-openpage-detached-test=\"{marker}\"]");
        assert_eq!(page.find_all(&selector)?.len(), 0);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("detached add_ele(info) runtime regression");
}

#[test]
fn page_and_frame_js_helper_wrappers_support_args_async_and_init_scripts() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-frame-js-helpers").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let page_script_path = temp_dir.join("page-run-js.js");
        let page_args_script_path = temp_dir.join("page-run-js-args.js");
        fs::write(&page_script_path, "return 40 + 2;")
            .map_err(|err| OpenPageError::Io(format!("write page js file: {err}")))?;
        fs::write(
            &page_args_script_path,
            "return arguments[0] + arguments[1];",
        )
        .map_err(|err| OpenPageError::Io(format!("write page args js file: {err}")))?;

        assert_eq!(
            page.run_js(page_script_path.to_str().ok_or_else(|| {
                OpenPageError::PageOperation("page script path was not valid utf-8".to_string())
            })?,)?,
            Value::from(42)
        );

        assert_eq!(
            page.run_js_with_args(
                page_args_script_path.to_str().ok_or_else(|| {
                    OpenPageError::PageOperation(
                        "page args script path was not valid utf-8".to_string(),
                    )
                })?,
                &[Value::from(1), Value::from(2)],
                false,
            )?,
            Value::from(3)
        );
        assert_eq!(
            page.run_js_with_options(
                "2 + 3",
                &[Value::from(2), Value::from(3)],
                true,
                Some(1_000),
            )?,
            Value::from(5)
        );
        assert_eq!(page.run_js_loaded("return 20 + 1;")?, Value::from(21));

        page.run_async_js("setTimeout(() => { window.__pageAsync = 'done'; }, 0);")?;
        wait_until(Duration::from_millis(1_500), || {
            match page.run_js("window.__pageAsync || null").ok()? {
                Value::String(value) if value == "done" => Some(()),
                _ => None,
            }
        })?;

        let set_iframe = |text: &str| -> crate::OpenPageResult<()> {
            let srcdoc = serde_json::to_string(&format!(
                "<html><body><button id='inner'>{text}</button></body></html>"
            ))
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
            page.run_js(&format!(
                "(() => {{ \
                        document.body.innerHTML = '<iframe id=\"demo-frame\"></iframe>'; \
                        document.getElementById('demo-frame').srcdoc = {srcdoc}; \
                        return true; \
                    }})()"
            ))?;
            Ok(())
        };

        let page_init_id =
            page.add_init_js("window.__pageFrameInit = (window.__pageFrameInit || 0) + 1;")?;

        set_iframe("first")?;
        let frame = wait_until(Duration::from_millis(1_500), || {
            page.get_frame_context(1usize).ok()
        })?;

        assert_eq!(
            frame.run_js_with_args(
                page_args_script_path.to_str().ok_or_else(|| {
                    OpenPageError::PageOperation(
                        "page args script path was not valid utf-8".to_string(),
                    )
                })?,
                &[Value::from(4), Value::from(5)],
                false,
            )?,
            Value::from(9)
        );
        assert_eq!(
            frame.run_js_with_options(
                "6 + 7",
                &[Value::from(6), Value::from(7)],
                true,
                Some(1_000),
            )?,
            Value::from(13)
        );
        assert_eq!(frame.run_js_loaded("return 8 + 1;")?, Value::from(9));
        frame.run_async_js("setTimeout(() => { window.__frameAsync = 'done'; }, 0);")?;
        wait_until(Duration::from_millis(1_500), || {
            match frame.run_js("window.__frameAsync || null").ok()? {
                Value::String(value) if value == "done" => Some(()),
                _ => None,
            }
        })?;
        assert_eq!(frame.run_js("window.__pageFrameInit || 0")?, Value::from(1));

        let frame_init_id = frame
            .add_init_js("window.__frameWrapperInit = (window.__frameWrapperInit || 0) + 1;")?;

        set_iframe("second")?;
        let second_frame = wait_until(Duration::from_millis(1_500), || {
            page.get_frame_context(1usize).ok()
        })?;
        assert_eq!(
            second_frame.run_js("window.__pageFrameInit || 0")?,
            Value::from(1)
        );
        assert_eq!(
            second_frame.run_js("window.__frameWrapperInit || 0")?,
            Value::from(1)
        );

        frame.remove_init_js(Some(&frame_init_id))?;
        set_iframe("third")?;
        let third_frame = wait_until(Duration::from_millis(1_500), || {
            page.get_frame_context(1usize).ok()
        })?;
        assert_eq!(
            third_frame.run_js("window.__pageFrameInit || 0")?,
            Value::from(1)
        );
        assert_eq!(
            third_frame.run_js("window.__frameWrapperInit || 0")?,
            Value::from(0)
        );

        page.remove_init_js(Some(&page_init_id))?;
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page/frame js helper runtime regression");
}

#[test]
fn page_run_js_loaded_waits_for_loaded_document_before_evaluating() {
    let (load_url, load_server) = spawn_delayed_load_site(Duration::from_millis(250));
    let (browser, temp_dir) =
        launch_headless_test_browser("page-run-js-loaded").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let load_url_json = serde_json::to_string(&load_url)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        page.run_js(&format!("window.location.href = {load_url_json};"))?;
        assert!(page.wait_for_load_start(1_000)?);

        wait_until(Duration::from_millis(1_500), || {
            match page
                .run_js("document.body ? document.body.dataset.ready : null")
                .ok()?
            {
                Value::String(value) if value == "pending" => Some(()),
                _ => None,
            }
        })?;

        assert_eq!(
            page.run_js_loaded_with_options("document.body.dataset.ready", &[], true, Some(1_000))?,
            Value::from("loaded")
        );
        assert_eq!(
            page.run_js_loaded_with_args(
                "return document.getElementById('status').textContent + arguments[0];",
                &[Value::from("-ok")],
                false,
            )?,
            Value::from("loaded-ok")
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    let server_result = load_server.join();

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    if let Err(err) = server_result {
        panic!("join delayed load server: {err:?}");
    }
    result.expect("run_js_loaded should wait for delayed document load");
}

#[test]
fn page_retry_and_timeouts_runtime_settings_update_browser_backed_page() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-runtime-settings").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert_eq!(page.retry_times()?, 3);
        assert_eq!(page.retry_interval()?, 2.0);
        assert_eq!(page.timeouts()?.get("base").copied(), Some(1.0));
        assert_eq!(page.timeouts()?.get("page_load").copied(), Some(5.0));
        assert_eq!(page.timeouts()?.get("script").copied(), Some(1.0));

        page.set_retry(Some(5), Some(0.25))?;
        page.set_timeouts(Some(1.5), Some(6.0), Some(0.75))?;

        assert_eq!(page.retry_times()?, 5);
        assert_eq!(page.retry_interval()?, 0.25);
        let timeouts = page.timeouts()?;
        assert_eq!(timeouts.get("base").copied(), Some(1.5));
        assert_eq!(timeouts.get("page_load").copied(), Some(6.0));
        assert_eq!(timeouts.get("script").copied(), Some(0.75));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page runtime retry/timeout settings regression");
}

#[test]
fn page_set_wrapper_updates_storage_and_runtime_settings() {
    let (page_url, page_server) = spawn_delayed_load_site(Duration::from_millis(0));
    let (browser, temp_dir) =
        launch_headless_test_browser("page-set-wrapper").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        page.goto(&page_url)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        page.set()
            .session_storage("set-wrapper-session", Some("one"))?;
        page.set().local_storage("set-wrapper-local", Some("two"))?;
        assert_eq!(
            page.session_storage(Some("set-wrapper-session"))?,
            Value::from("one")
        );
        assert_eq!(
            page.local_storage(Some("set-wrapper-local"))?,
            Value::from("two")
        );

        page.set().load_mode().eager()?;
        assert_eq!(page.load_mode()?, "eager");
        page.set().retry_times(4)?;
        page.set().retry_interval(0.5)?;
        page.set().timeouts(Some(2.0), Some(6.0), Some(1.5))?;

        assert_eq!(page.retry_times()?, 4);
        assert_eq!(page.retry_interval()?, 0.5);
        let timeouts = page.timeouts()?;
        assert_eq!(timeouts.get("base").copied(), Some(2.0));
        assert_eq!(timeouts.get("page_load").copied(), Some(6.0));
        assert_eq!(timeouts.get("script").copied(), Some(1.5));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    let server_result = page_server.join();

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    if let Err(err) = server_result {
        panic!("join page set wrapper server: {err:?}");
    }
    result.expect("page set wrapper regression");
}

#[test]
fn page_scroll_wrapper_controls_page_scroll_position() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-scroll-wrapper").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = '<div style="height:4000px;width:4000px"></div>';
                    document.documentElement.scrollTop = 0;
                    document.documentElement.scrollLeft = 0;
                    return true;
                })()"#,
        )?;

        page.scroll().down(120.0)?;
        page.scroll().right(80.0)?;
        assert_eq!(
            page.run_js(
                "[document.scrollingElement.scrollLeft, document.scrollingElement.scrollTop]"
            )?,
            Value::Array(vec![Value::from(80), Value::from(120)])
        );

        page.scroll().to_location(25.0, 35.0)?;
        assert_eq!(
            page.run_js(
                "[document.scrollingElement.scrollLeft, document.scrollingElement.scrollTop]"
            )?,
            Value::Array(vec![Value::from(25), Value::from(35)])
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page scroll wrapper regression");
}

#[test]
fn page_wait_for_ele_methods_accept_element_targets_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-wait-ele-targets").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <button id="ready">Ready</button>
                        <div id="hidden" style="display:none">Hidden</div>
                        <button id="delete-me">Delete me</button>
                    `;
                    return true;
                })()"#,
        )?;

        let ready = page.find("#ready")?;
        let hidden = page.find("#hidden")?;
        let delete_me = page.find("#delete-me")?;

        assert!(page.wait_for_ele_displayed(&ready, 1_000)?);
        assert!(page.wait_for_ele_enabled(&ready, 1_000)?);
        assert!(page.wait_for_ele_clickable(&ready, 1_000)?);
        assert!(page.wait_for_ele_hidden(&hidden, 1_000)?);

        page.run_js(
            r#"(() => {
                    setTimeout(() => document.getElementById('delete-me')?.remove(), 50);
                    return true;
                })()"#,
        )?;
        assert!(page.wait_for_ele_deleted(&delete_me, 2_000)?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element target wait regression");
}

#[test]
fn page_get_frame_methods_accept_element_and_frame_targets_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-get-frame-targets").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div id="host">
                            <iframe id="demo-frame" name="demo-frame"
                                srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                            </iframe>
                        </div>
                    `;
                    return true;
                })()"#,
            )?;

        let frame_element = page
            .get_frame_ele("css:#demo-frame")
            .map_err(|err| OpenPageError::PageOperation(format!("locator get_frame_ele: {err}")))?;
        let frame = page
            .get_frame("css:#demo-frame")
            .map_err(|err| OpenPageError::PageOperation(format!("locator get_frame: {err}")))?;
        let web_frame = WebFrame::Browser(frame.clone());
        let frame_by_index = page
            .get_frame(1usize)
            .map_err(|err| OpenPageError::PageOperation(format!("index get_frame: {err}")))?;
        let frame_element_by_index = page
            .get_frame_ele(1usize)
            .map_err(|err| OpenPageError::PageOperation(format!("index get_frame_ele: {err}")))?;
        let frame_context_from_locator =
            page.get_frame_context("css:#demo-frame").map_err(|err| {
                OpenPageError::PageOperation(format!("locator get_frame_context: {err}"))
            })?;
        let frame_context_by_index = page.get_frame_context(-1isize).map_err(|err| {
            OpenPageError::PageOperation(format!("index get_frame_context: {err}"))
        })?;

        assert_eq!(
            page.get_frame(&frame_element)
                .and_then(|frame| frame.attr("id"))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "get_frame(&Element): {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            page.get_frame(&frame)
                .and_then(|frame| frame.attr("name"))
                .map_err(|err| OpenPageError::PageOperation(format!("get_frame(&Frame): {err}")))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_by_index
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!("get_frame(1) attr: {err}")))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_element_by_index
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "get_frame_ele(1) attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );

        let frame_from_element = page
            .get_frame_context(&frame_element)
            .map_err(|err| OpenPageError::PageOperation(format!("from element: {err}")))?;
        let frame_from_frame = page
            .get_frame_context(&frame)
            .map_err(|err| OpenPageError::PageOperation(format!("from frame: {err}")))?;
        let frame_from_web_frame = page
            .get_frame_context(&web_frame)
            .map_err(|err| OpenPageError::PageOperation(format!("from web frame: {err}")))?;
        let host = page
            .find("css:#host")
            .map_err(|err| OpenPageError::PageOperation(format!("host find: {err}")))?;
        let host_frame = host.get_frame("css:#demo-frame").map_err(|err| {
            OpenPageError::PageOperation(format!("host get_frame(locator): {err}"))
        })?;
        let host_frame_by_index = host
            .get_frame(1usize)
            .map_err(|err| OpenPageError::PageOperation(format!("host get_frame(1): {err}")))?;
        let host_frame_from_frame = host.get_frame(&frame).map_err(|err| {
            OpenPageError::PageOperation(format!("host get_frame(&Frame): {err}"))
        })?;
        let web_host = WebElement::Browser(
            page.find("css:#host")
                .map_err(|err| OpenPageError::PageOperation(format!("web host find: {err}")))?,
        );
        let web_host_frame = web_host.get_frame("css:#demo-frame").map_err(|err| {
            OpenPageError::PageOperation(format!("web host get_frame(locator): {err}"))
        })?;
        let web_host_frame_by_index = web_host
            .get_frame(1usize)
            .map_err(|err| OpenPageError::PageOperation(format!("web host get_frame(1): {err}")))?;

        assert_eq!(
            frame_from_element
                .frame_element()
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "frame_from_element attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_from_element
                .name()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "frame_from_element name: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_from_frame
                .name()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "frame_from_frame name: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_from_web_frame.name().map_err(|err| {
                OpenPageError::PageOperation(format!("frame_from_web_frame name: {err}"))
            })?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_context_by_index
                .name()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "frame_context_by_index name: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_context_from_locator
                .name()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "frame_context_from_locator name: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            frame_from_frame.frame_element().attr("id").map_err(|err| {
                OpenPageError::PageOperation(format!("frame_from_frame attr: {err}"))
            })?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            page.get_frame_ele(&frame)
                .and_then(|element| element.attr("id"))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "get_frame_ele(&Frame): {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            page.get_frame_ele_with_timeout(&web_frame, 10)
                .and_then(|element| element.attr("id"))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "get_frame_ele_with_timeout(&WebFrame): {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            page.get_frame_ele_with_timeout(frame.clone(), 10)
                .and_then(|element| element.attr("name"))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "get_frame_ele_with_timeout(Frame): {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            host_frame
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!("host_frame attr: {err}")))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            host_frame_by_index
                .attr("name")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "host_frame_by_index attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            host_frame_from_frame
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "host_frame_from_frame attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            web_host_frame
                .attr("id")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "web_host_frame attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        assert_eq!(
            web_host_frame_by_index
                .attr("name")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "web_host_frame_by_index attr: {err}"
                )))?,
            Some("demo-frame".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame target lookup regression");
}

#[test]
fn page_get_frame_with_timeout_waits_for_delayed_iframe() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-get-frame-timeout").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

        assert!(page.get_frame("css:#delayed-frame").is_err());

        let frame = page.get_frame_with_timeout("css:#delayed-frame", 2_000)?;
        assert!(frame.wait_for_doc_loaded(2_000)?);
        assert_eq!(frame.attr("name")?, Some("delayed-frame".to_string()));
        assert_eq!(
            frame.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );

        let frame_by_index = page.get_frame_by_index_with_timeout(1, 500)?;
        assert_eq!(
            frame_by_index.attr("id")?,
            Some("delayed-frame".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame timeout lookup regression");
}

#[test]
fn frame_index_helpers_accept_negative_indexes_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("frame-negative-index").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div id="host">
                            <iframe id="first-frame" name="first-frame"
                                srcdoc="<html><body><div>first</div></body></html>">
                            </iframe>
                            <iframe id="second-frame" name="second-frame"
                                srcdoc="<html><body><div id='nested-host'></div></body></html>">
                            </iframe>
                        </div>
                    `;
                    return true;
                })()"#,
        )?;

        let last_frame = page.get_frame_by_index(-1isize)?;
        let last_frame_ele = page.get_frame_ele_by_index(-1i32)?;
        let last_context = page.get_frame_context_by_index(-1i64)?;

        assert_eq!(last_frame.attr("id")?, Some("second-frame".to_string()));
        assert_eq!(last_frame_ele.attr("id")?, Some("second-frame".to_string()));
        assert_eq!(last_context.attr("id")?, Some("second-frame".to_string()));

        assert!(last_frame.wait_for_doc_loaded(2_000)?);
        last_frame.run_js(
            r#"(() => {
                    document.getElementById('nested-host').innerHTML = `
                        <iframe id="nested-first" name="nested-first"
                            srcdoc="<html><body>nested first</body></html>">
                        </iframe>
                        <iframe id="nested-second" name="nested-second"
                            srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let nested_last = last_frame.get_frame_by_index(-1isize)?;
        let nested_last_ele = last_frame.get_frame_ele_by_index(-1i32)?;

        assert_eq!(nested_last.attr("id")?, Some("nested-second".to_string()));
        assert_eq!(
            nested_last_ele.attr("id")?,
            Some("nested-second".to_string())
        );
        assert_eq!(
            nested_last.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame negative index regression");
}

#[test]
fn get_frames_with_timeout_waits_for_delayed_iframes() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-get-frames-timeout").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><div id='outer-host'></div></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
        )?;

        assert!(page.get_frames(Some("css:#delayed-frame"))?.is_empty());
        let frames = page.get_frames_with_timeout(Some("css:#delayed-frame"), 2_000)?;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].attr("name")?, Some("delayed-frame".to_string()));

        let outer = frames.into_iter().next().expect("frame exists");
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'nested-frame';
                        frame.name = 'nested-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

        assert!(outer.get_frames(Some("css:#nested-frame"))?.is_empty());
        let nested = outer.get_frames_with_timeout(Some("css:#nested-frame"), 2_000)?;
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].attr("id")?, Some("nested-frame".to_string()));
        assert_eq!(
            nested[0].find("css:#inside")?.text()?,
            Some("inside".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame batch timeout lookup regression");
}

#[test]
fn get_frame_eles_with_timeout_waits_for_delayed_iframe_elements() {
    let (browser, temp_dir) = launch_headless_test_browser("page-get-frame-eles-timeout")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'delayed-frame';
                        frame.name = 'delayed-frame';
                        frame.srcdoc = "<html><body><div id='outer-host'></div></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
        )?;

        assert!(page.get_frame_eles(Some("css:#delayed-frame"))?.is_empty());
        let frame_ele = page.get_frame_ele_with_timeout("css:#delayed-frame", 2_000)?;
        assert_eq!(frame_ele.attr("name")?, Some("delayed-frame".to_string()));
        let frame_eles = page.get_frame_eles_with_timeout(Some("css:#delayed-frame"), 500)?;
        assert_eq!(frame_eles.len(), 1);

        let outer = page.get_frame(&frame_ele)?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'nested-frame';
                        frame.name = 'nested-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

        assert!(outer.get_frame_eles(Some("css:#nested-frame"))?.is_empty());
        let nested_ele = outer.get_frame_ele_with_timeout("css:#nested-frame", 2_000)?;
        assert_eq!(nested_ele.attr("id")?, Some("nested-frame".to_string()));
        let nested_eles = outer.get_frame_eles_with_timeout(Some("css:#nested-frame"), 500)?;
        assert_eq!(nested_eles.len(), 1);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame element timeout lookup regression");
}

#[test]
fn frame_get_frame_finds_nested_iframe_in_frame_context() {
    let (browser, temp_dir) =
        launch_headless_test_browser("frame-get-nested-frame").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
            r#"(() => {
                    const frame = document.createElement('iframe');
                    frame.id = 'inner-frame';
                    frame.name = 'inner-frame';
                    frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                    document.getElementById('outer-host').appendChild(frame);
                    return true;
                })()"#,
        )?;

        let inner = outer.get_frame("css:#inner-frame")?;
        let inner_by_index = outer.get_frame_by_index(1)?;
        let inner_ele = outer.get_frame_ele("css:#inner-frame")?;
        let nested_frames = outer.get_frames(Some((By::TAG_NAME, "iframe")))?;

        assert!(inner.wait_for_doc_loaded(2_000)?);
        assert_eq!(inner.attr("name")?, Some("inner-frame".to_string()));
        assert_eq!(inner_by_index.attr("id")?, Some("inner-frame".to_string()));
        assert_eq!(inner_ele.attr("id")?, Some("inner-frame".to_string()));
        assert_eq!(nested_frames.len(), 1);
        assert_eq!(
            inner.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("nested frame lookup regression");
}

#[test]
fn singleton_tab_obj_reuses_nested_frame_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("frame-nested-singleton-enabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
            r#"(() => {
                    const frame = document.createElement('iframe');
                    frame.id = 'inner-frame';
                    frame.name = 'inner-frame';
                    frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                    document.getElementById('outer-host').appendChild(frame);
                    return true;
                })()"#,
        )?;

        let inner = outer.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(2_000)?);
        inner.set_none_element_value(Some("nested missing"), true)?;

        let inner_by_index = outer.get_frame_by_index(1)?;
        let inner_by_index_timeout = outer.get_frame_by_index_with_timeout(1, 500)?;
        let nested_frames = outer.get_frames(Some((By::TAG_NAME, "iframe")))?;
        let nested_frames_timeout =
            outer.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 500)?;
        let host = outer.find("css:#outer-host")?;
        let inner_from_host = host.get_frame("css:#inner-frame")?;

        assert_eq!(inner_by_index.id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            inner_by_index.frame_element()
        ));
        assert_eq!(inner_by_index_timeout.id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            inner_by_index_timeout.frame_element()
        ));
        assert_eq!(nested_frames.len(), 1);
        assert_eq!(nested_frames[0].id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            nested_frames[0].frame_element()
        ));
        assert_eq!(nested_frames_timeout.len(), 1);
        assert_eq!(nested_frames_timeout[0].id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            nested_frames_timeout[0].frame_element()
        ));
        assert_eq!(inner_from_host.id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            inner_from_host.frame_element()
        ));
        assert_eq!(
            inner_from_host.ele(".does-not-exist")?.text()?,
            Some("nested missing".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("nested singleton frame runtime-state regression");
}

#[test]
fn singleton_tab_obj_drops_stale_nested_frame_after_recreation() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("frame-nested-recreated-singleton")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.set_none_element_value(Some("outer missing"), true)?;
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><button id='inside'>first</button></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let first = outer.get_frame("css:#inner-frame")?;
        assert!(first.wait_for_doc_loaded(2_000)?);
        first.set_none_element_value(Some("first missing"), true)?;

        outer.run_js(
            r#"(() => {
                    document.getElementById('inner-frame').remove();
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><button id='inside'>second</button></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let second = outer.get_frame("css:#inner-frame")?;
        assert!(second.wait_for_doc_loaded(2_000)?);
        assert_eq!(
            second.find("css:#inside")?.text()?,
            Some("second".to_string())
        );
        assert_ne!(second.id(), first.id());
        assert_eq!(
            second.ele(".does-not-exist")?.text()?,
            Some("outer missing".to_string())
        );
        assert!(!first.is_alive()?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("stale nested singleton frame cache regression");
}

#[test]
fn singleton_tab_obj_prunes_nested_frame_after_parent_frame_navigation() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("frame-parent-navigation-singleton")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.set_none_element_value(Some("outer missing"), true)?;
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><button id='inside'>first</button></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let first_inner = outer.get_frame("css:#inner-frame")?;
        assert!(first_inner.wait_for_doc_loaded(2_000)?);
        first_inner.set_none_element_value(Some("inner missing"), true)?;

        page.run_js(
                r#"(() => {
                    document.getElementById('outer-frame').srcdoc = `
                        <html><body>
                            <iframe id="inner-frame" name="inner-frame"
                                srcdoc="<html><body><button id='inside'>second</button></body></html>">
                            </iframe>
                        </body></html>
                    `;
                    return true;
                })()"#,
            )?;
        assert!(outer.wait_for_doc_loaded(2_000)?);

        let second_inner = outer.get_frame("css:#inner-frame")?;
        assert!(second_inner.wait_for_doc_loaded(2_000)?);
        assert_ne!(second_inner.id(), first_inner.id());
        assert_eq!(
            second_inner.find("css:#inside")?.text()?,
            Some("second".to_string())
        );
        assert_eq!(
            second_inner.ele(".does-not-exist")?.text()?,
            Some("outer missing".to_string())
        );
        assert!(!first_inner.is_alive()?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("parent frame navigation cache prune regression");
}

#[test]
fn singleton_tab_obj_reuses_shadow_root_frame_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("frame-shadow-singleton-enabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `<div id="host"></div>`;
                    const host = document.getElementById('host');
                    const root = host.attachShadow({mode: 'open'});
                    root.innerHTML = `
                        <div id="shadow-wrapper">
                            <iframe id="shadow-frame" name="shadow-frame"
                                srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                            </iframe>
                        </div>
                    `;
                    return true;
                })()"#,
            )?;

        let host = page.find("css:#host")?;
        let shadow_root = host.shadow_root()?.expect("host shadow root");
        let wrapper = shadow_root.find("css:#shadow-wrapper")?;
        let frame = wrapper.get_frame("css:#shadow-frame").map_err(|err| {
            OpenPageError::PageOperation(format!("shadow wrapper get_frame first: {err}"))
        })?;
        assert!(frame.wait_for_doc_loaded(2_000)?);
        frame.set_none_element_value(Some("shadow missing"), true)?;

        let same_frame = wrapper.get_frame((By::ID, "shadow-frame")).map_err(|err| {
            OpenPageError::PageOperation(format!("shadow wrapper get_frame second: {err}"))
        })?;
        assert_eq!(same_frame.id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            same_frame.frame_element()
        ));
        assert_eq!(
            same_frame.ele(".does-not-exist")?.text()?,
            Some("shadow missing".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("shadow root singleton frame runtime-state regression");
}

#[test]
fn frame_get_frame_with_timeout_waits_for_delayed_nested_iframe() {
    let (browser, temp_dir) = launch_headless_test_browser("frame-get-nested-frame-timeout")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
                r#"(() => {
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'inner-frame';
                        frame.name = 'inner-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('outer-host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

        assert!(outer.get_frame("css:#inner-frame").is_err());

        let inner = outer.get_frame_with_timeout("css:#inner-frame", 2_000)?;
        assert!(inner.wait_for_doc_loaded(2_000)?);
        assert_eq!(inner.attr("name")?, Some("inner-frame".to_string()));
        assert_eq!(
            inner.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );

        let inner_by_index = outer.get_frame_by_index_with_timeout(1, 500)?;
        assert_eq!(inner_by_index.attr("id")?, Some("inner-frame".to_string()));
        let inner_web_frame = WebFrame::Browser(inner.clone());
        assert_eq!(
            outer
                .get_frame_ele_with_timeout(&inner_web_frame, 10)?
                .attr("id")?,
            Some("inner-frame".to_string())
        );
        assert_eq!(
            outer
                .get_frame_ele_with_timeout(inner.clone(), 10)?
                .attr("name")?,
            Some("inner-frame".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("nested frame timeout lookup regression");
}

#[test]
fn element_get_frame_with_timeout_waits_for_delayed_iframe_child() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-get-frame-timeout").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<div id="host"></div>';
                    setTimeout(() => {
                        const frame = document.createElement('iframe');
                        frame.id = 'child-frame';
                        frame.name = 'child-frame';
                        frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                        document.getElementById('host').appendChild(frame);
                    }, 150);
                    return true;
                })()"#,
            )?;

        let host = page.find("css:#host")?;
        assert!(host.get_frame("css:#child-frame").is_err());

        let frame = host.get_frame_with_timeout("css:#child-frame", 2_000)?;
        assert!(frame.wait_for_doc_loaded(2_000)?);
        assert_eq!(frame.attr("name")?, Some("child-frame".to_string()));
        assert_eq!(
            frame.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );

        let frame_by_index = host.get_frame_by_index_with_timeout(1, 500)?;
        assert_eq!(frame_by_index.attr("id")?, Some("child-frame".to_string()));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element frame timeout lookup regression");
}

#[test]
fn page_save_returns_mhtml_and_pdf_content_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-save").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.title = "Save Capability";
                    document.body.innerHTML = `
                        <main id="content">
                            <h1>save target</h1>
                            <p>Rust page.save runtime coverage.</p>
                        </main>
                    `;
                    return true;
                })()"#,
        )?;

        let mhtml = page.save(None, None, false)?;
        match mhtml {
            PageSaveContent::Mhtml(data) => {
                assert!(data.contains("save target"));
                assert!(data.contains("Content-Location:"));
            }
            other => panic!("expected mhtml save content, got {other:?}"),
        }

        let mhtml_dir = temp_dir.join("page-save-files");
        let mhtml = page.save(Some(&mhtml_dir), Some("saved-page"), false)?;
        let mhtml_path = mhtml_dir.join("saved-page.mhtml");
        assert!(mhtml_path.exists());
        let saved_mhtml = fs::read_to_string(&mhtml_path).expect("read saved mhtml");
        assert!(saved_mhtml.contains("save target"));
        match mhtml {
            PageSaveContent::Mhtml(data) => {
                assert_eq!(data, saved_mhtml);
            }
            other => panic!("expected saved mhtml content, got {other:?}"),
        }

        let pdf = page.save_with_options(
            Some(&temp_dir),
            Some("saved-page"),
            true,
            Some(PrintToPdfParams::builder().landscape(true).build()),
        )?;
        let pdf_path = temp_dir.join("saved-page.pdf");
        assert!(pdf_path.exists());
        match pdf {
            PageSaveContent::Pdf(bytes) => {
                assert!(bytes.starts_with(b"%PDF"));
                assert_eq!(bytes, fs::read(&pdf_path).expect("read saved pdf"));
            }
            other => panic!("expected pdf save content, got {other:?}"),
        }

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page save runtime regression");
}

#[test]
fn page_actions_support_mouse_keyboard_and_scroll_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-actions").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="kw" />
                        <button id="btn">Go</button>
                        <div style="height: 2000px"></div>
                        <button id="far-btn">Far</button>
                    `;
                    window.__clicked = 0;
                    window.__farClicked = 0;
                    window.__mouse = [];
                    window.__moveButtons = [];
                    window.__moveShift = [];
                    window.__wheel = [];
                    window.__keys = [];
                    document.getElementById('btn').addEventListener('click', () => window.__clicked += 1);
                    document.getElementById('far-btn').addEventListener('click', () => window.__farClicked += 1);
                    document.addEventListener('mousedown', (event) => window.__mouse.push(`down:${event.button}`));
                    document.addEventListener('mouseup', (event) => window.__mouse.push(`up:${event.button}`));
                    document.addEventListener('mousemove', (event) => {
                        window.__moveButtons.push(event.buttons);
                        window.__moveShift.push(event.shiftKey);
                    });
                    document.addEventListener('wheel', (event) => window.__wheel.push([event.deltaY, event.deltaX]));
                    document.addEventListener('keydown', (event) => window.__keys.push(`down:${event.key}`));
                    document.addEventListener('keyup', (event) => window.__keys.push(`up:${event.key}`));
                    return true;
                })()"#,
            )?;

        let mut actions = page.actions()?;
        actions
            .move_to("css:#btn", None, None, 0.0)?
            .click(None::<&str>, 1)?
            .move_to((20, 20), None, None, 0.0)?
            .hold(None::<&str>)?
            .r#move(25.0, 0.0, 0.0)?
            .release(None::<&str>)?
            .r_hold(None::<&str>)?
            .r_release(None::<&str>)?
            .m_hold(None::<&str>)?
            .m_release(None::<&str>)?
            .move_to("css:#kw", None, None, 0.0)?
            .click(None::<&str>, 1)?
            .input("Drission")?
            .r#type(["Page"])?
            .scroll(120.0, 10.0, None::<&str>)?
            .key_down("Shift")?
            .r#move(15.0, 5.0, 0.0)?
            .key_up("Shift")?;

        assert_eq!(page.run_js("window.__clicked")?, Value::from(1));
        assert_eq!(
            page.run_js("document.getElementById('kw').value")?,
            Value::from("DrissionPage")
        );
        let mouse = page.run_js("window.__mouse.join(',')")?;
        match mouse {
            Value::String(mouse) => {
                assert!(mouse.contains("down:0"));
                assert!(mouse.contains("up:0"));
                assert!(mouse.contains("down:1"));
                assert!(mouse.contains("up:1"));
                assert!(mouse.contains("down:2"));
                assert!(mouse.contains("up:2"));
            }
            other => panic!("unexpected mouse event payload: {other}"),
        }
        assert_eq!(
            page.run_js("[window.__wheel.length, window.__wheel[0][0], window.__wheel[0][1]]")?,
            json!([1, 120, 10])
        );
        assert_eq!(
            page.run_js("window.__moveButtons.includes(1)")?,
            Value::Bool(true)
        );
        assert_eq!(
            page.run_js("window.__moveShift.includes(true)")?,
            Value::Bool(true)
        );
        let keys = page.run_js("window.__keys.join(',')")?;
        match keys {
            Value::String(keys) => {
                assert!(keys.contains("down:Shift"));
                assert!(keys.contains("up:Shift"));
            }
            other => panic!("unexpected key event payload: {other}"),
        }
        assert!(actions.curr_x() > 0);
        assert!(actions.curr_y() >= 0);

        let mut no_wait_actions = page.new_actions();
        no_wait_actions.move_to((20, 30), None, None, 0.0)?;
        assert_eq!(
            (no_wait_actions.curr_x(), no_wait_actions.curr_y()),
            (20, 30)
        );
        let mut absolute_actions = page.new_actions();
        absolute_actions.move_to((30, 900), None, None, 0.0)?;
        let absolute_scroll_y = page
            .run_js("window.scrollY")?
            .as_f64()
            .expect("window.scrollY as f64");
        assert!(absolute_scroll_y > 0.0);
        assert!(((absolute_actions.curr_y() as f64) - (900.0 - absolute_scroll_y)).abs() <= 1.0);

        let mut far_element_actions = page.new_actions();
        far_element_actions
            .move_to("css:#far-btn", None, None, 0.0)?
            .click(None::<&str>, 1)?;
        let far_scroll_y = page
            .run_js("window.scrollY")?
            .as_f64()
            .expect("window.scrollY as f64");
        assert!(far_scroll_y > 0.0);
        assert_eq!(page.run_js("window.__farClicked")?, Value::from(1));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("actions runtime regression");
}

#[test]
fn page_actions_type_supports_modifier_events_and_interval_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-actions-type").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `<input id="kw" value="">`;
                    window.__keys = [];
                    window.__letterTimes = [];
                    window.__keyStates = [];
                    const kw = document.getElementById('kw');
                    kw.addEventListener('keydown', event => {
                        window.__keys.push(`down:${event.key}`);
                        window.__keyStates.push([event.key, event.ctrlKey, event.metaKey, event.shiftKey]);
                        if (event.key.length === 1) {
                            window.__letterTimes.push(performance.now());
                        }
                    });
                    kw.addEventListener('keyup', event => window.__keys.push(`up:${event.key}`));
                    return true;
                })()"#,
            )?;

        let input = page.find("css:#kw")?;
        let mut actions = page.actions()?;
        actions.click(Some("css:#kw"), 1)?;
        let start = Instant::now();
        actions.type_with_interval("abc", 0.12)?;
        assert!(start.elapsed() >= Duration::from_millis(300));
        assert_eq!(input.value()?, Some("abc".to_string()));
        assert_eq!(
                page.run_js("window.__letterTimes.length === 3 && (window.__letterTimes[2] - window.__letterTimes[0]) >= 200")?,
                Value::from(true)
            );

        page.run_js(
            r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = '';
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
        )?;

        actions.type_keys_with_interval(["Shift", "a"], 0.01)?;
        assert_eq!(input.value()?, Some("A".to_string()));
        let keys = page
            .run_js("window.__keys.join(',')")
            .map_err(|err| OpenPageError::PageOperation(format!("post combo keys: {err}")))?;
        match keys {
            Value::String(keys) => {
                assert!(keys.contains("down:Shift"));
                assert!(keys.contains("up:Shift"));
                assert!(keys.contains("down:a") || keys.contains("down:A"));
                assert!(keys.contains("up:a") || keys.contains("up:A"));
            }
            other => panic!("unexpected actions type key payload: {other}"),
        }
        assert_eq!(
                page.run_js("window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && item[3] === true)")
                    .map_err(|err| OpenPageError::PageOperation(format!("post combo key states: {err}")))?,
                Value::from(true)
            );

        page.run_js(
            r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = '';
                    kw.focus();
                    kw.setSelectionRange(0, 0);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
        )?;

        actions.type_keys_with_interval(["Shift", "1"], 0.01)?;
        assert_eq!(input.value()?, Some("!".to_string()));
        assert_eq!(
            page.run_js("window.__keyStates.some(item => item[0] === '!' && item[3] === true)")
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "post symbol key states: {err}"
                )))?,
            Value::from(true)
        );

        page.run_js(
            r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.value = 'abcdef';
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keys = [];
                    window.__keyStates = [];
                    return true;
                })()"#,
        )?;

        actions.type_keys(Keys::CTRL_A)?;
        let selection_start = input
            .property("selectionStart")?
            .and_then(|value| value.as_u64())
            .expect("selectionStart as u64");
        let selection_end = input
            .property("selectionEnd")?
            .and_then(|value| value.as_u64())
            .expect("selectionEnd as u64");
        let selection_len = input
            .value()?
            .map(|value| value.len() as u64)
            .expect("input value length");
        assert_eq!(
            Value::from(selection_start == 0 && selection_end == selection_len),
            Value::from(true)
        );
        assert_eq!(
                page.run_js(
                    "window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && (item[1] === true || item[2] === true))"
                )
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "post shortcut modifier states: {err}"
                )))?,
                Value::from(true)
            );

        actions.type_keys("q")?;
        assert_eq!(input.value()?, Some("q".to_string()));

        actions.type_keys(Keys::CTRL_Z)?;
        assert_eq!(input.value()?, Some("abcdef".to_string()));

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("actions type interval runtime regression");
}

#[test]
fn page_actions_shortcuts_support_cut_redo_and_held_modifiers_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-actions-shortcuts").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="kw" value="abcdef">
                        <input id="clone" value="">
                    `;
                    window.__keyStates = [];
                    const kw = document.getElementById('kw');
                    kw.addEventListener('keydown', event => {
                        window.__keyStates.push([event.key, event.ctrlKey, event.metaKey, event.shiftKey]);
                    });
                    return true;
                })()"#,
            )?;

        let input = page.find("css:#kw")?;
        let mut actions = page.actions()?;
        actions.click(Some("css:#kw"), 1)?;

        actions.type_keys(Keys::CTRL_A)?;
        actions.type_keys(Keys::CTRL_X)?;
        assert_eq!(input.value()?, Some(String::new()));

        actions.type_keys(Keys::CTRL_Z)?;
        assert_eq!(input.value()?, Some("abcdef".to_string()));

        actions.type_keys(Keys::CTRL_Y)?;
        assert_eq!(input.value()?, Some(String::new()));

        input.set().value("abcdef")?;
        let clone = page.find("css:#clone")?;
        clone.set().value("")?;
        page.run_js(
            r#"(() => {
                    const kw = document.getElementById('kw');
                    kw.focus();
                    kw.setSelectionRange(kw.value.length, kw.value.length);
                    window.__keyStates = [];
                    return true;
                })()"#,
        )?;

        actions
            .key_down(Keys::CTRL_COMM)?
            .type_keys("a")?
            .key_up(Keys::CTRL_COMM)?;

        let selection_start = input
            .property("selectionStart")?
            .and_then(|value| value.as_u64())
            .expect("selectionStart as u64");
        let selection_end = input
            .property("selectionEnd")?
            .and_then(|value| value.as_u64())
            .expect("selectionEnd as u64");
        let selection_len = input
            .value()?
            .map(|value| value.len() as u64)
            .expect("input value length");
        assert_eq!((selection_start, selection_end), (0, selection_len));
        assert_eq!(
                page.run_js(
                    "window.__keyStates.some(item => (item[0] === 'a' || item[0] === 'A') && (item[1] === true || item[2] === true))"
                )
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "held shortcut modifier states: {err}"
                )))?,
                Value::from(true)
            );

        actions.type_keys(Keys::CTRL_C)?;
        actions.click(Some("css:#clone"), 1)?;
        actions.type_keys(Keys::CTRL_V)?;
        assert_eq!(clone.value()?, Some("abcdef".to_string()));

        clone.set().value("")?;
        actions.click(Some("css:#kw"), 1)?;
        actions
            .key_down(Keys::CTRL_COMM)?
            .type_keys("a")?
            .type_keys("x")?
            .key_up(Keys::CTRL_COMM)?;
        assert_eq!(input.value()?, Some(String::new()));

        input.set().value("abcdef")?;
        actions
            .key_down(Keys::CTRL_COMM)?
            .type_keys("a")?
            .type_keys("c")?
            .key_up(Keys::CTRL_COMM)?;
        actions
            .click(Some("css:#clone"), 1)?
            .key_down(Keys::CTRL_COMM)?
            .type_keys("v")?
            .key_up(Keys::CTRL_COMM)?;
        assert_eq!(clone.value()?, Some("abcdef".to_string()));

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("actions shortcut runtime regression");
}

#[test]
fn page_actions_drag_in_supports_files_and_text_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-actions-drag").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div id="drop" style="width: 240px; height: 120px; border: 1px solid #333;">
                            Drop here
                        </div>
                    `;
                    window.__dragEvents = [];
                    const target = document.getElementById('drop');
                    const capture = (type, event) => {
                        event.preventDefault();
                        const files = event.dataTransfer ? Array.from(event.dataTransfer.files || []).map(file => file.name) : [];
                        const itemTypes = event.dataTransfer ? Array.from(event.dataTransfer.items || []).map(item => item.type) : [];
                        let text = '';
                        let uri = '';
                        let html = '';
                        try {
                            if (event.dataTransfer) {
                                text = event.dataTransfer.getData('text/plain') || '';
                                uri = event.dataTransfer.getData('text/uri-list') || '';
                                html = event.dataTransfer.getData('text/html') || '';
                                if (!text) {
                                    text = uri || html || '';
                                }
                            }
                        } catch (error) {
                            text = `error:${error && error.message ? error.message : error}`;
                        }
                        window.__dragEvents.push({ type, files, itemTypes, text, uri, html });
                    };
                    target.addEventListener('dragenter', event => capture('dragenter', event));
                    target.addEventListener('dragover', event => capture('dragover', event));
                    target.addEventListener('drop', event => capture('drop', event));
                    return true;
                })()"#,
            )?;

        let file_path = temp_dir.join("drag-file.txt");
        fs::write(&file_path, "drag payload")?;
        let file_path = file_path.to_string_lossy().into_owned();

        let mut actions = page.actions()?;
        actions.drag_in(
            "css:#drop",
            crate::ActionsDragData::files(vec![file_path.clone()]),
        )?;

        assert_eq!(
            page.run_js("window.__dragEvents.some(event => event.type === 'dragenter')")?,
            Value::from(true)
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).type")?,
            Value::from("drop")
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).files[0]")?,
            Value::from("drag-file.txt")
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).itemTypes[0]")?,
            Value::from("text/plain")
        );

        page.run_js("window.__dragEvents = [];")?;
        actions.drag_in("css:#drop", crate::ActionsDragData::text("Dragged text"))?;

        assert_eq!(
            page.run_js("window.__dragEvents.some(event => event.type === 'dragenter')")?,
            Value::from(true)
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).type")?,
            Value::from("drop")
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).text")?,
            Value::from("Dragged text")
        );
        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).itemTypes[0]")?,
            Value::from("text/plain")
        );

        page.run_js("window.__dragEvents = [];")?;
        actions.drag_in(
            "css:#drop",
            crate::ActionsDragData::link("https://example.test/path", "Example title"),
        )?;

        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).type")?,
            Value::from("drop")
        );
        assert_eq!(
            page.run_js("window.__dragEvents.length > 0")?,
            Value::from(true)
        );

        page.run_js("window.__dragEvents = [];")?;
        actions.drag_in(
            "css:#drop",
            crate::ActionsDragData::html(
                "<strong>Dragged html</strong>",
                "https://example.test/base/",
            ),
        )?;

        assert_eq!(
            page.run_js("window.__dragEvents.at(-1).type")?,
            Value::from("drop")
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("actions drag_in runtime regression");
}

#[test]
fn actions_drag_payload_preserves_link_and_html_metadata() {
    let link_payload = action_drag_payload(crate::ActionsDragData::link(
        "https://example.test/path",
        "Example title",
    ))
    .expect("link payload");
    assert_eq!(link_payload.items.len(), 1);
    assert_eq!(link_payload.items[0].mime_type, "text/uri-list");
    assert_eq!(link_payload.items[0].data, "https://example.test/path");
    assert_eq!(
        link_payload.items[0].title.as_deref(),
        Some("Example title")
    );
    assert_eq!(link_payload.items[0].base_url, None);
    assert_eq!(link_payload.files, None);

    let html_payload = action_drag_payload(crate::ActionsDragData::html(
        "<strong>Dragged html</strong>",
        "https://example.test/base/",
    ))
    .expect("html payload");
    assert_eq!(html_payload.items.len(), 1);
    assert_eq!(html_payload.items[0].mime_type, "text/uri-list");
    assert_eq!(html_payload.items[0].data, "<strong>Dragged html</strong>");
    assert_eq!(html_payload.items[0].title, None);
    assert_eq!(
        html_payload.items[0].base_url.as_deref(),
        Some("https://example.test/base/")
    );
    assert_eq!(html_payload.files, None);
}

#[test]
fn page_element_list_getter_returns_attrs_links_and_texts_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-list-getter").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <a class="item" href="https://example.test/one">One</a>
                        <a class="item">Two</a>
                        <img class="item" src="https://example.test/three.png">
                    `;
                    return true;
                })()"#,
        )?;

        let items = page.find_all(".item")?;
        assert_eq!(
            items.get().attrs("href")?,
            vec![Some("https://example.test/one".to_string()), None, None,]
        );
        assert_eq!(
            items.get().links()?,
            vec![
                Some("https://example.test/one".to_string()),
                None,
                Some("https://example.test/three.png".to_string()),
            ]
        );
        assert_eq!(
            items.get().texts()?,
            vec![Some("One".to_string()), Some("Two".to_string()), None,]
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element list getter runtime regression");
}

#[test]
fn page_and_web_element_lists_support_filter_and_filter_one_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-list-filter").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" data-kind="keep">Alpha keep</button>
                        <button class="item" data-kind="drop" style="display:none">Hidden keep</button>
                        <button class="item" data-kind="drop" disabled>Disabled keep</button>
                        <button class="item" data-kind="keep">Gamma keep</button>
                    `;
                    return true;
                })()"#,
            )?;

        let items = page.find_all(".item")?;
        let active_keep = items
            .filter()
            .displayed(true)?
            .enabled(true)?
            .attr("data-kind", "keep", true)?
            .text("keep", true, true)?;
        assert_eq!(active_keep.len(), 2);
        assert_eq!(
            active_keep.get().texts()?,
            vec![
                Some("Alpha keep".to_string()),
                Some("Gamma keep".to_string()),
            ]
        );
        assert_eq!(
            active_keep
                .into_iter()
                .map(|element| element.text())
                .collect::<crate::OpenPageResult<Vec<_>>>()?,
            vec![
                Some("Alpha keep".to_string()),
                Some("Gamma keep".to_string()),
            ]
        );

        let second_displayed = items
            .filter_one_at(2)
            .displayed(true)?
            .expect("second displayed element");
        assert_eq!(second_displayed.text()?, Some("Disabled keep".to_string()));

        let disabled = items
            .filter_one()
            .enabled(false)?
            .expect("disabled element");
        assert_eq!(disabled.text()?, Some("Disabled keep".to_string()));

        let web_items = page
            .find_all(".item")?
            .into_iter()
            .map(WebElement::Browser)
            .collect::<Vec<_>>();
        assert_eq!(web_items.filter().displayed(true)?.len(), 3);
        let second_keep = web_items
            .filter_one_at(2)
            .attr("data-kind", "keep", true)?
            .expect("second keep web element");
        assert_eq!(second_keep.text()?, Some("Gamma keep".to_string()));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element list filter runtime regression");
}

#[test]
fn page_and_web_element_lists_support_extended_filters_and_search_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-list-search").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input class="item" id="checked-input" type="checkbox" checked />
                        <button class="item" id="primary-btn" style="display:block">Primary</button>
                        <button class="item" id="disabled-btn" disabled>Disabled</button>
                        <select>
                            <option class="item" id="plain-option">Plain</option>
                            <option class="item" id="selected-option" selected>Selected</option>
                        </select>
                        <span class="item" id="zero-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</span>
                        <div class="item" id="hidden-box" style="display:none">Hidden</div>
                    `;
                    return true;
                })()"#,
            )?;

        let items = page.find_all(".item")?;

        assert_eq!(
            items.filter().checked(true)?.get().attrs("id")?,
            vec![Some("checked-input".to_string())]
        );
        assert_eq!(
            items.filter().selected(true)?.get().attrs("id")?,
            vec![Some("selected-option".to_string())]
        );
        assert_eq!(
            items.filter().clickable(true)?.get().attrs("id")?,
            vec![
                Some("checked-input".to_string()),
                Some("primary-btn".to_string()),
            ]
        );
        assert_eq!(
            items.filter().have_rect(false)?.get().attrs("id")?,
            vec![
                Some("plain-option".to_string()),
                Some("selected-option".to_string()),
                Some("zero-rect".to_string()),
                Some("hidden-box".to_string()),
            ]
        );
        assert_eq!(items.filter().tag("option", true)?.len(), 2);
        assert_eq!(
            items
                .filter()
                .style("overflow", "hidden", true)?
                .get()
                .attrs("id")?,
            vec![Some("zero-rect".to_string())]
        );
        assert_eq!(items.filter().property("id", "primary-btn", true)?.len(), 1);

        let selected = items.filter_one().selected(true)?;
        assert_eq!(selected.attr("id")?, Some("selected-option".to_string()));
        assert_eq!(selected.is_selected()?, Some(true));

        let primary_button = items
            .filter()
            .tag("button", true)?
            .clickable(true)?
            .first()
            .expect("clickable primary");
        assert_eq!(primary_button.attr("id")?, Some("primary-btn".to_string()));

        let search = crate::ElementsSearch::new()
            .checked(true)
            .selected(true)
            .tag("button");
        let searched = items.search(&search)?;
        assert_eq!(searched.len(), 4);
        assert_eq!(
            searched.get().attrs("id")?,
            vec![
                Some("checked-input".to_string()),
                Some("primary-btn".to_string()),
                Some("disabled-btn".to_string()),
                Some("selected-option".to_string()),
            ]
        );

        let second_search_match = items.search_one_at(2, &search)?;
        assert_eq!(
            second_search_match.attr("id")?,
            Some("primary-btn".to_string())
        );
        assert_eq!(second_search_match.is_displayed()?, Some(true));

        let filtered_search = items
            .filter()
            .enabled(true)?
            .search(&crate::ElementsSearch::new().tag("button").selected(true))?;
        assert_eq!(
            filtered_search.get().attrs("id")?,
            vec![
                Some("primary-btn".to_string()),
                Some("selected-option".to_string()),
            ]
        );

        let web_items = page
            .find_all(".item")?
            .into_iter()
            .map(WebElement::Browser)
            .collect::<Vec<_>>();
        assert_eq!(web_items.filter().checked(true)?.len(), 1);
        assert_eq!(web_items.filter().selected(true)?.len(), 1);
        let checked_web = web_items
            .filter_one()
            .property("id", "checked-input", true)?
            .expect("checked web element");
        assert!(checked_web.is_checked()?);
        let disabled_web = web_items
            .filter_one()
            .property("id", "disabled-btn", true)?
            .expect("disabled web element");
        assert!(!disabled_web.is_enabled()?);
        assert_eq!(
            web_items
                .search_one(&crate::ElementsSearch::new().tag("button"))?
                .attr("id")?,
            Some("primary-btn".to_string())
        );
        assert_eq!(
            web_items
                .filter_one()
                .property("id", "selected-option", true)?
                .text()?,
            Some("Selected".to_string())
        );
        let missing = web_items.filter_one().property("id", "missing", true)?;
        assert!(missing.is_none());
        assert_eq!(missing.text()?, None);
        assert_eq!(missing.is_enabled()?, None);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("extended element list filter/search runtime regression");
}

#[test]
fn elements_one_supports_common_interactions_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("elements-one-interactions").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" data-role="cta">Click me</button>
                        <input class="item" id="name" value="">
                        <div class="item" id="text-block">alpha <span>beta</span> <em>gamma</em></div>
                        <link class="item" id="asset-link" rel="prefetch" href="data:text/plain;base64,aGVsbG8=">
                        <select id="single-picker">
                            <option class="item" id="single-a" value="a" selected>Single A</option>
                            <option class="item" id="single-b" value="b">Single B</option>
                        </select>
                        <select id="multi-picker" multiple>
                            <option class="item" id="multi-a" value="a">Multi A</option>
                            <option class="item" id="multi-b" value="b">Multi B</option>
                        </select>
                    `;
                    window.__oneClicks = 0;
                    window.__oneHover = 0;
                    document.querySelector('[data-role="cta"]').addEventListener('click', () => {
                        window.__oneClicks += 1;
                    });
                    document.querySelector('[data-role="cta"]').addEventListener('mouseenter', () => {
                        window.__oneHover += 1;
                    });
                    return true;
                })()"#,
            )?;

        let page_items = page.find_all(".item")?;
        let button_one = page_items.filter_one().attr("data-role", "cta", true)?;
        assert!(button_one.click()?);
        assert!(button_one.clicker().left()?);
        assert!(button_one.hover()?);
        assert_eq!(page.run_js("window.__oneClicks")?, Value::from(2));
        assert_eq!(page.run_js("window.__oneHover")?, Value::from(1));

        let input_one = page_items.filter_one().tag("input", true)?;
        assert!(input_one.focus()?);
        assert_eq!(
            page.run_js("document.activeElement && document.activeElement.id")?,
            Value::from("name")
        );
        assert!(input_one.input("Gamma")?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("Gamma")
        );
        assert!(input_one.clear()?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("")
        );

        let text_one = page_items.filter_one().attr("id", "text-block", true)?;
        assert_eq!(text_one.texts(true)?, Some(vec!["alpha".to_string()]));
        assert_eq!(
            text_one.texts(false)?,
            Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ])
        );
        let text_size = text_one.size()?.expect("text block size");
        assert!(text_size.0 > 0.0);
        assert!(text_size.1 > 0.0);

        let asset_one = page_items.filter_one().attr("id", "asset-link", true)?;
        assert_eq!(
            asset_one.src(500, true)?,
            Some(crate::ElementResource::Bytes(b"hello".to_vec()))
        );

        let single_option_one = page_items.filter_one().attr("id", "single-b", true)?;
        assert!(single_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('single-picker').value")?,
            Value::from("b")
        );
        let multi_option_one = page_items.filter_one().attr("id", "multi-a", true)?;
        assert!(multi_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('multi-a').selected")?,
            Value::from(true)
        );
        assert!(multi_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('multi-a').selected")?,
            Value::from(false)
        );

        let missing_page = page_items.filter_one().attr("data-role", "missing", true)?;
        assert!(!missing_page.click()?);
        assert!(!missing_page.clicker().left()?);
        assert!(!missing_page.input("noop")?);
        assert!(!missing_page.set().value("noop")?);
        assert!(!missing_page.scroll().to_top()?);
        assert!(!missing_page.select().by_text("noop")?);
        assert_eq!(missing_page.select().is_multi()?, None);
        assert!(!missing_page.clear()?);
        assert!(!missing_page.focus()?);
        assert!(!missing_page.hover()?);
        assert_eq!(missing_page.texts(false)?, None);
        assert_eq!(missing_page.size()?, None);
        assert_eq!(missing_page.src(500, true)?, None);

        let web_items = vec![
            WebElement::Browser(page.wait_for("css:[data-role='cta']", 1_000)?),
            WebElement::Browser(page.wait_for("css:#name", 1_000)?),
            WebElement::Browser(page.wait_for("css:#text-block", 1_000)?),
            WebElement::Browser(page.wait_for("css:#asset-link", 1_000)?),
            WebElement::Browser(page.wait_for("css:#single-b", 1_000)?),
            WebElement::Browser(page.wait_for("css:#multi-b", 1_000)?),
        ];
        let web_button_one = web_items.filter_one().attr("data-role", "cta", true)?;
        assert!(web_button_one.click()?);
        assert!(web_button_one.clicker().left()?);
        assert!(web_button_one.hover()?);
        assert_eq!(page.run_js("window.__oneClicks")?, Value::from(4));

        let web_input_one = web_items.filter_one().tag("input", true)?;
        assert!(web_input_one.focus()?);
        assert!(web_input_one.input("Delta")?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("Delta")
        );
        assert!(web_input_one.clear()?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("")
        );

        let web_text_one = web_items.filter_one().attr("id", "text-block", true)?;
        assert_eq!(web_text_one.texts(true)?, Some(vec!["alpha".to_string()]));
        assert_eq!(
            web_text_one.texts(false)?,
            Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ])
        );
        let web_text_size = web_text_one.size()?.expect("web text block size");
        assert!(web_text_size.0 > 0.0);
        assert!(web_text_size.1 > 0.0);

        let web_asset_one = web_items.filter_one().attr("id", "asset-link", true)?;
        assert_eq!(
            web_asset_one.src(500, true)?,
            Some(crate::ElementResource::Bytes(b"hello".to_vec()))
        );

        let web_single_option_one = web_items.filter_one().attr("id", "single-b", true)?;
        assert!(web_single_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('single-picker').value")?,
            Value::from("b")
        );
        let web_multi_option_one = web_items.filter_one().attr("id", "multi-b", true)?;
        assert!(web_multi_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('multi-b').selected")?,
            Value::from(true)
        );
        assert!(web_multi_option_one.clicker().left()?);
        assert_eq!(
            page.run_js("document.getElementById('multi-b').selected")?,
            Value::from(false)
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one interaction runtime regression");
}

#[test]
fn elements_one_runtime_config_supports_none_value_and_raise_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("elements-one-none-config").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
        )?;

        page.set_none_element_value(Some("missing"), true)?;
        let page_items = page.find_all(".item")?;
        let missing_page = page_items.filter_one().attr("data-role", "missing", true)?;
        assert_eq!(missing_page.text()?, Some("missing".to_string()));
        assert_eq!(missing_page.attr("id")?, Some("missing".to_string()));
        assert_eq!(
            missing_page.texts(false)?,
            Some(vec!["missing".to_string()])
        );
        assert_eq!(missing_page.property("id")?, Some(Value::from("missing")));
        assert_eq!(missing_page.comments()?, Some(vec!["missing".to_string()]));
        assert_eq!(missing_page.child_count()?, None);

        let web_items = vec![
            WebElement::Browser(page.wait_for("css:[data-role='keep']", 1_000)?),
            WebElement::Browser(page.wait_for("css:[data-role='other']", 1_000)?),
        ];
        let missing_web = web_items.filter_one().attr("data-role", "missing", true)?;
        assert_eq!(missing_web.text()?, Some("missing".to_string()));
        assert_eq!(
            missing_web.src(100, true)?,
            Some(crate::ElementResource::Text("missing".to_string()))
        );

        page.set_raise_when_ele_not_found(true)?;
        let error = page_items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("page items missing filter should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected page filter error: {error}"
        );

        let error = web_items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("web items missing filter should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected web filter error: {error}"
        );

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one runtime config regression");
}

#[test]
fn page_inherits_global_raise_when_ele_not_found_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_raise_when_ele_not_found(true);

    let (browser, temp_dir) =
        launch_headless_test_browser("page-global-none-config").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
        )?;

        let error = page
            .find_all(".item")?
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("missing filter should use global raise setting");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected page filter error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page global missing-element setting regression");
}

#[test]
fn singleton_tab_obj_reuses_runtime_page_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) =
        launch_headless_test_browser("page-singleton-enabled").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
        )?;

        page.set_raise_when_ele_not_found(true)?;
        let same_page = browser.get_page(&page.target_id())?;
        let error = same_page
            .find_all(".item")?
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("singleton page should reuse missing-element runtime setting");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected singleton page error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("singleton page runtime-state regression");
}

#[test]
fn singleton_tab_obj_returns_fresh_runtime_page_state_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (browser, temp_dir) =
        launch_headless_test_browser("page-singleton-disabled").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div class="item" data-role="keep">Alpha</div>
                        <div class="item" data-role="other">Beta</div>
                    `;
                    return true;
                })()"#,
        )?;

        page.set_raise_when_ele_not_found(true)?;
        let fresh_page = browser.get_page(&page.target_id())?;
        let items = fresh_page.find_all(".item")?;
        let missing = items.filter_one().attr("data-role", "missing", true)?;
        assert_eq!(missing.text()?, None);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-singleton page runtime-state regression");
}

#[test]
fn singleton_tab_obj_reuses_runtime_frame_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) =
        launch_headless_test_browser("frame-singleton-enabled").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("missing"), true)?;

        let same_frame = page.get_frame_context("css:#demo-frame")?;
        assert!(std::ptr::eq(
            frame.frame_element(),
            same_frame.frame_element()
        ));
        let frames = page.get_frames(Some("css:iframe"))?;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            frames[0].frame_element()
        ));
        let frames_with_timeout = page.get_frames_with_timeout(Some("css:iframe"), 500)?;
        assert_eq!(frames_with_timeout.len(), 1);
        assert_eq!(frames_with_timeout[0].id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            frames_with_timeout[0].frame_element()
        ));
        let frame_by_index_timeout = page.get_frame_by_index_with_timeout(1, 500)?;
        assert_eq!(frame_by_index_timeout.id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            frame_by_index_timeout.frame_element()
        ));
        assert_eq!(
            same_frame.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        assert_eq!(
            frames_with_timeout[0].ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("singleton frame runtime-state regression");
}

#[test]
fn singleton_frame_runtime_cache_prunes_detached_frames() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) =
        launch_headless_test_browser("frame-cache-prune").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        let old_frame_id = frame.id().to_string();
        frame.set_none_element_value(Some("stale"), true)?;

        assert!(
            page.frame_none_element_configs
                .lock()
                .expect("frame runtime cache")
                .contains_key(&old_frame_id)
        );
        assert!(
            page.frame_cache
                .lock()
                .expect("frame object cache")
                .contains_key(&old_frame_id)
        );

        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="replacement-frame"
                            srcdoc="<html><body><div id='inside'>fresh</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let replacement = page.get_frame_context("css:#replacement-frame")?;
        assert!(replacement.wait_for_doc_loaded(5_000)?);
        assert_ne!(replacement.id(), old_frame_id);

        let configs = page
            .frame_none_element_configs
            .lock()
            .expect("frame runtime cache");
        assert!(!configs.contains_key(&old_frame_id));
        assert!(configs.contains_key(replacement.id()));
        drop(configs);

        let frames = page.frame_cache.lock().expect("frame object cache");
        assert!(!frames.contains_key(&old_frame_id));
        assert!(frames.contains_key(replacement.id()));
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("detached frame runtime cache prune regression");
}

#[test]
fn singleton_tab_obj_returns_fresh_runtime_frame_state_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (browser, temp_dir) =
        launch_headless_test_browser("frame-singleton-disabled").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("missing"), true)?;

        let same_handle = page.get_frame_context(&frame)?;
        assert_eq!(
            same_handle.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        let same_owned_handle = page.get_frame_context(frame.clone())?;
        assert_eq!(
            same_owned_handle.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        let host = page.find("css:body")?;
        let same_handle_from_element = host.get_frame(&frame)?;
        assert_eq!(
            same_handle_from_element.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        let same_owned_handle_from_element = host.get_frame(frame.clone())?;
        assert_eq!(
            same_owned_handle_from_element
                .ele(".does-not-exist")?
                .text()?,
            Some("missing".to_string())
        );

        let fresh_frame = page.get_frame_context("css:#demo-frame")?;
        assert_eq!(fresh_frame.ele(".does-not-exist")?.text()?, None);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-singleton frame runtime-state regression");
}

#[test]
fn singleton_tab_obj_keeps_nested_runtime_frame_state_isolated_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (browser, temp_dir) = launch_headless_test_browser("nested-frame-singleton-disabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(2_000)?);
        outer.run_js(
            r#"(() => {
                    const frame = document.createElement('iframe');
                    frame.id = 'inner-frame';
                    frame.name = 'inner-frame';
                    frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                    document.getElementById('outer-host').appendChild(frame);
                    return true;
                })()"#,
        )?;

        let inner = outer.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(2_000)?);
        inner.set_none_element_value(Some("nested missing"), true)?;

        let same_handle = outer.get_frame(&inner)?;
        assert_eq!(
            same_handle.ele(".does-not-exist")?.text()?,
            Some("nested missing".to_string())
        );

        let host = outer.find("css:#outer-host")?;
        let same_handle_from_element = host.get_frame(&inner)?;
        assert_eq!(
            same_handle_from_element.ele(".does-not-exist")?.text()?,
            Some("nested missing".to_string())
        );

        let fresh_inner = outer.get_frame("css:#inner-frame")?;
        assert_eq!(fresh_inner.id(), inner.id());
        assert_eq!(fresh_inner.ele(".does-not-exist")?.text()?, None);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-singleton nested frame runtime-state regression");
}

#[test]
fn frame_initial_runtime_config_inherits_current_page_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("frame-runtime-config-inherit")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        page.set_none_element_value(Some("page-default"), true)?;
        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            frame.ele(".does-not-exist")?.text()?,
            Some("page-default".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("frame runtime-state inheritance regression");
}

#[test]
fn nested_frame_initial_runtime_config_inherits_parent_frame_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("nested-frame-config-inherit")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame_context("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("outer-default"), true)?;
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let inner = outer.get_frame_context("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            inner.ele(".does-not-exist")?.text()?,
            Some("outer-default".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("nested frame runtime-state inheritance regression");
}

#[test]
fn element_frame_initial_runtime_config_inherits_parent_frame_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("element-frame-config-inherit")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame_context("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("element-default"), true)?;
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let host = outer.find("css:#outer-host")?;
        let inner = host.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            inner.ele(".does-not-exist")?.text()?,
            Some("element-default".to_string())
        );

        inner.set_none_element_value(Some("element-target"), true)?;
        let inner_web_frame = WebFrame::Browser(inner.clone());
        let frame_timeout_target = outer.get_frame_with_timeout(inner.clone(), 10)?;
        assert_eq!(frame_timeout_target.id(), inner.id());
        assert_eq!(
            frame_timeout_target.ele(".does-not-exist")?.text()?,
            Some("element-target".to_string())
        );
        let element_timeout_target = host.get_frame_with_timeout(&inner_web_frame, 10)?;
        assert_eq!(element_timeout_target.id(), inner.id());
        assert_eq!(
            element_timeout_target.ele(".does-not-exist")?.text()?,
            Some("element-target".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element frame runtime-state inheritance regression");
}

#[test]
fn elements_one_frame_initial_runtime_config_inherits_parent_frame_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("elements-one-frame-config-inherit")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame_context("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("elements-one-default"), true)?;
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let owned_host = outer.ele("css:#outer-host")?;
        let owned_inner = owned_host
            .get_frame("css:#inner-frame")?
            .expect("owned ElementsOne should find inner frame");
        assert!(owned_inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            owned_inner.ele(".does-not-exist")?.text()?,
            Some("elements-one-default".to_string())
        );

        let hosts = outer.find_all("css:#outer-host")?;
        let borrowed_inner = hosts
            .filter_one()
            .get_frame("css:#inner-frame")?
            .expect("borrowed ElementsOne should find inner frame");
        assert!(borrowed_inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            borrowed_inner.ele(".does-not-exist")?.text()?,
            Some("elements-one-default".to_string())
        );

        owned_inner.set_none_element_value(Some("elements-one-target"), true)?;
        let inner_web_frame = WebFrame::Browser(owned_inner.clone());
        let owned_frame_target = owned_host
            .get_frame(&owned_inner)?
            .expect("owned ElementsOne should accept borrowed Frame target");
        assert_eq!(owned_frame_target.id(), owned_inner.id());
        assert_eq!(
            owned_frame_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let owned_frame_owned_target = owned_host
            .get_frame(owned_inner.clone())?
            .expect("owned ElementsOne should accept owned Frame target");
        assert_eq!(owned_frame_owned_target.id(), owned_inner.id());
        assert_eq!(
            owned_frame_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let owned_webframe_target = owned_host
            .get_frame(&inner_web_frame)?
            .expect("owned ElementsOne should accept borrowed WebFrame target");
        assert_eq!(owned_webframe_target.id(), owned_inner.id());
        assert_eq!(
            owned_webframe_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let owned_webframe_owned_target = owned_host
            .get_frame(inner_web_frame.clone())?
            .expect("owned ElementsOne should accept owned WebFrame target");
        assert_eq!(owned_webframe_owned_target.id(), owned_inner.id());
        assert_eq!(
            owned_webframe_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );

        let borrowed_frame_target = hosts
            .filter_one()
            .get_frame(&owned_inner)?
            .expect("borrowed ElementsOne should accept borrowed Frame target");
        assert_eq!(borrowed_frame_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_frame_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let borrowed_frame_owned_target = hosts
            .filter_one()
            .get_frame(owned_inner.clone())?
            .expect("borrowed ElementsOne should accept owned Frame target");
        assert_eq!(borrowed_frame_owned_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_frame_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let borrowed_webframe_target = hosts
            .filter_one()
            .get_frame(&inner_web_frame)?
            .expect("borrowed ElementsOne should accept borrowed WebFrame target");
        assert_eq!(borrowed_webframe_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_webframe_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let borrowed_webframe_owned_target = hosts
            .filter_one()
            .get_frame(inner_web_frame.clone())?
            .expect("borrowed ElementsOne should accept owned WebFrame target");
        assert_eq!(borrowed_webframe_owned_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_webframe_owned_target
                .ele(".does-not-exist")?
                .text()?,
            Some("elements-one-target".to_string())
        );

        let owned_frame_timeout_target = owned_host
            .get_frame_with_timeout(&owned_inner, 10)?
            .expect("owned ElementsOne timeout should accept borrowed Frame target");
        assert_eq!(owned_frame_timeout_target.id(), owned_inner.id());
        assert_eq!(
            owned_frame_timeout_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );
        let owned_webframe_timeout_target = owned_host
            .get_frame_with_timeout(inner_web_frame.clone(), 10)?
            .expect("owned ElementsOne timeout should accept owned WebFrame target");
        assert_eq!(owned_webframe_timeout_target.id(), owned_inner.id());
        assert_eq!(
            owned_webframe_timeout_target
                .ele(".does-not-exist")?
                .text()?,
            Some("elements-one-target".to_string())
        );

        let borrowed_frame_timeout_target = hosts
            .filter_one()
            .get_frame_with_timeout(owned_inner.clone(), 10)?
            .expect("borrowed ElementsOne timeout should accept owned Frame target");
        assert_eq!(borrowed_frame_timeout_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_frame_timeout_target
                .ele(".does-not-exist")?
                .text()?,
            Some("elements-one-target".to_string())
        );
        let borrowed_webframe_timeout_target = hosts
            .filter_one()
            .get_frame_with_timeout(&inner_web_frame, 10)?
            .expect("borrowed ElementsOne timeout should accept borrowed WebFrame target");
        assert_eq!(borrowed_webframe_timeout_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_webframe_timeout_target
                .ele(".does-not-exist")?
                .text()?,
            Some("elements-one-target".to_string())
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one frame runtime-state inheritance regression");
}

#[test]
fn singleton_tab_obj_keeps_elements_one_frame_state_isolated_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (browser, temp_dir) = launch_headless_test_browser("elements-one-frame-singleton-disabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="outer-frame" name="outer-frame"
                            srcdoc="<html><body><div id='outer-host'></div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let outer = page.get_frame_context("css:#outer-frame")?;
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.run_js(
            r#"(() => {
                    document.getElementById('outer-host').innerHTML = `
                        <iframe id="inner-frame" name="inner-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
        )?;

        let owned_host = outer.ele("css:#outer-host")?;
        let owned_inner = owned_host
            .get_frame("css:#inner-frame")?
            .expect("owned ElementsOne should find inner frame");
        assert!(owned_inner.wait_for_doc_loaded(5_000)?);
        owned_inner.set_none_element_value(Some("elements-one-target"), true)?;

        let same_owned_target = owned_host
            .get_frame(&owned_inner)?
            .expect("owned ElementsOne should accept borrowed Frame target");
        assert_eq!(
            same_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );

        let hosts = outer.find_all("css:#outer-host")?;
        let same_borrowed_target = hosts
            .filter_one()
            .get_frame(owned_inner.clone())?
            .expect("borrowed ElementsOne should accept owned Frame target");
        assert_eq!(
            same_borrowed_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target".to_string())
        );

        let fresh_owned_locator = owned_host
            .get_frame("css:#inner-frame")?
            .expect("owned ElementsOne should re-find inner frame");
        assert_eq!(fresh_owned_locator.id(), owned_inner.id());
        assert_eq!(fresh_owned_locator.ele(".does-not-exist")?.text()?, None);

        let fresh_borrowed_locator = hosts
            .filter_one()
            .get_frame("css:#inner-frame")?
            .expect("borrowed ElementsOne should re-find inner frame");
        assert_eq!(fresh_borrowed_locator.id(), owned_inner.id());
        assert_eq!(fresh_borrowed_locator.ele(".does-not-exist")?.text()?, None);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-singleton elements-one frame runtime-state regression");
}

#[test]
fn latest_tab_returns_page_reference_when_singleton_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("page-latest-tab-singleton-enabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let _page = browser.new_page(None)?;
        let expected_id = browser
            .tab_ids()?
            .into_iter()
            .next()
            .expect("tab_ids should include latest tab");
        let latest = browser
            .latest_tab()?
            .expect("latest tab should exist after new page");
        match latest {
            BrowserTabReference::Page(latest_page) => {
                assert_eq!(latest_page.target_id(), expected_id);
            }
            BrowserTabReference::WebPage(latest_page) => {
                panic!(
                    "singleton latest_tab from Page should return page, got webpage {}",
                    latest_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("singleton latest_tab should return page, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("singleton latest_tab return-type regression");
}

#[test]
fn latest_tab_returns_id_when_singleton_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (browser, temp_dir) = launch_headless_test_browser("page-latest-tab-singleton-disabled")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let _page = browser.new_page(None)?;
        let expected_id = browser
            .tab_ids()?
            .into_iter()
            .next()
            .expect("tab_ids should include latest tab");
        let latest = browser
            .latest_tab()?
            .expect("latest tab should exist after new page");
        match latest {
            BrowserTabReference::Id(id) => {
                assert_eq!(id, expected_id);
            }
            BrowserTabReference::WebPage(latest_page) => {
                panic!(
                    "non-singleton latest_tab from Page should return id, got webpage {}",
                    latest_page.target_id()
                );
            }
            BrowserTabReference::Page(latest_page) => {
                panic!(
                    "non-singleton latest_tab should return id, got page {}",
                    latest_page.target_id()
                );
            }
        }
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-singleton latest_tab return-type regression");
}

#[test]
fn page_chromium_page_tab_wrapper_signatures_accept_common_inputs() {
    fn assert_calls(page: &Page) {
        let tab_types = vec!["page".to_string(), "tab".to_string()];
        let target_ids = vec!["tab-1".to_string(), "tab-2".to_string()];
        let indices = vec![1usize, 2usize];
        let pages = vec![page];
        let selectors = [
            BrowserTabSelector::from("tab-1"),
            BrowserTabSelector::from(1usize),
        ];

        let _ = page.tabs_count();
        let _ = page.tab_ids();
        let _ = page.latest_tab();
        let _ = page.process_id();
        let _ = page.browser_version();
        let _ = page.address();
        let _ = page.reconnect(0);
        let _ = page.get_tab(Some("target-id"), None, None, None::<&str>, false);
        let _ = page.get_tab(Some(1usize), Some("Docs"), None, Some("page"), true);
        let _ = page.get_tab(
            Some(-1isize),
            None,
            Some("example"),
            Some(&tab_types),
            false,
        );
        let _ = page.get_tabs(None, None, Some("page"), false);
        let _ = page.get_tabs(Some("Docs"), Some("example"), Some(&tab_types), true);
        let _ = page.new_tab(None, false, true, false);
        let _ = page.new_tab(None, false, true, true);
        let _ = page.activate_tab("tab-1");
        let _ = page.activate_tab(1usize);
        let _ = page.activate_tab(page);
        let _ = page.activate_tab(page.clone());
        let _ = page.close_tabs("tab-1", false);
        let _ = page.close_tabs(1usize, false);
        let _ = page.close_tabs(page, false);
        let _ = page.close_tabs(page.clone(), false);
        let _ = page.close_tabs(&target_ids, false);
        let _ = page.close_tabs(&indices, false);
        let _ = page.close_tabs(&pages, false);
        let _ = page.close_tabs(&selectors[..], false);
        let _ = page.close_with_options(false, false);
        let _ = page.close_with_options(true, false);
        let _ = page.close_with_options(false, true);
        let _ = page.quit();
    }

    let _ = assert_calls as fn(&Page);
}

#[test]
fn page_listener_interceptor_alias_signatures_accept_calls() {
    fn assert_calls(page: &Page) {
        let _ = page.listener();
        let _ = page.listen();
        let _ = page.interceptor();
        let _ = page.intercept();
    }

    let _ = assert_calls as fn(&Page);
}

#[test]
fn frame_reconnect_signature_accepts_wait_argument() {
    fn assert_calls(frame: &Frame) {
        let _ = frame.reconnect(0);
    }

    let _ = assert_calls as fn(&Frame);
}

#[test]
fn page_and_frame_disconnect_signatures_accept_roundtrip_calls() {
    let _ = Page::disconnect as fn(Page) -> OpenPageResult<DisconnectedPage>;
    let _ = Frame::disconnect as fn(Frame) -> OpenPageResult<DisconnectedFrame>;
    let _ = DisconnectedPage::reconnect as fn(&DisconnectedPage, u64) -> OpenPageResult<Page>;
    let _ = DisconnectedFrame::reconnect as fn(&DisconnectedFrame, u64) -> OpenPageResult<Frame>;
}

#[test]
fn page_exposes_chromium_page_tab_wrappers_at_runtime() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) = launch_headless_test_browser("page-chromium-tab-wrappers")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert_eq!(page.tabs_count()?, browser.tabs_count()?);
        assert_eq!(page.tab_ids()?, browser.tab_ids()?);
        assert_eq!(page.address()?, browser.address());
        assert_eq!(page.browser_version()?, browser.version()?);
        assert_eq!(page.process_id(), browser.browser_pid());

        let current = page
            .get_tab(Some(&page), None, None, None::<&str>, false)?
            .expect("current tab should resolve");
        match current {
            BrowserTabReference::Page(current_page) => {
                assert_eq!(current_page.target_id(), page.target_id());
            }
            BrowserTabReference::WebPage(current_page) => {
                panic!(
                    "page.get_tab() should return page, got webpage {}",
                    current_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("current tab wrapper should return page, got id {id}");
            }
        }

        let expected_latest_id = page
            .tab_ids()?
            .into_iter()
            .next()
            .expect("tab_ids should include latest tab");
        let latest = page
            .latest_tab()?
            .expect("latest tab should exist after new page");
        match latest {
            BrowserTabReference::Page(latest_page) => {
                assert_eq!(latest_page.target_id(), expected_latest_id);
            }
            BrowserTabReference::WebPage(latest_page) => {
                panic!(
                    "singleton page.latest_tab() should return page, got webpage {}",
                    latest_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("singleton page.latest_tab() should return page, got id {id}");
            }
        }

        let new_tab = page.new_tab(Some("about:blank"), false, true, false)?;
        assert!(new_tab.wait_for_doc_loaded(5_000)?);
        page.activate_tab(&new_tab)?;

        let tab_types = ["page", "tab"];
        let tab_ids = page
            .get_tabs(None, None, Some(&tab_types[..]), true)?
            .into_iter()
            .map(|reference| match reference {
                BrowserTabReference::Id(id) => id,
                BrowserTabReference::WebPage(tab_page) => tab_page.target_id(),
                BrowserTabReference::Page(tab_page) => tab_page.target_id(),
            })
            .collect::<Vec<_>>();
        assert!(tab_ids.contains(&new_tab.target_id()));

        let closed_tab_id = new_tab.target_id();
        let closed = page.close_tabs(&new_tab, false)?;
        assert_eq!(closed, 1);
        wait_until(Duration::from_millis(5_000), || match page.tab_ids() {
            Ok(ids) if !ids.contains(&closed_tab_id) => Some(()),
            _ => None,
        })?;
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("chromium page tab wrapper regression");
}

#[test]
fn page_close_with_options_controls_current_and_other_tabs() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (browser, temp_dir) =
        launch_headless_test_browser("page-close-options").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        let current_id = page.target_id();

        let other = page.new_tab(Some("about:blank"), false, true, false)?;
        assert!(other.wait_for_doc_loaded(5_000)?);
        let other_id = other.target_id();
        assert!(page.tab_ids()?.contains(&other_id));

        page.close_with_options(true, false)?;
        wait_until(Duration::from_millis(5_000), || match page.tab_ids() {
            Ok(ids) if ids.contains(&current_id) && !ids.contains(&other_id) => Some(()),
            _ => None,
        })?;

        let closing = page.new_tab(Some("about:blank"), false, true, false)?;
        assert!(closing.wait_for_doc_loaded(5_000)?);
        let closing_id = closing.target_id();
        assert!(page.tab_ids()?.contains(&closing_id));

        closing.close_with_options(false, true)?;
        wait_until(Duration::from_millis(5_000), || match browser.tab_ids() {
            Ok(ids) if !ids.contains(&closing_id) => Some(()),
            _ => None,
        })?;
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page close_with_options runtime regression");
}

#[test]
fn browser_page_and_frame_reconnect_rebuild_fresh_connections() {
    let (browser, temp_dir) = launch_headless_test_browser("browser-page-frame-reconnect")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<Page> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = '<iframe id="demo-frame" srcdoc="<html><body><div id=&quot;msg&quot;>frame reconnect</div></body></html>"></iframe><div id="msg">page reconnect</div>';
                    return true;
                })()"#,
            )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);

        let reconnected_browser = browser
            .reconnect()
            .map_err(|err| OpenPageError::PageOperation(format!("browser reconnect: {err}")))?;
        assert_eq!(reconnected_browser.address(), browser.address());
        assert_eq!(reconnected_browser.process_id(), browser.process_id());
        let browser_page = reconnected_browser
            .get_page(&page.target_id())
            .map_err(|err| OpenPageError::PageOperation(format!("browser get_page: {err}")))?;
        assert_eq!(browser_page.target_id(), page.target_id());
        assert_eq!(
            browser_page
                .run_js("document.querySelector('#msg').textContent")
                .map_err(|err| {
                    OpenPageError::PageOperation(format!("browser page run_js: {err}"))
                })?,
            Value::from("page reconnect")
        );

        let reconnected_page = page
            .reconnect(0)
            .map_err(|err| OpenPageError::PageOperation(format!("page reconnect: {err}")))?;
        assert_eq!(reconnected_page.target_id(), page.target_id());
        assert_eq!(reconnected_page.address()?, page.address()?);
        assert_eq!(reconnected_page.process_id(), page.process_id());
        assert_eq!(
            reconnected_page
                .run_js("document.querySelector('#msg').textContent")
                .map_err(|err| OpenPageError::PageOperation(format!("page run_js: {err}")))?,
            Value::from("page reconnect")
        );

        let reconnected_frame = frame
            .reconnect(0)
            .map_err(|err| OpenPageError::PageOperation(format!("frame reconnect: {err}")))?;
        assert_eq!(
            reconnected_frame
                .run_js("document.querySelector('#msg').textContent")
                .map_err(|err| OpenPageError::PageOperation(format!("frame run_js: {err}")))?,
            Value::from("frame reconnect")
        );

        let disconnected_page = reconnected_page.clone().disconnect()?;
        let roundtrip_page = disconnected_page.reconnect(0)?;
        assert_eq!(roundtrip_page.target_id(), page.target_id());
        assert_eq!(
            roundtrip_page.run_js("document.querySelector('#msg').textContent")?,
            Value::from("page reconnect")
        );

        let disconnected_frame = reconnected_frame.disconnect()?;
        let roundtrip_frame = disconnected_frame.reconnect(0)?;
        assert_eq!(
            roundtrip_frame.run_js("document.querySelector('#msg').textContent")?,
            Value::from("frame reconnect")
        );

        Ok(roundtrip_page)
    })();

    let reconnected_page = match result {
        Ok(page) => page,
        Err(err) => {
            let _ = browser.close();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("reconnect regression failed before cleanup: {err}");
        }
    };

    let close_result = reconnected_page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser after reconnect: {err}");
    }
}

#[test]
fn frame_reconnect_preserves_runtime_config() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("frame-reconnect-runtime-config")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<Page> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>frame reconnect</div></body></html>">
                        </iframe>
                    `;
                    return true;
                })()"#,
            )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("frame missing"), true)?;

        let reconnected = frame.reconnect(0)?;
        assert_eq!(
            reconnected.ele(".does-not-exist")?.text()?,
            Some("frame missing".to_string())
        );

        let disconnected = reconnected.disconnect()?;
        let roundtrip = disconnected.reconnect(0)?;
        assert_eq!(
            roundtrip.ele(".does-not-exist")?.text()?,
            Some("frame missing".to_string())
        );
        Ok(roundtrip.owner().clone())
    })();

    let page = match result {
        Ok(page) => page,
        Err(err) => {
            let _ = browser.close();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("frame reconnect runtime config regression failed before cleanup: {err}");
        }
    };

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser after frame reconnect config test: {err}");
    }
}

#[test]
fn page_new_tab_with_new_context_creates_and_closes_isolated_tab() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) =
        launch_headless_test_browser("page-new-tab-new-context").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let new_tab = page.new_tab(Some("about:blank"), false, true, true)?;
        assert!(new_tab.wait_for_doc_loaded(5_000)?);

        let target_id = new_tab.target_id();
        new_tab.close()?;

        wait_until(Duration::from_secs(5), || {
            let tab_ids = browser.tab_ids().ok()?;
            if tab_ids.iter().all(|tab_id| tab_id != &target_id) {
                Some(())
            } else {
                None
            }
        })?;
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("new_context page tab regression");
}

#[test]
fn page_wait_failures_raise_timeout_when_global_setting_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_raise_when_wait_failed(true);
    Settings::set_language("cn");

    let (load_url, load_server) = spawn_delayed_load_site(Duration::from_millis(250));
    let (browser, temp_dir) =
        launch_headless_test_browser("page-global-wait-failed").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let load_url_json = serde_json::to_string(&load_url)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        page.run_js(&format!("window.location.href = {load_url_json};"))?;
        assert!(page.wait_for_load_start(1_000)?);

        let error = page
            .wait_for_doc_loaded(50)
            .expect_err("wait_for_doc_loaded should raise timeout");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::wait_for_doc_loaded()") && message.contains("等待超时")),
            "unexpected wait error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    let _ = load_server.join();

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page global wait-failed setting regression");
}

#[test]
fn page_execute_cdp_respects_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) =
        launch_headless_test_browser("page-global-cdp-timeout").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        Settings::set_cdp_timeout(0.01);

        let params = EvaluateParams::builder()
            .expression("new Promise(resolve => setTimeout(() => resolve('ok'), 150))")
            .await_promise(true)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        let error = page
            .execute_cdp(params)
            .expect_err("execute_cdp should respect global timeout");
        assert!(
            matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::execute_cdp()")),
            "unexpected cdp timeout error: {error}"
        );
        Ok(())
    })();

    Settings::reset();
    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page global cdp-timeout setting regression");
}

#[test]
fn page_navigation_listener_registration_respects_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");
    let result = runtime.block_on(async {
        register_navigation_listener_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), &'static str>(())
            },
            "register navigation lifecycle listener",
        )
        .await
    });

    Settings::reset();

    let error = result.expect_err("navigation listener registration should time out");
    assert!(
        matches!(error, OpenPageError::Timeout(ref message) if message.contains("register navigation lifecycle listener")),
        "unexpected navigation registration timeout error: {error}"
    );
}

#[test]
fn page_navigation_snapshot_reports_tracker_initialization_errors() {
    let shared = Arc::new(NavigationShared::new(PageNavigationSnapshot {
        current_url: Some("about:blank".to_string()),
        ..PageNavigationSnapshot::default()
    }));
    let tracker = NavigationTracker {
        shared: Arc::clone(&shared),
    };

    assert_eq!(
        tracker
            .snapshot()
            .expect("navigation snapshot without error")
            .current_url
            .as_deref(),
        Some("about:blank")
    );

    super::set_navigation_last_error(&shared, "navigation setup failed".to_string());
    let error = tracker
        .snapshot()
        .expect_err("navigation setup error should be reported")
        .to_string();
    assert!(error.contains("navigation setup failed"));
}

#[test]
fn page_is_alive_respects_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");
    let result = runtime.block_on(async {
        run_with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), OpenPageError>(())
            },
            timeout_duration_millis(cdp_timeout_duration()),
            "Page::is_alive()",
        )
        .await
    });

    Settings::reset();

    let error = result.expect_err("page is_alive should time out");
    assert!(
        matches!(error, OpenPageError::Timeout(ref message) if message.contains("Page::is_alive()")),
        "unexpected page is_alive timeout error: {error}"
    );
}

#[test]
fn page_cookie_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");
    let result = runtime.block_on(async {
        run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), &'static str>(())
            },
            "set cookie",
        )
        .await
    });

    Settings::reset();

    let error = result.expect_err("page cookie operation should time out");
    assert!(
        matches!(error, OpenPageError::Timeout(ref message) if message.contains("set cookie")),
        "unexpected page cookie timeout error: {error}"
    );
}

#[test]
fn page_url_and_title_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let url_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Option<String>, &'static str>(Some("https://example.com".to_string()))
            },
            "read url",
        ))
        .expect_err("page url operation should time out");
    assert!(
        matches!(url_error, OpenPageError::Timeout(ref message) if message.contains("read url")),
        "unexpected page url timeout error: {url_error}"
    );

    let title_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Option<String>, &'static str>(Some("example".to_string()))
            },
            "read title",
        ))
        .expect_err("page title operation should time out");

    Settings::reset();

    assert!(
        matches!(title_error, OpenPageError::Timeout(ref message) if message.contains("read title")),
        "unexpected page title timeout error: {title_error}"
    );
}

#[test]
fn page_content_and_visual_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let html_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<String, &'static str>("<html></html>".to_string())
            },
            "read html",
        ))
        .expect_err("page html operation should time out");
    assert!(
        matches!(html_error, OpenPageError::Timeout(ref message) if message.contains("read html")),
        "unexpected page html timeout error: {html_error}"
    );

    let screenshot_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Vec<u8>, &'static str>(vec![1, 2, 3])
            },
            "capture screenshot",
        ))
        .expect_err("page screenshot operation should time out");

    Settings::reset();

    assert!(
        matches!(screenshot_error, OpenPageError::Timeout(ref message) if message.contains("capture screenshot")),
        "unexpected page screenshot timeout error: {screenshot_error}"
    );
}

#[test]
fn page_lookup_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let lookup_error = runtime
        .block_on(run_page_lookup_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), &'static str>(())
            },
            "find element",
        ))
        .expect_err("page lookup should time out");
    assert!(
        matches!(lookup_error, OpenPageError::Timeout(ref message) if message.contains("find element")),
        "unexpected page lookup timeout error: {lookup_error}"
    );

    Settings::reset();

    let lookup_error = runtime
        .block_on(run_page_lookup_future_with_cdp_timeout(
            async { Err::<(), &'static str>("missing") },
            "find element",
        ))
        .expect_err("page lookup failure should remain ElementNotFound");
    assert!(
        matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "page operation find element failed: missing"),
        "unexpected page lookup error: {lookup_error}"
    );

    Settings::set_language("cn");

    let lookup_error = runtime
        .block_on(run_page_lookup_future_with_cdp_timeout(
            async { Err::<(), &'static str>("missing") },
            "find element",
        ))
        .expect_err("page lookup failure should localize");
    assert!(
        matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "页面操作 find element 失败: missing"),
        "unexpected localized page lookup error: {lookup_error}"
    );
}

#[test]
fn page_cookie_pdf_and_close_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let cookie_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Vec<chromiumoxide::cdp::browser_protocol::network::Cookie>, &'static str>(
                    Vec::new(),
                )
            },
            "read cookies",
        ))
        .expect_err("page cookie read should time out");
    assert!(
        matches!(cookie_error, OpenPageError::Timeout(ref message) if message.contains("read cookies")),
        "unexpected page cookie read timeout error: {cookie_error}"
    );

    let pdf_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), &'static str>(())
            },
            "save pdf",
        ))
        .expect_err("page save_pdf should time out");

    Settings::reset();

    assert!(
        matches!(pdf_error, OpenPageError::Timeout(ref message) if message.contains("save pdf")),
        "unexpected page save_pdf timeout error: {pdf_error}"
    );
}

#[test]
fn page_frame_metadata_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let frame_name_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Option<String>, &'static str>(Some("frame-a".to_string()))
            },
            "read frame name",
        ))
        .expect_err("page frame name read should time out");
    assert!(
        matches!(frame_name_error, OpenPageError::Timeout(ref message) if message.contains("read frame name")),
        "unexpected page frame name timeout error: {frame_name_error}"
    );

    let frame_context_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Option<ExecutionContextId>, &'static str>(None)
            },
            "read frame execution context",
        ))
        .expect_err("page frame execution context read should time out");

    Settings::reset();

    assert!(
        matches!(frame_context_error, OpenPageError::Timeout(ref message) if message.contains("read frame execution context")),
        "unexpected page frame execution context timeout error: {frame_context_error}"
    );
}

#[test]
fn page_navigation_operations_respect_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let navigate_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<(), &'static str>(())
            },
            "navigate",
        ))
        .expect_err("page navigation should time out");
    assert!(
        matches!(navigate_error, OpenPageError::Timeout(ref message) if message.contains("navigate")),
        "unexpected page navigation timeout error: {navigate_error}"
    );

    let cookie_helper_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Vec<chromiumoxide::cdp::browser_protocol::network::Cookie>, &'static str>(
                    Vec::new(),
                )
            },
            "read cookies",
        ))
        .expect_err("page cookie helper read should time out");

    Settings::reset();

    assert!(
        matches!(cookie_helper_error, OpenPageError::Timeout(ref message) if message.contains("read cookies")),
        "unexpected page cookie helper timeout error: {cookie_helper_error}"
    );
}

#[test]
fn page_pdf_generation_respects_global_timeout_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_cdp_timeout(0.01);

    let runtime = Runtime::new().expect("create tokio runtime");

    let pdf_error = runtime
        .block_on(run_page_future_with_cdp_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok::<Vec<u8>, &'static str>(vec![1, 2, 3])
            },
            "print pdf",
        ))
        .expect_err("page pdf generation should time out");

    Settings::reset();

    assert!(
        matches!(pdf_error, OpenPageError::Timeout(ref message) if message.contains("print pdf")),
        "unexpected page pdf timeout error: {pdf_error}"
    );
}

#[test]
fn page_and_element_frame_index_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("page-frame-index-localization")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `<div id="host"></div>`;
                    return true;
                })()"#,
        )?;

        let host = page.find("css:#host")?;
        let assert_error = |label: &str, err: OpenPageError, expected: &str| {
            assert!(
                matches!(err, OpenPageError::ElementNotFound(ref message) if message == expected),
                "unexpected {label} error: {err}"
            );
        };

        assert_error(
            "page.get_frame(0)",
            page.get_frame(0isize)
                .err()
                .expect("page.get_frame(0) should fail"),
            "frame index must start from 1 or use negative indices from -1",
        );
        assert_error(
            "host.get_frame(0)",
            host.get_frame(0isize)
                .err()
                .expect("host.get_frame(0) should fail"),
            "frame index must start from 1 or use negative indices from -1",
        );
        assert_error(
            "page.get_frame(1)",
            page.get_frame(1isize)
                .err()
                .expect("page.get_frame(1) should fail without any frame"),
            "frame index out of range: 1",
        );

        page.run_js(
            r#"(() => {
                    const frame = document.createElement('iframe');
                    frame.id = 'child-frame';
                    frame.srcdoc = "<html><body>child</body></html>";
                    document.getElementById('host').appendChild(frame);
                    return true;
                })()"#,
        )?;

        assert_error(
            "host.get_frame(2)",
            host.get_frame(2isize)
                .err()
                .expect("host.get_frame(2) should fail with one frame"),
            "frame index out of range: 2",
        );

        Settings::set_language("cn");

        assert_error(
            "page.get_frame(0) localized",
            page.get_frame(0isize)
                .err()
                .expect("page.get_frame(0) should localize"),
            "frame 序号必须从 1 开始，或使用从 -1 开始的负序号",
        );
        assert_error(
            "host.get_frame(0) localized",
            host.get_frame(0isize)
                .err()
                .expect("host.get_frame(0) should localize"),
            "frame 序号必须从 1 开始，或使用从 -1 开始的负序号",
        );
        assert_error(
            "page.get_frame(2) localized",
            page.get_frame(2isize)
                .err()
                .expect("page.get_frame(2) should localize"),
            "frame 序号超出范围: 2",
        );
        assert_error(
            "host.get_frame(2) localized",
            host.get_frame(2isize)
                .err()
                .expect("host.get_frame(2) should localize"),
            "frame 序号超出范围: 2",
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page frame index localization regression");
}

#[test]
fn page_ele_runtime_config_supports_none_value_and_nested_queries() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-ele-none-config").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <section id="card">
                            <span class="name">Alpha</span>
                            <span class="phone">10086</span>
                        </section>
                        <section id="tail">Omega</section>
                    `;
                    return true;
                })()"#,
        )?;

        assert_eq!(page.eles(".missing")?.len(), 0);

        let card = page.ele("#card")?;
        assert!(card.is_some());
        assert_eq!(card.ele(".name")?.text()?, Some("Alpha".to_string()));

        let missing_default = page.ele(".missing")?;
        assert!(missing_default.is_none());
        assert_eq!(missing_default.text()?, None);
        assert!(!missing_default.click()?);

        page.set_none_element_value(Some("missing"), true)?;

        let missing = page.ele(".missing")?;
        assert_eq!(missing.text()?, Some("missing".to_string()));
        assert_eq!(missing.attr("id")?, Some("missing".to_string()));
        assert_eq!(missing.ele(".child")?.text()?, Some("missing".to_string()));
        assert_eq!(missing.child()?.text()?, Some("missing".to_string()));
        assert_eq!(missing.parent()?.text()?, Some("missing".to_string()));
        assert_eq!(missing.next()?.text()?, Some("missing".to_string()));
        assert_eq!(missing.before()?.text()?, Some("missing".to_string()));
        assert_eq!(missing.after()?.text()?, Some("missing".to_string()));
        assert_eq!(missing.over()?.text()?, Some("missing".to_string()));
        assert_eq!(
            missing
                .offset::<&str>(None, Some(0.0), Some(0.0), 50)?
                .text()?,
            Some("missing".to_string())
        );
        assert_eq!(
            missing.east(None::<&str>, None, 1)?.text()?,
            Some("missing".to_string())
        );
        assert_eq!(
            page.ele("#card")?.ele(".phone")?.text()?,
            Some("10086".to_string())
        );
        assert!(missing.wait().deleted(100)?);

        page.set_raise_when_ele_not_found(true)?;
        let error = page.ele(".missing").expect_err("page ele should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected page ele error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page ele runtime config regression");
}

#[test]
fn elements_one_owned_shadow_root_supports_existing_and_missing_elements() {
    let (browser, temp_dir) =
        launch_headless_test_browser("elements-one-shadow-root").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <div id="host"></div>
                        <div id="plain">Plain</div>
                    `;
                    const host = document.getElementById('host');
                    const root = host.attachShadow({mode: 'open'});
                    root.innerHTML = `
                        <span class="inside">Shadow Text</span>
                        <span class="inside">Shadow Extra</span>
                    `;
                    return true;
                })()"#,
        )?;

        let host = page.ele("#host")?;
        let shadow_root = host.shadow_root()?.expect("host shadow root");
        assert!(shadow_root.inner_html()?.contains("Shadow Text"));
        let inside = shadow_root.find(".inside").expect("shadow root css find");
        assert_eq!(inside.text()?, Some("Shadow Text".to_string()));
        let inside_by_xpath = shadow_root
            .find("xpath:.//*[@class='inside']")
            .expect("shadow root xpath find");
        assert_eq!(inside_by_xpath.text()?, Some("Shadow Text".to_string()));
        let inside_list = shadow_root
            .find_all(".inside")
            .expect("shadow root css find_all");
        assert_eq!(inside_list.len(), 2);
        assert_eq!(inside_list[0].text()?, Some("Shadow Text".to_string()));
        assert_eq!(inside_list[1].text()?, Some("Shadow Extra".to_string()));
        let inside_xpath_list = shadow_root
            .find_all("xpath:.//*[@class='inside']")
            .expect("shadow root xpath find_all");
        assert_eq!(inside_xpath_list.len(), 2);
        assert_eq!(
            inside_xpath_list[0].text()?,
            Some("Shadow Text".to_string())
        );
        assert_eq!(
            inside_xpath_list[1].text()?,
            Some("Shadow Extra".to_string())
        );
        let direct_child = shadow_root
            .child_with(Some("xpath:./span[@class='inside']"), 2)
            .expect("shadow root xpath child");
        assert_eq!(direct_child.text()?, Some("Shadow Extra".to_string()));
        let direct_children = shadow_root
            .children_with(Some("xpath:./span[@class='inside']"))
            .expect("shadow root xpath children");
        assert_eq!(direct_children.len(), 2);
        let shadow_root_alias = host.sr()?.expect("host sr alias");
        assert!(shadow_root_alias.inner_html()?.contains("Shadow Text"));

        let plain = page.ele("#plain")?;
        assert!(plain.shadow_root()?.is_none());

        let web_host = page.ele("#host")?.map(WebElement::Browser);
        let web_shadow_root = web_host.shadow_root()?.expect("web host shadow root");
        assert!(web_shadow_root.inner_html()?.contains("Shadow Text"));
        let direct_web = WebElement::Browser(page.wait_for("css:#host", 1_000)?);
        let direct_web_shadow = direct_web.sr()?.expect("direct web sr alias");
        assert!(direct_web_shadow.inner_html()?.contains("Shadow Text"));

        page.set_none_element_value(Some("missing"), true)?;
        let missing = page.ele(".missing")?;
        assert!(missing.shadow_root()?.is_none());
        assert!(missing.sr()?.is_none());

        let missing_web = page.ele(".missing")?.map(WebElement::Browser);
        assert!(missing_web.shadow_root()?.is_none());
        assert!(missing_web.sr()?.is_none());

        page.set_raise_when_ele_not_found(true)?;
        let error = missing
            .shadow_root()
            .expect_err("missing shadow_root should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected missing shadow_root error: {error}"
        );
        let error = missing_web
            .shadow_root()
            .expect_err("missing web shadow_root should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected missing web shadow_root error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one owned shadow root regression");
}

#[test]
fn elements_one_supports_set_scroll_and_select_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("elements-one-objects").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input class="item" id="name" value="">
                        <input class="item" id="agree" type="checkbox">
                        <div class="item" id="content">Old</div>
                        <div class="item" id="scrollbox" style="width:120px;height:80px;overflow:auto;">
                            <div style="width:600px;height:400px;"></div>
                        </div>
                        <select class="item" id="picker" multiple>
                            <option value="one">One</option>
                            <option value="two">Two</option>
                            <option value="three">Three</option>
                        </select>
                        <select class="item" id="single-picker">
                            <option value="solo">Solo</option>
                            <option value="duo">Duo</option>
                        </select>
                    `;
                    return true;
                })()"#,
            )?;

        let page_items = page.find_all(".item")?;
        let input_one = page_items.filter_one().tag("input", true)?;
        assert!(input_one.set().value("Omega")?);
        assert!(input_one.set().attr("data-role", "primary")?);
        assert!(input_one.set().property("tabIndex", &Value::from(3))?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("Omega")
        );
        assert_eq!(
            page.run_js("document.getElementById('name').getAttribute('data-role')")?,
            Value::from("primary")
        );
        assert_eq!(
            page.run_js("document.getElementById('name').tabIndex")?,
            Value::from(3)
        );
        assert!(input_one.remove_attr("data-role")?);
        assert_eq!(
            page.run_js("document.getElementById('name').getAttribute('data-role') === null")?,
            Value::from(true)
        );

        let content_one = page_items.filter_one().attr("id", "content", true)?;
        assert!(content_one.set().inner_html("<span>Changed</span>")?);
        assert!(content_one.set().style("display", "block")?);
        assert_eq!(
            page.run_js("document.getElementById('content').textContent")?,
            Value::from("Changed")
        );
        let content_one_select_err = content_one
            .select()
            .is_multi()
            .expect_err("div ElementsOne select().is_multi() should error");
        assert!(matches!(
            content_one_select_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let content_one_direct_select_err = content_one
            .select_by_text("noop")
            .expect_err("div ElementsOne select_by_text() should error");
        assert!(matches!(
            content_one_direct_select_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));

        let scroll_one = page_items.filter_one().attr("id", "scrollbox", true)?;
        assert!(scroll_one.scroll().to_location(30.0, 40.0)?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollTop")?,
            Value::from(40)
        );
        assert!(scroll_one.scroll().to_top()?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollTop")?,
            Value::from(0)
        );
        assert!(scroll_one.scroll().to_half()?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollTop > 0")?,
            Value::from(true)
        );
        assert!(scroll_one.scroll().to_rightmost()?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollLeft > 0")?,
            Value::from(true)
        );
        assert!(scroll_one.scroll().to_leftmost()?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollLeft")?,
            Value::from(0)
        );
        assert!(scroll_one.scroll_to_bottom()?);
        assert!(scroll_one.scroll().up(15.0)?);
        assert!(scroll_one.scroll().down(15.0)?);
        assert!(scroll_one.scroll().left(10.0)?);
        assert!(scroll_one.scroll().right(10.0)?);
        assert!(scroll_one.scroll().to_see(Some(true))?);
        assert!(scroll_one.scroll().to_center()?);

        let checkbox_one = page_items.filter_one().attr("id", "agree", true)?;
        assert!(checkbox_one.check(false, true)?);
        assert_eq!(
            page.run_js("document.getElementById('agree').checked")?,
            Value::from(true)
        );
        assert!(checkbox_one.uncheck(true)?);
        assert_eq!(
            page.run_js("document.getElementById('agree').checked")?,
            Value::from(false)
        );

        let select_one = page_items
            .filter_one()
            .attr("id", "picker", true)
            .map_err(|err| OpenPageError::PageOperation(format!("select_one picker: {err}")))?;
        assert_eq!(select_one.select_is_multi()?, Some(true));
        assert_eq!(select_one.select().is_multi()?, Some(true));
        assert_eq!(select_one.select_options()?.unwrap().len(), 3);
        assert_eq!(select_one.select().options()?.unwrap().len(), 3);
        assert!(select_one.select_by_value(["one", "three"])?);
        assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("one,three")
            );
        assert_eq!(select_one.select_selected_options()?.unwrap().len(), 2);
        assert_eq!(select_one.select().selected_options()?.unwrap().len(), 2);
        assert_eq!(
            select_one
                .select_selected_option()?
                .and_then(|option| option.value().ok())
                .flatten(),
            Some("one".to_string())
        );
        assert!(select_one.select_clear()?);
        assert!(select_one.select().clear()?);
        assert!(select_one.select_by_locator("css:option[value='two']")?);
        assert!(select_one.select().by_index(2)?);
        assert!(select_one.select_by_index(2)?);
        assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("two")
            );
        assert!(
            select_one
                .select()
                .cancel_by_locator("css:option[value='two']")?
        );
        assert!(select_one.select().by_indices(&[1, 3])?);
        let select_options = select_one.select_options()?.unwrap();
        let select_option_refs = [&select_options[0], &select_options[2]];
        assert!(select_one.cancel_by_options(&select_option_refs)?);
        assert!(select_one.select().all()?);
        assert!(select_one.select_invert()?);
        assert_eq!(
            page.run_js("document.getElementById('picker').selectedOptions.length")?,
            Value::from(0)
        );

        let single_select_one = page_items
            .filter_one()
            .attr("id", "single-picker", true)
            .map_err(|err| {
                OpenPageError::PageOperation(format!("single_select_one filter: {err}"))
            })?;
        assert_eq!(
            single_select_one
                .select_is_multi()
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "single_select_one.select_is_multi(): {err}"
                )))?,
            Some(false)
        );
        assert_eq!(
            single_select_one.select().is_multi().map_err(|err| {
                OpenPageError::PageOperation(format!(
                    "single_select_one.select().is_multi(): {err}"
                ))
            })?,
            Some(false)
        );
        let single_select_all_err = single_select_one
            .select_all()
            .expect_err("single ElementsOne select_all() should error");
        assert!(matches!(
            single_select_all_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let single_select_clear_err = single_select_one
            .select()
            .clear()
            .expect_err("single ElementsOne select().clear() should error");
        assert!(matches!(
            single_select_clear_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let single_select_invert_err = single_select_one
            .select_invert()
            .expect_err("single ElementsOne select_invert() should error");
        assert!(matches!(
            single_select_invert_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        assert_eq!(
            page.run_js("document.getElementById('single-picker').value")?,
            Value::from("solo")
        );

        let web_items = vec![
            WebElement::Browser(page.wait_for("css:#name", 1_000)?),
            WebElement::Browser(page.wait_for("css:#content", 1_000)?),
            WebElement::Browser(page.wait_for("css:#scrollbox", 1_000)?),
            WebElement::Browser(page.wait_for("css:#picker", 1_000)?),
        ];
        let web_input_one = web_items.filter_one().tag("input", true)?;
        assert!(web_input_one.set().value("Sigma")?);
        assert_eq!(
            page.run_js("document.getElementById('name').value")?,
            Value::from("Sigma")
        );
        let web_input_one_select_err = web_input_one
            .select()
            .is_multi()
            .expect_err("input ElementsOne<WebElement> select().is_multi() should error");
        assert!(matches!(
            web_input_one_select_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        assert!(web_input_one.set().attr("data-extra", "demo")?);
        assert!(web_input_one.set().property("tabIndex", &Value::from(5))?);
        assert!(web_input_one.remove_attr("data-extra")?);
        assert_eq!(
            page.run_js("document.getElementById('name').tabIndex")?,
            Value::from(5)
        );

        let web_scroll_one = web_items.filter_one().attr("id", "scrollbox", true)?;
        assert!(web_scroll_one.scroll().to_location(10.0, 20.0)?);
        assert!(web_scroll_one.scroll().to_rightmost()?);
        assert!(web_scroll_one.scroll().to_leftmost()?);
        assert!(web_scroll_one.scroll().up(5.0)?);
        assert!(web_scroll_one.scroll().down(5.0)?);
        assert_eq!(
            page.run_js("document.getElementById('scrollbox').scrollTop")?,
            Value::from(20)
        );

        let web_select_one = web_items
            .filter_one()
            .attr("id", "picker", true)
            .map_err(|err| OpenPageError::PageOperation(format!("web_select_one picker: {err}")))?;
        assert_eq!(web_select_one.select_is_multi()?, Some(true));
        assert_eq!(web_select_one.select().is_multi()?, Some(true));
        assert!(web_select_one.select_by_text(["One", "Two"])?);
        assert_eq!(
            web_select_one.select().selected_options()?.unwrap().len(),
            2
        );
        assert_eq!(
            page.run_js("document.getElementById('picker').selectedOptions.length")?,
            Value::from(2)
        );
        assert!(web_select_one.select().cancel_by_value(["one", "two"])?);
        assert!(web_select_one.select_by_locator("css:option[value='three']")?);
        assert!(web_select_one.select().clear()?);
        assert!(web_select_one.select_clear()?);
        assert_eq!(
            page.run_js("document.getElementById('picker').selectedOptions.length")?,
            Value::from(0)
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one object-ops runtime regression");
}

#[test]
fn elements_one_object_wrappers_support_clicker_and_select_waiting_at_runtime() {
    let (browser, temp_dir) = launch_headless_test_browser("elements-one-clicker-select")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <a class="item" id="open-tab" href="about:blank#elements-one-open" target="_blank">Open tab</a>
                        <a class="item" id="middle-tab" href="about:blank#elements-one-middle">Middle tab</a>
                        <button class="item" id="click-target">Click target</button>
                        <select class="item" id="picker" multiple></select>
                    `;
                    window.__clicks = 0;
                    window.__rightClicks = 0;
                    document.getElementById('click-target').addEventListener('click', () => {
                        window.__clicks += 1;
                    });
                    document.getElementById('click-target').addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__rightClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

        let page_items = page.find_all(".item")?;
        let button_one = page_items.filter_one().attr("id", "click-target", true)?;
        assert!(button_one.clicker().multi(2)?);
        assert!(button_one.clicker().at(Some(5.0), Some(5.0), "left", 1)?);
        assert_eq!(page.run_js("window.__clicks")?, Value::from(3));

        let select_one = page_items.filter_one().attr("id", "picker", true)?;
        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const one = document.createElement('option');
                        one.value = 'late-one';
                        one.text = 'Late One';
                        picker.appendChild(one);
                        const two = document.createElement('option');
                        two.value = 'late-two';
                        two.text = 'Late Two';
                        picker.appendChild(two);
                    }, 150);
                    return true;
                })()"#,
        )?;
        assert!(
            select_one
                .select()
                .by_value_with_timeout(["late-one", "late-two"], Some(1_000))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "elements one select by_value_with_timeout: {err}"
                )))?
        );
        assert_eq!(
                page.run_js("Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')")?,
                Value::from("late-one,late-two")
            );

        let missing_page = page_items.filter_one().attr("id", "missing", true)?;
        assert!(!missing_page.clicker().left()?);
        assert!(
            !missing_page
                .select()
                .by_text_with_timeout("noop", Some(100))?
        );
        assert_eq!(missing_page.select().is_multi()?, None);

        let web_items = vec![
            WebElement::Browser(page.wait_for("css:#click-target", 1_000)?),
            WebElement::Browser(page.wait_for("css:#open-tab", 1_000)?),
            WebElement::Browser(page.wait_for("css:#picker", 1_000)?),
        ];
        let web_button_one = web_items.filter_one().attr("id", "click-target", true)?;
        assert!(web_button_one.clicker().right()?);
        assert_eq!(page.run_js("window.__rightClicks")?, Value::from(1));

        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'web-late';
                        option.text = 'Web Late';
                        option.dataset.kind = 'late';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
        )?;
        let web_select_one = web_items.filter_one().attr("id", "picker", true)?;
        assert!(
            web_select_one
                .select()
                .by_locator_with_timeout("css:option[data-kind='late']", Some(1_000))
                .map_err(|err| OpenPageError::PageOperation(format!(
                    "web elements one select by_locator_with_timeout: {err}"
                )))?
        );
        assert_eq!(
            page.run_js("document.getElementById('picker').selectedOptions[0].value")?,
            Value::from("web-late")
        );

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one object wrapper runtime regression");
}

#[test]
fn elements_one_supports_states_rect_and_wait_at_runtime() {
    let (browser, temp_dir) = launch_headless_test_browser("elements-one-state-rect-wait")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button class="item" id="show-me" style="display:none">Show</button>
                        <button class="item" id="hide-me">Hide</button>
                        <button class="item" id="enable-me" disabled>Enable</button>
                        <button class="item" id="disabled-now" disabled>Disabled</button>
                        <button class="item" id="delete-me">Delete</button>
                        <div class="item" id="cover-wrap" style="position:relative;width:140px;height:40px;">
                            <button class="item" id="covered-btn" style="position:absolute;left:0;top:0;width:140px;height:40px;">Covered</button>
                            <div id="overlay" style="position:absolute;left:0;top:0;width:140px;height:40px;background:rgba(0,0,0,0.2);"></div>
                        </div>
                        <div class="item" id="no-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</div>
                        <div class="item" id="static-box" style="display:block;width:120px;height:80px;">Static</div>
                        <div class="item" id="scroll-box" style="display:block;width:80px;height:40px;overflow:auto;">
                            <div style="width:200px;height:120px;"></div>
                        </div>
                        <div style="height:1200px;"></div>
                    `;
                    setTimeout(() => {
                        document.getElementById('show-me').style.display = 'block';
                        document.getElementById('hide-me').style.display = 'none';
                        document.getElementById('enable-me').disabled = false;
                        document.getElementById('delete-me')?.remove();
                        const zero = document.getElementById('no-rect');
                        zero.style.width = '40px';
                        zero.style.height = '20px';
                    }, 200);
                    setTimeout(() => document.getElementById('overlay')?.remove(), 1_000);
                    return true;
                })()"#,
            )?;

        let page_items = page.find_all(".item")?;
        page.run_js(
            "(() => { \
                    window.scrollTo(0, 60); \
                    document.getElementById('scroll-box')?.scrollTo(25, 35); \
                    return true; \
                })()",
        )?;
        let scroll_y = page
            .run_js("(() => window.scrollY)()")?
            .as_f64()
            .expect("window.scrollY as f64");
        let static_one = page_items.filter_one().attr("id", "static-box", true)?;
        assert_eq!(static_one.states().is_alive()?, Some(true));
        assert_eq!(static_one.states().is_displayed()?, Some(true));
        assert_eq!(static_one.states().has_rect()?, Some(true));
        assert_eq!(static_one.states().is_in_viewport()?, Some(true));
        assert_eq!(
            static_one
                .rect()
                .size()?
                .map(|(width, height)| (width.round() as i64, height.round() as i64)),
            Some((120, 80))
        );
        assert_eq!(
            static_one.rect().corners()?.map(|corners| corners.len()),
            Some(4)
        );
        assert_eq!(
            static_one
                .rect()
                .viewport_corners()?
                .map(|corners| corners.len()),
            Some(4)
        );
        let static_location = static_one.rect().location()?.expect("static box location");
        let static_viewport_location = static_one
            .rect()
            .viewport_location()?
            .expect("static box viewport location");
        assert!((static_location.1 - (static_viewport_location.1 + scroll_y)).abs() < 1.0);
        let static_midpoint = static_one.rect().midpoint()?.expect("static box midpoint");
        let static_viewport_midpoint = static_one
            .rect()
            .viewport_midpoint()?
            .expect("static box viewport midpoint");
        assert!((static_midpoint.1 - (static_viewport_midpoint.1 + scroll_y)).abs() < 1.0);
        let static_click_point = static_one
            .rect()
            .click_point()?
            .expect("static box click point");
        let static_viewport_click_point = static_one
            .rect()
            .viewport_click_point()?
            .expect("static box viewport click point");
        assert!((static_click_point.1 - (static_viewport_click_point.1 + scroll_y)).abs() < 1.0);
        assert!(static_one.rect().screen_location()?.is_some());
        assert!(static_one.rect().screen_midpoint()?.is_some());
        assert!(static_one.rect().screen_click_point()?.is_some());
        assert_eq!(static_one.rect().scroll_position()?, Some((0.0, 0.0)));
        assert!(static_one.wait().stop_moving(500)?);
        let scroll_box_one = page_items.filter_one().attr("id", "scroll-box", true)?;
        assert_eq!(
            scroll_box_one
                .rect()
                .scroll_position()?
                .map(|(x, y)| (x.round() as i64, y.round() as i64)),
            Some((25, 35))
        );

        let covered_one = page_items.filter_one().attr("id", "covered-btn", true)?;
        assert_eq!(covered_one.states().is_covered()?, Some(true));
        assert!(covered_one.wait().covered(500)?);
        assert!(covered_one.wait().not_covered(1_500)?);
        assert_eq!(covered_one.states().is_covered()?, Some(false));

        let show_one = page_items.filter_one().attr("id", "show-me", true)?;
        assert!(show_one.wait().displayed(1_500)?);
        let hide_one = page_items.filter_one().attr("id", "hide-me", true)?;
        assert!(hide_one.wait().hidden(1_500)?);
        let enable_one = page_items.filter_one().attr("id", "enable-me", true)?;
        assert!(enable_one.wait().enabled(1_500)?);
        assert_eq!(enable_one.states().is_clickable()?, Some(true));
        let disabled_one = page_items.filter_one().attr("id", "disabled-now", true)?;
        assert!(disabled_one.wait().disabled(100)?);
        assert!(disabled_one.wait().disabled_or_deleted(100)?);
        let no_rect_one = page_items.filter_one().attr("id", "no-rect", true)?;
        assert!(no_rect_one.wait().has_rect(1_500)?);
        assert_eq!(no_rect_one.states().has_rect()?, Some(true));
        let delete_one = page_items.filter_one().attr("id", "delete-me", true)?;
        assert!(delete_one.wait().deleted(1_500)?);
        assert_eq!(delete_one.states().is_alive()?, Some(false));

        let missing_one = page_items.filter_one().attr("id", "missing", true)?;
        assert_eq!(missing_one.states().is_alive()?, None);
        assert_eq!(missing_one.rect().size()?, None);
        assert_eq!(missing_one.rect().click_point()?, None);
        assert_eq!(missing_one.rect().scroll_position()?, None);
        assert!(!missing_one.wait().displayed(100)?);
        assert!(missing_one.wait().deleted(100)?);
        assert!(missing_one.wait().disabled_or_deleted(100)?);

        let web_items = page
            .find_all(".item")?
            .into_iter()
            .map(WebElement::Browser)
            .collect::<Vec<_>>();
        let web_static_one = web_items.filter_one().attr("id", "static-box", true)?;
        assert_eq!(web_static_one.states().is_alive()?, Some(true));
        assert_eq!(
            web_static_one
                .rect()
                .size()?
                .map(|(width, height)| (width.round() as i64, height.round() as i64)),
            Some((120, 80))
        );
        assert_eq!(
            web_static_one
                .rect()
                .viewport_corners()?
                .map(|corners| corners.len()),
            Some(4)
        );
        let web_static_location = web_static_one
            .rect()
            .location()?
            .expect("web static box location");
        let web_static_viewport_location = web_static_one
            .rect()
            .viewport_location()?
            .expect("web static box viewport location");
        assert!(web_static_location.1 >= web_static_viewport_location.1);
        let web_static_midpoint = web_static_one
            .rect()
            .midpoint()?
            .expect("web static box midpoint");
        let web_static_viewport_midpoint = web_static_one
            .rect()
            .viewport_midpoint()?
            .expect("web static box viewport midpoint");
        assert!(web_static_midpoint.1 >= web_static_viewport_midpoint.1);
        let web_static_click_point = web_static_one
            .rect()
            .click_point()?
            .expect("web static box click point");
        let web_static_viewport_click_point = web_static_one
            .rect()
            .viewport_click_point()?
            .expect("web static box viewport click point");
        assert!(web_static_click_point.1 >= web_static_viewport_click_point.1);
        assert!(web_static_one.rect().screen_location()?.is_some());
        assert!(web_static_one.rect().screen_midpoint()?.is_some());
        assert!(web_static_one.rect().screen_click_point()?.is_some());
        assert_eq!(web_static_one.rect().scroll_position()?, Some((0.0, 0.0)));
        let web_scroll_box_one = web_items.filter_one().attr("id", "scroll-box", true)?;
        assert_eq!(
            web_scroll_box_one
                .rect()
                .scroll_position()?
                .map(|(x, y)| (x.round() as i64, y.round() as i64)),
            Some((25, 35))
        );
        let web_show_one = web_items.filter_one().attr("id", "show-me", true)?;
        assert!(web_show_one.wait().displayed(100)?);
        let web_enable_one = web_items.filter_one().attr("id", "enable-me", true)?;
        assert!(web_enable_one.wait().clickable(100)?);
        let web_no_rect_one = web_items.filter_one().attr("id", "no-rect", true)?;
        assert!(web_no_rect_one.wait().has_rect(100)?);
        let web_delete_one = web_items.filter_one().attr("id", "delete-me", true)?;
        assert!(web_delete_one.wait().deleted(100)?);
        assert_eq!(web_delete_one.states().is_alive()?, None);
        let missing_web_one = web_items.filter_one().attr("id", "missing", true)?;
        assert_eq!(missing_web_one.states().is_alive()?, None);
        assert_eq!(missing_web_one.rect().size()?, None);
        assert_eq!(missing_web_one.rect().click_point()?, None);
        assert_eq!(missing_web_one.rect().scroll_position()?, None);
        assert!(!missing_web_one.wait().clickable(100)?);
        assert!(missing_web_one.wait().deleted(100)?);
        assert!(missing_web_one.wait().disabled_or_deleted(100)?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("elements one state/rect/wait runtime regression");
}

#[test]
fn element_and_webelement_object_wrappers_work_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-object-wrappers").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <input id="name" value="" />
                        <div id="content">Old</div>
                        <div id="scrollbox" style="width:120px;height:80px;overflow:auto;">
                            <div style="width:600px;height:400px;"></div>
                        </div>
                        <select id="single-picker">
                            <option value="solo">Solo</option>
                            <option value="duo">Duo</option>
                        </select>
                        <select id="picker" multiple>
                            <option value="one" data-kind="primary">One</option>
                            <option value="two" data-kind="secondary">Two</option>
                            <option value="three" data-kind="secondary">Three</option>
                        </select>
                    `;
                    return true;
                })()"#,
        )?;

        let input = page.wait_for("css:#name", 1_000)?;
        input.set().value("Alpha")?;
        input.set().attr("data-role", "primary")?;
        assert_eq!(input.value()?, Some("Alpha".to_string()));
        assert_eq!(input.attr("data-role")?, Some("primary".to_string()));

        let content = page.wait_for("css:#content", 1_000)?;
        content
            .set()
            .inner_html("<span class=\"inner\">Changed</span>")?;
        assert_eq!(content.text()?, Some("Changed".to_string()));
        let content_select_err = content
            .select()
            .is_multi()
            .expect_err("div select().is_multi() should error");
        assert!(matches!(
            content_select_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let input_select_err = input
            .select()
            .by_text("noop")
            .expect_err("input select().by_text() should error");
        assert!(matches!(
            input_select_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));

        let scrollbox = page.wait_for("css:#scrollbox", 1_000)?;
        scrollbox.scroll().to_location(40.0, 60.0)?;
        assert_eq!(
            scrollbox.run_js("return this.scrollLeft === 40 && this.scrollTop === 60;")?,
            Value::from(true)
        );
        scrollbox.scroll().to_top()?;
        assert_eq!(
            scrollbox.run_js("return this.scrollTop === 0;")?,
            Value::from(true)
        );

        let select = page.wait_for("css:#picker", 1_000)?;
        assert!(select.select().is_multi()?);
        let options = select.select().options()?;
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].text()?, Some("One".to_string()));
        assert!(select.select().by_value("two")?);
        let selected_option = select
            .select()
            .selected_option()?
            .expect("selected option element");
        assert_eq!(selected_option.text()?, Some("Two".to_string()));
        assert_eq!(
            select.run_js("return this.options[1].selected && !this.options[0].selected;")?,
            Value::from(true)
        );
        assert!(
            select
                .select()
                .by_locator("css:option[data-kind='secondary']")?
        );
        assert_eq!(select.select().selected_options()?.len(), 2);
        assert!(select.select().cancel_by_text("Two")?);
        assert_eq!(
            select.run_js("return this.options[1].selected || this.options[2].selected;")?,
            Value::from(true)
        );
        assert!(select.select().cancel_by_value("three")?);
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        assert!(select.select().by_text(["One", "Three"])?);
        assert_eq!(select.select().selected_options()?.len(), 2);
        assert!(select.select().cancel_by_value(["one", "three"])?);
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        assert!(select.select().by_index([1, 3])?);
        assert_eq!(select.select().selected_options()?.len(), 2);
        assert!(select.select().cancel_by_index([1, 3])?);
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        assert!(select.select().by_option(&options[0])?);
        assert!(select.select().by_index(2)?);
        assert!(select.select().cancel_by_index(1)?);
        assert_eq!(
            select.run_js("return this.options[1].selected && !this.options[0].selected;")?,
            Value::from(true)
        );
        assert!(select.select().by_option([&options[0], &options[2]])?);
        assert_eq!(select.select().selected_options()?.len(), 3);
        assert!(
            select
                .select()
                .cancel_by_option([&options[0], &options[2]])?
        );
        assert_eq!(
                select.run_js("return this.options[1].selected && !this.options[0].selected && !this.options[2].selected;")?,
                Value::from(true)
            );
        assert!(select.select().by_indices(&[1, 3])?);
        assert_eq!(select.select().selected_options()?.len(), 3);
        assert!(select.select().cancel_by_indices(&[1, 2, 3])?);
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        let option_refs = [&options[0], &options[2]];
        assert!(select.select().by_options(&option_refs)?);
        assert_eq!(select.select().selected_options()?.len(), 2);
        assert!(select.select().cancel_by_options(&option_refs)?);
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        select.select().all()?;
        assert_eq!(select.select().selected_options()?.len(), 3);
        select.select().invert()?;
        assert_eq!(
            select.run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );

        let single_select = page.wait_for("css:#single-picker", 1_000)?;
        assert!(!single_select.select().is_multi()?);
        let single_all_err = single_select
            .select()
            .all()
            .expect_err("single select all() should error");
        assert!(matches!(
            single_all_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let single_clear_err = single_select
            .select()
            .clear()
            .expect_err("single select clear() should error");
        assert!(matches!(
            single_clear_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let single_invert_err = single_select
            .select()
            .invert()
            .expect_err("single select invert() should error");
        assert!(matches!(
            single_invert_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        assert_eq!(
            single_select.run_js("return this.value;")?,
            Value::from("solo")
        );

        let web_input = WebElement::Browser(page.wait_for("css:#name", 1_000)?);
        web_input.set().value("Beta")?;
        assert_eq!(web_input.value()?, Some("Beta".to_string()));
        assert!(web_input.select().selected_options()?.is_empty());

        let web_scrollbox = WebElement::Browser(page.wait_for("css:#scrollbox", 1_000)?);
        web_scrollbox.scroll().down(30.0)?;
        assert_eq!(
            web_scrollbox.run_js("return this.scrollTop === 30;")?,
            Value::from(true)
        );

        let web_select = WebElement::Browser(page.wait_for("css:#picker", 1_000)?);
        assert!(web_select.select().is_multi()?);
        let web_options = web_select.select().options()?;
        assert_eq!(web_options.len(), 3);
        assert_eq!(web_options[2].text()?, Some("Three".to_string()));
        web_select.select().clear()?;
        assert!(web_select.select().by_index(1)?);
        assert_eq!(
            web_select.run_js("return this.options[0].selected && !this.options[1].selected;")?,
            Value::from(true)
        );
        assert!(web_select.select().by_option(&web_options[1])?);
        assert!(
            web_select
                .select()
                .cancel_by_locator("css:option[data-kind='secondary']")?
        );
        assert_eq!(
            web_select.run_js("return this.options[1].selected || this.options[2].selected;")?,
            Value::from(false)
        );
        assert!(web_select.select().by_value(["one", "three"])?);
        assert_eq!(web_select.select().selected_options()?.len(), 2);
        assert!(web_select.select().cancel_by_text(["One", "Three"])?);
        assert_eq!(
            web_select
                .run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        assert!(web_select.select().by_index([1, 3])?);
        assert_eq!(web_select.select().selected_options()?.len(), 2);
        assert!(web_select.select().cancel_by_index([1, 3])?);
        assert_eq!(
            web_select
                .run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        let all_locators = vec![
            "css:option[data-kind='primary']".to_string(),
            "css:option[data-kind='secondary']".to_string(),
        ];
        assert!(web_select.select().by_locator(&all_locators)?);
        assert_eq!(web_select.select().selected_options()?.len(), 3);
        assert!(web_select.select().cancel_by_indices(&[1, 2, 3])?);
        assert_eq!(
            web_select
                .run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );
        let web_option_refs = [&web_options[0], &web_options[2]];
        assert!(
            web_select
                .select()
                .by_option([&web_options[0], &web_options[2]])?
        );
        assert!(
            web_select
                .select()
                .cancel_by_option([&web_options[0], &web_options[2]])?
        );
        assert_eq!(
            web_select.run_js("return this.options[0].selected || this.options[2].selected;")?,
            Value::from(false)
        );
        assert!(web_select.select().by_options(&web_option_refs)?);
        assert!(web_select.select().cancel_by_options(&web_option_refs)?);
        assert_eq!(
            web_select.run_js("return this.options[0].selected || this.options[2].selected;")?,
            Value::from(false)
        );
        web_select.select().all()?;
        assert_eq!(web_select.select().selected_options()?.len(), 3);
        web_select.select().invert()?;
        web_select.select().clear()?;
        assert_eq!(
            web_select
                .run_js("return Array.from(this.options).every(option => !option.selected);")?,
            Value::from(true)
        );

        let web_single_select = WebElement::Browser(page.wait_for("css:#single-picker", 1_000)?);
        assert!(!web_single_select.select().is_multi()?);
        let web_single_all_err = web_single_select
            .select()
            .all()
            .expect_err("web single select all() should error");
        assert!(matches!(
            web_single_all_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let web_single_clear_err = web_single_select
            .select()
            .clear()
            .expect_err("web single select clear() should error");
        assert!(matches!(
            web_single_clear_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        let web_single_invert_err = web_single_select
            .select()
            .invert()
            .expect_err("web single select invert() should error");
        assert!(matches!(
            web_single_invert_err,
            crate::OpenPageError::UnsupportedOperation(_)
        ));
        assert_eq!(
            web_single_select.run_js("return this.value;")?,
            Value::from("solo")
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element object wrapper runtime regression");
}

#[test]
fn element_and_webelement_states_rect_and_wait_wrappers_work_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-state-rect-wait").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="show-me" style="display:none">Show</button>
                        <button id="hide-me">Hide</button>
                        <button id="enable-me" disabled>Enable</button>
                        <button id="disabled-now" disabled>Disabled</button>
                        <button id="delete-me">Delete</button>
                        <div id="cover-wrap" style="position:relative;width:140px;height:40px;">
                            <button id="covered-btn" style="position:absolute;left:0;top:0;width:140px;height:40px;">Covered</button>
                            <div id="overlay" style="position:absolute;left:0;top:0;width:140px;height:40px;background:rgba(0,0,0,0.2);"></div>
                        </div>
                        <div id="no-rect" style="display:inline-block;width:0;height:0;overflow:hidden;">Zero</div>
                        <div id="static-box" style="display:block;width:120px;height:80px;">Static</div>
                        <div id="scroll-box" style="display:block;width:80px;height:40px;overflow:auto;">
                            <div style="width:200px;height:120px;"></div>
                        </div>
                        <div style="height:1200px;"></div>
                    `;
                    setTimeout(() => {
                        document.getElementById('show-me').style.display = 'block';
                        document.getElementById('hide-me').style.display = 'none';
                        document.getElementById('enable-me').disabled = false;
                        document.getElementById('delete-me')?.remove();
                        const zero = document.getElementById('no-rect');
                        zero.style.width = '40px';
                        zero.style.height = '20px';
                    }, 200);
                    setTimeout(() => document.getElementById('overlay')?.remove(), 1_000);
                    return true;
                })()"#,
            )?;

        let show_me = page.wait_for("css:#show-me", 1_000)?;
        let web_show_me = WebElement::Browser(page.wait_for("css:#show-me", 1_000)?);
        let hide_me = page.wait_for("css:#hide-me", 1_000)?;
        let web_hide_me = WebElement::Browser(page.wait_for("css:#hide-me", 1_000)?);
        let enable_me = page.wait_for("css:#enable-me", 1_000)?;
        let web_enable_me = WebElement::Browser(page.wait_for("css:#enable-me", 1_000)?);
        let disabled_now = page.wait_for("css:#disabled-now", 1_000)?;
        let web_disabled_now = WebElement::Browser(page.wait_for("css:#disabled-now", 1_000)?);
        let delete_me = page.wait_for("css:#delete-me", 1_000)?;
        let web_delete_me = WebElement::Browser(page.wait_for("css:#delete-me", 1_000)?);
        let covered_btn = page.wait_for("css:#covered-btn", 1_000)?;
        let web_covered_btn = WebElement::Browser(page.wait_for("css:#covered-btn", 1_000)?);
        let no_rect = page.wait_for("css:#no-rect", 1_000)?;
        let web_no_rect = WebElement::Browser(page.wait_for("css:#no-rect", 1_000)?);
        let static_box = page.wait_for("css:#static-box", 1_000)?;
        let web_static_box = WebElement::Browser(page.wait_for("css:#static-box", 1_000)?);
        let scroll_box = page.wait_for("css:#scroll-box", 1_000)?;
        let web_scroll_box = WebElement::Browser(page.wait_for("css:#scroll-box", 1_000)?);
        page.run_js(
            "(() => { \
                    window.scrollTo(0, 60); \
                    document.getElementById('scroll-box')?.scrollTo(25, 35); \
                    return true; \
                })()",
        )?;
        let scroll_y = page
            .run_js("(() => window.scrollY)()")?
            .as_f64()
            .expect("window.scrollY as f64");

        assert!(static_box.states().is_alive()?);
        assert!(static_box.states().is_displayed()?);
        assert!(static_box.states().is_enabled()?);
        assert!(static_box.states().has_rect()?);
        assert!(static_box.states().is_in_viewport()?);
        assert!(static_box.states().is_whole_in_viewport()?);
        assert!(!static_box.states().is_covered()?);
        assert_eq!(
            static_box
                .rect()
                .size()?
                .map(|(width, height)| (width.round() as i64, height.round() as i64)),
            Some((120, 80))
        );
        assert_eq!(
            static_box.rect().corners()?.map(|corners| corners.len()),
            Some(4)
        );
        assert_eq!(
            static_box
                .rect()
                .viewport_corners()?
                .map(|corners| corners.len()),
            Some(4)
        );
        let static_location = static_box.rect().location()?.expect("static box location");
        let static_viewport_location = static_box
            .rect()
            .viewport_location()?
            .expect("static box viewport location");
        assert!((static_location.1 - (static_viewport_location.1 + scroll_y)).abs() < 1.0);
        let static_midpoint = static_box.rect().midpoint()?.expect("static box midpoint");
        let static_viewport_midpoint = static_box
            .rect()
            .viewport_midpoint()?
            .expect("static box viewport midpoint");
        assert!((static_midpoint.1 - (static_viewport_midpoint.1 + scroll_y)).abs() < 1.0);
        let static_click_point = static_box
            .rect()
            .click_point()?
            .expect("static box click point");
        let static_viewport_click_point = static_box
            .rect()
            .viewport_click_point()?
            .expect("static box viewport click point");
        assert!((static_click_point.1 - (static_viewport_click_point.1 + scroll_y)).abs() < 1.0);
        assert!(static_box.rect().screen_location()?.is_some());
        assert!(static_box.rect().screen_midpoint()?.is_some());
        assert!(static_box.rect().screen_click_point()?.is_some());
        assert_eq!(static_box.rect().scroll_position()?, Some((0.0, 0.0)));
        assert_eq!(
            scroll_box
                .rect()
                .scroll_position()?
                .map(|(x, y)| (x.round() as i64, y.round() as i64)),
            Some((25, 35))
        );
        assert!(static_box.wait().stop_moving(500)?);

        assert!(web_static_box.states().is_alive()?);
        assert!(web_static_box.states().is_displayed()?);
        assert!(web_static_box.states().is_enabled()?);
        assert!(web_static_box.states().has_rect()?);
        assert!(web_static_box.states().is_in_viewport()?);
        assert!(web_static_box.states().is_whole_in_viewport()?);
        assert_eq!(
            web_static_box
                .rect()
                .size()?
                .map(|(width, height)| (width.round() as i64, height.round() as i64)),
            Some((120, 80))
        );
        assert_eq!(
            web_static_box
                .rect()
                .corners()?
                .map(|corners| corners.len()),
            Some(4)
        );
        assert_eq!(
            web_static_box
                .rect()
                .viewport_corners()?
                .map(|corners| corners.len()),
            Some(4)
        );
        let web_static_location = web_static_box
            .rect()
            .location()?
            .expect("web static box location");
        let web_static_viewport_location = web_static_box
            .rect()
            .viewport_location()?
            .expect("web static box viewport location");
        assert!(web_static_location.1 >= web_static_viewport_location.1);
        let web_static_midpoint = web_static_box
            .rect()
            .midpoint()?
            .expect("web static box midpoint");
        let web_static_viewport_midpoint = web_static_box
            .rect()
            .viewport_midpoint()?
            .expect("web static box viewport midpoint");
        assert!(web_static_midpoint.1 >= web_static_viewport_midpoint.1);
        let web_static_click_point = web_static_box
            .rect()
            .click_point()?
            .expect("web static box click point");
        let web_static_viewport_click_point = web_static_box
            .rect()
            .viewport_click_point()?
            .expect("web static box viewport click point");
        assert!(web_static_click_point.1 >= web_static_viewport_click_point.1);
        assert!(web_static_box.rect().screen_location()?.is_some());
        assert!(web_static_box.rect().screen_midpoint()?.is_some());
        assert!(web_static_box.rect().screen_click_point()?.is_some());
        assert_eq!(web_static_box.rect().scroll_position()?, Some((0.0, 0.0)));
        assert_eq!(
            web_scroll_box
                .rect()
                .scroll_position()?
                .map(|(x, y)| (x.round() as i64, y.round() as i64)),
            Some((25, 35))
        );

        assert!(covered_btn.states().is_covered()?);
        assert!(covered_btn.wait().covered(500)?);
        assert!(web_covered_btn.wait().not_covered(1_500)?);
        assert!(!web_covered_btn.states().is_covered()?);

        assert!(show_me.wait().displayed(1_500)?);
        assert!(web_show_me.wait().displayed(100)?);
        assert!(hide_me.wait().hidden(1_500)?);
        assert!(web_hide_me.wait().hidden(100)?);

        assert!(enable_me.wait().enabled(1_500)?);
        assert!(web_enable_me.wait().clickable(1_500)?);
        assert!(web_enable_me.states().is_clickable()?);

        assert!(disabled_now.wait().disabled(100)?);
        assert!(disabled_now.wait().disabled_or_deleted(100)?);
        assert!(web_disabled_now.wait().disabled(100)?);

        assert!(no_rect.wait().has_rect(1_500)?);
        assert!(web_no_rect.wait().has_rect(100)?);
        assert!(web_no_rect.states().has_rect()?);

        assert!(delete_me.wait().deleted(1_500)?);
        assert!(!web_delete_me.states().is_alive()?);
        assert!(web_delete_me.wait().deleted(100)?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element state/rect/wait wrapper runtime regression");
}

#[test]
fn element_screen_points_follow_dp_device_pixel_ratio_formula() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-screen-points").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <div
                            id="box"
                            style="position:absolute;left:80px;top:120px;width:100px;height:60px;border:4px solid #111;padding:6px;background:#eee;"
                        ></div>
                    `;
                    return true;
                })()"#,
            )?;

        let element = page.wait_for("css:#box", 1_000)?;
        let web_element = WebElement::Browser(page.wait_for("css:#box", 1_000)?);
        let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
            expected_dp_viewport_screen_origin(&page)?;
        assert!(
            (device_pixel_ratio - 2.0).abs() < 0.01,
            "devicePixelRatio override did not apply: {device_pixel_ratio}"
        );

        let viewport_location = element
            .rect_viewport_location()?
            .expect("element viewport location");
        let screen_location = element
            .rect_screen_location()?
            .expect("element screen location");
        assert_pair_close(
            screen_location,
            (
                (viewport_screen_x + viewport_location.0) * device_pixel_ratio,
                (viewport_screen_y + viewport_location.1) * device_pixel_ratio,
            ),
            "element screen_location",
        );

        let viewport_midpoint = element
            .rect_viewport_midpoint()?
            .expect("element viewport midpoint");
        let screen_midpoint = element
            .rect_screen_midpoint()?
            .expect("element screen midpoint");
        assert_pair_close(
            screen_midpoint,
            (
                (viewport_screen_x + viewport_midpoint.0) * device_pixel_ratio,
                (viewport_screen_y + viewport_midpoint.1) * device_pixel_ratio,
            ),
            "element screen_midpoint",
        );

        let viewport_click_point = element
            .rect_viewport_click_point()?
            .expect("element viewport click point");
        let screen_click_point = element
            .rect_screen_click_point()?
            .expect("element screen click point");
        assert_pair_close(
            screen_click_point,
            (
                (viewport_screen_x + viewport_click_point.0) * device_pixel_ratio,
                (viewport_screen_y + viewport_click_point.1) * device_pixel_ratio,
            ),
            "element screen_click_point",
        );

        assert_pair_close(
            web_element
                .rect_screen_location()?
                .expect("web element screen location"),
            screen_location,
            "web element screen_location",
        );
        assert_pair_close(
            web_element
                .rect_screen_midpoint()?
                .expect("web element screen midpoint"),
            screen_midpoint,
            "web element screen_midpoint",
        );
        assert_pair_close(
            web_element
                .rect_screen_click_point()?
                .expect("web element screen click point"),
            screen_click_point,
            "web element screen_click_point",
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element screen point formula regression");
}

#[test]
fn iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
    let (browser, temp_dir) = launch_headless_test_browser("iframe-element-screen-points")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <iframe
                            id="demo-frame"
                            style="position:absolute;left:160px;top:90px;width:420px;height:260px;border:0;"
                            srcdoc="<html><head><title>Inside Frame</title></head><body style='margin:0;height:1600px'><div id='inner-box' style='position:absolute;left:48px;top:72px;width:90px;height:54px;border:3px solid #111;padding:5px;background:#eee;'></div></body></html>"
                        ></iframe>
                    `;
                    return true;
                })()"#,
            )?;

        let frame = page.get_frame_context("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        assert_eq!(frame.title()?, Some("Inside Frame".to_string()));
        assert!(frame.inner_html()?.contains("inner-box"));
        frame.run_js("(window.scrollTo(0, 23), true)")?;
        let frame_scroll_position = frame.scroll_position()?;
        assert_eq!(
            (
                frame_scroll_position.0.round() as i64,
                frame_scroll_position.1.round() as i64,
            ),
            (0, 23)
        );
        let element = frame.find("css:#inner-box")?;
        let web_element = WebElement::Browser(frame.find("css:#inner-box")?);

        let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
            expected_dp_viewport_screen_origin(&page)?;
        let frame_viewport_location = frame
            .frame_element()
            .rect_viewport_location()?
            .expect("frame viewport location");

        let viewport_location = element
            .rect_viewport_location()?
            .expect("iframe element viewport location");
        let screen_location = element
            .rect_screen_location()?
            .expect("iframe element screen location");
        assert_pair_close(
            screen_location,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_location.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_location.1)
                    * device_pixel_ratio,
            ),
            "iframe element screen_location",
        );

        let viewport_midpoint = element
            .rect_viewport_midpoint()?
            .expect("iframe element viewport midpoint");
        let screen_midpoint = element
            .rect_screen_midpoint()?
            .expect("iframe element screen midpoint");
        assert_pair_close(
            screen_midpoint,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_midpoint.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_midpoint.1)
                    * device_pixel_ratio,
            ),
            "iframe element screen_midpoint",
        );

        let viewport_click_point = element
            .rect_viewport_click_point()?
            .expect("iframe element viewport click point");
        let screen_click_point = element
            .rect_screen_click_point()?
            .expect("iframe element screen click point");
        assert_pair_close(
            screen_click_point,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_click_point.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_click_point.1)
                    * device_pixel_ratio,
            ),
            "iframe element screen_click_point",
        );

        assert_pair_close(
            web_element
                .rect_screen_location()?
                .expect("iframe web element screen location"),
            screen_location,
            "iframe web element screen_location",
        );
        assert_pair_close(
            web_element
                .rect_screen_midpoint()?
                .expect("iframe web element screen midpoint"),
            screen_midpoint,
            "iframe web element screen_midpoint",
        );
        assert_pair_close(
            web_element
                .rect_screen_click_point()?
                .expect("iframe web element screen click point"),
            screen_click_point,
            "iframe web element screen_click_point",
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("iframe element screen point formula regression");
}

#[test]
fn cross_origin_iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
    let (browser, temp_dir) = launch_headless_test_browser("xorigin-iframe-screen-points")
        .expect("launch headless browser");
    let (parent_url, parent_server, child_server) = spawn_cross_origin_iframe_site();

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
        page.goto(&parent_url)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let frame = page.get_frame_context("css:#cross-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        assert_eq!(frame.title()?, Some("Cross Origin Child".to_string()));

        let element = frame.find("css:#inner-box")?;
        let web_element = WebElement::Browser(frame.find("css:#inner-box")?);
        let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
            expected_dp_viewport_screen_origin(&page)?;
        let frame_viewport_location = frame
            .frame_element()
            .rect_viewport_location()?
            .expect("cross-origin frame viewport location");

        let viewport_location = element
            .rect_viewport_location()?
            .expect("cross-origin iframe element viewport location");
        let screen_location = element
            .rect_screen_location()?
            .expect("cross-origin iframe element screen location");
        assert_pair_close(
            screen_location,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_location.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_location.1)
                    * device_pixel_ratio,
            ),
            "cross-origin iframe element screen_location",
        );

        let viewport_midpoint = element
            .rect_viewport_midpoint()?
            .expect("cross-origin iframe element viewport midpoint");
        let screen_midpoint = element
            .rect_screen_midpoint()?
            .expect("cross-origin iframe element screen midpoint");
        assert_pair_close(
            screen_midpoint,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_midpoint.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_midpoint.1)
                    * device_pixel_ratio,
            ),
            "cross-origin iframe element screen_midpoint",
        );

        let viewport_click_point = element
            .rect_viewport_click_point()?
            .expect("cross-origin iframe element viewport click point");
        let screen_click_point = element
            .rect_screen_click_point()?
            .expect("cross-origin iframe element screen click point");
        assert_pair_close(
            screen_click_point,
            (
                (viewport_screen_x + frame_viewport_location.0 + viewport_click_point.0)
                    * device_pixel_ratio,
                (viewport_screen_y + frame_viewport_location.1 + viewport_click_point.1)
                    * device_pixel_ratio,
            ),
            "cross-origin iframe element screen_click_point",
        );

        assert_pair_close(
            web_element
                .rect_screen_location()?
                .expect("cross-origin web element screen location"),
            screen_location,
            "cross-origin web element screen_location",
        );
        assert_pair_close(
            web_element
                .rect_screen_midpoint()?
                .expect("cross-origin web element screen midpoint"),
            screen_midpoint,
            "cross-origin web element screen_midpoint",
        );
        assert_pair_close(
            web_element
                .rect_screen_click_point()?
                .expect("cross-origin web element screen click point"),
            screen_click_point,
            "cross-origin web element screen_click_point",
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    parent_server.join().expect("join parent iframe server");
    child_server.join().expect("join child iframe server");

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("cross-origin iframe element screen point formula regression");
}

#[test]
fn nested_cross_origin_iframe_element_screen_points_follow_dp_device_pixel_ratio_formula() {
    let (browser, temp_dir) = launch_headless_test_browser("nested-xorigin-iframe-screen-points")
        .expect("launch headless browser");
    let (parent_url, parent_server, child_server, grandchild_server) =
        spawn_nested_cross_origin_iframe_site();

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        page.execute_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 2.0, false))?;
        page.goto(&parent_url)?;
        assert!(page.wait_for_doc_loaded(5_000)?);

        let outer_frame = page
            .get_frame_context("css:#outer-frame")
            .map_err(|err| OpenPageError::PageOperation(format!("outer frame context: {err}")))?;
        assert!(outer_frame.wait_for_doc_loaded(5_000).map_err(|err| {
            OpenPageError::PageOperation(format!("outer frame wait_for_doc_loaded: {err}"))
        })?);
        let inner_frame_element = outer_frame.find("css:#inner-frame").map_err(|err| {
            OpenPageError::PageOperation(format!("outer frame find inner frame: {err}"))
        })?;
        let inner_frame = page
            .get_frame_context(&inner_frame_element)
            .map_err(|err| {
                OpenPageError::PageOperation(format!("inner frame context from element: {err}"))
            })?;
        assert!(inner_frame.wait_for_doc_loaded(5_000).map_err(|err| {
            OpenPageError::PageOperation(format!("inner frame wait_for_doc_loaded: {err}"))
        })?);
        assert_eq!(
            inner_frame
                .title()
                .map_err(|err| OpenPageError::PageOperation(format!("inner frame title: {err}")))?,
            Some("Nested Cross Origin Grandchild".to_string())
        );

        let element = inner_frame.find("css:#deep-box").map_err(|err| {
            OpenPageError::PageOperation(format!("inner frame find deep-box: {err}"))
        })?;
        let web_element =
            WebElement::Browser(inner_frame.find("css:#deep-box").map_err(|err| {
                OpenPageError::PageOperation(format!(
                    "inner frame find deep-box for web element: {err}"
                ))
            })?);
        let (viewport_screen_x, viewport_screen_y, device_pixel_ratio) =
            expected_dp_viewport_screen_origin(&page)?;
        let outer_frame_viewport_location = outer_frame
            .frame_element()
            .rect_viewport_location()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("outer frame rect_viewport_location: {err}"))
            })?
            .expect("outer frame viewport location");
        let inner_frame_viewport_location = inner_frame
            .frame_element()
            .rect_viewport_location()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("inner frame rect_viewport_location: {err}"))
            })?
            .expect("inner frame viewport location");

        let viewport_location = element
            .rect_viewport_location()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_viewport_location: {err}"))
            })?
            .expect("nested cross-origin iframe element viewport location");
        let screen_location = element
            .rect_screen_location()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_screen_location: {err}"))
            })?
            .expect("nested cross-origin iframe element screen location");
        assert_pair_close(
            screen_location,
            (
                (viewport_screen_x
                    + outer_frame_viewport_location.0
                    + inner_frame_viewport_location.0
                    + viewport_location.0)
                    * device_pixel_ratio,
                (viewport_screen_y
                    + outer_frame_viewport_location.1
                    + inner_frame_viewport_location.1
                    + viewport_location.1)
                    * device_pixel_ratio,
            ),
            "nested cross-origin iframe element screen_location",
        );

        let viewport_midpoint = element
            .rect_viewport_midpoint()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_viewport_midpoint: {err}"))
            })?
            .expect("nested cross-origin iframe element viewport midpoint");
        let screen_midpoint = element
            .rect_screen_midpoint()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_screen_midpoint: {err}"))
            })?
            .expect("nested cross-origin iframe element screen midpoint");
        assert_pair_close(
            screen_midpoint,
            (
                (viewport_screen_x
                    + outer_frame_viewport_location.0
                    + inner_frame_viewport_location.0
                    + viewport_midpoint.0)
                    * device_pixel_ratio,
                (viewport_screen_y
                    + outer_frame_viewport_location.1
                    + inner_frame_viewport_location.1
                    + viewport_midpoint.1)
                    * device_pixel_ratio,
            ),
            "nested cross-origin iframe element screen_midpoint",
        );

        let viewport_click_point = element
            .rect_viewport_click_point()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_viewport_click_point: {err}"))
            })?
            .expect("nested cross-origin iframe element viewport click point");
        let screen_click_point = element
            .rect_screen_click_point()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("deep-box rect_screen_click_point: {err}"))
            })?
            .expect("nested cross-origin iframe element screen click point");
        assert_pair_close(
            screen_click_point,
            (
                (viewport_screen_x
                    + outer_frame_viewport_location.0
                    + inner_frame_viewport_location.0
                    + viewport_click_point.0)
                    * device_pixel_ratio,
                (viewport_screen_y
                    + outer_frame_viewport_location.1
                    + inner_frame_viewport_location.1
                    + viewport_click_point.1)
                    * device_pixel_ratio,
            ),
            "nested cross-origin iframe element screen_click_point",
        );

        assert_pair_close(
            web_element
                .rect_screen_location()?
                .expect("nested cross-origin web element screen location"),
            screen_location,
            "nested cross-origin web element screen_location",
        );
        assert_pair_close(
            web_element
                .rect_screen_midpoint()?
                .expect("nested cross-origin web element screen midpoint"),
            screen_midpoint,
            "nested cross-origin web element screen_midpoint",
        );
        assert_pair_close(
            web_element
                .rect_screen_click_point()?
                .expect("nested cross-origin web element screen click point"),
            screen_click_point,
            "nested cross-origin web element screen_click_point",
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    parent_server
        .join()
        .expect("join nested parent iframe server");
    child_server
        .join()
        .expect("join nested child iframe server");
    grandchild_server
        .join()
        .expect("join nested grandchild iframe server");

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("nested cross-origin iframe element screen point formula regression");
}

#[test]
fn select_waits_for_delayed_options_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("select-delayed-options").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `<select id="picker" multiple></select>`;
                    return true;
                })()"#,
        )?;

        let select = page.wait_for("css:#picker", 1_000)?;

        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-text';
                        option.text = 'Late Text';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
        )?;
        let start = Instant::now();
        assert!(select.select().by_text("Late Text")?);
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(
            select.run_js("return this.value;")?,
            Value::from("late-text")
        );

        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-value';
                        option.text = 'Late Value';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
        )?;
        let web_select = WebElement::Browser(page.wait_for("css:#picker", 1_000)?);
        let start = Instant::now();
        assert!(
            web_select
                .select()
                .by_value_with_timeout("late-value", Some(1_000))?
        );
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(
            web_select.run_js("return this.value;")?,
            Value::from("late-value")
        );

        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        for (const [index, value] of ['one', 'two'].entries()) {
                            const option = document.createElement('option');
                            option.value = value;
                            option.text = `Option ${index + 1}`;
                            picker.appendChild(option);
                        }
                    }, 150);
                    return true;
                })()"#,
        )?;
        let page_selects = page.find_all("css:#picker")?;
        let select_one = page_selects.filter_one().tag("select", true)?;
        let start = Instant::now();
        assert!(select_one.select_by_index([1, 2])?);
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(
                page.run_js(
                    "Array.from(document.getElementById('picker').selectedOptions).map(option => option.value).join(',')"
                )?,
                Value::from("one,two")
            );

        page.run_js(
            r#"(() => {
                    const picker = document.getElementById('picker');
                    picker.innerHTML = '';
                    setTimeout(() => {
                        const option = document.createElement('option');
                        option.value = 'late-locator';
                        option.text = 'Late Locator';
                        option.dataset.kind = 'locator';
                        picker.appendChild(option);
                    }, 150);
                    return true;
                })()"#,
        )?;
        let web_selects = vec![WebElement::Browser(page.wait_for("css:#picker", 1_000)?)];
        let web_select_one = web_selects.filter_one().tag("select", true)?;
        let start = Instant::now();
        assert!(
            web_select_one
                .select_by_locator_with_timeout("css:option[data-kind='locator']", Some(1_000))?
        );
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(
            page.run_js("document.getElementById('picker').selectedOptions[0].value")?,
            Value::from("late-locator")
        );

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("select delayed options runtime regression");
}

#[test]
fn element_and_webelement_clicker_work_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-clicker").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="click-target">Click target</button>
                        <select id="single-picker">
                            <option id="single-a" value="a" selected>Single A</option>
                            <option id="single-b" value="b">Single B</option>
                        </select>
                        <select id="multi-picker" multiple>
                            <option id="multi-a" value="a">Multi A</option>
                            <option id="multi-b" value="b">Multi B</option>
                        </select>
                    `;
                    window.__clicks = 0;
                    window.__rightClicks = 0;
                    document.getElementById('click-target').addEventListener('click', () => {
                        window.__clicks += 1;
                    });
                    document.getElementById('click-target').addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__rightClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

        let click_target = page.wait_for("css:#click-target", 1_000)?;
        click_target.clicker().multi(2)?;
        click_target.clicker().at(Some(5.0), Some(5.0), "left", 1)?;
        assert_eq!(page.run_js("window.__clicks")?, Value::from(3));

        page.wait_for("css:#single-b", 1_000)?
            .clicker()
            .left()
            .map_err(|err| {
                OpenPageError::PageOperation(format!("single-b clicker.left(): {err}"))
            })?;
        assert_eq!(
            page.run_js("document.getElementById('single-picker').value")?,
            Value::from("b")
        );
        let multi_a = page.wait_for("css:#multi-a", 1_000)?;
        multi_a.clicker().left().map_err(|err| {
            OpenPageError::PageOperation(format!("multi-a first clicker.left(): {err}"))
        })?;
        assert_eq!(
            page.run_js("document.getElementById('multi-a').selected")?,
            Value::from(true)
        );
        multi_a.clicker().left().map_err(|err| {
            OpenPageError::PageOperation(format!("multi-a second clicker.left(): {err}"))
        })?;
        assert_eq!(
            page.run_js("document.getElementById('multi-a').selected")?,
            Value::from(false)
        );

        let web_click_target = WebElement::Browser(page.wait_for("css:#click-target", 1_000)?);
        web_click_target.clicker().right()?;
        assert_eq!(page.run_js("window.__rightClicks")?, Value::from(1));
        let web_multi_b = WebElement::Browser(page.wait_for("css:#multi-b", 1_000)?);
        web_multi_b.clicker().left().map_err(|err| {
            OpenPageError::PageOperation(format!("web multi-b clicker.left(): {err}"))
        })?;
        assert_eq!(
            page.run_js("document.getElementById('multi-b').selected")?,
            Value::from(true)
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element clicker runtime regression");
}

#[test]
fn element_clicker_left_with_options_supports_js_fallback_and_click_failed_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) =
        launch_headless_test_browser("element-clicker-options").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button id="hidden-click" style="visibility:hidden;width:120px;height:32px;">
                            Hidden click
                        </button>
                    `;
                    window.__hiddenClicks = 0;
                    document.getElementById('hidden-click').addEventListener('click', () => {
                        window.__hiddenClicks += 1;
                    });
                    return true;
                })()"#,
            )?;

        let hidden = page.wait_for("css:#hidden-click", 1_000)?;
        hidden.click()?;
        assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(0));

        page.click("css:#hidden-click")?;
        assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(0));

        assert!(hidden.clicker().left_with_options(None, Some(100), false)?);
        assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(1));

        assert!(
            !hidden
                .clicker()
                .left_with_options(Some(false), Some(100), false)?
        );
        assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(1));

        let web_hidden = WebElement::Browser(page.wait_for("css:#hidden-click", 1_000)?);
        assert!(
            web_hidden
                .clicker()
                .left_with_options(Some(true), Some(100), false)?
        );
        assert_eq!(page.run_js("window.__hiddenClicks")?, Value::from(2));

        Settings::set_raise_when_click_failed(true);
        let direct_error = hidden
            .click()
            .expect_err("direct click should raise when global setting is enabled");
        assert!(
            matches!(direct_error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
            "unexpected direct click failure error: {direct_error}"
        );

        let page_error = page
            .click("css:#hidden-click")
            .expect_err("page.click() should raise when global setting is enabled");
        assert!(
            matches!(page_error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
            "unexpected page.click() failure error: {page_error}"
        );

        let error = hidden
            .clicker()
            .left_with_options(Some(false), Some(100), false)
            .expect_err("click failure should raise when global setting is enabled");
        assert!(
            matches!(error, OpenPageError::PageOperation(ref message) if message.contains("hidden or disabled")),
            "unexpected click failure error: {error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element clicker option runtime regression");
}

#[test]
fn non_left_click_helpers_share_click_failed_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (browser, temp_dir) = launch_headless_test_browser("element-clicker-non-left-fail")
        .expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <button
                            id="no-rect-click"
                            style="display:inline-block;width:0;height:0;overflow:hidden;padding:0;border:0;margin:0;"
                        >
                            No rect
                        </button>
                    `;
                    window.__noRectClicks = 0;
                    window.__noRectAuxClicks = 0;
                    window.__noRectContextMenus = 0;
                    const button = document.getElementById('no-rect-click');
                    button.addEventListener('click', () => {
                        window.__noRectClicks += 1;
                    });
                    button.addEventListener('auxclick', event => {
                        if (event.button === 1) {
                            window.__noRectAuxClicks += 1;
                        }
                    });
                    button.addEventListener('contextmenu', event => {
                        event.preventDefault();
                        window.__noRectContextMenus += 1;
                    });
                    return true;
                })()"#,
            )?;

        let no_rect = page.wait_for("css:#no-rect-click", 1_000)?;
        let web_no_rect = WebElement::Browser(page.wait_for("css:#no-rect-click", 1_000)?);

        assert!(!no_rect.has_rect()?);
        assert!(!web_no_rect.has_rect()?);

        Settings::set_raise_when_click_failed(false);

        no_rect.click_right()?;
        no_rect.click_middle()?;
        no_rect.click_multi(2)?;
        no_rect.click_at(None, None, "left", 1)?;
        no_rect.clicker().right()?;
        assert!(no_rect.clicker().middle(false)?.is_none());
        no_rect.clicker().multi(2)?;
        no_rect.clicker().at(None, None, "left", 1)?;

        web_no_rect.click_right()?;
        web_no_rect.click_middle()?;
        web_no_rect.click_multi(2)?;
        web_no_rect.click_at(None, None, "left", 1)?;
        web_no_rect.clicker().right()?;
        assert!(web_no_rect.clicker().middle(false)?.is_none());
        web_no_rect.clicker().multi(2)?;
        web_no_rect.clicker().at(None, None, "left", 1)?;

        assert!(
            page.click_middle("css:#no-rect-click", Some(100), false)?
                .is_none()
        );
        assert_eq!(
            page.run_js(
                "[window.__noRectClicks, window.__noRectAuxClicks, window.__noRectContextMenus]"
            )?,
            Value::Array(vec![Value::from(0), Value::from(0), Value::from(0)])
        );

        let assert_visible_rect_error = |label: &str, err: OpenPageError| {
            assert!(
                matches!(err, OpenPageError::PageOperation(ref message) if message.contains("visible rect")),
                "unexpected {label} failure error: {err}"
            );
        };

        Settings::set_raise_when_click_failed(true);

        assert_visible_rect_error(
            "element.click_right()",
            no_rect
                .click_right()
                .expect_err("element.click_right() should raise"),
        );
        assert_visible_rect_error(
            "element.click_middle()",
            no_rect
                .click_middle()
                .expect_err("element.click_middle() should raise"),
        );
        assert_visible_rect_error(
            "element.click_multi()",
            no_rect
                .click_multi(2)
                .expect_err("element.click_multi() should raise"),
        );
        assert_visible_rect_error(
            "element.click_at()",
            no_rect
                .click_at(None, None, "left", 1)
                .expect_err("element.click_at() should raise"),
        );
        assert_visible_rect_error(
            "element.clicker().right()",
            no_rect
                .clicker()
                .right()
                .expect_err("element.clicker().right() should raise"),
        );
        assert_visible_rect_error(
            "element.clicker().middle(false)",
            no_rect
                .clicker()
                .middle(false)
                .expect_err("element.clicker().middle(false) should raise"),
        );
        assert_visible_rect_error(
            "element.clicker().multi()",
            no_rect
                .clicker()
                .multi(2)
                .expect_err("element.clicker().multi() should raise"),
        );
        assert_visible_rect_error(
            "element.clicker().at()",
            no_rect
                .clicker()
                .at(None, None, "left", 1)
                .expect_err("element.clicker().at() should raise"),
        );

        assert_visible_rect_error(
            "web_element.click_right()",
            web_no_rect
                .click_right()
                .expect_err("web_element.click_right() should raise"),
        );
        assert_visible_rect_error(
            "web_element.click_middle()",
            web_no_rect
                .click_middle()
                .expect_err("web_element.click_middle() should raise"),
        );
        assert_visible_rect_error(
            "web_element.click_multi()",
            web_no_rect
                .click_multi(2)
                .expect_err("web_element.click_multi() should raise"),
        );
        assert_visible_rect_error(
            "web_element.click_at()",
            web_no_rect
                .click_at(None, None, "left", 1)
                .expect_err("web_element.click_at() should raise"),
        );
        assert_visible_rect_error(
            "web_element.clicker().right()",
            web_no_rect
                .clicker()
                .right()
                .expect_err("web_element.clicker().right() should raise"),
        );
        assert_visible_rect_error(
            "web_element.clicker().middle(false)",
            web_no_rect
                .clicker()
                .middle(false)
                .expect_err("web_element.clicker().middle(false) should raise"),
        );
        assert_visible_rect_error(
            "web_element.clicker().multi()",
            web_no_rect
                .clicker()
                .multi(2)
                .expect_err("web_element.clicker().multi() should raise"),
        );
        assert_visible_rect_error(
            "web_element.clicker().at()",
            web_no_rect
                .clicker()
                .at(None, None, "left", 1)
                .expect_err("web_element.clicker().at() should raise"),
        );
        assert_visible_rect_error(
            "page.click_middle()",
            page.click_middle("css:#no-rect-click", Some(100), false)
                .expect_err("page.click_middle() should raise"),
        );

        Settings::set_language("cn");
        let localized_error = no_rect
            .click_right()
            .expect_err("element.click_right() should raise localized message");
        assert!(
            matches!(localized_error, OpenPageError::PageOperation(ref message) if message.contains("可见位置及大小")),
            "unexpected localized click failure error: {localized_error}"
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("non-left click failure setting runtime regression");
}

#[test]
fn element_and_webelement_clicker_tabs_work_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-clicker-tabs").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    const newTabUrl = 'about:blank#clicker-new-tab';
                    const middleUrl = 'about:blank#clicker-middle-tab';
                    document.body.innerHTML = `
                        <a id="open-tab" href="${newTabUrl}" target="_blank">Open tab</a>
                        <a id="middle-open-tab" href="${middleUrl}">Open by middle click</a>
                    `;
                    return true;
                })()"#,
        )?;

        let new_page = page
            .wait_for("css:#open-tab", 1_000)?
            .clicker()
            .for_new_tab(Some(5_000), false)
            .map_err(|err| OpenPageError::PageOperation(format!("clicker.for_new_tab(): {err}")))?
            .expect("clicker new tab");
        assert!(new_page.wait_for_doc_loaded(5_000).map_err(|err| {
            OpenPageError::PageOperation(format!("new_tab.wait_for_doc_loaded(): {err}"))
        })?);
        assert_eq!(new_page.url()?, "about:blank#clicker-new-tab".to_string());

        let middle_page = WebElement::Browser(page.wait_for("css:#middle-open-tab", 1_000)?)
            .clicker()
            .middle(true)
            .map_err(|err| OpenPageError::PageOperation(format!("clicker.middle(true): {err}")))?
            .expect("clicker middle tab");
        let BrowserTabReference::Page(middle_page) = middle_page else {
            panic!("browser-backed WebElement should return a Page tab reference");
        };
        assert!(middle_page.wait_for_doc_loaded(5_000).map_err(|err| {
            OpenPageError::PageOperation(format!("middle_tab.wait_for_doc_loaded(): {err}"))
        })?);
        assert_eq!(
            middle_page.url()?,
            "about:blank#clicker-middle-tab".to_string()
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("element clicker tab runtime regression");
}

#[test]
fn page_and_element_tab_helpers_raise_when_no_new_tab_is_opened() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_language("cn");

    let (browser, temp_dir) =
        launch_headless_test_browser("page-no-new-tab-error").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        let latest_tab =
            browser.new_tab(Some("about:blank#existing-latest"), false, false, false)?;
        assert!(latest_tab.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                    document.body.innerHTML = `
                        <button id="stay-put" type="button">Stay put</button>
                    `;
                    return true;
                })()"#,
        )?;

        let assert_no_new_tab_error = |label: &str, err: OpenPageError| {
            assert!(
                matches!(err, OpenPageError::PageOperation(ref message) if message == "没有等到新标签页"),
                "unexpected {label} error: {err}"
            );
        };

        assert_no_new_tab_error(
            "page.click_for_new_tab()",
            page.click_for_new_tab("css:#stay-put", Some(100), false)
                .expect_err("page.click_for_new_tab() should raise"),
        );
        assert_no_new_tab_error(
            "page.click_middle(get_tab=true)",
            page.click_middle("css:#stay-put", Some(100), true)
                .expect_err("page.click_middle(get_tab=true) should raise"),
        );

        let element = page.wait_for("css:#stay-put", 1_000)?;
        assert_no_new_tab_error(
            "element.clicker().for_new_tab()",
            element
                .clicker()
                .for_new_tab(Some(100), false)
                .expect_err("element.clicker().for_new_tab() should raise"),
        );
        assert_no_new_tab_error(
            "element.clicker().middle(true)",
            element
                .clicker()
                .middle(true)
                .expect_err("element.clicker().middle(true) should raise"),
        );

        let web_element = WebElement::Browser(page.wait_for("css:#stay-put", 1_000)?);
        assert_no_new_tab_error(
            "web_element.clicker().for_new_tab()",
            web_element
                .clicker()
                .for_new_tab(Some(100), false)
                .expect_err("web_element.clicker().for_new_tab() should raise"),
        );
        assert_no_new_tab_error(
            "web_element.clicker().middle(true)",
            web_element
                .clicker()
                .middle(true)
                .expect_err("web_element.clicker().middle(true) should raise"),
        );

        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page and element no-new-tab runtime regression");
}

#[test]
fn element_and_webelement_clicker_upload_and_download_work_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("element-clicker-transfer").expect("launch headless browser");
    let (download_url, download_server) = spawn_download_site();

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                r#"(() => {
                    document.body.innerHTML = `
                        <input id="picker" type="file" multiple
                            onchange='document.getElementById("out").textContent = Array.from(this.files).map(f => f.name).join(",")' />
                        <div id="out"></div>
                    `;
                    return true;
                })()"#,
            )?;

        let first = temp_dir.join("first.txt");
        let second = temp_dir.join("second.txt");
        fs::write(&first, "first")?;
        fs::write(&second, "second")?;
        let files = vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ];

        page.wait_for("css:#picker", 1_000)?
            .clicker()
            .to_upload(&files, Some(5_000), false)?;
        assert_eq!(
            page.run_js("document.getElementById('picker').files.length")?,
            Value::from(2)
        );
        assert_eq!(
            page.run_js("document.getElementById('out').textContent")?,
            Value::from("first.txt,second.txt")
        );

        page.goto(&download_url)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.set_download_path(temp_dir.to_string_lossy().as_ref())?;
        let mission = WebElement::Browser(page.wait_for("css:#download", 1_000)?)
            .clicker()
            .to_download(None, None, None, false, Some(5_000), false, false)?
            .expect("clicker download mission");
        assert_eq!(mission.suggested_filename()?, "openpage.txt".to_string());
        let final_path = mission
            .wait(false, Some(10_000), false)?
            .expect("download final path");
        assert!(PathBuf::from(&final_path).exists());
        assert!(final_path.ends_with("openpage.txt"));
        Ok(())
    })();

    let close_result = browser.close();
    let server_result = download_server.join();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    if let Err(err) = server_result {
        panic!("join download server: {err:?}");
    }
    result.expect("element clicker upload/download runtime regression");
}

#[test]
fn page_zoom_css_fallback_roundtrips_at_runtime() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-zoom-css").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
                "(() => { document.documentElement.style.zoom = '0.9'; return getComputedStyle(document.documentElement).zoom; })()",
            )?;

        page.set_zoom_factor(1.25)?;
        let zoom = page.zoom_factor()?;
        assert!(
            (zoom - 1.25).abs() < 0.01,
            "expected managed zoom near 1.25, got {zoom}"
        );
        assert_eq!(
            page.run_js("document.documentElement.getAttribute('data-openpage-zoom-managed')")?,
            Value::from("1")
        );
        assert_eq!(
            page.run_js("getComputedStyle(document.documentElement).zoom")?,
            Value::from("1.25")
        );

        page.reset_zoom_factor()?;
        assert_eq!(page.zoom_factor()?, 1.0);
        assert_eq!(
            page.run_js(
                "(() => document.documentElement.hasAttribute('data-openpage-zoom-managed'))()",
            )?,
            Value::from(false)
        );
        assert_eq!(
            page.run_js("getComputedStyle(document.documentElement).zoom")?,
            Value::from("0.9")
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page zoom css fallback runtime regression");
}

#[test]
fn page_clipboard_roundtrips_with_permission_override() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-clipboard").expect("launch headless browser");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind clipboard server");
    listener
        .set_nonblocking(true)
        .expect("set clipboard server nonblocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("clipboard server addr")
    );
    let server = thread::spawn(move || {
        let html = r#"<!doctype html>
<html>
<body>
  <main id="app">clipboard test</main>
</body>
</html>
"#;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut served = false;
        while Instant::now() < deadline && !served {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(_) => break,
            };
            let mut buffer = [0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer) else {
                continue;
            };
            if read == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            match path {
                "/" => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    );
                    served = true;
                }
                _ => {
                    let body = "not found";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            }
        }
    });

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(Some(address.as_str()))?;
        assert!(page.wait_for_doc_loaded(5_000)?);
        assert_eq!(
                page.run_js(
                    "(() => ({ secure: window.isSecureContext, hasClipboard: !!navigator.clipboard }))()",
                )?,
                json!({"secure": true, "hasClipboard": true})
            );

        page.set_permission("clipboard-read", "granted", None, None)?;
        page.set_permission("clipboard-write", "granted", None, None)?;
        page.clipboard_write_text("openpage clipboard runtime")?;
        assert_eq!(
            page.clipboard_read_text()?,
            "openpage clipboard runtime".to_string()
        );
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);
    let server_result = server.join();

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    if let Err(err) = server_result {
        panic!("join clipboard server: {err:?}");
    }
    result.expect("page clipboard permission runtime regression");
}

#[test]
fn page_window_id_distinguishes_same_and_new_window_tabs() {
    let (browser, temp_dir) =
        launch_headless_test_browser("page-window-id").expect("launch headless browser");

    let result = (|| -> crate::OpenPageResult<()> {
        let page = browser.new_page(None)?;
        let same_window_tab = browser.new_tab(None, false, false, false)?;
        let new_window_tab = browser.new_tab(None, true, false, false)?;

        assert_eq!(page.window_id()?, same_window_tab.window_id()?);
        assert_ne!(page.window_id()?, new_window_tab.window_id()?);
        Ok(())
    })();

    let close_result = browser.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless browser: {err}");
    }
    result.expect("page window id runtime regression");
}
