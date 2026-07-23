use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{WebElement, WebFrame, WebMode, WebPage, webpage_timeout_seconds_to_millis};
use crate::browser::{BrowserTabReference, LaunchOptions};
use crate::element_list::{ElementsListExt, ElementsOne, ElementsOneOwned};
use crate::session::snapshot_root;
use crate::settings::scoped_test_settings;
use crate::{
    By, DownloadFileExistsMode, Element, Frame, Keys, LocatorInput, OpenPageError, OpenPageResult,
    Page, Session, SessionCookieParam, SessionElement, SessionOptions, Settings, ShadowRoot,
};

fn runtime_test_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openpage-webpage-{name}-{}-{nanos}",
        std::process::id()
    ))
}

fn launch_headless_test_webpage(
    name: &str,
    mode: WebMode,
) -> crate::OpenPageResult<(WebPage, PathBuf)> {
    let temp_dir = runtime_test_temp_dir(name);
    fs::create_dir_all(&temp_dir).expect("create runtime test temp dir");

    let mut options = LaunchOptions::default();
    options.headless(true);
    options.auto_port(true);
    options.new_env(true);
    options.set_tmp_path(&temp_dir);
    options.set_timeouts(Some(1.0), Some(5.0), Some(1.0));

    WebPage::new(mode, options, SessionOptions::default()).map(|page| (page, temp_dir))
}

fn write_test_html(path: &Path, html: &str) -> crate::OpenPageResult<()> {
    fs::write(path, html).map_err(|err| {
        crate::OpenPageError::PageOperation(format!(
            "write runtime session test html {}: {err}",
            path.display()
        ))
    })
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
        while Instant::now() < deadline {
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
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{html}",
                html.len()
            );
            break;
        }
    });
    (port, handle)
}

#[test]
fn webpage_timeout_validation_follows_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let english = webpage_timeout_seconds_to_millis(f64::NAN)
        .expect_err("english timeout validation should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("timeout must be a finite non-negative number")
    ));

    Settings::set_language("cn");

    let chinese = webpage_timeout_seconds_to_millis(f64::NAN)
        .expect_err("chinese timeout validation should fail");
    assert!(matches!(
        chinese,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("timeout 必须是有限且非负的数字")
    ));
}

#[test]
fn webpage_session_tail_driver_only_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-tail-driver-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .set_timeouts(None, Some(1.0), None)
            .expect_err("session-mode WebPage page_load timeout should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains(
                    "set_timeouts(page_load/script) is only available in driver mode"
                )
        ));

        Settings::set_language("cn");

        let element = WebElement::Session(
            snapshot_root("<html><body><button>OK</button></body></html>")
                .expect("session snapshot root should parse"),
        );
        let chinese_clicker = element
            .clicker()
            .middle(false)
            .expect_err("session-backed WebElement clicker should fail");
        assert!(matches!(
            chinese_clicker,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("clicker() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session tail driver-only errors should localize");
}

#[test]
fn web_element_session_frame_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><iframe></iframe></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = match element.get_frame("tag:iframe") {
        Err(error) => error,
        Ok(_) => panic!("session-backed WebElement get_frame should fail"),
    };
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("get_frame() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese = match element.get_frame_by_index(1) {
        Err(error) => error,
        Ok(_) => panic!("session-backed WebElement get_frame_by_index should fail"),
    };
    assert!(matches!(
        chinese,
        OpenPageError::UnsupportedOperation(ref message)
        if message.contains("get_frame_by_index() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_static_find_aliases_delegate_to_snapshot() {
    let element = WebElement::Session(
        snapshot_root(
            r#"<html><body><section id="root"><span class="item">A</span><span class="item">B</span></section></body></html>"#,
        )
        .expect("session snapshot root should parse"),
    );

    assert_eq!(
        element
            .s_ele((By::ID, "root"))
            .expect("s_ele should find session element")
            .attr("id")
            .expect("id attr should read"),
        Some("root".to_string())
    );
    assert_eq!(
        element
            .s_eles((By::CLASS_NAME, "item"))
            .expect("s_eles should find session elements")
            .len(),
        2
    );
}

#[test]
fn webpage_session_frame_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-frame-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = match page.get_frame("tag:iframe") {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage get_frame should fail"),
        };
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("get_frame() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_index = match page.get_frame_by_index(1) {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage get_frame_by_index should fail"),
        };
        assert!(matches!(
            chinese_index,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("get_frame_by_index() 仅在 driver 模式可用")
        ));
        let chinese_contexts = match page.get_frame_contexts(None::<&str>) {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage get_frame_contexts should fail"),
        };
        assert!(matches!(
            chinese_contexts,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("get_frame_contexts() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session frame errors should localize");
}

#[test]
fn webpage_session_script_cdp_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-script-cdp-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .run_js("return 1")
            .expect_err("session-mode WebPage run_js should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("run_js() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_loaded = page
            .run_js_loaded("return 1")
            .expect_err("session-mode WebPage run_js_loaded should fail");
        assert!(matches!(
            chinese_loaded,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("run_js_loaded() 仅在 driver 模式可用")
        ));
        let chinese_async = page
            .run_async_js("return 1")
            .expect_err("session-mode WebPage run_async_js should fail");
        assert!(matches!(
            chinese_async,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("run_async_js() 仅在 driver 模式可用")
        ));
        let chinese_stop = page
            .stop_loading()
            .expect_err("session-mode WebPage stop_loading should fail");
        assert!(matches!(
            chinese_stop,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("stop_loading() 仅在 driver 模式可用")
        ));
        let chinese_cdp = page
            .run_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false))
            .expect_err("session-mode WebPage run_cdp should fail");
        assert!(matches!(
            chinese_cdp,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("run_cdp() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session script/cdp errors should localize");
}

#[test]
fn webpage_session_tool_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-tool-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .add_init_js("window.__openpageInit = true;")
            .expect_err("session-mode WebPage add_init_js should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("add_init_js() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_cache = page
            .clear_cache(true, true, true, true)
            .expect_err("session-mode WebPage clear_cache should fail");
        assert!(matches!(
            chinese_cache,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("clear_cache() 仅在 driver 模式可用")
        ));
        let chinese_permission = page
            .set_permission("clipboard-read", "granted", None, None)
            .expect_err("session-mode WebPage set_permission should fail");
        assert!(matches!(
            chinese_permission,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("set_permission() 仅在 driver 模式可用")
        ));
        let chinese_clipboard = page
            .clipboard_write_text("openpage")
            .expect_err("session-mode WebPage clipboard_write_text should fail");
        assert!(matches!(
            chinese_clipboard,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("clipboard_write_text() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session tool errors should localize");
}

#[test]
fn web_element_session_driver_only_info_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><input id='q' value='rust'></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .property("value")
        .expect_err("session-backed WebElement property should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("property() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_state = element
        .is_clickable()
        .expect_err("session-backed WebElement is_clickable should fail");
    assert!(matches!(
        chinese_state,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("is_clickable() 仅在 driver 模式可用")
    ));
    let chinese_style = element
        .style("display", None)
        .expect_err("session-backed WebElement style should fail");
    assert!(matches!(
        chinese_style,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("style() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_scroll_resource_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><img id='logo' src='logo.png'></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .scroll_to_top()
        .expect_err("session-backed WebElement scroll_to_top should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("scroll_to_top() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_src = element
        .src(100, false)
        .expect_err("session-backed WebElement src should fail");
    assert!(matches!(
        chinese_src,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("src() 仅在 driver 模式可用")
    ));
    let chinese_shadow = element
        .shadow_root()
        .expect_err("session-backed WebElement shadow_root should fail");
    assert!(matches!(
        chinese_shadow,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("shadow_root() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_interaction_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><button id='ok'>OK</button></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = match element.offset(None::<&str>, None, None, 100) {
        Err(error) => error,
        Ok(_) => panic!("session-backed WebElement offset should fail"),
    };
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("offset() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_click = element
        .click()
        .expect_err("session-backed WebElement click should fail");
    assert!(matches!(
        chinese_click,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("click() 仅在 driver 模式可用")
    ));
    let chinese_input = element
        .input("text")
        .expect_err("session-backed WebElement input should fail");
    assert!(matches!(
        chinese_input,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("input() 仅在 driver 模式可用")
    ));
    let chinese_key = element
        .press_key("Enter")
        .expect_err("session-backed WebElement press_key should fail");
    assert!(matches!(
        chinese_key,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("press_key() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_script_capture_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><div id='app'></div></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .run_js("return 1")
        .expect_err("session-backed WebElement run_js should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("run_js() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_capture = element
        .screenshot_bytes(false, 100)
        .expect_err("session-backed WebElement screenshot_bytes should fail");
    assert!(matches!(
        chinese_capture,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("screenshot_bytes() 仅在 driver 模式可用")
    ));
    let chinese_focus = element
        .focus()
        .expect_err("session-backed WebElement focus should fail");
    assert!(matches!(
        chinese_focus,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("focus() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_drag_set_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><input id='agree' type='checkbox'></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .drag(10.0, 5.0, 0.1)
        .expect_err("session-backed WebElement drag should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("drag() is only available in driver mode")
    ));
    let english_drag_to = element
        .drag_to(&element, 0.1)
        .expect_err("session-backed WebElement drag_to should fail");
    assert!(matches!(
        english_drag_to,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("drag_to() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_set = element
        .set_attr("data-x", "1")
        .expect_err("session-backed WebElement set_attr should fail");
    assert!(matches!(
        chinese_set,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("set_attr() 仅在 driver 模式可用")
    ));
    let chinese_check = element
        .check(false, false)
        .expect_err("session-backed WebElement check should fail");
    assert!(matches!(
        chinese_check,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("check() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_select_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><select><option>A</option></select></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .select_by_text("A")
        .expect_err("session-backed WebElement select_by_text should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("select_by_text() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_index = element
        .select_by_index(1usize)
        .expect_err("session-backed WebElement select_by_index should fail");
    assert!(matches!(
        chinese_index,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("select_by_index() 仅在 driver 模式可用")
    ));
    let chinese_option = element
        .select_by_option(&element)
        .expect_err("session-backed WebElement select_by_option should fail");
    assert!(matches!(
        chinese_option,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("select_by_option() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_cancel_select_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root(
            "<html><body><select multiple><option selected>A</option></select></body></html>",
        )
        .expect("session snapshot root should parse"),
    );
    let english = element
        .cancel_by_text("A")
        .expect_err("session-backed WebElement cancel_by_text should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("cancel_by_text() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_index = element
        .cancel_by_index(1usize)
        .expect_err("session-backed WebElement cancel_by_index should fail");
    assert!(matches!(
        chinese_index,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("cancel_by_index() 仅在 driver 模式可用")
    ));
    let chinese_option = element
        .cancel_by_option(&element)
        .expect_err("session-backed WebElement cancel_by_option should fail");
    assert!(matches!(
        chinese_option,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("cancel_by_option() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_select_tail_rect_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><div id='box'>box</div></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .select_all()
        .expect_err("session-backed WebElement select_all should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("select_all() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_rect = element
        .rect_location()
        .expect_err("session-backed WebElement rect_location should fail");
    assert!(matches!(
        chinese_rect,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("rect_location() 仅在 driver 模式可用")
    ));
    let chinese_size = element
        .rect_size()
        .expect_err("session-backed WebElement rect_size should fail");
    assert!(matches!(
        chinese_size,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("rect_size() 仅在 driver 模式可用")
    ));
}

#[test]
fn web_element_session_driver_only_wait_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let element = WebElement::Session(
        snapshot_root("<html><body><button id='ok'>OK</button></body></html>")
            .expect("session snapshot root should parse"),
    );
    let english = element
        .wait_until_displayed(100)
        .expect_err("session-backed WebElement wait_until_displayed should fail");
    assert!(matches!(
        english,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("wait_until_displayed() is only available in driver mode")
    ));

    Settings::set_language("cn");

    let chinese_clickable = element
        .wait_until_clickable(100)
        .expect_err("session-backed WebElement wait_until_clickable should fail");
    assert!(matches!(
        chinese_clickable,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("wait_until_clickable() 仅在 driver 模式可用")
    ));
    let chinese_stop_moving = element
        .wait_until_stop_moving(100)
        .expect_err("session-backed WebElement wait_until_stop_moving should fail");
    assert!(matches!(
        chinese_stop_moving,
        OpenPageError::UnsupportedOperation(ref message)
            if message.contains("wait_until_stop_moving() 仅在 driver 模式可用")
    ));
}

#[test]
fn webpage_session_driver_action_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-driver-action-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .navigation_snapshot()
            .expect_err("session-mode WebPage navigation_snapshot should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("navigation_snapshot() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_actions = match page.actions() {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage actions should fail"),
        };
        assert!(matches!(
            chinese_actions,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("actions() 仅在 driver 模式可用")
        ));
        let chinese_new_actions = match page.new_actions() {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage new_actions should fail"),
        };
        assert!(matches!(
            chinese_new_actions,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("new_actions() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session driver action errors should localize");
}

#[test]
fn webpage_session_click_helper_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-click-helper-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = match page.click_to_download(
            "css:#download",
            None,
            None,
            None,
            false,
            Some(100),
            false,
            false,
        ) {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage click_to_download should fail"),
        };
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("click_to_download() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let files = vec!["/tmp/upload.txt".to_string()];
        let chinese_upload = page
            .click_to_upload("css:#upload", &files, Some(100), false)
            .expect_err("session-mode WebPage click_to_upload should fail");
        assert!(matches!(
            chinese_upload,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("click_to_upload() 仅在 driver 模式可用")
        ));
        let chinese_new_tab = match page.click_for_new_tab("css:#open", Some(100), false) {
            Err(error) => error,
            Ok(_) => panic!("session-mode WebPage click_for_new_tab should fail"),
        };
        assert!(matches!(
            chinese_new_tab,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("click_for_new_tab() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session click helper errors should localize");
}

#[test]
fn webpage_session_capture_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-capture-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .save_screenshot(temp_dir.join("page.png"), false)
            .expect_err("session-mode WebPage save_screenshot should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("save_screenshot() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_bytes = page
            .screenshot_bytes(false, None, None)
            .expect_err("session-mode WebPage screenshot_bytes should fail");
        assert!(matches!(
            chinese_bytes,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("screenshot_bytes() 仅在 driver 模式可用")
        ));
        let chinese_pdf = page
            .save_pdf(temp_dir.join("page.pdf"))
            .expect_err("session-mode WebPage save_pdf should fail");
        assert!(matches!(
            chinese_pdf,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("save_pdf() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session capture errors should localize");
}

#[test]
fn webpage_session_viewport_navigation_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) = launch_headless_test_webpage(
        "webpage-session-viewport-navigation-errors",
        WebMode::Session,
    )
    .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .scroll_position()
            .expect_err("session-mode WebPage scroll_position should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("scroll_position() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_viewport = page
            .viewport_size()
            .expect_err("session-mode WebPage viewport_size should fail");
        assert!(matches!(
            chinese_viewport,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("viewport_size() 仅在 driver 模式可用")
        ));
        let chinese_back = page
            .back(1)
            .expect_err("session-mode WebPage back should fail");
        assert!(matches!(
            chinese_back,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("back() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session viewport/navigation errors should localize");
}

#[test]
fn webpage_session_scroll_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-scroll-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .scroll_to_top()
            .expect_err("session-mode WebPage scroll_to_top should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("scroll_to_top() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_location = page
            .scroll_to_location(10.0, 20.0)
            .expect_err("session-mode WebPage scroll_to_location should fail");
        assert!(matches!(
            chinese_location,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("scroll_to_location() 仅在 driver 模式可用")
        ));
        let chinese_right = page
            .scroll_right(100.0)
            .expect_err("session-mode WebPage scroll_right should fail");
        assert!(matches!(
            chinese_right,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("scroll_right() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session scroll errors should localize");
}

#[test]
fn webpage_session_element_mutation_errors_follow_language_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-session-element-mutation-errors", WebMode::Session)
            .expect("launch headless webpage");
    let result = (|| -> crate::OpenPageResult<()> {
        let english = page
            .active_element()
            .expect_err("session-mode WebPage active_element should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("active_element() is only available in driver mode")
        ));

        Settings::set_language("cn");

        let chinese_remove = page
            .remove_element("css:#gone")
            .expect_err("session-mode WebPage remove_element should fail");
        assert!(matches!(
            chinese_remove,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("remove_element() 仅在 driver 模式可用")
        ));
        let chinese_add = page
            .add_element_html("<div></div>", None::<&str>, None::<&str>)
            .expect_err("session-mode WebPage add_element_html should fail");
        assert!(matches!(
            chinese_add,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("add_element_html() 仅在 driver 模式可用")
        ));
        Ok(())
    })();

    let _ = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);
    result.expect("webpage session element mutation errors should localize");
}

#[test]
fn webpage_browser_info_wrapper_signatures_accept_calls() {
    fn assert_calls(page: &WebPage) {
        let _ = page.browser_pid();
        let _ = page.process_id();
        let _ = page.browser_version();
        let _ = page.address();
        let _ = page.reconnect(0);
        let _ = page.close(false, false);
        let _ = page.close(true, true);
        let _ = page.close_with_options(false, false);
        let _ = page.close_with_options(true, true);
    }

    let _ = assert_calls as fn(&WebPage);
}

#[test]
fn webpage_listener_interceptor_alias_signatures_accept_calls() {
    fn assert_calls(page: &WebPage) {
        let _ = page.listener();
        let _ = page.listen();
        let _ = page.interceptor();
        let _ = page.intercept();
    }

    let _ = assert_calls as fn(&WebPage);
}

#[test]
fn webpage_close_driver_and_close_session_signatures_accept_roundtrip_types() {
    let _ = WebPage::close_driver as fn(WebPage) -> OpenPageResult<Session>;
    let _ = WebPage::close_session as fn(WebPage) -> OpenPageResult<Page>;
}

#[test]
fn webpage_exposes_browser_info_wrappers_at_runtime() {
    let (page, temp_dir) = launch_headless_test_webpage("browser-info-wrappers", WebMode::Driver)
        .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert_eq!(page.process_id(), page.browser_pid());
        assert_eq!(page.process_id(), page.browser.process_id());
        assert_eq!(page.address()?, page.browser.address());
        assert_eq!(page.browser_version()?, page.browser.version()?);
        assert_eq!(
            page.browser().map(|browser| browser.address()),
            Some(page.browser.address())
        );
        let main_frame_id = page.main_frame_id()?;
        assert!(!main_frame_id.is_empty());
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage browser-info wrapper regression");
}

#[test]
fn webpage_reconnect_rebuilds_browser_connection() {
    let (page, temp_dir) = launch_headless_test_webpage("webpage-reconnect", WebMode::Driver)
        .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<WebPage> {
        page.driver.run_js(
            r#"(() => {
                document.body.innerHTML = '<div id="msg">webpage reconnect</div>';
                return true;
            })()"#,
        )?;

        let reconnected = page.reconnect(0)?;
        assert_eq!(reconnected.target_id(), page.target_id());
        assert_eq!(reconnected.address()?, page.address()?);
        assert_eq!(reconnected.process_id(), page.process_id());
        assert_eq!(
            reconnected
                .driver
                .run_js("document.querySelector('#msg').textContent")?,
            Value::from("webpage reconnect")
        );
        Ok(reconnected)
    })();

    let reconnected = match result {
        Ok(page) => page,
        Err(err) => {
            let _ = page.quit();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("webpage reconnect regression failed before cleanup: {err}");
        }
    };

    let close_result = reconnected.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage after reconnect: {err}");
    }
}

#[test]
fn webpage_close_driver_returns_session_page_with_synced_state() {
    let (page, temp_dir) = launch_headless_test_webpage("webpage-close-driver", WebMode::Driver)
        .expect("launch headless webpage");
    let html_path = temp_dir.join("close-driver.html");
    let html_path_str = html_path.to_str().expect("html path str");

    let result = (|| -> crate::OpenPageResult<Session> {
        write_test_html(
            &html_path,
            r#"
            <html>
              <body>
                <div id="msg">driver close</div>
              </body>
            </html>
            "#,
        )?;
        assert!(page.get(html_path_str)?);

        let session_page = page.close_driver()?;
        let url = session_page.url()?.ok_or_else(|| {
            OpenPageError::PageOperation("session url missing after close_driver".to_string())
        })?;
        assert!(
            url.starts_with("file://") || url == html_path_str,
            "unexpected session url after close_driver: {url}"
        );
        assert!(
            session_page.html()?.contains("driver close"),
            "session html should keep driver page content"
        );
        Ok(session_page)
    })();

    let session_page = match result {
        Ok(page) => page,
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("webpage close_driver regression failed before cleanup: {err}");
        }
    };

    let close_result = session_page.close();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close session page after close_driver: {err}");
    }
}

#[test]
fn webpage_close_session_returns_driver_page_with_synced_state() {
    let (page, temp_dir) = launch_headless_test_webpage("webpage-close-session", WebMode::Session)
        .expect("launch headless webpage");
    let html_path = temp_dir.join("close-session.html");
    let html_path_str = html_path.to_str().expect("html path str");

    let result = (|| -> crate::OpenPageResult<Page> {
        write_test_html(
            &html_path,
            r#"
            <html>
              <body>
                <div id="msg">session close</div>
              </body>
            </html>
            "#,
        )?;
        assert!(page.get(html_path_str)?);

        let driver_page = page.close_session()?;
        assert!(driver_page.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            driver_page.run_js("document.querySelector('#msg').textContent")?,
            Value::from("session close")
        );
        Ok(driver_page)
    })();

    let driver_page = match result {
        Ok(page) => page,
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("webpage close_session regression failed before cleanup: {err}");
        }
    };

    let close_result = driver_page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close driver page after close_session: {err}");
    }
}

#[test]
fn webpage_close_can_close_other_tabs_without_closing_self() {
    let (page, temp_dir) = launch_headless_test_webpage("webpage-close-others", WebMode::Driver)
        .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        let baseline_tabs = page.tabs_count()?;
        assert!(baseline_tabs >= 1);
        let extra = page.new_tab(None, false, false, false)?;
        assert_eq!(page.tabs_count()?, baseline_tabs + 1);

        page.close(true, true)?;
        assert_eq!(page.tabs_count()?, 1);

        let current = page
            .get_tab(Some(&page), None, None, None::<&str>, false)?
            .expect("current tab should still exist");
        match current {
            BrowserTabReference::WebPage(current_page) => {
                assert_eq!(current_page.target_id(), page.target_id());
            }
            BrowserTabReference::Page(current_page) => {
                panic!(
                    "webpage.get_tab() should return webpage, got page {}",
                    current_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("current tab should stay as webpage, got id {id}");
            }
        }

        assert!(
            page.tab_ids()?
                .into_iter()
                .all(|target_id| target_id != extra.target_id()),
            "other tabs should be closed"
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage close others regression");
}

#[test]
fn webpage_tab_wrappers_return_webpage_objects_when_requested() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) = launch_headless_test_webpage("webpage-tab-wrappers", WebMode::Driver)
        .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        let current = page
            .get_tab(Some(&page), None, None, None::<&str>, false)?
            .expect("current tab should resolve");
        match current {
            BrowserTabReference::WebPage(current_page) => {
                assert_eq!(current_page.target_id(), page.target_id());
                assert_eq!(current_page.mode()?, page.mode()?);
            }
            BrowserTabReference::Page(current_page) => {
                panic!(
                    "webpage.get_tab() should return webpage, got page {}",
                    current_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webpage.get_tab() should return webpage, got id {id}");
            }
        }

        let latest = page.latest_tab()?.expect("latest tab should exist");
        match latest {
            BrowserTabReference::WebPage(latest_page) => {
                assert_eq!(latest_page.target_id(), page.target_id());
            }
            BrowserTabReference::Page(latest_page) => {
                panic!(
                    "webpage.latest_tab() should return webpage, got page {}",
                    latest_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webpage.latest_tab() should return webpage, got id {id}");
            }
        }

        let tab_types = ["page", "tab"];
        let tabs = page.get_tabs(None, None, Some(&tab_types[..]), false)?;
        assert!(
            tabs.into_iter()
                .all(|reference| matches!(reference, BrowserTabReference::WebPage(_))),
            "webpage.get_tabs() should return webpage objects when as_id=false"
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage tab wrapper regression");
}

#[test]
fn webpage_new_tab_click_helpers_return_webpage_objects() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-click-new-tab-wrappers", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                const newTabUrl = 'about:blank#webpage-new-tab';
                const middleUrl = 'about:blank#webpage-middle-tab';
                document.body.innerHTML = `
                    <a id="open-tab" href="${newTabUrl}" target="_blank">Open tab</a>
                    <a id="middle-open-tab" href="${middleUrl}">Open by middle click</a>
                `;
                return true;
            })()"#,
        )?;

        let new_page = page
            .click_for_new_tab("css:#open-tab", Some(5_000), false)?
            .expect("webpage click_for_new_tab should return a tab");
        assert_eq!(new_page.mode()?, WebMode::Driver);
        assert!(new_page.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            new_page.url()?,
            Some("about:blank#webpage-new-tab".to_string())
        );

        let middle_page = page
            .click_middle("css:#middle-open-tab", Some(5_000), true)?
            .expect("webpage click_middle(get_tab=true) should return a tab");
        assert_eq!(middle_page.mode()?, WebMode::Driver);
        assert!(middle_page.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            middle_page.url()?,
            Some("about:blank#webpage-middle-tab".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage new-tab click wrapper regression");
}

#[test]
fn webframe_new_tab_click_helpers_return_webpage_references() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-click-new-tab-wrappers", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                const newTabUrl = 'about:blank#webframe-new-tab';
                const middleUrl = 'about:blank#webframe-middle-tab';
                document.body.innerHTML = `
                    <a id="open-tab" href="${newTabUrl}" target="_blank">Open tab</a>
                    <a id="middle-open-tab" href="${middleUrl}">Open by middle click</a>
                    <iframe id="demo-frame"
                        srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);

        let new_tab = frame
            .click_for_new_tab("css:#open-tab", Some(5_000), false)?
            .expect("webframe click_for_new_tab should return a tab");
        match new_tab {
            BrowserTabReference::WebPage(new_page) => {
                assert_eq!(new_page.mode()?, WebMode::Driver);
                assert!(new_page.wait_for_doc_loaded(5_000)?);
                assert_eq!(
                    new_page.url()?,
                    Some("about:blank#webframe-new-tab".to_string())
                );
            }
            BrowserTabReference::Page(new_page) => {
                panic!(
                    "webframe click_for_new_tab should return webpage, got page {}",
                    new_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webframe click_for_new_tab should return webpage, got id {id}");
            }
        }

        let middle_tab = frame
            .click_middle("css:#middle-open-tab", Some(5_000), true)?
            .expect("webframe click_middle(get_tab=true) should return a tab");
        match middle_tab {
            BrowserTabReference::WebPage(middle_page) => {
                assert_eq!(middle_page.mode()?, WebMode::Driver);
                assert!(middle_page.wait_for_doc_loaded(5_000)?);
                assert_eq!(
                    middle_page.url()?,
                    Some("about:blank#webframe-middle-tab".to_string())
                );
            }
            BrowserTabReference::Page(middle_page) => {
                panic!(
                    "webframe click_middle should return webpage, got page {}",
                    middle_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webframe click_middle should return webpage, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webframe new-tab click wrapper regression");
}

#[test]
fn webframe_tab_references_preserve_mix_context() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-tab-reference-wrappers", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        match frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "mix WebFrame owner_reference should return webpage, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("mix WebFrame owner_reference should return webpage, got id {id}");
            }
        }
        match frame.tab_reference() {
            BrowserTabReference::WebPage(tab) => {
                assert_eq!(tab.target_id(), page.target_id());
                assert_eq!(tab.mode()?, WebMode::Driver);
            }
            BrowserTabReference::Page(tab) => {
                panic!(
                    "mix WebFrame tab_reference should return webpage, got page {}",
                    tab.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("mix WebFrame tab_reference should return webpage, got id {id}");
            }
        }
        match frame.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            WebElement::Browser(element) => {
                panic!(
                    "mix WebFrame frame_element_reference should return mix element, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("mix WebFrame frame_element_reference should return mix element");
            }
        }

        let browser_frame = WebFrame::Browser(page.driver.get_frame("css:#demo-frame")?);
        match browser_frame.tab_reference() {
            BrowserTabReference::Page(tab) => {
                assert_eq!(tab.target_id(), page.target_id());
            }
            BrowserTabReference::WebPage(tab) => {
                panic!(
                    "browser WebFrame tab_reference should return page, got webpage {}",
                    tab.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("browser WebFrame tab_reference should return page, got id {id}");
            }
        }
        match browser_frame.frame_ele_reference()? {
            WebElement::Browser(element) => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
            }
            WebElement::Mix { page, .. } => {
                panic!(
                    "browser WebFrame frame_ele_reference should return browser element, got mix page {}",
                    page.target_id()
                );
            }
            WebElement::Session(_) => {
                panic!("browser WebFrame frame_ele_reference should return browser element");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webframe tab reference wrapper regression");
}

#[test]
fn mix_webelement_get_frame_preserves_webframe_context() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("webelement-get-frame-mix", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                document.body.innerHTML = `
                    <section id="host">
                        <iframe id="demo-frame"
                            srcdoc="<html><body><div id='inside'>inside</div></body></html>">
                        </iframe>
                    </section>
                `;
                return true;
            })()"#,
        )?;

        let host = page.find("css:#host")?;
        let frame = host.get_frame("css:#demo-frame").map_err(|err| {
            OpenPageError::PageOperation(format!("mix host get_frame first: {err}"))
        })?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            frame
                .find("css:#inside")
                .map_err(|err| OpenPageError::PageOperation(format!("frame find: {err}")))?
                .text()?,
            Some("inside".to_string())
        );
        frame.set_none_element_value(Some("mix missing"), true)?;
        let same_frame = host.get_frame((By::ID, "demo-frame")).map_err(|err| {
            OpenPageError::PageOperation(format!("mix host get_frame second: {err}"))
        })?;
        assert_eq!(same_frame.id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            same_frame.frame_element()
        ));
        assert_eq!(
            same_frame.ele(".does-not-exist")?.text()?,
            Some("mix missing".to_string())
        );
        match frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "Mix WebElement get_frame should return mix WebFrame, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("Mix WebElement get_frame should return mix WebFrame, got id {id}");
            }
        }
        match frame.frame_element_reference().map_err(|err| {
            OpenPageError::PageOperation(format!("frame element reference: {err}"))
        })? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "Mix WebElement get_frame frame element should stay mix, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("Mix WebElement get_frame frame element should stay mix");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("mix WebElement get_frame wrapper regression");
}

#[test]
fn disconnected_webframe_reconnect_preserves_mix_context() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) = launch_headless_test_webpage("webframe-disconnect-mix", WebMode::Driver)
        .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<WebFrame> {
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

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        let disconnected = frame.disconnect()?;
        let reconnected = disconnected.reconnect(0)?;
        assert_eq!(
            reconnected.run_js("document.querySelector('#inside').textContent")?,
            Value::from("frame reconnect")
        );
        match reconnected.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "reconnected mix WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("reconnected mix WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(reconnected)
    })();

    let reconnected = match result {
        Ok(frame) => frame,
        Err(err) => {
            let _ = page.quit();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("webframe disconnect mix regression failed before cleanup: {err}");
        }
    };

    let close_result = reconnected.owner().quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage after frame reconnect: {err}");
    }
}

#[test]
fn webframe_reconnect_preserves_mix_runtime_config() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-reconnect-runtime-config", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<WebFrame> {
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

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("webframe missing"), true)?;

        let reconnected = frame.reconnect(0)?;
        assert_eq!(
            reconnected.ele(".does-not-exist")?.text()?,
            Some("webframe missing".to_string())
        );
        match reconnected.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "reconnected WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("reconnected WebFrame should keep webpage owner, got id {id}");
            }
        }

        let disconnected = reconnected.disconnect()?;
        let roundtrip = disconnected.reconnect(0)?;
        assert_eq!(
            roundtrip.ele(".does-not-exist")?.text()?,
            Some("webframe missing".to_string())
        );
        match roundtrip.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "roundtrip WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("roundtrip WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(roundtrip)
    })();

    let frame = match result {
        Ok(frame) => frame,
        Err(err) => {
            let _ = page.quit();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("webframe reconnect runtime config regression failed before cleanup: {err}");
        }
    };

    let close_result = frame.owner().quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage after webframe reconnect config test: {err}");
    }
}

#[test]
fn nested_webframe_reconnect_preserves_mix_runtime_config() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("nested-webframe-reconnect-runtime-config", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<WebFrame> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                document.body.innerHTML = `
                    <iframe id="outer-frame"
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
                document.getElementById('outer-host').innerHTML = `
                    <iframe id="inner-frame"
                        srcdoc="<html><body><div id='inside'>nested reconnect</div></body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        let inner = outer.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(2_000)?);
        inner.set_none_element_value(Some("nested reconnect missing"), true)?;

        let reconnected = inner.reconnect(0)?;
        assert_eq!(
            reconnected.run_js("document.querySelector('#inside').textContent")?,
            Value::from("nested reconnect")
        );
        assert_eq!(
            reconnected.ele(".does-not-exist")?.text()?,
            Some("nested reconnect missing".to_string())
        );
        match reconnected.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "reconnected nested WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("reconnected nested WebFrame should keep webpage owner, got id {id}");
            }
        }

        let disconnected = reconnected.disconnect()?;
        let roundtrip = disconnected.reconnect(0)?;
        assert_eq!(
            roundtrip.run_js("document.querySelector('#inside').textContent")?,
            Value::from("nested reconnect")
        );
        assert_eq!(
            roundtrip.ele(".does-not-exist")?.text()?,
            Some("nested reconnect missing".to_string())
        );
        match roundtrip.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "roundtrip nested WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("roundtrip nested WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(roundtrip)
    })();

    let reconnected = match result {
        Ok(frame) => frame,
        Err(err) => {
            let _ = page.quit();
            let _ = fs::remove_dir_all(&temp_dir);
            panic!("nested webframe reconnect runtime config failed before cleanup: {err}");
        }
    };

    let close_result = reconnected.owner().quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage after nested frame reconnect: {err}");
    }
}

#[test]
fn webelement_new_tab_click_helpers_return_webpage_references() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webelement-click-new-tab-wrappers", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                const newTabUrl = 'about:blank#webelement-new-tab';
                const middleUrl = 'about:blank#webelement-middle-tab';
                document.body.innerHTML = `
                    <a id="open-tab" href="${newTabUrl}" target="_blank">Open tab</a>
                    <iframe id="demo-frame"
                        srcdoc="<html><body>
                            <a id='middle-open-tab' href='${middleUrl}'>Open by middle click</a>
                        </body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        let new_tab = page
            .find("css:#open-tab")?
            .clicker()
            .for_new_tab(Some(5_000), false)?
            .expect("webelement clicker for_new_tab should return a tab");
        match new_tab {
            BrowserTabReference::WebPage(new_page) => {
                assert_eq!(new_page.mode()?, WebMode::Driver);
                assert!(new_page.wait_for_doc_loaded(5_000)?);
                assert_eq!(
                    new_page.url()?,
                    Some("about:blank#webelement-new-tab".to_string())
                );
            }
            BrowserTabReference::Page(new_page) => {
                panic!(
                    "webelement clicker for_new_tab should return webpage, got page {}",
                    new_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webelement clicker for_new_tab should return webpage, got id {id}");
            }
        }

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        let middle_tab = frame
            .find("css:#middle-open-tab")?
            .clicker()
            .middle(true)?
            .expect("webelement clicker middle should return a tab");
        match middle_tab {
            BrowserTabReference::WebPage(middle_page) => {
                assert_eq!(middle_page.mode()?, WebMode::Driver);
                assert!(middle_page.wait_for_doc_loaded(5_000)?);
                assert_eq!(
                    middle_page.url()?,
                    Some("about:blank#webelement-middle-tab".to_string())
                );
            }
            BrowserTabReference::Page(middle_page) => {
                panic!(
                    "webelement clicker middle should return webpage, got page {}",
                    middle_page.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("webelement clicker middle should return webpage, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webelement new-tab click wrapper regression");
}

#[test]
fn page_and_frame_find_signatures_accept_by_tuples() {
    fn assert_calls(page: &Page, frame: &Frame) {
        let _ = page.find((By::ID, "root"));
        let _ = page.find_all((By::CLASS_NAME, "item"));
        let _ = frame.find((By::ID, "root"));
        let _ = frame.find_all((By::CLASS_NAME, "item"));
    }

    let _ = assert_calls as fn(&Page, &Frame);
}

#[test]
fn snapshot_find_signatures_accept_by_tuples() {
    fn assert_calls(
        page: &Page,
        frame: &Frame,
        element: &Element,
        shadow_root: &ShadowRoot,
        web_page: &WebPage,
        web_frame: &WebFrame,
        web_element: &WebElement,
    ) {
        let _ = page.snapshot_find((By::ID, "root"));
        let _ = page.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = frame.snapshot_find((By::ID, "root"));
        let _ = frame.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = element.snapshot_find((By::ID, "root"));
        let _ = element.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = shadow_root.snapshot_find((By::ID, "root"));
        let _ = shadow_root.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = web_page.snapshot_find((By::ID, "root"));
        let _ = web_page.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = web_frame.snapshot_find((By::ID, "root"));
        let _ = web_frame.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = web_element.snapshot_find((By::ID, "root"));
        let _ = web_element.snapshot_find_all((By::CLASS_NAME, "item"));
    }

    let _ =
        assert_calls as fn(&Page, &Frame, &Element, &ShadowRoot, &WebPage, &WebFrame, &WebElement);
}

#[test]
fn page_frame_element_and_web_wrappers_expose_static_find_aliases() {
    fn assert_calls(
        page: &Page,
        frame: &Frame,
        element: &Element,
        web_page: &WebPage,
        web_frame: &WebFrame,
        web_element: &WebElement,
        session_page: &Session,
        session_element: &SessionElement,
    ) {
        let _ = page.s_ele("#root");
        let _ = page.s_ele((By::ID, "root"));
        let _ = page.s_eles(".item");
        let _ = page.s_eles((By::CLASS_NAME, "item"));
        let _ = frame.s_ele("#root");
        let _ = frame.s_ele((By::ID, "root"));
        let _ = frame.s_eles(".item");
        let _ = frame.s_eles((By::CLASS_NAME, "item"));
        let _ = element.s_ele("#root");
        let _ = element.s_ele((By::ID, "root"));
        let _ = element.s_eles(".item");
        let _ = element.s_eles((By::CLASS_NAME, "item"));
        let _ = web_page.s_ele("#root");
        let _ = web_page.s_ele((By::ID, "root"));
        let _ = web_page.s_eles(".item");
        let _ = web_page.s_eles((By::CLASS_NAME, "item"));
        let _ = web_frame.s_ele("#root");
        let _ = web_frame.s_ele((By::ID, "root"));
        let _ = web_frame.s_eles(".item");
        let _ = web_frame.s_eles((By::CLASS_NAME, "item"));
        let _ = web_element.s_ele("#root");
        let _ = web_element.s_ele((By::ID, "root"));
        let _ = web_element.s_eles(".item");
        let _ = web_element.s_eles((By::CLASS_NAME, "item"));
        let _ = session_page.s_ele("#root");
        let _ = session_page.s_ele((By::ID, "root"));
        let _ = session_page.s_eles(".item");
        let _ = session_page.s_eles((By::CLASS_NAME, "item"));
        let _ = session_element.s_ele("#root");
        let _ = session_element.s_ele((By::ID, "root"));
        let _ = session_element.s_eles(".item");
        let _ = session_element.s_eles((By::CLASS_NAME, "item"));
    }

    let _ = assert_calls
        as fn(&Page, &Frame, &Element, &WebPage, &WebFrame, &WebElement, &Session, &SessionElement);
}

#[test]
fn webpage_and_webframe_find_signatures_accept_by_tuples() {
    fn assert_calls(page: &WebPage, frame: &WebFrame) {
        let _ = page.find((By::ID, "root"));
        let _ = page.find_all((By::CLASS_NAME, "item"));
        let _ = frame.find((By::ID, "root"));
        let _ = frame.find_all((By::CLASS_NAME, "item"));
    }

    let _ = assert_calls as fn(&WebPage, &WebFrame);
}

#[test]
fn page_frame_element_and_web_wrappers_find_locators_accept_locator_inputs() {
    fn assert_calls(
        page: &Page,
        frame: &Frame,
        element: &Element,
        web_page: &WebPage,
        web_frame: &WebFrame,
        web_element: &WebElement,
    ) {
        let locators = vec!["#root".to_string(), ".item".to_string()];
        let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
        let mixed_locators = [
            LocatorInput::from("#root"),
            LocatorInput::from((By::CLASS_NAME, "item")),
        ];

        let _ = page.find_locators((By::ID, "root"), true, true);
        let _ = page.find_locators(&locators, false, false);
        let _ = page.find_locators(&tuple_locators, false, false);
        let _ = page.find_locators(&mixed_locators, false, false);
        let _ = frame.find_locators((By::ID, "root"), true, true);
        let _ = frame.find_locators(&locators, false, false);
        let _ = frame.find_locators(&tuple_locators, false, false);
        let _ = frame.find_locators(&mixed_locators, false, false);
        let _ = element.find_locators((By::CLASS_NAME, "item"), true, true);
        let _ = element.find_locators(&locators, false, false);
        let _ = element.find_locators(&tuple_locators, false, false);
        let _ = element.find_locators(&mixed_locators, false, false);
        let _ = web_page.find_locators((By::ID, "root"), true, true);
        let _ = web_page.find_locators(&locators, false, false);
        let _ = web_page.find_locators(&tuple_locators, false, false);
        let _ = web_page.find_locators(&mixed_locators, false, false);
        let _ = web_frame.find_locators((By::ID, "root"), true, true);
        let _ = web_frame.find_locators(&locators, false, false);
        let _ = web_frame.find_locators(&tuple_locators, false, false);
        let _ = web_frame.find_locators(&mixed_locators, false, false);
        let _ = web_element.find_locators((By::CLASS_NAME, "item"), true, true);
        let _ = web_element.find_locators(&locators, false, false);
        let _ = web_element.find_locators(&tuple_locators, false, false);
        let _ = web_element.find_locators(&mixed_locators, false, false);
    }

    let _ = assert_calls as fn(&Page, &Frame, &Element, &WebPage, &WebFrame, &WebElement);
}

#[test]
fn elements_one_find_locators_signatures_accept_locator_inputs() {
    fn assert_calls(
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        one_session_element: ElementsOne<'_, SessionElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
        owned_session_element: &ElementsOneOwned<SessionElement>,
    ) {
        let locators = vec!["#root".to_string(), ".item".to_string()];
        let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
        let mixed_locators = [
            LocatorInput::from("#root"),
            LocatorInput::from((By::CLASS_NAME, "item")),
        ];

        let _ = one_element.find_locators((By::ID, "root"), true, true);
        let _ = one_element.find_locators(&locators, false, false);
        let _ = one_element.find_locators(&tuple_locators, false, false);
        let _ = one_element.find_locators(&mixed_locators, false, false);
        let _ = one_web_element.find_locators((By::ID, "root"), true, true);
        let _ = one_web_element.find_locators(&locators, false, false);
        let _ = one_web_element.find_locators(&tuple_locators, false, false);
        let _ = one_web_element.find_locators(&mixed_locators, false, false);
        let _ = one_session_element.find_locators((By::ID, "root"), true, true);
        let _ = one_session_element.find_locators(&locators, false, false);
        let _ = one_session_element.find_locators(&tuple_locators, false, false);
        let _ = one_session_element.find_locators(&mixed_locators, false, false);
        let _ = owned_element.find_locators((By::ID, "root"), true, true);
        let _ = owned_element.find_locators(&locators, false, false);
        let _ = owned_element.find_locators(&tuple_locators, false, false);
        let _ = owned_element.find_locators(&mixed_locators, false, false);
        let _ = owned_web_element.find_locators((By::ID, "root"), true, true);
        let _ = owned_web_element.find_locators(&locators, false, false);
        let _ = owned_web_element.find_locators(&tuple_locators, false, false);
        let _ = owned_web_element.find_locators(&mixed_locators, false, false);
        let _ = owned_session_element.find_locators((By::ID, "root"), true, true);
        let _ = owned_session_element.find_locators(&locators, false, false);
        let _ = owned_session_element.find_locators(&tuple_locators, false, false);
        let _ = owned_session_element.find_locators(&mixed_locators, false, false);
    }

    let _ = assert_calls
        as fn(
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            ElementsOne<'_, SessionElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
            &ElementsOneOwned<SessionElement>,
        );
}

#[test]
fn page_and_webpage_frame_lookup_signatures_accept_by_tuples_and_object_refs() {
    fn assert_calls(
        page: &Page,
        frame: &Frame,
        element: &Element,
        web_page: &WebPage,
        web_frame: &WebFrame,
        web_element: &WebElement,
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
    ) {
        let _cloned_frame: Frame = frame.clone();
        let _cloned_web_frame: WebFrame = web_frame.clone();

        let _ = page.get_frame((By::ID, "theFrame"));
        let _ = page.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = page.get_frame_ele((By::ID, "theFrame"));
        let _ = page.get_frame_ele_with_timeout((By::ID, "theFrame"), 10);
        let _ = page.get_frame(1usize);
        let _ = page.get_frame_by_index(-1isize);
        let _ = page.get_frame_by_index_with_timeout(1usize, 10);
        let _ = page.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = page.get_frame_ele(1usize);
        let _ = page.get_frame_ele_by_index(-1isize);
        let _ = page.get_frame_ele_by_index_with_timeout(1usize, 10);
        let _ = page.get_frame_ele_by_index_with_timeout(-1isize, 10);
        let _ = page.get_frame(-1isize);
        let _ = page.get_frame_ele(-1isize);
        let _ = page.get_frame(element);
        let _ = page.get_frame_ele(element);
        let _ = page.get_frame(frame);
        let _ = page.get_frame(frame.clone());
        let _ = page.get_frame_ele(frame);
        let _ = page.get_frame(web_frame.clone());
        let _ = page.get_frames(Some((By::TAG_NAME, "iframe")));
        let _ = page.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = page.get_frame_eles(Some((By::TAG_NAME, "iframe")));
        let _ = page.get_frame_eles_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = page.get_frame_context((By::ID, "theFrame"));
        let _ = page.get_frame_context(1usize);
        let _ = page.get_frame_context_by_index(-1isize);
        let _ = page.get_frame_context(-1isize);
        let _ = page.get_frame_context(element);
        let _ = page.get_frame_context(frame);
        let _ = page.get_frame_context(frame.clone());
        let _ = page.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
        let _ = frame.get_frame((By::ID, "childFrame"));
        let _ = frame.get_frame_with_timeout((By::ID, "childFrame"), 10);
        let _ = frame.get_frame_ele((By::ID, "childFrame"));
        let _ = frame.get_frame_ele_with_timeout((By::ID, "childFrame"), 10);
        let _ = frame.get_frame(1usize);
        let _ = frame.get_frame_by_index(-1isize);
        let _ = frame.get_frame_by_index_with_timeout(1usize, 10);
        let _ = frame.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = frame.get_frame_ele(1usize);
        let _ = frame.get_frame_ele_by_index(-1isize);
        let _ = frame.get_frame_ele_by_index_with_timeout(1usize, 10);
        let _ = frame.get_frame_ele_by_index_with_timeout(-1isize, 10);
        let _ = frame.get_frames(Some((By::TAG_NAME, "iframe")));
        let _ = frame.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = frame.get_frame_eles(Some((By::TAG_NAME, "iframe")));
        let _ = frame.get_frame_eles_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = frame.get_frame_context((By::ID, "childFrame"));
        let _ = frame.get_frame_context(1usize);
        let _ = frame.get_frame_context_by_index(-1isize);
        let _ = frame.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
        let _ = element.get_frame((By::ID, "theFrame"));
        let _ = element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = element.get_frame(1usize);
        let _ = element.get_frame_by_index(-1isize);
        let _ = element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = element.get_frame(frame);
        let _ = element.get_frame(frame.clone());
        let _ = element.get_frame(web_frame.clone());
        let _ = one_element.get_frame((By::ID, "theFrame"));
        let _ = one_element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = one_element.get_frame(1usize);
        let _ = one_element.get_frame_by_index(1usize);
        let _ = one_element.get_frame_by_index(-1isize);
        let _ = one_element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = one_element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = one_element.get_frame(frame);
        let _ = owned_element.get_frame((By::ID, "theFrame"));
        let _ = owned_element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = owned_element.get_frame(1usize);
        let _ = owned_element.get_frame_by_index(1usize);
        let _ = owned_element.get_frame_by_index(-1isize);
        let _ = owned_element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = owned_element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = owned_element.get_frame(frame);
        let _ = web_page.get_frame((By::ID, "theFrame"));
        let _ = web_page.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = web_page.get_frame_ele((By::ID, "theFrame"));
        let _ = web_page.get_frame_ele_with_timeout((By::ID, "theFrame"), 10);
        let _ = web_page.get_frame(1usize);
        let _ = web_page.get_frame_by_index(-1isize);
        let _ = web_page.get_frame_by_index_with_timeout(1usize, 10);
        let _ = web_page.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = web_page.get_frame_ele(1usize);
        let _ = web_page.get_frame_ele_by_index(-1isize);
        let _ = web_page.get_frame_ele_by_index_with_timeout(1usize, 10);
        let _ = web_page.get_frame_ele_by_index_with_timeout(-1isize, 10);
        let _ = web_page.get_frame(-1isize);
        let _ = web_page.get_frame_ele(-1isize);
        let _ = web_page.get_frame(web_element);
        let _ = web_page.get_frame_ele(web_element);
        let _ = web_page.get_frame(web_frame);
        let _ = web_page.get_frame(web_frame.clone());
        let _ = web_page.get_frame(frame.clone());
        let _ = web_page.get_frame_ele(web_frame);
        let _ = web_page.get_frames(Some((By::TAG_NAME, "iframe")));
        let _ = web_page.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = web_page.get_frame_eles(Some((By::TAG_NAME, "iframe")));
        let _ = web_page.get_frame_eles_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = web_page.get_frame_context((By::ID, "theFrame"));
        let _ = web_page.get_frame_context(1usize);
        let _ = web_page.get_frame_context_by_index(-1isize);
        let _ = web_page.get_frame_context(-1isize);
        let _ = web_page.get_frame_context(web_element);
        let _ = web_page.get_frame_context(web_frame);
        let _ = web_page.get_frame_context(web_frame.clone());
        let _ = web_page.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
        let _ = web_frame.get_frame((By::ID, "childFrame"));
        let _ = web_frame.get_frame_with_timeout((By::ID, "childFrame"), 10);
        let _ = web_frame.get_frame_ele((By::ID, "childFrame"));
        let _ = web_frame.get_frame_ele_with_timeout((By::ID, "childFrame"), 10);
        let _ = web_frame.get_frame(1usize);
        let _ = web_frame.get_frame_by_index(-1isize);
        let _ = web_frame.get_frame_by_index_with_timeout(1usize, 10);
        let _ = web_frame.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = web_frame.get_frame_ele(1usize);
        let _ = web_frame.get_frame_ele_by_index(-1isize);
        let _ = web_frame.get_frame_ele_by_index_with_timeout(1usize, 10);
        let _ = web_frame.get_frame_ele_by_index_with_timeout(-1isize, 10);
        let _ = web_frame.get_frames(Some((By::TAG_NAME, "iframe")));
        let _ = web_frame.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = web_frame.get_frame_eles(Some((By::TAG_NAME, "iframe")));
        let _ = web_frame.get_frame_eles_with_timeout(Some((By::TAG_NAME, "iframe")), 10);
        let _ = web_frame.get_frame_context((By::ID, "childFrame"));
        let _ = web_frame.get_frame_context(1usize);
        let _ = web_frame.get_frame_context_by_index(-1isize);
        let _ = web_frame.get_frame_contexts(Some((By::TAG_NAME, "iframe")));
        let _ = web_element.get_frame((By::ID, "theFrame"));
        let _ = web_element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = web_element.get_frame(1usize);
        let _ = web_element.get_frame_by_index(-1isize);
        let _ = web_element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = web_element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = web_element.get_frame(web_frame);
        let _ = web_element.get_frame(web_frame.clone());
        let _ = web_element.get_frame(frame.clone());
        let _ = one_web_element.get_frame((By::ID, "theFrame"));
        let _ = one_web_element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = one_web_element.get_frame(1usize);
        let _ = one_web_element.get_frame_by_index(1usize);
        let _ = one_web_element.get_frame_by_index(-1isize);
        let _ = one_web_element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = one_web_element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = one_web_element.get_frame(web_frame);
        let _ = owned_web_element.get_frame((By::ID, "theFrame"));
        let _ = owned_web_element.get_frame_with_timeout((By::ID, "theFrame"), 10);
        let _ = owned_web_element.get_frame(1usize);
        let _ = owned_web_element.get_frame_by_index(1usize);
        let _ = owned_web_element.get_frame_by_index(-1isize);
        let _ = owned_web_element.get_frame_by_index_with_timeout(1usize, 10);
        let _ = owned_web_element.get_frame_by_index_with_timeout(-1isize, 10);
        let _ = owned_web_element.get_frame(web_frame);
    }

    let _ = assert_calls
        as fn(
            &Page,
            &Frame,
            &Element,
            &WebPage,
            &WebFrame,
            &WebElement,
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
        );
}

#[test]
fn webpage_get_frame_returns_webframe_objects_at_runtime() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-get-frame-objects", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                document.body.innerHTML = `
                    <iframe id="demo-frame" name="demo-frame"
                        srcdoc="<html><body><button id='inside'>inside</button></body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        let frame = page.get_frame("css:#demo-frame")?;
        let frame_by_index = page.get_frame(1usize)?;
        let frame_ele = page.get_frame_ele("css:#demo-frame")?;
        let frames = page.get_frames(Some((By::TAG_NAME, "iframe")))?;
        let frame_context = page.get_frame_context("css:#demo-frame")?;

        assert_eq!(frame.attr("id")?, Some("demo-frame".to_string()));
        assert_eq!(frame_by_index.attr("name")?, Some("demo-frame".to_string()));
        assert_eq!(frame_ele.attr("id")?, Some("demo-frame".to_string()));
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].attr("id")?, Some("demo-frame".to_string()));
        assert_eq!(frame_context.attr("id")?, Some("demo-frame".to_string()));
        let frame_doc = "data:text/html,%3Chtml%3E%3Chead%3E%3Ctitle%3ENavigated%20Frame%3C/title%3E%3C/head%3E%3Cbody%3E%3Cdiv%20id%3D%27after-nav%27%3Eafter%3C/div%3E%3C/body%3E%3C/html%3E";
        assert!(frame.get(frame_doc)?);
        assert_eq!(frame.title()?, Some("Navigated Frame".to_string()));
        assert_eq!(
            frame.find("css:#after-nav")?.text()?,
            Some("after".to_string())
        );
        let reconnected_frame = frame.reconnect(0)?;
        assert_eq!(
            reconnected_frame.attr("id")?,
            Some("demo-frame".to_string())
        );
        let disconnected_frame = reconnected_frame.disconnect()?;
        let roundtrip_frame = disconnected_frame.reconnect(0)?;
        assert_eq!(roundtrip_frame.attr("id")?, Some("demo-frame".to_string()));
        frame.set_none_element_value(Some("missing"), true)?;
        assert_eq!(
            frame_context.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage get_frame runtime regression");
}

#[test]
fn webpage_frame_index_helpers_accept_negative_indexes_at_runtime() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webpage-frame-negative-index", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                document.body.innerHTML = `
                    <section id="host">
                        <iframe id="first-frame" name="first-frame"
                            srcdoc="<html><body><div>first</div></body></html>">
                        </iframe>
                        <iframe id="second-frame" name="second-frame"
                            srcdoc="<html><body><div id='nested-host'></div></body></html>">
                        </iframe>
                    </section>
                `;
                return true;
            })()"#,
        )?;

        let last_frame = page.get_frame_by_index(-1isize)?;
        let last_frame_ele = page.get_frame_ele_by_index(-1i32)?;
        let last_context = page.get_frame_context_by_index(-1i64)?;
        let host = page.find("css:#host")?;
        let host_last_frame = host.get_frame_by_index(-1isize)?;

        assert_eq!(last_frame.attr("id")?, Some("second-frame".to_string()));
        assert_eq!(last_context.attr("id")?, Some("second-frame".to_string()));
        assert_eq!(
            host_last_frame.attr("id")?,
            Some("second-frame".to_string())
        );
        match last_frame_ele {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("second-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "negative WebPage get_frame_ele_by_index should keep mix element, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("negative WebPage get_frame_ele_by_index should keep mix element");
            }
        }
        match host_last_frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "negative WebElement get_frame_by_index should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "negative WebElement get_frame_by_index should keep webpage owner, got id {id}"
                );
            }
        }

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
        match nested_last_ele {
            WebElement::Mix { element, .. } => {
                assert_eq!(element.attr("id")?, Some("nested-second".to_string()));
            }
            WebElement::Browser(element) => {
                panic!(
                    "negative WebFrame get_frame_ele_by_index should keep mix element, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("negative WebFrame get_frame_ele_by_index should keep mix element");
            }
        }
        assert_eq!(
            nested_last.find("css:#inside")?.text()?,
            Some("inside".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webpage frame negative index regression");
}

#[test]
fn nested_webframe_initial_runtime_config_inherits_parent_webframe_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("nested-webframe-config-inherit", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("webframe-default"), true)?;
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

        let inner = outer.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            inner.ele(".does-not-exist")?.text()?,
            Some("webframe-default".to_string())
        );
        match inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "nested WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("nested WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("nested webframe runtime-state inheritance regression");
}

#[test]
fn singleton_tab_obj_reuses_nested_webframe_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("nested-webframe-singleton-enabled", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
        inner.set_none_element_value(Some("nested web missing"), true)?;

        let inner_by_index_timeout = outer.get_frame_by_index_with_timeout(1, 500)?;
        assert_eq!(inner_by_index_timeout.id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            inner_by_index_timeout.frame_element()
        ));
        let nested_frames = outer.get_frames(Some((By::TAG_NAME, "iframe")))?;
        assert_eq!(nested_frames.len(), 1);
        assert_eq!(nested_frames[0].id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            nested_frames[0].frame_element()
        ));
        let nested_frames_timeout =
            outer.get_frames_with_timeout(Some((By::TAG_NAME, "iframe")), 500)?;
        assert_eq!(nested_frames_timeout.len(), 1);
        assert_eq!(nested_frames_timeout[0].id(), inner.id());
        assert!(std::ptr::eq(
            inner.frame_element(),
            nested_frames_timeout[0].frame_element()
        ));
        assert_eq!(
            nested_frames_timeout[0].ele(".does-not-exist")?.text()?,
            Some("nested web missing".to_string())
        );
        match nested_frames_timeout[0].owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "nested singleton WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("nested singleton WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("nested singleton webframe runtime-state regression");
}

#[test]
fn singleton_tab_obj_reuses_nested_frame_cache_across_page_and_webpage_wrappers() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("nested-webframe-cross-wrapper-singleton", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let driver_outer = page.driver.get_frame("css:#outer-frame")?;
        assert!(driver_outer.wait_for_doc_loaded(2_000)?);
        let mix_outer = page.get_frame("css:#outer-frame")?;
        assert_eq!(mix_outer.id(), driver_outer.id());
        assert!(std::ptr::eq(
            driver_outer.frame_element(),
            mix_outer.frame_element()
        ));
        match mix_outer.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "cross-wrapper nested outer WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "cross-wrapper nested outer WebFrame should keep webpage owner, got id {id}"
                );
            }
        }

        driver_outer.run_js(
            r#"(() => {
                const frame = document.createElement('iframe');
                frame.id = 'inner-frame';
                frame.name = 'inner-frame';
                frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                document.getElementById('outer-host').appendChild(frame);
                return true;
            })()"#,
        )?;

        let driver_inner = driver_outer.get_frame("css:#inner-frame")?;
        assert!(driver_inner.wait_for_doc_loaded(2_000)?);
        driver_inner.set_none_element_value(Some("driver nested missing"), true)?;

        let mix_inner = mix_outer.get_frame("css:#inner-frame")?;
        assert_eq!(mix_inner.id(), driver_inner.id());
        assert!(std::ptr::eq(
            driver_inner.frame_element(),
            mix_inner.frame_element()
        ));
        assert_eq!(
            mix_inner.ele(".does-not-exist")?.text()?,
            Some("driver nested missing".to_string())
        );
        match mix_inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "cross-wrapper nested inner WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "cross-wrapper nested inner WebFrame should keep webpage owner, got id {id}"
                );
            }
        }
        match mix_inner.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("inner-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "cross-wrapper nested frame element should stay mix, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("cross-wrapper nested frame element should stay mix");
            }
        }

        mix_inner.set_none_element_value(Some("mix nested missing"), true)?;
        let driver_inner_again = driver_outer.get_frame("css:#inner-frame")?;
        assert_eq!(driver_inner_again.id(), driver_inner.id());
        assert!(std::ptr::eq(
            driver_inner.frame_element(),
            driver_inner_again.frame_element()
        ));
        assert_eq!(
            driver_inner_again.ele(".does-not-exist")?.text()?,
            Some("mix nested missing".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("cross-wrapper nested singleton frame cache regression");
}

#[test]
fn web_element_frame_initial_runtime_config_inherits_parent_webframe_setting() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("web-element-frame-config-inherit", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("web-element-default"), true)?;
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
            Some("web-element-default".to_string())
        );
        match inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "nested WebElement frame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("nested WebElement frame should keep webpage owner, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("web element frame runtime-state inheritance regression");
}

#[test]
fn webframe_object_target_preserves_mix_owner_and_runtime_config() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-object-target-preserves-mix", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let inner = outer.get_frame("css:#inner-frame")?;
        assert!(inner.wait_for_doc_loaded(5_000)?);
        inner.set_none_element_value(Some("object-target-missing"), true)?;
        let inner_frame = inner.frame().clone();
        let host = outer.find("css:#outer-host")?;
        let assert_webframe_inner_target =
            |frame: &WebFrame, label: &str| -> crate::OpenPageResult<()> {
                assert_eq!(frame.id(), inner.id());
                assert_eq!(
                    frame.ele(".does-not-exist")?.text()?,
                    Some("object-target-missing".to_string())
                );
                match frame.owner_reference() {
                    BrowserTabReference::WebPage(owner) => {
                        assert_eq!(owner.target_id(), page.target_id());
                    }
                    BrowserTabReference::Page(owner) => {
                        panic!(
                            "{label} should keep webpage owner, got page {}",
                            owner.target_id()
                        );
                    }
                    BrowserTabReference::Id(id) => {
                        panic!("{label} should keep webpage owner, got id {id}");
                    }
                }
                Ok(())
            };

        let driver_target = page.driver.get_frame(&inner)?;
        assert_eq!(driver_target.id(), inner.id());
        assert_eq!(
            driver_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        let driver_owned_target = page.driver.get_frame(inner.clone())?;
        assert_eq!(driver_owned_target.id(), inner.id());
        assert_eq!(
            driver_owned_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );

        let page_target = page.get_frame(&inner)?;
        assert_eq!(page_target.id(), inner.id());
        assert_eq!(
            page_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match page_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebPage get_frame(&WebFrame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebPage get_frame(&WebFrame) should keep webpage owner, got id {id}");
            }
        }
        let page_owned_target = page.get_frame(inner.clone())?;
        assert_eq!(page_owned_target.id(), inner.id());
        assert_eq!(
            page_owned_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match page_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebPage get_frame(WebFrame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebPage get_frame(WebFrame) should keep webpage owner, got id {id}");
            }
        }
        let page_frame_target = page.get_frame(&inner_frame)?;
        assert_eq!(page_frame_target.id(), inner.id());
        assert_eq!(
            page_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match page_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebPage get_frame(&Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebPage get_frame(&Frame) should keep webpage owner, got id {id}");
            }
        }
        let page_owned_frame_target = page.get_frame(inner_frame.clone())?;
        assert_eq!(page_owned_frame_target.id(), inner.id());
        assert_eq!(
            page_owned_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match page_owned_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebPage get_frame(Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebPage get_frame(Frame) should keep webpage owner, got id {id}");
            }
        }

        let frame_target = outer.get_frame(&inner)?;
        assert_eq!(frame_target.id(), inner.id());
        assert_eq!(
            frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebFrame get_frame(&WebFrame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebFrame get_frame(&WebFrame) should keep webpage owner, got id {id}");
            }
        }
        let frame_owned_target = outer.get_frame(inner.clone())?;
        assert_eq!(frame_owned_target.id(), inner.id());
        assert_eq!(
            frame_owned_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match frame_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebFrame get_frame(WebFrame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebFrame get_frame(WebFrame) should keep webpage owner, got id {id}");
            }
        }
        let frame_frame_target = outer.get_frame(&inner_frame)?;
        assert_eq!(frame_frame_target.id(), inner.id());
        assert_eq!(
            frame_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match frame_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebFrame get_frame(&Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebFrame get_frame(&Frame) should keep webpage owner, got id {id}");
            }
        }
        let frame_owned_frame_target = outer.get_frame(inner_frame.clone())?;
        assert_eq!(frame_owned_frame_target.id(), inner.id());
        assert_eq!(
            frame_owned_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match frame_owned_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebFrame get_frame(Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebFrame get_frame(Frame) should keep webpage owner, got id {id}");
            }
        }
        let element_frame_target = host.get_frame(&inner_frame)?;
        assert_eq!(element_frame_target.id(), inner.id());
        assert_eq!(
            element_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match element_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebElement get_frame(&Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebElement get_frame(&Frame) should keep webpage owner, got id {id}");
            }
        }
        let element_owned_frame_target = host.get_frame(inner_frame.clone())?;
        assert_eq!(element_owned_frame_target.id(), inner.id());
        assert_eq!(
            element_owned_frame_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );
        match element_owned_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "WebElement get_frame(Frame) should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("WebElement get_frame(Frame) should keep webpage owner, got id {id}");
            }
        }

        let driver_frame_timeout_target = page.driver.get_frame_with_timeout(&inner_frame, 10)?;
        assert_eq!(driver_frame_timeout_target.id(), inner.id());
        assert_eq!(
            driver_frame_timeout_target.ele(".does-not-exist")?.text()?,
            Some("object-target-missing".to_string())
        );

        let page_timeout_target = page.get_frame_with_timeout(&inner, 10)?;
        assert_webframe_inner_target(&page_timeout_target, "WebPage timeout WebFrame target")?;

        let frame_timeout_target = outer.get_frame_with_timeout(inner_frame.clone(), 10)?;
        assert_webframe_inner_target(&frame_timeout_target, "WebFrame timeout Frame target")?;

        let element_timeout_target = host.get_frame_with_timeout(&inner_frame, 10)?;
        assert_webframe_inner_target(&element_timeout_target, "WebElement timeout Frame target")?;
        let page_context_target = page.get_frame_context(&inner)?;
        assert_webframe_inner_target(&page_context_target, "WebPage context WebFrame target")?;
        let frame_context_target = outer.get_frame_context(inner_frame.clone())?;
        assert_webframe_inner_target(&frame_context_target, "WebFrame context Frame target")?;

        assert_eq!(
            page.driver
                .get_frame_ele_with_timeout(&inner_frame, 10)?
                .attr("id")?,
            Some("inner-frame".to_string())
        );
        assert_eq!(
            page.get_frame_ele_with_timeout(&inner, 10)?.attr("id")?,
            Some("inner-frame".to_string())
        );
        assert_eq!(
            outer
                .get_frame_ele_with_timeout(inner_frame.clone(), 10)?
                .attr("name")?,
            Some("inner-frame".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("webframe object target owner/runtime regression");
}

#[test]
fn elements_one_webframe_initial_runtime_config_keeps_mix_owner() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (page, temp_dir) =
        launch_headless_test_webpage("elements-one-webframe-config-inherit", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
        assert!(outer.wait_for_doc_loaded(5_000)?);
        outer.set_none_element_value(Some("elements-one-web-default"), true)?;
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
            .expect("owned WebElement ElementsOne should find inner frame");
        assert!(owned_inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            owned_inner.ele(".does-not-exist")?.text()?,
            Some("elements-one-web-default".to_string())
        );
        match owned_inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "owned ElementsOne WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("owned ElementsOne WebFrame should keep webpage owner, got id {id}");
            }
        }
        let hosts = outer.find_all("css:#outer-host")?;
        let borrowed_inner = hosts
            .filter_one()
            .get_frame("css:#inner-frame")?
            .expect("borrowed WebElement ElementsOne should find inner frame");
        assert!(borrowed_inner.wait_for_doc_loaded(5_000)?);
        assert_eq!(
            borrowed_inner.ele(".does-not-exist")?.text()?,
            Some("elements-one-web-default".to_string())
        );
        match borrowed_inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "borrowed ElementsOne WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("borrowed ElementsOne WebFrame should keep webpage owner, got id {id}");
            }
        }

        owned_inner.set_none_element_value(Some("elements-one-target-default"), true)?;
        let inner_frame = owned_inner.frame().clone();
        let assert_owned_inner_target =
            |frame: &WebFrame, label: &str| -> crate::OpenPageResult<()> {
                assert_eq!(frame.id(), owned_inner.id());
                assert_eq!(
                    frame.ele(".does-not-exist")?.text()?,
                    Some("elements-one-target-default".to_string())
                );
                match frame.owner_reference() {
                    BrowserTabReference::WebPage(owner) => {
                        assert_eq!(owner.target_id(), page.target_id());
                    }
                    BrowserTabReference::Page(owner) => {
                        panic!(
                            "{label} should keep webpage owner, got page {}",
                            owner.target_id()
                        );
                    }
                    BrowserTabReference::Id(id) => {
                        panic!("{label} should keep webpage owner, got id {id}");
                    }
                }
                Ok(())
            };

        let owned_frame_target = owned_host
            .get_frame(&inner_frame)?
            .expect("owned ElementsOne should accept borrowed Frame target");
        assert_eq!(owned_frame_target.id(), owned_inner.id());
        assert_eq!(
            owned_frame_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match owned_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "owned ElementsOne Frame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("owned ElementsOne Frame target should keep webpage owner, got id {id}");
            }
        }

        let owned_frame_owned_target = owned_host
            .get_frame(inner_frame.clone())?
            .expect("owned ElementsOne should accept owned Frame target");
        assert_eq!(owned_frame_owned_target.id(), owned_inner.id());
        assert_eq!(
            owned_frame_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match owned_frame_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "owned ElementsOne owned Frame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "owned ElementsOne owned Frame target should keep webpage owner, got id {id}"
                );
            }
        }
        let borrowed_frame_target = hosts
            .filter_one()
            .get_frame(&inner_frame)?
            .expect("borrowed ElementsOne should accept borrowed Frame target");
        assert_eq!(borrowed_frame_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_frame_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match borrowed_frame_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "borrowed ElementsOne Frame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("borrowed ElementsOne Frame target should keep webpage owner, got id {id}");
            }
        }

        let borrowed_frame_owned_target = hosts
            .filter_one()
            .get_frame(inner_frame.clone())?
            .expect("borrowed ElementsOne should accept owned Frame target");
        assert_eq!(borrowed_frame_owned_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_frame_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match borrowed_frame_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "borrowed ElementsOne owned Frame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "borrowed ElementsOne owned Frame target should keep webpage owner, got id {id}"
                );
            }
        }

        let owned_webframe_target = owned_host
            .get_frame(&owned_inner)?
            .expect("owned ElementsOne should accept borrowed WebFrame target");
        assert_eq!(owned_webframe_target.id(), owned_inner.id());
        assert_eq!(
            owned_webframe_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match owned_webframe_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "owned ElementsOne WebFrame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("owned ElementsOne WebFrame target should keep webpage owner, got id {id}");
            }
        }

        let owned_webframe_owned_target = owned_host
            .get_frame(owned_inner.clone())?
            .expect("owned ElementsOne should accept owned WebFrame target");
        assert_eq!(owned_webframe_owned_target.id(), owned_inner.id());
        assert_eq!(
            owned_webframe_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match owned_webframe_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "owned ElementsOne owned WebFrame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "owned ElementsOne owned WebFrame target should keep webpage owner, got id {id}"
                );
            }
        }

        let borrowed_webframe_target = hosts
            .filter_one()
            .get_frame(&owned_inner)?
            .expect("borrowed ElementsOne should accept borrowed WebFrame target");
        assert_eq!(borrowed_webframe_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_webframe_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-target-default".to_string())
        );
        match borrowed_webframe_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "borrowed ElementsOne WebFrame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "borrowed ElementsOne WebFrame target should keep webpage owner, got id {id}"
                );
            }
        }

        let borrowed_webframe_owned_target = hosts
            .filter_one()
            .get_frame(owned_inner.clone())?
            .expect("borrowed ElementsOne should accept owned WebFrame target");
        assert_eq!(borrowed_webframe_owned_target.id(), owned_inner.id());
        assert_eq!(
            borrowed_webframe_owned_target
                .ele(".does-not-exist")?
                .text()?,
            Some("elements-one-target-default".to_string())
        );
        match borrowed_webframe_owned_target.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "borrowed ElementsOne owned WebFrame target should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "borrowed ElementsOne owned WebFrame target should keep webpage owner, got id {id}"
                );
            }
        }

        let owned_frame_timeout_target = owned_host
            .get_frame_with_timeout(&inner_frame, 10)?
            .expect("owned ElementsOne timeout should accept borrowed Frame target");
        assert_owned_inner_target(
            &owned_frame_timeout_target,
            "owned ElementsOne timeout Frame target",
        )?;

        let owned_webframe_timeout_target = owned_host
            .get_frame_with_timeout(owned_inner.clone(), 10)?
            .expect("owned ElementsOne timeout should accept owned WebFrame target");
        assert_owned_inner_target(
            &owned_webframe_timeout_target,
            "owned ElementsOne timeout WebFrame target",
        )?;

        let borrowed_frame_timeout_target = hosts
            .filter_one()
            .get_frame_with_timeout(inner_frame.clone(), 10)?
            .expect("borrowed ElementsOne timeout should accept owned Frame target");
        assert_owned_inner_target(
            &borrowed_frame_timeout_target,
            "borrowed ElementsOne timeout Frame target",
        )?;

        let borrowed_webframe_timeout_target = hosts
            .filter_one()
            .get_frame_with_timeout(&owned_inner, 10)?
            .expect("borrowed ElementsOne timeout should accept borrowed WebFrame target");
        assert_owned_inner_target(
            &borrowed_webframe_timeout_target,
            "borrowed ElementsOne timeout WebFrame target",
        )?;
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("elements one webframe runtime-state inheritance regression");
}

#[test]
fn singleton_tab_obj_reuses_webframe_state_when_enabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-singleton-enabled", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("missing"), true)?;

        let same_frame = page.get_frame("css:#demo-frame")?;
        assert_eq!(same_frame.id(), frame.id());
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
        let host = page.find("css:body")?;
        let frame_from_element = host.get_frame("css:#demo-frame")?;
        assert_eq!(frame_from_element.id(), frame.id());
        assert!(std::ptr::eq(
            frame.frame_element(),
            frame_from_element.frame_element()
        ));
        assert_eq!(
            same_frame.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        match same_frame.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            WebElement::Browser(element) => {
                panic!(
                    "singleton mix WebFrame should keep mix frame element, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("singleton mix WebFrame should keep mix frame element");
            }
        }
        match same_frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "singleton mix WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("singleton mix WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("singleton webframe runtime-state regression");
}

#[test]
fn singleton_tab_obj_reuses_frame_cache_across_page_and_webpage_wrappers() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-cross-wrapper-singleton", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let driver_frame = page.driver.get_frame("css:#demo-frame")?;
        assert!(driver_frame.wait_for_doc_loaded(5_000)?);
        driver_frame.set_none_element_value(Some("driver missing"), true)?;

        let mix_frame = page.get_frame("css:#demo-frame")?;
        assert_eq!(mix_frame.id(), driver_frame.id());
        assert!(std::ptr::eq(
            driver_frame.frame_element(),
            mix_frame.frame_element()
        ));
        assert_eq!(
            mix_frame.ele(".does-not-exist")?.text()?,
            Some("driver missing".to_string())
        );
        match mix_frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
                assert_eq!(owner.mode()?, WebMode::Driver);
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "cross-wrapper singleton WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("cross-wrapper singleton WebFrame should keep webpage owner, got id {id}");
            }
        }
        match mix_frame.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "cross-wrapper singleton frame element should stay mix, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("cross-wrapper singleton frame element should stay mix");
            }
        }

        mix_frame.set_none_element_value(Some("mix missing"), true)?;
        let driver_again = page.driver.get_frame("css:#demo-frame")?;
        assert_eq!(driver_again.id(), driver_frame.id());
        assert!(std::ptr::eq(
            driver_frame.frame_element(),
            driver_again.frame_element()
        ));
        assert_eq!(
            driver_again.ele(".does-not-exist")?.text()?,
            Some("mix missing".to_string())
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("cross-wrapper singleton frame cache regression");
}

#[test]
fn singleton_tab_obj_drops_stale_webframe_after_recreation() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-recreated-singleton", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
        assert!(page.wait_for_doc_loaded(5_000)?);
        page.run_js(
            r#"(() => {
                document.body.innerHTML = `
                    <iframe id="demo-frame"
                        srcdoc="<html><body><button id='inside'>first</button></body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        page.set_none_element_value(Some("page missing"), true)?;
        let first = page.get_frame("css:#demo-frame")?;
        assert!(first.wait_for_doc_loaded(2_000)?);
        first.set_none_element_value(Some("first missing"), true)?;

        page.run_js(
            r#"(() => {
                document.getElementById('demo-frame').remove();
                document.body.innerHTML = `
                    <iframe id="demo-frame"
                        srcdoc="<html><body><button id='inside'>second</button></body></html>">
                    </iframe>
                `;
                return true;
            })()"#,
        )?;

        let second = page.get_frame("css:#demo-frame")?;
        assert!(second.wait_for_doc_loaded(2_000)?);
        assert_eq!(
            second.find("css:#inside")?.text()?,
            Some("second".to_string())
        );
        assert_ne!(second.id(), first.id());
        assert_eq!(
            second.ele(".does-not-exist")?.text()?,
            Some("page missing".to_string())
        );
        assert!(!first.is_alive()?);
        match second.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "recreated WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!("recreated WebFrame should keep webpage owner, got id {id}");
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("stale singleton webframe cache regression");
}

#[test]
fn singleton_tab_obj_prunes_nested_webframe_after_parent_frame_navigation() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(true);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-parent-navigation-singleton", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
        match second_inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "nested WebFrame after parent navigation should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "nested WebFrame after parent navigation should keep webpage owner, got id {id}"
                );
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("parent webframe navigation cache prune regression");
}

#[test]
fn singleton_tab_obj_returns_fresh_webframe_state_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-singleton-disabled", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let frame = page.get_frame("css:#demo-frame")?;
        assert!(frame.wait_for_doc_loaded(5_000)?);
        frame.set_none_element_value(Some("missing"), true)?;

        let same_handle = page.get_frame(&frame)?;
        assert_eq!(
            same_handle.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );
        let host = page.find("css:body")?;
        let same_handle_from_element = host.get_frame(&frame)?;
        assert_eq!(
            same_handle_from_element.ele(".does-not-exist")?.text()?,
            Some("missing".to_string())
        );

        let fresh_frame = page.get_frame("css:#demo-frame")?;
        assert_eq!(fresh_frame.ele(".does-not-exist")?.text()?, None);
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("non-singleton webframe runtime-state regression");
}

#[test]
fn singleton_tab_obj_keeps_elements_one_webframe_state_isolated_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (page, temp_dir) =
        launch_headless_test_webpage("elements-one-webframe-singleton-disabled", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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
            .expect("owned WebElement ElementsOne should find inner frame");
        assert!(owned_inner.wait_for_doc_loaded(5_000)?);
        owned_inner.set_none_element_value(Some("elements-one-web-target"), true)?;

        let same_owned_target = owned_host
            .get_frame(&owned_inner)?
            .expect("owned ElementsOne should accept borrowed WebFrame target");
        assert_eq!(
            same_owned_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-web-target".to_string())
        );

        let hosts = outer.find_all("css:#outer-host")?;
        let same_borrowed_target = hosts
            .filter_one()
            .get_frame(owned_inner.clone())?
            .expect("borrowed ElementsOne should accept owned WebFrame target");
        assert_eq!(
            same_borrowed_target.ele(".does-not-exist")?.text()?,
            Some("elements-one-web-target".to_string())
        );

        let fresh_owned_locator = owned_host
            .get_frame("css:#inner-frame")?
            .expect("owned ElementsOne should re-find inner WebFrame");
        assert_eq!(fresh_owned_locator.id(), owned_inner.id());
        assert_eq!(fresh_owned_locator.ele(".does-not-exist")?.text()?, None);
        match fresh_owned_locator.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "non-singleton owned ElementsOne locator WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "non-singleton owned ElementsOne locator WebFrame should keep webpage owner, got id {id}"
                );
            }
        }

        let fresh_borrowed_locator = hosts
            .filter_one()
            .get_frame("css:#inner-frame")?
            .expect("borrowed ElementsOne should re-find inner WebFrame");
        assert_eq!(fresh_borrowed_locator.id(), owned_inner.id());
        assert_eq!(fresh_borrowed_locator.ele(".does-not-exist")?.text()?, None);
        match fresh_borrowed_locator.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "non-singleton borrowed ElementsOne locator WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "non-singleton borrowed ElementsOne locator WebFrame should keep webpage owner, got id {id}"
                );
            }
        }
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("non-singleton elements-one webframe runtime-state regression");
}

#[test]
fn singleton_tab_obj_keeps_cross_wrapper_webframe_state_isolated_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (page, temp_dir) =
        launch_headless_test_webpage("webframe-cross-wrapper-singleton-disabled", WebMode::Driver)
            .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let driver_frame = page.driver.get_frame("css:#demo-frame")?;
        assert!(driver_frame.wait_for_doc_loaded(5_000)?);
        driver_frame.set_none_element_value(Some("driver missing"), true)?;

        let same_driver_handle = page.get_frame(&driver_frame)?;
        assert_eq!(
            same_driver_handle.ele(".does-not-exist")?.text()?,
            Some("driver missing".to_string())
        );

        let host = page.find("css:body")?;
        let same_driver_handle_from_element = host.get_frame(&driver_frame)?;
        assert_eq!(
            same_driver_handle_from_element
                .ele(".does-not-exist")?
                .text()?,
            Some("driver missing".to_string())
        );

        let mix_frame = page.get_frame("css:#demo-frame")?;
        assert_eq!(mix_frame.id(), driver_frame.id());
        assert_eq!(mix_frame.ele(".does-not-exist")?.text()?, None);
        match mix_frame.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "non-singleton cross-wrapper WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "non-singleton cross-wrapper WebFrame should keep webpage owner, got id {id}"
                );
            }
        }
        match mix_frame.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("demo-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "non-singleton cross-wrapper frame element should stay mix, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("non-singleton cross-wrapper frame element should stay mix");
            }
        }

        mix_frame.set_none_element_value(Some("mix missing"), true)?;
        let driver_fresh_again = page.driver.get_frame("css:#demo-frame")?;
        assert_eq!(driver_fresh_again.id(), driver_frame.id());
        assert_eq!(driver_fresh_again.ele(".does-not-exist")?.text()?, None);
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("non-singleton cross-wrapper webframe regression");
}

#[test]
fn singleton_tab_obj_keeps_nested_cross_wrapper_webframe_state_isolated_when_disabled() {
    let _settings = scoped_test_settings();
    Settings::reset();
    Settings::set_singleton_tab_obj(false);

    let (page, temp_dir) = launch_headless_test_webpage(
        "nested-webframe-cross-wrapper-singleton-disabled",
        WebMode::Driver,
    )
    .expect("launch headless webpage");

    let result = (|| -> crate::OpenPageResult<()> {
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

        let driver_outer = page.driver.get_frame("css:#outer-frame")?;
        assert!(driver_outer.wait_for_doc_loaded(2_000)?);
        let mix_outer = page.get_frame("css:#outer-frame")?;
        assert_eq!(mix_outer.id(), driver_outer.id());
        match mix_outer.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "non-singleton nested outer WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "non-singleton nested outer WebFrame should keep webpage owner, got id {id}"
                );
            }
        }

        driver_outer.run_js(
            r#"(() => {
                const frame = document.createElement('iframe');
                frame.id = 'inner-frame';
                frame.name = 'inner-frame';
                frame.srcdoc = "<html><body><button id='inside'>inside</button></body></html>";
                document.getElementById('outer-host').appendChild(frame);
                return true;
            })()"#,
        )?;

        let driver_inner = driver_outer.get_frame("css:#inner-frame")?;
        assert!(driver_inner.wait_for_doc_loaded(2_000)?);
        driver_inner.set_none_element_value(Some("driver nested missing"), true)?;

        let same_driver_handle = mix_outer.get_frame(&driver_inner)?;
        assert_eq!(
            same_driver_handle.ele(".does-not-exist")?.text()?,
            Some("driver nested missing".to_string())
        );

        let outer_host = mix_outer.find("css:#outer-host")?;
        let same_driver_handle_from_element = outer_host.get_frame(&driver_inner)?;
        assert_eq!(
            same_driver_handle_from_element
                .ele(".does-not-exist")?
                .text()?,
            Some("driver nested missing".to_string())
        );

        let mix_inner = mix_outer.get_frame("css:#inner-frame")?;
        assert_eq!(mix_inner.id(), driver_inner.id());
        assert_eq!(mix_inner.ele(".does-not-exist")?.text()?, None);
        match mix_inner.owner_reference() {
            BrowserTabReference::WebPage(owner) => {
                assert_eq!(owner.target_id(), page.target_id());
            }
            BrowserTabReference::Page(owner) => {
                panic!(
                    "non-singleton nested inner WebFrame should keep webpage owner, got page {}",
                    owner.target_id()
                );
            }
            BrowserTabReference::Id(id) => {
                panic!(
                    "non-singleton nested inner WebFrame should keep webpage owner, got id {id}"
                );
            }
        }
        match mix_inner.frame_element_reference()? {
            WebElement::Mix {
                element,
                page: owner,
            } => {
                assert_eq!(element.attr("id")?, Some("inner-frame".to_string()));
                assert_eq!(owner.target_id(), page.target_id());
            }
            WebElement::Browser(element) => {
                panic!(
                    "non-singleton nested cross-wrapper frame element should stay mix, got browser element {:?}",
                    element.attr("id")?
                );
            }
            WebElement::Session(_) => {
                panic!("non-singleton nested cross-wrapper frame element should stay mix");
            }
        }

        mix_inner.set_none_element_value(Some("mix nested missing"), true)?;
        let driver_inner_fresh_again = driver_outer.get_frame("css:#inner-frame")?;
        assert_eq!(driver_inner_fresh_again.id(), driver_inner.id());
        assert_eq!(
            driver_inner_fresh_again.ele(".does-not-exist")?.text()?,
            None
        );
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("non-singleton nested cross-wrapper webframe regression");
}

#[test]
fn page_frame_webpage_and_webframe_js_helper_signatures_accept_common_inputs() {
    fn assert_calls(page: &Page, frame: &Frame, web_page: &WebPage, web_frame: &WebFrame) {
        let args = [Value::from(1), Value::from(2)];

        let _ = page.run_js_loaded("1 + 2");
        let _ = page.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ =
            page.run_js_loaded_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = page.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ = page.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = page.run_async_js("window.__pageAsync = true;");
        let _ = page.run_async_js_with_args("window.__pageArg = arguments[0];", &args[..1], false);
        let _ =
            page.run_async_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));

        let _ = frame.run_js_loaded("1 + 2");
        let _ = frame.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ =
            frame.run_js_loaded_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = frame.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ = frame.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = frame.run_async_js("window.__frameAsync = true;");
        let _ =
            frame.run_async_js_with_args("window.__frameArg = arguments[0];", &args[..1], false);
        let _ =
            frame.run_async_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = frame.add_init_js("window.__frameInit = true;");
        let _ = frame.remove_init_js(None);

        let _ = web_page.run_js_loaded("1 + 2");
        let _ =
            web_page.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ = web_page.run_js_loaded_with_options(
            "arguments[0] + arguments[1]",
            &args,
            true,
            Some(500),
        );
        let _ = web_page.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ = web_page.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = web_page.run_async_js("window.__webPageAsync = true;");
        let _ = web_page.run_async_js_with_args(
            "window.__webPageArg = arguments[0];",
            &args[..1],
            false,
        );
        let _ = web_page.run_async_js_with_options(
            "arguments[0] + arguments[1]",
            &args,
            true,
            Some(500),
        );

        let _ = web_frame.run_js_loaded("1 + 2");
        let _ =
            web_frame.run_js_loaded_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ = web_frame.run_js_loaded_with_options(
            "arguments[0] + arguments[1]",
            &args,
            true,
            Some(500),
        );
        let _ = web_frame.run_js_with_args("return arguments[0] + arguments[1];", &args, false);
        let _ =
            web_frame.run_js_with_options("arguments[0] + arguments[1]", &args, true, Some(500));
        let _ = web_frame.run_async_js("window.__webFrameAsync = true;");
        let _ = web_frame.run_async_js_with_args(
            "window.__webFrameArg = arguments[0];",
            &args[..1],
            false,
        );
        let _ = web_frame.run_async_js_with_options(
            "arguments[0] + arguments[1]",
            &args,
            true,
            Some(500),
        );
        let _ = web_frame.add_init_js("window.__webFrameInit = true;");
        let _ = web_frame.remove_init_js(None);
    }

    let _ = assert_calls as fn(&Page, &Frame, &WebPage, &WebFrame);
}

#[test]
fn frame_and_webframe_element_reader_signatures_accept_common_inputs() {
    fn assert_calls(frame: &Frame, web_frame: &WebFrame) {
        let _ = frame.text();
        let _ = frame.raw_text();
        let _ = frame.value();
        let _ = frame.comments();
        let _ = frame.texts(false);
        let _ = frame.texts(true);
        let _ = frame.src(500, false);
        let _ = frame.src(500, true);
        let _ = frame.save(None, Some("frame.jpg"), 500, true);
        let _ = frame.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = frame.pseudo_before();
        let _ = frame.pseudo_after();
        let _ = frame.scroll_to_see(Some(true));
        let _ = frame.scroll_to_center();

        let _ = web_frame.text();
        let _ = web_frame.raw_text();
        let _ = web_frame.value();
        let _ = web_frame.comments();
        let _ = web_frame.texts(false);
        let _ = web_frame.texts(true);
        let _ = web_frame.src(500, false);
        let _ = web_frame.src(500, true);
        let _ = web_frame.save(None, Some("frame.jpg"), 500, true);
        let _ = web_frame.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = web_frame.pseudo_before();
        let _ = web_frame.pseudo_after();
        let _ = web_frame.scroll_to_see(Some(true));
        let _ = web_frame.scroll_to_center();
    }

    let _ = assert_calls as fn(&Frame, &WebFrame);
}

#[test]
fn frame_and_webframe_element_interaction_signatures_accept_common_inputs() {
    fn assert_calls(frame: &Frame, web_frame: &WebFrame) {
        let key_sequence = vec!["Control".to_string(), "a".to_string()];

        let _ = frame.click();
        let _ = frame.click_with_options(Some(false), Some(500), true);
        let _ = frame.click_at(Some(1.0), Some(2.0), "left", 1);
        let _ = frame.click_multi(2);
        let _ = frame.click_left();
        let _ = frame.click_left_with_options(Some(false), Some(500), true);
        let _ = frame.click_right();
        let _ = frame.input("hello");
        let _ = frame.input_with_options("hello", true, false);
        let _ = frame.input_keys_with_options(&key_sequence, true, false);
        let _ = frame.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = frame.press_key("Enter");
        let _ = frame.clear();
        let _ = frame.clear_with_mode(true);
        let _ = frame.submit();
        let _ = frame.focus();
        let _ = frame.hover();
        let _ = frame.hover_with_offset(Some(1.0), Some(2.0));
        let _ = frame.drag(1.0, 2.0, 0.1);
        let _ = frame.drag_to((10.0, 20.0), 0.1);
        let _ = frame.drag_to("css:#target", 0.1);
        let _ = frame.drag_to_point(10.0, 20.0, 0.1);
        let _ = frame.set_checked(true);
        let _ = frame.check(false, true);
        let _ = frame.uncheck(true);

        let _ = web_frame.click();
        let _ = web_frame.click_with_options(Some(false), Some(500), true);
        let _ = web_frame.click_at(Some(1.0), Some(2.0), "left", 1);
        let _ = web_frame.click_multi(2);
        let _ = web_frame.click_left();
        let _ = web_frame.click_left_with_options(Some(false), Some(500), true);
        let _ = web_frame.click_right();
        let _ = web_frame.input("hello");
        let _ = web_frame.input_with_options("hello", true, false);
        let _ = web_frame.input_keys_with_options(&key_sequence, true, false);
        let _ = web_frame.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = web_frame.press_key("Enter");
        let _ = web_frame.clear();
        let _ = web_frame.clear_with_mode(true);
        let _ = web_frame.submit();
        let _ = web_frame.focus();
        let _ = web_frame.hover();
        let _ = web_frame.hover_with_offset(Some(1.0), Some(2.0));
        let _ = web_frame.drag(1.0, 2.0, 0.1);
        let _ = web_frame.drag_to((10.0, 20.0), 0.1);
        let _ = web_frame.drag_to("css:#target", 0.1);
        let _ = web_frame.drag_to_point(10.0, 20.0, 0.1);
        let _ = web_frame.set_checked(true);
        let _ = web_frame.check(false, true);
        let _ = web_frame.uncheck(true);
    }

    let _ = assert_calls as fn(&Frame, &WebFrame);
}

#[test]
fn frame_and_webframe_relative_signatures_accept_common_inputs() {
    fn assert_calls(frame: &Frame, web_frame: &WebFrame) {
        let _ = frame.parent();
        let _ = frame.parent_level(2);
        let _ = frame.parent_with((By::ID, "root"), 1);
        let _ = frame.child();
        let _ = frame.child_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = frame.child_with(None::<&str>, 1);
        let _ = frame.children();
        let _ = frame.children_with(Some((By::CLASS_NAME, "item")));
        let _ = frame.children_with(None::<&str>);
        let _ = frame.prev();
        let _ = frame.prev_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = frame.prevs();
        let _ = frame.prevs_with(Some((By::CLASS_NAME, "item")));
        let _ = frame.next();
        let _ = frame.next_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = frame.nexts();
        let _ = frame.nexts_with(Some((By::CLASS_NAME, "item")));
        let _ = frame.before();
        let _ = frame.before_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = frame.befores();
        let _ = frame.befores_with(Some((By::CLASS_NAME, "item")));
        let _ = frame.after();
        let _ = frame.after_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = frame.afters();
        let _ = frame.afters_with(Some((By::CLASS_NAME, "item")));
        let _ = frame.over();
        let _ = frame.over_with_timeout(100);
        let _ = frame.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
        let _ = frame.offset(None::<&str>, None, None, 100);
        let _ = frame.east(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = frame.south(Some((By::CLASS_NAME, "item")), Some(10), 1);
        let _ = frame.west(None::<&str>, None, 1);
        let _ = frame.north(None::<&str>, Some(10), 1);

        let _ = web_frame.parent();
        let _ = web_frame.parent_level(2);
        let _ = web_frame.parent_with((By::ID, "root"), 1);
        let _ = web_frame.child();
        let _ = web_frame.child_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_frame.child_with(None::<&str>, 1);
        let _ = web_frame.children();
        let _ = web_frame.children_with(Some((By::CLASS_NAME, "item")));
        let _ = web_frame.children_with(None::<&str>);
        let _ = web_frame.prev();
        let _ = web_frame.prev_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_frame.prevs();
        let _ = web_frame.prevs_with(Some((By::CLASS_NAME, "item")));
        let _ = web_frame.next();
        let _ = web_frame.next_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_frame.nexts();
        let _ = web_frame.nexts_with(Some((By::CLASS_NAME, "item")));
        let _ = web_frame.before();
        let _ = web_frame.before_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_frame.befores();
        let _ = web_frame.befores_with(Some((By::CLASS_NAME, "item")));
        let _ = web_frame.after();
        let _ = web_frame.after_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_frame.afters();
        let _ = web_frame.afters_with(Some((By::CLASS_NAME, "item")));
        let _ = web_frame.over();
        let _ = web_frame.over_with_timeout(100);
        let _ = web_frame.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
        let _ = web_frame.offset(None::<&str>, None, None, 100);
        let _ = web_frame.east(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = web_frame.south(Some((By::CLASS_NAME, "item")), Some(10), 1);
        let _ = web_frame.west(None::<&str>, None, 1);
        let _ = web_frame.north(None::<&str>, Some(10), 1);
    }

    let _ = assert_calls as fn(&Frame, &WebFrame);
}

#[test]
fn webframe_transfer_helper_signatures_accept_common_inputs() {
    fn assert_calls(frame: &Frame, web_page: &WebPage, web_frame: &WebFrame) {
        let files = vec!["/tmp/demo.txt".to_string()];
        let upload_path = PathBuf::from("/tmp/demo.txt");
        let borrowed_files = ["/tmp/demo.txt", "/tmp/alt.txt"];
        let cookies = json!({"sid": "abc", "domain": ".example.test", "path": "/"});

        let _ = frame.set().cookie().set(&cookies);
        let _ = frame.set().cookie().clear();
        let _ = frame.set().cookie().remove("sid", None, None, None);
        let _ = frame.set().clear_cookies();
        let _ = frame.set().remove_cookie("sid", None, None, None);
        let _ = frame.frame_id();
        let _ = frame.page();
        let _ = frame.owner();
        let _ = frame.tab();
        let _ = frame.set_upload_files(&files);
        let _ = frame.set_upload_files("/tmp/demo.txt");
        let _ = frame.set_upload_files(&upload_path);
        let _ = frame.set_upload_paths(&files);
        let _ = frame.set_upload_paths(&borrowed_files);
        let _ = frame.set_download_path("/tmp");
        let _ = frame.set_download_file_exists_mode(DownloadFileExistsMode::Overwrite);
        let _ = frame.set_when_download_file_exists("overwrite");
        let _ = frame.set_download_filename(Some("demo"), Some(".txt"), true);
        let _ = frame.set_download_file_name(Some("demo"), Some(".txt"), true);
        let _ = frame.wait_for_upload_paths_inputted(1_000);
        let _ = frame.wait_for_download_begin(1_000, false);
        let _ = frame.wait_for_downloads_done(1_000, true);
        let _ = frame.click_to_download(
            "css:#download",
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = frame.click_to_download(
            (By::ID, "download"),
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = frame.click_to_upload("css:#upload", &files, Some(1_000), false);
        let _ = frame.click_to_upload((By::ID, "upload"), &files, Some(1_000), false);
        let _ = frame.click_to_upload("css:#upload", "/tmp/demo.txt", Some(1_000), false);
        let _ = frame.click_for_new_tab("css:#open", Some(1_000), false);
        let _ = frame.click_for_new_tab((By::ID, "open"), Some(1_000), false);
        let _ = frame.click_middle("css:#open", Some(1_000), true);
        let _ = frame.click_middle((By::ID, "open"), Some(1_000), true);
        let _ = frame.get("https://example.test/frame");
        let _ = frame.goto("https://example.test/frame");
        let _ = frame.refresh_with_options(true);
        let _ = frame.save_screenshot("/tmp/frame.png");

        let _ = web_page.set_upload_files(&files);
        let _ = web_page.set_upload_files("/tmp/demo.txt");
        let _ = web_page.set_upload_files(&upload_path);
        let _ = web_page.set_upload_paths(&files);
        let _ = web_page.set_upload_paths(&borrowed_files);
        let _ = web_page.set_download_path("/tmp");
        let _ = web_page.set_download_file_exists_mode(DownloadFileExistsMode::Overwrite);
        let _ = web_page.when_download_file_exists("overwrite");
        let _ = web_page.set_tab_download_path("/tmp");
        let _ = web_page.set_tab_download_file_exists_mode(DownloadFileExistsMode::Overwrite);
        let _ = web_page.set_tab_when_download_file_exists("overwrite");
        let _ = web_page.set_tab_download_filename(Some("demo"), Some(".txt"), true);
        let _ = web_page.set_tab_download_file_name(Some("demo"), Some(".txt"), true);
        let _ = web_page.set_download_filename(Some("demo"), Some(".txt"), true);
        let _ = web_page.set_download_file_name(Some("demo"), Some(".txt"), true);
        let _ = web_page.set_current_tab_download_file_name(Some("demo"), Some(".txt"), true);
        let _ = web_page.wait_for_upload_paths_inputted(1_000);
        let _ = web_page.wait_for_download_begin(1_000, false);
        let _ = web_page.wait_for_downloads_done(1_000, true);
        let _ = web_page.click_to_download(
            "css:#download",
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = web_page.click_to_download(
            (By::ID, "download"),
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = web_page.click_to_upload("css:#upload", &files, Some(1_000), false);
        let _ = web_page.click_to_upload((By::ID, "upload"), &files, Some(1_000), false);
        let _ = web_page.click_to_upload("css:#upload", &upload_path, Some(1_000), false);
        let _ = web_page.click_for_new_tab("css:#open", Some(1_000), false);
        let _ = web_page.click_for_new_tab((By::ID, "open"), Some(1_000), false);
        let _ = web_page.click_middle("css:#open", Some(1_000), true);
        let _ = web_page.click_middle((By::ID, "open"), Some(1_000), true);

        let _ = web_frame.set().cookie().set(&cookies);
        let _ = web_frame.set().cookie().clear();
        let _ = web_frame.set().cookie().remove("sid", None, None, None);
        let _ = web_frame.set_cookies(&cookies);
        let _ = web_frame.clear_cookies();
        let _ = web_frame.remove_cookie("sid", None, None, None);
        let _ = web_frame.frame_id();
        let _ = web_frame.page();
        let _ = web_frame.owner();
        let _ = web_frame.tab();
        let _ = web_frame.set_upload_files(&files);
        let _ = web_frame.set_upload_files("/tmp/demo.txt");
        let _ = web_frame.set_upload_files(&upload_path);
        let _ = web_frame.set_upload_paths(&files);
        let _ = web_frame.set_upload_paths(&borrowed_files);
        let _ = web_frame.set_download_path("/tmp");
        let _ = web_frame.set_download_file_exists_mode(DownloadFileExistsMode::Overwrite);
        let _ = web_frame.set_when_download_file_exists("overwrite");
        let _ = web_frame.set_download_filename(Some("demo"), Some(".txt"), true);
        let _ = web_frame.set_download_file_name(Some("demo"), Some(".txt"), true);
        let _ = web_frame.wait_for_upload_paths_inputted(1_000);
        let _ = web_frame.wait_for_download_begin(1_000, false);
        let _ = web_frame.wait_for_downloads_done(1_000, true);
        let _ = web_frame.click_to_download(
            "css:#download",
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = web_frame.click_to_download(
            (By::ID, "download"),
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = web_frame.click_to_upload("css:#upload", &files, Some(1_000), false);
        let _ = web_frame.click_to_upload((By::ID, "upload"), &files, Some(1_000), false);
        let _ = web_frame.click_to_upload("css:#upload", &borrowed_files, Some(1_000), false);
        let _ = web_frame.click_for_new_tab("css:#open", Some(1_000), false);
        let _ = web_frame.click_for_new_tab((By::ID, "open"), Some(1_000), false);
        let _ = web_frame.click_middle("css:#open", Some(1_000), true);
        let _ = web_frame.click_middle((By::ID, "open"), Some(1_000), true);
        let _ = web_frame.get("https://example.test/frame");
        let _ = web_frame.goto("https://example.test/frame");
        let _ = web_frame.refresh_with_options(true);
        let _ = web_frame.save_screenshot("/tmp/web-frame.png");
    }

    let _ = assert_calls as fn(&Frame, &WebPage, &WebFrame);
}

#[test]
fn page_and_webpage_run_cdp_alias_signatures_accept_command_types() {
    fn assert_calls(page: &Page, web_page: &WebPage) {
        let _ = page.run_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
        let _ = page.run_cdp_loaded(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
        let _ = page.evaluate("1 + 1");
        let _ = web_page.run_cdp(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
        let _ = web_page.run_cdp_loaded(SetDeviceMetricsOverrideParams::new(1280, 720, 1.0, false));
        let _ = web_page.evaluate("1 + 1");
    }

    let _ = assert_calls as fn(&Page, &WebPage);
}

#[test]
fn page_and_webpage_runtime_setting_signatures_accept_common_inputs() {
    fn assert_calls(page: &Page, web_page: &WebPage) {
        let _ = page.retry_times();
        let _ = page.retry_interval();
        let _ = page.timeouts();
        let _ = page.browser();
        let _ = page.set_retry(Some(5), Some(0.25));
        let _ = page.set_timeouts(Some(1.5), Some(6.0), Some(0.75));

        let _ = web_page.goto("https://example.test/");
        let _ = web_page.browser();
        let _ = web_page.retry_times();
        let _ = web_page.retry_interval();
        let _ = web_page.timeouts();
        let _ = web_page.set_retry(Some(5), Some(0.25));
        let _ = web_page.set_timeouts(Some(1.5), Some(6.0), Some(0.75));
        let _ = web_page.set_encoding("utf-8");
        let _ = web_page.set_encoding(None);
    }

    let _ = assert_calls as fn(&Page, &WebPage);
}

#[test]
fn page_and_webpage_set_wrapper_signatures_accept_common_inputs() {
    fn assert_calls(page: &Page, web_page: &WebPage) {
        let headers = [("Accept".to_string(), "text/html".to_string())];
        let urls = vec!["*.css*".to_string()];
        let files = vec!["/tmp/demo.txt".to_string()];
        let upload_path = PathBuf::from("/tmp/demo.txt");
        let borrowed_files = ["/tmp/demo.txt", "/tmp/alt.txt"];
        let cookies = json!({"sid": "abc", "domain": ".example.test", "path": "/"});

        let _ = page.set().window().max();
        let _ = page.set().window().mini();
        let _ = page.set().window().full();
        let _ = page.set().window().normal();
        let _ = page.set().window().size(Some(800), Some(600));
        let _ = page.set().window().location(Some(10), Some(20));
        let _ = page.set().window().hide();
        let _ = page.set().window().show();
        let _ = page.set().load_mode().normal();
        let _ = page.set().load_mode().eager();
        let _ = page.set().load_mode().none();
        let _ = page.set_blocked_urls("*.png*");
        let _ = page.set().blocked_urls(&urls);
        let _ = page.set().blocked_urls("*.css*");
        let _ = page.set().blocked_urls(["*.css*", "*.js*"]);
        let _ = page.set_headers("Accept: text/html\nX-Test: 1");
        let _ = page.set().headers(&headers);
        let _ = page.set().headers("Accept: text/html\nX-Test: 1");
        let _ = page
            .set()
            .headers([("Accept", "text/html"), ("X-Test", "1")]);
        let _ = page.set().user_agent("demo-agent", Some("linux"));
        let _ = page.set().session_storage("foo", Some("bar"));
        let _ = page.set().local_storage("foo", Some("bar"));
        let _ = page.set().auto_handle_alert(Some(true), Some("ok"));
        let _ = page.set().cookies(&cookies);
        let _ = page.set().cookie().set(&cookies);
        let _ = page.set().cookie().clear();
        let _ = page.set().cookie().remove("sid", None, None, None);
        let _ = page.set().clear_cookies();
        let _ = page.set().remove_cookie("sid", None, None, None);
        let _ = page.set().download_path("/tmp");
        let _ = page
            .set()
            .download_file_exists(DownloadFileExistsMode::Rename);
        let _ = page.set().when_download_file_exists("rename");
        let _ = page.set().download_file_name(Some("file"), Some(".txt"));
        let _ = page.set().upload_files(&files);
        let _ = page.set().upload_files("/tmp/demo.txt");
        let _ = page.set().upload_files(&upload_path);
        let _ = page.set().upload_paths(&files);
        let _ = page.set().upload_paths(&borrowed_files);
        let _ = page.set().activate();
        let _ = page.set().retry(Some(5), Some(0.25));
        let _ = page.set().retry_times(5);
        let _ = page.set().retry_interval(0.25);
        let _ = page.set().timeout(1.0);
        let _ = page.set().timeouts(Some(1.0), Some(2.0), Some(3.0));

        let _ = web_page.set().window().max();
        let _ = web_page.set().window().mini();
        let _ = web_page.set().window().full();
        let _ = web_page.set().window().normal();
        let _ = web_page.set().window().size(Some(800), Some(600));
        let _ = web_page.set().window().location(Some(10), Some(20));
        let _ = web_page.set().window().hide();
        let _ = web_page.set().window().show();
        let _ = web_page.set().load_mode().normal();
        let _ = web_page.set().load_mode().eager();
        let _ = web_page.set().load_mode().none();
        let _ = web_page.set_blocked_urls("*.png*");
        let _ = web_page.set().blocked_urls(&urls);
        let _ = web_page.set().blocked_urls("*.css*");
        let _ = web_page.set().blocked_urls(["*.css*", "*.js*"]);
        let _ = web_page.set_headers("Accept: text/html\nX-Test: 1");
        let _ = web_page.set().headers(&headers);
        let _ = web_page.set().headers("Accept: text/html\nX-Test: 1");
        let _ = web_page
            .set()
            .headers([("Accept", "text/html"), ("X-Test", "1")]);
        let _ = web_page.set().user_agent("demo-agent", Some("linux"));
        let _ = web_page.set().encoding("utf-8");
        let _ = web_page.set().encoding(None);
        let _ = web_page.set().session_storage("foo", Some("bar"));
        let _ = web_page.set().local_storage("foo", Some("bar"));
        let _ = web_page.set().auto_handle_alert(Some(true), Some("ok"));
        let _ = web_page.set().cookies(&cookies);
        let _ = web_page.set().cookie().set(&cookies);
        let _ = web_page.set().cookie().clear();
        let _ = web_page.set().cookie().remove("sid", None, None, None);
        let _ = web_page.set().clear_cookies();
        let _ = web_page.set().remove_cookie("sid", None, None, None);
        let _ = web_page.set().download_path("/tmp");
        let _ = web_page
            .set()
            .download_file_exists(DownloadFileExistsMode::Rename);
        let _ = web_page.set().when_download_file_exists("rename");
        let _ = web_page
            .set()
            .download_file_name(Some("file"), Some(".txt"));
        let _ = web_page.set().upload_files(&files);
        let _ = web_page.set().upload_files("/tmp/demo.txt");
        let _ = web_page.set().upload_files(&upload_path);
        let _ = web_page.set().upload_paths(&files);
        let _ = web_page.set().upload_paths(&borrowed_files);
        let _ = web_page.set().activate();
        let _ = web_page.set().retry(Some(5), Some(0.25));
        let _ = web_page.set().retry_times(5);
        let _ = web_page.set().retry_interval(0.25);
        let _ = web_page.set().timeout(1.0);
        let _ = web_page.set().timeouts(Some(1.0), Some(2.0), Some(3.0));
    }

    let _ = assert_calls as fn(&Page, &WebPage);
}

#[test]
fn page_and_webpage_scroll_wrapper_signatures_accept_common_inputs() {
    fn assert_calls(page: &Page, web_page: &WebPage) {
        let _ = page.scroll().to_top();
        let _ = page.scroll().to_bottom();
        let _ = page.scroll().to_half();
        let _ = page.scroll().to_rightmost();
        let _ = page.scroll().to_leftmost();
        let _ = page.scroll().to_location(10.0, 20.0);
        let _ = page.scroll().up(10.0);
        let _ = page.scroll().down(10.0);
        let _ = page.scroll().left(10.0);
        let _ = page.scroll().right(10.0);

        let _ = web_page.scroll().to_top();
        let _ = web_page.scroll().to_bottom();
        let _ = web_page.scroll().to_half();
        let _ = web_page.scroll().to_rightmost();
        let _ = web_page.scroll().to_leftmost();
        let _ = web_page.scroll().to_location(10.0, 20.0);
        let _ = web_page.scroll().up(10.0);
        let _ = web_page.scroll().down(10.0);
        let _ = web_page.scroll().left(10.0);
        let _ = web_page.scroll().right(10.0);
    }

    let _ = assert_calls as fn(&Page, &WebPage);
}

#[test]
fn page_and_webpage_actions_signatures_accept_locators_elements_and_coordinates() {
    fn assert_calls(page: &Page, web_page: &WebPage, element: &Element, web_element: &WebElement) {
        let _ = page.actions();
        let _ = page
            .new_actions()
            .move_to((10, 20), None, None, 0.0)
            .and_then(|actions| actions.move_to((By::ID, "root"), Some(3.0), Some(4.0), 0.0))
            .and_then(|actions| actions.move_to(element, None, None, 0.0))
            .and_then(|actions| actions.click(Some(element), 1))
            .and_then(|actions| actions.r_click(Some((By::ID, "root")), 1))
            .and_then(|actions| actions.m_click(Some((12.0, 24.0)), 1))
            .and_then(|actions| actions.hold(Some(element)))
            .and_then(|actions| actions.release(Some((By::ID, "root"))))
            .and_then(|actions| actions.r_hold(Some((By::ID, "root"))))
            .and_then(|actions| actions.r_release(Some((By::ID, "root"))))
            .and_then(|actions| actions.m_hold(Some((By::ID, "root"))))
            .and_then(|actions| actions.m_release(Some((By::ID, "root"))))
            .and_then(|actions| actions.scroll(120.0, 0.0, Some((By::ID, "root"))))
            .and_then(|actions| actions.key_down("Shift"))
            .and_then(|actions| actions.key_up("Shift"))
            .and_then(|actions| actions.input("demo"))
            .and_then(|actions| actions.r#type(["Control", "a"]))
            .and_then(|actions| actions.type_with_interval("demo", 0.05))
            .and_then(|actions| actions.type_keys(vec!["b", "c"]))
            .and_then(|actions| actions.type_keys_with_interval(["d", "e"], 0.05))
            .and_then(|actions| {
                actions.drag_in(
                    element,
                    crate::ActionsDragData::files(["./fixtures/demo.txt"]),
                )
            })
            .and_then(|actions| {
                actions.drag_in((By::ID, "root"), crate::ActionsDragData::text("demo"))
            })
            .and_then(|actions| {
                actions.drag_in(
                    (By::ID, "root"),
                    crate::ActionsDragData::link("https://example.test", "Example"),
                )
            })
            .and_then(|actions| {
                actions.drag_in(
                    (By::ID, "root"),
                    crate::ActionsDragData::html("<b>demo</b>", "https://example.test/base"),
                )
            })
            .and_then(|actions| actions.r#move(5.0, 6.0, 0.0))
            .and_then(|actions| actions.wait(0.0, None));

        let _ = web_page.actions();
        let _ = web_page
            .new_actions()
            .and_then(|mut actions| {
                actions.drag_in(web_element, crate::ActionsDragData::text("demo"))?;
                actions.drag_in(
                    web_element,
                    crate::ActionsDragData::link("https://example.test", "Example"),
                )?;
                actions.drag_in(
                    web_element,
                    crate::ActionsDragData::html("<b>demo</b>", "https://example.test/base"),
                )?;
                Ok(actions)
            })
            .and_then(|mut actions| actions.move_to(web_element, None, None, 0.0).map(|_| ()));
    }

    let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement);

    fn assert_owned_page_remove(page: &Page, element: Element) {
        let _ = page.remove_element(element);
    }

    fn assert_owned_page_insert(page: &Page, parent: Element, before: Element) {
        let _ = page.add_element_html("<div>demo</div>", Some(parent), Some(before));
    }

    fn assert_owned_page_action_move(page: &Page, element: Element) {
        let mut actions = page.new_actions();
        let _ = actions.move_to(element, None, None, 0.0);
    }

    fn assert_owned_webpage_remove(page: &WebPage, element: WebElement) {
        let _ = page.remove_element(element);
    }

    fn assert_owned_webpage_insert(page: &WebPage, parent: WebElement, before: WebElement) {
        let _ = page.add_element_html("<div>demo</div>", Some(parent), Some(before));
    }

    fn assert_owned_webpage_action_move(page: &WebPage, element: WebElement) {
        let _ = page.new_actions().and_then(|mut actions| {
            actions.move_to(element, None, None, 0.0)?;
            Ok(actions)
        });
    }

    let _ = assert_owned_page_remove as fn(&Page, Element);
    let _ = assert_owned_page_insert as fn(&Page, Element, Element);
    let _ = assert_owned_page_action_move as fn(&Page, Element);
    let _ = assert_owned_webpage_remove as fn(&WebPage, WebElement);
    let _ = assert_owned_webpage_insert as fn(&WebPage, WebElement, WebElement);
    let _ = assert_owned_webpage_action_move as fn(&WebPage, WebElement);
}

#[test]
fn element_and_webelement_object_wrappers_expose_scroll_set_and_select_signatures() {
    fn assert_calls(element: &Element, web_element: &WebElement) {
        let _ = element.drag_to(element, 0.1);
        let _ = element.drag_to("css:#target", 0.1);
        let _ = element.drag_to((By::ID, "target"), 0.1);
        let _ = element.drag_to((50, 50), 0.1);
        let _ = element.drag_to((50.0, 50.0), 0.1);
        let _ = web_element.drag_to(web_element, 0.1);
        let _ = web_element.drag_to("css:#target", 0.1);
        let _ = web_element.drag_to((By::ID, "target"), 0.1);
        let _ = web_element.drag_to((50, 50), 0.1);
        let _ = web_element.drag_to((50.0, 50.0), 0.1);
        let _ = web_element.drag_to_element(web_element, 0.1);

        let _ = element.states().is_in_viewport();
        let _ = element.states().is_whole_in_viewport();
        let _ = element.states().is_alive();
        let _ = element.states().is_checked();
        let _ = element.states().is_selected();
        let _ = element.states().is_enabled();
        let _ = element.states().is_displayed();
        let _ = element.states().is_covered();
        let _ = element.states().is_clickable();
        let _ = element.states().has_rect();

        let _ = element.rect().corners();
        let _ = element.rect().viewport_corners();
        let _ = element.rect().location();
        let _ = element.rect().viewport_location();
        let _ = element.rect().screen_location();
        let _ = element.rect().midpoint();
        let _ = element.rect().viewport_midpoint();
        let _ = element.rect().click_point();
        let _ = element.rect().viewport_click_point();
        let _ = element.rect().screen_midpoint();
        let _ = element.rect().screen_click_point();
        let _ = element.rect().size();
        let _ = element.rect().scroll_position();

        let _ = element.wait().displayed(1_000);
        let _ = element.wait().hidden(1_000);
        let _ = element.wait().enabled(1_000);
        let _ = element.wait().disabled(1_000);
        let _ = element.wait().deleted(1_000);
        let _ = element.wait().clickable(1_000);
        let _ = element.wait().has_rect(1_000);
        let _ = element.wait().covered(1_000);
        let _ = element.wait().not_covered(1_000);
        let _ = element.wait().disabled_or_deleted(1_000);
        let _ = element.wait().stop_moving(1_000);

        let _ = element.scroll().to_top();
        let _ = element.scroll().to_bottom();
        let _ = element.scroll().to_half();
        let _ = element.scroll().to_rightmost();
        let _ = element.scroll().to_leftmost();
        let _ = element.scroll().to_location(10.0, 20.0);
        let _ = element.scroll().up(10.0);
        let _ = element.scroll().down(10.0);
        let _ = element.scroll().left(10.0);
        let _ = element.scroll().right(10.0);
        let _ = element.scroll().to_see(Some(true));
        let _ = element.scroll().to_center();

        let _ = element.set().inner_html("<span>demo</span>");
        let _ = element.set().property("value", &serde_json::json!("demo"));
        let _ = element.set().style("display", "block");
        let _ = element.set().attr("data-role", "demo");
        let _ = element.set().value("demo");

        let _ = element.select().by_text("demo");
        let _ = element.select().by_text(["demo", "alt"]);
        let _ = element.select().by_text_with_timeout("demo", Some(1_000));
        let _ = element.select().by_value("demo");
        let _ = element.select().by_value(["demo", "alt"]);
        let _ = element.select().by_value_with_timeout("demo", Some(1_000));
        let _ = element.select().by_index(1);
        let _ = element.select().by_index([1, 2]);
        let _ = element.select().by_index_with_timeout([1, 2], Some(1_000));
        let _ = element.select().by_indices(&[1, 2]);
        let _ = element
            .select()
            .by_indices_with_timeout(&[1, 2], Some(1_000));
        let _ = element.select().by_locator("css:option");
        let locator_list = vec!["css:option".to_string(), "css:option.demo".to_string()];
        let _ = element.select().by_locator(&locator_list);
        let _ = element
            .select()
            .by_locator_with_timeout(&locator_list, Some(1_000));
        let _ = element.select().by_option(element);
        let _ = element.select().by_option([element, element]);
        let option_refs = [element, element];
        let _ = element.select().by_options(&option_refs);
        let _ = element.select().cancel_by_text("demo");
        let _ = element.select().cancel_by_text(["demo", "alt"]);
        let _ = element
            .select()
            .cancel_by_text_with_timeout("demo", Some(1_000));
        let _ = element.select().cancel_by_value("demo");
        let _ = element.select().cancel_by_value(["demo", "alt"]);
        let _ = element
            .select()
            .cancel_by_value_with_timeout("demo", Some(1_000));
        let _ = element.select().cancel_by_index(1);
        let _ = element.select().cancel_by_index([1, 2]);
        let _ = element
            .select()
            .cancel_by_index_with_timeout([1, 2], Some(1_000));
        let _ = element.select().cancel_by_indices(&[1, 2]);
        let _ = element
            .select()
            .cancel_by_indices_with_timeout(&[1, 2], Some(1_000));
        let _ = element.select().cancel_by_locator("css:option");
        let _ = element.select().cancel_by_locator(&locator_list);
        let _ = element
            .select()
            .cancel_by_locator_with_timeout(&locator_list, Some(1_000));
        let _ = element.select().cancel_by_option(element);
        let _ = element.select().cancel_by_option([element, element]);
        let _ = element.select().cancel_by_options(&option_refs);
        let _ = element.select().all();
        let _ = element.select().clear();
        let _ = element.select().invert();
        let _ = element.select().is_multi();
        let _ = element.select().options();
        let _ = element.select().selected_option();
        let _ = element.select().selected_options();

        let _ = web_element.states().is_in_viewport();
        let _ = web_element.states().is_whole_in_viewport();
        let _ = web_element.states().is_alive();
        let _ = web_element.states().is_checked();
        let _ = web_element.states().is_selected();
        let _ = web_element.states().is_enabled();
        let _ = web_element.states().is_displayed();
        let _ = web_element.states().is_covered();
        let _ = web_element.states().is_clickable();
        let _ = web_element.states().has_rect();

        let _ = web_element.rect().corners();
        let _ = web_element.rect().viewport_corners();
        let _ = web_element.rect().location();
        let _ = web_element.rect().viewport_location();
        let _ = web_element.rect().screen_location();
        let _ = web_element.rect().midpoint();
        let _ = web_element.rect().viewport_midpoint();
        let _ = web_element.rect().click_point();
        let _ = web_element.rect().viewport_click_point();
        let _ = web_element.rect().screen_midpoint();
        let _ = web_element.rect().screen_click_point();
        let _ = web_element.rect().size();
        let _ = web_element.rect().scroll_position();

        let _ = web_element.wait().displayed(1_000);
        let _ = web_element.wait().hidden(1_000);
        let _ = web_element.wait().enabled(1_000);
        let _ = web_element.wait().disabled(1_000);
        let _ = web_element.wait().deleted(1_000);
        let _ = web_element.wait().clickable(1_000);
        let _ = web_element.wait().has_rect(1_000);
        let _ = web_element.wait().covered(1_000);
        let _ = web_element.wait().not_covered(1_000);
        let _ = web_element.wait().disabled_or_deleted(1_000);
        let _ = web_element.wait().stop_moving(1_000);

        let _ = web_element.scroll().to_top();
        let _ = web_element.scroll().to_bottom();
        let _ = web_element.scroll().to_half();
        let _ = web_element.scroll().to_rightmost();
        let _ = web_element.scroll().to_leftmost();
        let _ = web_element.scroll().to_location(10.0, 20.0);
        let _ = web_element.scroll().up(10.0);
        let _ = web_element.scroll().down(10.0);
        let _ = web_element.scroll().left(10.0);
        let _ = web_element.scroll().right(10.0);
        let _ = web_element.scroll().to_see(Some(true));
        let _ = web_element.scroll().to_center();

        let _ = web_element.set().inner_html("<span>demo</span>");
        let _ = web_element
            .set()
            .property("value", &serde_json::json!("demo"));
        let _ = web_element.set().style("display", "block");
        let _ = web_element.set().attr("data-role", "demo");
        let _ = web_element.set().value("demo");

        let _ = web_element.select().by_text("demo");
        let _ = web_element.select().by_text(["demo", "alt"]);
        let _ = web_element
            .select()
            .by_text_with_timeout("demo", Some(1_000));
        let _ = web_element.select().by_value("demo");
        let _ = web_element.select().by_value(["demo", "alt"]);
        let _ = web_element
            .select()
            .by_value_with_timeout("demo", Some(1_000));
        let _ = web_element.select().by_index(1);
        let _ = web_element.select().by_index([1, 2]);
        let _ = web_element
            .select()
            .by_index_with_timeout([1, 2], Some(1_000));
        let _ = web_element.select().by_indices(&[1, 2]);
        let _ = web_element
            .select()
            .by_indices_with_timeout(&[1, 2], Some(1_000));
        let _ = web_element.select().by_locator("css:option");
        let web_locator_list = vec!["css:option".to_string(), "css:option.demo".to_string()];
        let _ = web_element.select().by_locator(&web_locator_list);
        let _ = web_element
            .select()
            .by_locator_with_timeout(&web_locator_list, Some(1_000));
        let _ = web_element.select().by_option(web_element);
        let _ = web_element.select().by_option([web_element, web_element]);
        let web_option_refs = [web_element, web_element];
        let _ = web_element.select().by_options(&web_option_refs);
        let _ = web_element.select().cancel_by_text("demo");
        let _ = web_element.select().cancel_by_text(["demo", "alt"]);
        let _ = web_element
            .select()
            .cancel_by_text_with_timeout("demo", Some(1_000));
        let _ = web_element.select().cancel_by_value("demo");
        let _ = web_element.select().cancel_by_value(["demo", "alt"]);
        let _ = web_element
            .select()
            .cancel_by_value_with_timeout("demo", Some(1_000));
        let _ = web_element.select().cancel_by_index(1);
        let _ = web_element.select().cancel_by_index([1, 2]);
        let _ = web_element
            .select()
            .cancel_by_index_with_timeout([1, 2], Some(1_000));
        let _ = web_element.select().cancel_by_indices(&[1, 2]);
        let _ = web_element
            .select()
            .cancel_by_indices_with_timeout(&[1, 2], Some(1_000));
        let _ = web_element.select().cancel_by_locator("css:option");
        let _ = web_element.select().cancel_by_locator(&web_locator_list);
        let _ = web_element
            .select()
            .cancel_by_locator_with_timeout(&web_locator_list, Some(1_000));
        let _ = web_element.select().cancel_by_option(web_element);
        let _ = web_element
            .select()
            .cancel_by_option([web_element, web_element]);
        let _ = web_element.select().cancel_by_options(&web_option_refs);
        let _ = web_element.select().all();
        let _ = web_element.select().clear();
        let _ = web_element.select().invert();
        let _ = web_element.select().is_multi();
        let _ = web_element.select().options();
        let _ = web_element.select().selected_option();
        let _ = web_element.select().selected_options();
    }

    let _ = assert_calls as fn(&Element, &WebElement);

    fn assert_owned_webframe_drag_target(frame: &WebFrame, target: WebElement) {
        let _ = frame.drag_to(target, 0.1);
    }

    fn assert_owned_webelement_drag_target(element: &WebElement, target: WebElement) {
        let _ = element.drag_to(target, 0.1);
    }

    let _ = assert_owned_webframe_drag_target as fn(&WebFrame, WebElement);
    let _ = assert_owned_webelement_drag_target as fn(&WebElement, WebElement);
}

#[test]
fn element_and_webelement_clicker_expose_signatures() {
    fn assert_calls(
        element: &Element,
        web_element: &WebElement,
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
    ) {
        let files = vec!["./fixtures/demo.txt".to_string()];
        let upload_path = PathBuf::from("./fixtures/demo.txt");
        let borrowed_files = ["./fixtures/demo.txt", "./fixtures/alt.txt"];

        let _ = element.click_with_options(None, Some(1_000), true);
        let _ = element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = element.clicker().left();
        let _ = element
            .clicker()
            .left_with_options(Some(true), Some(1_000), false);
        let _ = element.clicker().right();
        let _ = element.clicker().middle(true);
        let _ = element.clicker().multi(2);
        let _ = element.clicker().at(Some(5.0), Some(6.0), "left", 1);
        let _ = element.set_file_input_files(&files);
        let _ = element.set_file_input_files("./fixtures/demo.txt");
        let _ = element.set_file_input_files(&upload_path);
        let _ = element.set_file_input_files(borrowed_files);
        let _ = element.clicker().to_upload(&files, Some(1_000), false);
        let _ = element
            .clicker()
            .to_upload("./fixtures/demo.txt", Some(1_000), false);
        let _ = element
            .clicker()
            .to_upload(&upload_path, Some(1_000), false);
        let _ = element
            .clicker()
            .to_download(None, None, None, false, Some(1_000), false, false);
        let _ = element.clicker().for_new_tab(Some(1_000), false);

        let _ = web_element.click_with_options(None, Some(1_000), true);
        let _ = web_element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = web_element.clicker().left();
        let _ = web_element
            .clicker()
            .left_with_options(Some(true), Some(1_000), false);
        let _ = web_element.clicker().right();
        let _ = web_element.clicker().middle(true);
        let _ = web_element.clicker().multi(2);
        let _ = web_element.clicker().at(Some(5.0), Some(6.0), "left", 1);
        let _ = web_element.set_file_input_files(&files);
        let _ = web_element.set_file_input_files("./fixtures/demo.txt");
        let _ = web_element.set_file_input_files(&upload_path);
        let _ = web_element.set_file_input_files(borrowed_files);
        let _ = web_element.clicker().to_upload(&files, Some(1_000), false);
        let _ = web_element
            .clicker()
            .to_upload(&borrowed_files, Some(1_000), false);
        let _ =
            web_element
                .clicker()
                .to_download(None, None, None, false, Some(1_000), false, false);
        let _ = web_element.clicker().for_new_tab(Some(1_000), false);

        let _ = one_element.click_with_options(Some(false), Some(1_000), false);
        let _ = one_element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = one_element.click_at(Some(5.0), Some(6.0), "left", 1);
        let _ = one_element.click_left();
        let _ = one_element.click_middle();
        let _ = one_element.click_multi(2);
        let _ = one_element.click_right();
        let _ = one_element.set_file_input_files(&files);
        let _ = one_element.set_file_input_files("./fixtures/demo.txt");
        let _ = one_element.set_file_input_files(&upload_path);
        let _ = one_web_element.click_with_options(Some(false), Some(1_000), false);
        let _ = one_web_element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = one_web_element.click_at(Some(5.0), Some(6.0), "left", 1);
        let _ = one_web_element.click_left();
        let _ = one_web_element.click_middle();
        let _ = one_web_element.click_multi(2);
        let _ = one_web_element.click_right();
        let _ = one_web_element.set_file_input_files(&files);
        let _ = one_web_element.set_file_input_files("./fixtures/demo.txt");
        let _ = one_web_element.set_file_input_files(&upload_path);

        let _ = owned_element.click_with_options(Some(false), Some(1_000), false);
        let _ = owned_element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = owned_element.click_at(Some(5.0), Some(6.0), "left", 1);
        let _ = owned_element.click_left();
        let _ = owned_element.click_middle();
        let _ = owned_element.click_multi(2);
        let _ = owned_element.click_right();
        let _ = owned_element.set_file_input_files(&files);
        let _ = owned_element.set_file_input_files("./fixtures/demo.txt");
        let _ = owned_element.set_file_input_files(&upload_path);
        let _ = owned_web_element.click_with_options(Some(false), Some(1_000), false);
        let _ = owned_web_element.click_left_with_options(Some(false), Some(1_000), false);
        let _ = owned_web_element.click_at(Some(5.0), Some(6.0), "left", 1);
        let _ = owned_web_element.click_left();
        let _ = owned_web_element.click_middle();
        let _ = owned_web_element.click_multi(2);
        let _ = owned_web_element.click_right();
        let _ = owned_web_element.set_file_input_files(&files);
        let _ = owned_web_element.set_file_input_files("./fixtures/demo.txt");
        let _ = owned_web_element.set_file_input_files(&upload_path);
    }

    let _ = assert_calls
        as fn(
            &Element,
            &WebElement,
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
        );

    fn assert_owned_element_drag_target(element: &Element, target: Element) {
        let _ = element.drag_to(target, 0.1);
    }

    fn assert_owned_element_option_select(element: &Element, option: Element) {
        let _ = element.select().by_option(option);
    }

    fn assert_owned_element_option_cancel(element: &Element, option: Element) {
        let _ = element.select().cancel_by_option(option);
    }

    fn assert_owned_webelement_option_select(element: &WebElement, option: WebElement) {
        let _ = element.select().by_option(option);
    }

    fn assert_owned_webelement_option_cancel(element: &WebElement, option: WebElement) {
        let _ = element.select().cancel_by_option(option);
    }

    let _ = assert_owned_element_drag_target as fn(&Element, Element);
    let _ = assert_owned_element_option_select as fn(&Element, Element);
    let _ = assert_owned_element_option_cancel as fn(&Element, Element);
    let _ = assert_owned_webelement_option_select as fn(&WebElement, WebElement);
    let _ = assert_owned_webelement_option_cancel as fn(&WebElement, WebElement);
}

#[test]
fn element_and_webelement_input_expose_sequence_signatures() {
    fn assert_calls(
        element: &Element,
        web_element: &WebElement,
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
    ) {
        let key_sequence = vec!["Control".to_string(), "a".to_string()];
        let js_args = vec![serde_json::json!("demo")];

        let _ = element.input("hello");
        let _ = element.input(["hello", "world"]);
        let _ = element.input(Keys::CTRL_A);
        let _ = element.input_with_options("hello", true, false);
        let _ = element.input_with_options(["hello", "world"], true, false);
        let _ = element.input_keys_with_options(&key_sequence, true, false);
        let _ = element.input_keys_with_options(Keys::CTRL_A, true, false);

        let _ = web_element.input("hello");
        let _ = web_element.input(["hello", "world"]);
        let _ = web_element.input(Keys::CTRL_A);
        let _ = web_element.input_with_options("hello", true, false);
        let _ = web_element.input_with_options(["hello", "world"], true, false);
        let _ = web_element.input_keys_with_options(&key_sequence, true, false);
        let _ = web_element.input_keys_with_options(Keys::CTRL_A, true, false);

        let _ = one_element.input_with_options("hello", true, false);
        let _ = one_element.input_keys_with_options(&key_sequence, true, false);
        let _ = one_element.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = one_element.press_key("Enter");
        let _ = one_element.clear_with_mode(false);
        let _ = one_element.submit();
        let _ = one_element.hover_with_offset(Some(1.0), Some(2.0));
        let _ = one_element.drag(5.0, 6.0, 0.1);
        let _ = one_element.drag_to(element, 0.1);
        let _ = one_element.drag_to((50.0, 60.0), 0.1);
        let _ = one_element.drag_to_point(50.0, 60.0, 0.1);
        let _ = one_element.set_property("value", &serde_json::json!("demo"));
        let _ = one_element.set_checked(true);
        let _ = one_element.run_js("return this.id;");
        let _ = one_element.run_js_with_args("return arguments[0];", &js_args, false);
        let _ = one_element.run_js_with_options("return arguments[0];", &js_args, false, Some(500));
        let _ = one_element.run_async_js("return this.id;");
        let _ = one_element.run_async_js_with_args("return arguments[0];", &js_args, false);
        let _ = one_element.run_async_js_with_options(
            "return arguments[0];",
            &js_args,
            false,
            Some(500),
        );
        let _ = one_element.save(None, Some("element.jpg"), 500, true);
        let _ = one_element.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = one_element.screenshot_bytes(true, 500);
        let _ = one_element.screenshot_base64(false, 500);
        let _ = one_element.get_screenshot(Some(Path::new("/tmp")), Some("element.png"), true, 500);
        let _ = one_element.save_screenshot("/tmp/element.png");
        let _ = one_web_element.input_with_options("hello", true, false);
        let _ = one_web_element.input_keys_with_options(&key_sequence, true, false);
        let _ = one_web_element.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = one_web_element.press_key("Enter");
        let _ = one_web_element.clear_with_mode(false);
        let _ = one_web_element.submit();
        let _ = one_web_element.hover_with_offset(Some(1.0), Some(2.0));
        let _ = one_web_element.drag(5.0, 6.0, 0.1);
        let _ = one_web_element.drag_to(web_element, 0.1);
        let _ = one_web_element.drag_to((50.0, 60.0), 0.1);
        let _ = one_web_element.drag_to_point(50.0, 60.0, 0.1);
        let _ = one_web_element.set_property("value", &serde_json::json!("demo"));
        let _ = one_web_element.set_checked(true);
        let _ = one_web_element.run_js("return this.id;");
        let _ = one_web_element.run_js_with_args("return arguments[0];", &js_args, false);
        let _ =
            one_web_element.run_js_with_options("return arguments[0];", &js_args, false, Some(500));
        let _ = one_web_element.run_async_js("return this.id;");
        let _ = one_web_element.run_async_js_with_args("return arguments[0];", &js_args, false);
        let _ = one_web_element.run_async_js_with_options(
            "return arguments[0];",
            &js_args,
            false,
            Some(500),
        );
        let _ = one_web_element.save(None, Some("element.jpg"), 500, true);
        let _ = one_web_element.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = one_web_element.screenshot_bytes(true, 500);
        let _ = one_web_element.screenshot_base64(false, 500);
        let _ = one_web_element.get_screenshot(
            Some(Path::new("/tmp")),
            Some("web-element.png"),
            true,
            500,
        );
        let _ = one_web_element.save_screenshot("/tmp/web-element.png");

        let _ = owned_element.input_with_options("hello", true, false);
        let _ = owned_element.input_keys_with_options(&key_sequence, true, false);
        let _ = owned_element.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = owned_element.press_key("Enter");
        let _ = owned_element.clear_with_mode(false);
        let _ = owned_element.submit();
        let _ = owned_element.hover_with_offset(Some(1.0), Some(2.0));
        let _ = owned_element.drag(5.0, 6.0, 0.1);
        let _ = owned_element.drag_to(element, 0.1);
        let _ = owned_element.drag_to((50.0, 60.0), 0.1);
        let _ = owned_element.drag_to_point(50.0, 60.0, 0.1);
        let _ = owned_element.set_property("value", &serde_json::json!("demo"));
        let _ = owned_element.set_checked(true);
        let _ = owned_element.run_js("return this.id;");
        let _ = owned_element.run_js_with_args("return arguments[0];", &js_args, false);
        let _ =
            owned_element.run_js_with_options("return arguments[0];", &js_args, false, Some(500));
        let _ = owned_element.run_async_js("return this.id;");
        let _ = owned_element.run_async_js_with_args("return arguments[0];", &js_args, false);
        let _ = owned_element.run_async_js_with_options(
            "return arguments[0];",
            &js_args,
            false,
            Some(500),
        );
        let _ = owned_element.save(None, Some("element.jpg"), 500, true);
        let _ = owned_element.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = owned_element.screenshot_bytes(true, 500);
        let _ = owned_element.screenshot_base64(false, 500);
        let _ = owned_element.get_screenshot(
            Some(Path::new("/tmp")),
            Some("owned-element.png"),
            true,
            500,
        );
        let _ = owned_element.save_screenshot("/tmp/owned-element.png");
        let _ = owned_web_element.input_with_options("hello", true, false);
        let _ = owned_web_element.input_keys_with_options(&key_sequence, true, false);
        let _ = owned_web_element.input_keys_with_options(Keys::CTRL_A, true, false);
        let _ = owned_web_element.press_key("Enter");
        let _ = owned_web_element.clear_with_mode(false);
        let _ = owned_web_element.submit();
        let _ = owned_web_element.hover_with_offset(Some(1.0), Some(2.0));
        let _ = owned_web_element.drag(5.0, 6.0, 0.1);
        let _ = owned_web_element.drag_to(web_element, 0.1);
        let _ = owned_web_element.drag_to((50.0, 60.0), 0.1);
        let _ = owned_web_element.drag_to_point(50.0, 60.0, 0.1);
        let _ = owned_web_element.set_property("value", &serde_json::json!("demo"));
        let _ = owned_web_element.set_checked(true);
        let _ = owned_web_element.run_js("return this.id;");
        let _ = owned_web_element.run_js_with_args("return arguments[0];", &js_args, false);
        let _ = owned_web_element.run_js_with_options(
            "return arguments[0];",
            &js_args,
            false,
            Some(500),
        );
        let _ = owned_web_element.run_async_js("return this.id;");
        let _ = owned_web_element.run_async_js_with_args("return arguments[0];", &js_args, false);
        let _ = owned_web_element.run_async_js_with_options(
            "return arguments[0];",
            &js_args,
            false,
            Some(500),
        );
        let _ = owned_web_element.save(None, Some("element.jpg"), 500, true);
        let _ = owned_web_element.save(Some(Path::new("/tmp")), None, 500, false);
        let _ = owned_web_element.screenshot_bytes(true, 500);
        let _ = owned_web_element.screenshot_base64(false, 500);
        let _ = owned_web_element.get_screenshot(
            Some(Path::new("/tmp")),
            Some("owned-web-element.png"),
            true,
            500,
        );
        let _ = owned_web_element.save_screenshot("/tmp/owned-web-element.png");
    }

    let _ = assert_calls
        as fn(
            &Element,
            &WebElement,
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
        );
}

#[test]
fn page_webpage_and_session_element_lists_expose_getter_and_filter_signatures() {
    fn assert_calls(
        page_elements: &Vec<Element>,
        web_elements: &Vec<WebElement>,
        session_elements: &Vec<crate::SessionElement>,
    ) {
        let search = crate::ElementsSearch::new()
            .displayed(true)
            .enabled(true)
            .tag("button");

        let _ = page_elements.get().attrs("href");
        let _ = page_elements.get().links();
        let _ = page_elements.get().texts();
        let _ = web_elements.get().attrs("href");
        let _ = web_elements.get().links();
        let _ = web_elements.get().texts();
        let _ = session_elements.get().attrs("href");
        let _ = session_elements.get().links();
        let _ = session_elements.get().texts();

        let _ = page_elements.filter().displayed(true);
        let _ = page_elements.filter().checked(true);
        let _ = page_elements.filter().selected(true);
        let _ = page_elements.filter().enabled(true);
        let _ = page_elements.filter().clickable(true);
        let _ = page_elements.filter().have_rect(true);
        let _ = page_elements
            .filter()
            .attr("href", "https://example.test", true);
        let _ = page_elements.filter().text("demo", true, true);
        let _ = page_elements.filter().tag("button", true);
        let _ = page_elements.filter().style("display", "block", true);
        let _ = page_elements.filter().property("id", "root", true);
        let _ = page_elements.filter().get().texts();
        let _ = page_elements.search(&search);
        let _ = page_elements.search_one(&search);
        let _ = page_elements.search_one_at(2, &search);
        let _ = page_elements.filter_one().displayed(true);
        let _ = page_elements.filter_one().checked(true);
        let _ = page_elements.filter_one().selected(true);
        let _ = page_elements.filter_one_at(2).enabled(true);
        let _ = page_elements.filter_one().clickable(true);
        let _ = page_elements.filter_one().have_rect(true);
        let _ = page_elements
            .filter_one()
            .attr("href", "https://example.test", true);
        let _ = page_elements.filter_one().text("demo", true, true);
        let _ = page_elements.filter_one().tag("button", true);
        let _ = page_elements.filter_one().style("display", "block", true);
        let _ = page_elements.filter_one().property("id", "root", true);
        let _ = page_elements.filter().search(&search);
        let _ = page_elements.filter_one().search(&search);
        let _ = page_elements
            .filter_one()
            .tag("button", true)
            .and_then(|element| element.text());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.is_displayed());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.html());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.inner_html());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.value());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.click());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.input("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.clear());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.focus());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.hover());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.clicker().left());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.clicker().middle(false));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.remove_attr("data-role"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.check(false, true));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.uncheck(true));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set_value("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set_attr("data-role", "demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set_style("display", "block"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set_inner_html("<span>demo</span>"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set().value("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set().attr("data-role", "demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.set().property("tabIndex", &serde_json::json!(3)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_top());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_bottom());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_location(1.0, 2.0));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_up(1.0));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_down(1.0));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_left(1.0));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_right(1.0));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_see(Some(true)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_center());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_top());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_half());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_rightmost());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_text("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_text_with_timeout("demo", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_value("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_value_with_timeout("demo", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_index(1));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_index([1, 2]));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_locator("css:option"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_locator_with_timeout("css:option", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_indices(&[1, 2]));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_by_indices_with_timeout(&[1, 2], Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_text("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_text_with_timeout("demo", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_value("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_value_with_timeout("demo", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_index(1));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_index([1, 2]));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_indices(&[1, 2]));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_indices_with_timeout(&[1, 2], Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_locator("css:option"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_locator_with_timeout("css:option", Some(1_000)));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_clear());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_all());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_invert());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_is_multi());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_options());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_selected_option());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select_selected_options());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select().by_text("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select().cancel_by_value("demo"));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select().is_multi());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.select().selected_options());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.attrs());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.child_count());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.css_path());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.xpath());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.comments());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.states().is_alive());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.states().is_clickable());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().corners());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_corners());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().location());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_location());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().midpoint());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_midpoint());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().click_point());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_click_point());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_location());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_midpoint());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_click_point());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().scroll_position());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.rect().size());
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.wait().displayed(1_000));
        let _ = page_elements
            .search_one(&search)
            .and_then(|element| element.wait().stop_moving(1_000));

        let _ = web_elements.filter().displayed(true);
        let _ = web_elements.filter().checked(true);
        let _ = web_elements.filter().selected(true);
        let _ = web_elements.filter().enabled(true);
        let _ = web_elements.filter().clickable(true);
        let _ = web_elements.filter().have_rect(true);
        let _ = web_elements
            .filter()
            .attr("href", "https://example.test", true);
        let _ = web_elements.filter().text("demo", true, true);
        let _ = web_elements.filter().tag("button", true);
        let _ = web_elements.filter().style("display", "block", true);
        let _ = web_elements.filter().property("id", "root", true);
        let _ = web_elements.filter().get().texts();
        let _ = web_elements.search(&search);
        let _ = web_elements.search_one(&search);
        let _ = web_elements.search_one_at(2, &search);
        let _ = web_elements.filter_one().displayed(true);
        let _ = web_elements.filter_one().checked(true);
        let _ = web_elements.filter_one().selected(true);
        let _ = web_elements.filter_one_at(2).enabled(true);
        let _ = web_elements.filter_one().clickable(true);
        let _ = web_elements.filter_one().have_rect(true);
        let _ = web_elements
            .filter_one()
            .attr("href", "https://example.test", true);
        let _ = web_elements.filter_one().text("demo", true, true);
        let _ = web_elements.filter_one().tag("button", true);
        let _ = web_elements.filter_one().style("display", "block", true);
        let _ = web_elements.filter_one().property("id", "root", true);
        let _ = web_elements.filter().search(&search);
        let _ = web_elements.filter_one().search(&search);
        let _ = web_elements
            .filter_one()
            .tag("button", true)
            .and_then(|element| element.attr("id"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.is_enabled());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.html());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.inner_html());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.value());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.click());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.input("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.clear());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.focus());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.hover());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.clicker().left());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.clicker().middle(false));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.remove_attr("data-role"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.check(false, true));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.uncheck(true));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set_value("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set_attr("data-role", "demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set_style("display", "block"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set_inner_html("<span>demo</span>"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set().value("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set().attr("data-role", "demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.set().property("tabIndex", &serde_json::json!(3)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_top());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_bottom());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_location(1.0, 2.0));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_up(1.0));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_down(1.0));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_left(1.0));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_right(1.0));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_see(Some(true)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll_to_center());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_top());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_half());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.scroll().to_rightmost());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_text("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_text_with_timeout("demo", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_value("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_value_with_timeout("demo", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_index(1));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_index([1, 2]));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_locator("css:option"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_locator_with_timeout("css:option", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_indices(&[1, 2]));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_by_indices_with_timeout(&[1, 2], Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_text("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_text_with_timeout("demo", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_value("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_value_with_timeout("demo", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_index(1));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_index([1, 2]));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_indices(&[1, 2]));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_indices_with_timeout(&[1, 2], Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_locator("css:option"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.cancel_by_locator_with_timeout("css:option", Some(1_000)));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_clear());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_all());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_invert());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_is_multi());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_options());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_selected_option());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select_selected_options());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select().by_text("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select().cancel_by_value("demo"));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select().is_multi());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.select().selected_options());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.attrs());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.child_count());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.css_path());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.xpath());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.comments());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.states().is_alive());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.states().is_clickable());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().corners());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_corners());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().location());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_location());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().midpoint());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_midpoint());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().click_point());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().viewport_click_point());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_location());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_midpoint());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().screen_click_point());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().scroll_position());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.rect().size());
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.wait().displayed(1_000));
        let _ = web_elements
            .search_one(&search)
            .and_then(|element| element.wait().stop_moving(1_000));

        let _ = session_elements
            .filter()
            .attr("href", "https://example.test", true);
        let _ = session_elements.filter().text("demo", true, true);
        let _ = session_elements.filter().tag("a", true);
        let _ = session_elements.filter().get().texts();
        let _ = session_elements
            .filter_one()
            .attr("href", "https://example.test", true);
        let _ = session_elements.filter_one().text("demo", true, true);
        let _ = session_elements.filter_one().tag("a", true);
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.tag());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.html());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.inner_html());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.value());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.attrs());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.child_count());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.css_path());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.xpath());
        let _ = session_elements
            .filter_one()
            .tag("a", true)
            .and_then(|element| element.comments());
    }

    let _ = assert_calls as fn(&Vec<Element>, &Vec<WebElement>, &Vec<crate::SessionElement>);
}

#[test]
fn page_and_webpage_page_operation_signatures_accept_by_tuples_and_element_refs() {
    fn assert_calls(page: &Page, web_page: &WebPage, element: &Element, web_element: &WebElement) {
        let info = [("innerText", "demo"), ("href", "https://example.test")];
        let value_info = [
            ("tabIndex", serde_json::json!(3)),
            ("draggable", serde_json::json!(false)),
        ];
        let files = vec!["/tmp/demo.txt".to_string()];

        let _ = page.remove_element((By::ID, "root"));
        let _ = page.remove_element(element);
        let _ = page.remove_ele((By::ID, "root"));
        let _ = page.remove_ele(element);
        let _ = page.add_element_html(
            "<div>demo</div>",
            Some((By::ID, "root")),
            Some((By::TAG_NAME, "span")),
        );
        let _ = page.add_element("<div>demo</div>", Some(element), Some(element));
        let _ = page.add_element(("a", &info), None::<&str>, None::<&str>);
        let _ = page.add_ele("<div>demo</div>", Some(element), Some(element));
        let _ = page.add_ele(("a", &info), None::<&str>, None::<&str>);
        let _ = page.add_element_html("<div>demo</div>", Some(element), Some(element));
        let _ = page.add_element_info(("a", &info), None::<&str>, None::<&str>);
        let _ = page.add_element_info(("a", &info), Some(element), Some(element));
        let _ = page.add_element_info(("button", &value_info), Some(element), Some(element));
        let _ = page.click_to_download(
            (By::ID, "download"),
            None,
            Some("demo"),
            Some(".txt"),
            true,
            Some(1_000),
            false,
            false,
        );
        let _ = page.click_to_upload((By::ID, "upload"), &files, Some(1_000), false);
        let _ = page.click_for_new_tab((By::ID, "open"), Some(1_000), false);
        let _ = page.click_middle((By::ID, "open"), Some(1_000), true);
        let _ = web_page.remove_element((By::ID, "root"));
        let _ = web_page.remove_element(web_element);
        let _ = web_page.remove_ele((By::ID, "root"));
        let _ = web_page.remove_ele(web_element);
        let _ = web_page.add_element_html(
            "<div>demo</div>",
            Some((By::ID, "root")),
            Some((By::TAG_NAME, "span")),
        );
        let _ = web_page.add_element("<div>demo</div>", Some(web_element), Some(web_element));
        let _ = web_page.add_element(("a", &info), None::<&str>, None::<&str>);
        let _ = web_page.add_ele("<div>demo</div>", Some(web_element), Some(web_element));
        let _ = web_page.add_ele(("a", &info), None::<&str>, None::<&str>);
        let _ = web_page.add_element_html("<div>demo</div>", Some(web_element), Some(web_element));
        let _ = web_page.add_element_info(("a", &info), None::<&str>, None::<&str>);
        let _ = web_page.add_element_info(("a", &info), Some(web_element), Some(web_element));
        let _ = web_page.add_element_info(
            ("button", &value_info),
            Some(web_element),
            Some(web_element),
        );
    }

    let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement);
}

#[test]
fn page_and_webpage_wait_signatures_accept_by_tuples_and_element_refs() {
    fn assert_calls(
        page: &Page,
        web_page: &WebPage,
        element: &Element,
        web_element: &WebElement,
        session_element: &SessionElement,
    ) {
        let locators = vec!["#root".to_string(), ".item".to_string()];
        let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
        let mixed_locators = [
            LocatorInput::from("#root"),
            LocatorInput::from((By::CLASS_NAME, "item")),
        ];
        let session_web_element = WebElement::Session(session_element.clone());

        let _ = page.wait_for((By::ID, "root"), 1_000);
        let _ = page.click("#root");
        let _ = page.click((By::ID, "root"));
        let _ = page.fill("#root", "demo");
        let _ = page.fill((By::ID, "root"), "demo");
        let _ = page.text("#root");
        let _ = page.text((By::ID, "root"));
        let _ = page.attr("#root", "href");
        let _ = page.attr((By::ID, "root"), "href");
        let _ = page.wait_for_elements_loaded((By::ID, "root"), false, 1_000);
        let _ = page.wait_for_elements_loaded(&locators, false, 1_000);
        let _ = page.wait_for_elements_loaded(&tuple_locators, false, 1_000);
        let _ = page.wait_for_elements_loaded(&mixed_locators, false, 1_000);
        let _ = page.wait_for_ele_displayed((By::ID, "root"), 1_000);
        let _ = page.wait_for_ele_hidden((By::ID, "root"), 1_000);
        let _ = page.wait_for_ele_enabled((By::ID, "root"), 1_000);
        let _ = page.wait_for_ele_deleted((By::ID, "root"), 1_000);
        let _ = page.wait_for_ele_clickable((By::ID, "root"), 1_000);
        let _ = page.wait_for_ele_displayed(element, 1_000);
        let _ = page.wait_for_ele_hidden(element, 1_000);
        let _ = page.wait_for_ele_enabled(element, 1_000);
        let _ = page.wait_for_ele_deleted(element, 1_000);
        let _ = page.wait_for_ele_clickable(element, 1_000);
        let _ = page.wait_for_ele_displayed(session_element, 1_000);
        let _ = page.wait_for_ele_hidden(session_element, 1_000);
        let _ = page.wait_for_ele_enabled(session_element, 1_000);
        let _ = page.wait_for_ele_deleted(session_element, 1_000);
        let _ = page.wait_for_ele_clickable(session_element, 1_000);
        let _ = page.wait_for_ele_displayed(&session_web_element, 1_000);
        let _ = page.wait_for_ele_hidden(&session_web_element, 1_000);
        let _ = page.wait_for_ele_enabled(&session_web_element, 1_000);
        let _ = page.wait_for_ele_deleted(&session_web_element, 1_000);
        let _ = page.wait_for_ele_clickable(&session_web_element, 1_000);
        let _ = web_page.wait_for((By::ID, "root"), 1_000);
        let _ = web_page.click("#root");
        let _ = web_page.click((By::ID, "root"));
        let _ = web_page.fill("#root", "demo");
        let _ = web_page.fill((By::ID, "root"), "demo");
        let _ = web_page.text("#root");
        let _ = web_page.text((By::ID, "root"));
        let _ = web_page.attr("#root", "href");
        let _ = web_page.attr((By::ID, "root"), "href");
        let _ = web_page.wait_for_elements_loaded((By::ID, "root"), false, 1_000);
        let _ = web_page.wait_for_elements_loaded(&locators, false, 1_000);
        let _ = web_page.wait_for_elements_loaded(&tuple_locators, false, 1_000);
        let _ = web_page.wait_for_elements_loaded(&mixed_locators, false, 1_000);
        let _ = web_page.wait_for_ele_displayed((By::ID, "root"), 1_000);
        let _ = web_page.wait_for_ele_hidden((By::ID, "root"), 1_000);
        let _ = web_page.wait_for_ele_enabled((By::ID, "root"), 1_000);
        let _ = web_page.wait_for_ele_deleted((By::ID, "root"), 1_000);
        let _ = web_page.wait_for_ele_clickable((By::ID, "root"), 1_000);
        let _ = web_page.wait_for_ele_displayed(web_element, 1_000);
        let _ = web_page.wait_for_ele_hidden(web_element, 1_000);
        let _ = web_page.wait_for_ele_enabled(web_element, 1_000);
        let _ = web_page.wait_for_ele_deleted(web_element, 1_000);
        let _ = web_page.wait_for_ele_clickable(web_element, 1_000);
        let _ = web_page.wait_for_ele_displayed(session_element, 1_000);
        let _ = web_page.wait_for_ele_hidden(session_element, 1_000);
        let _ = web_page.wait_for_ele_enabled(session_element, 1_000);
        let _ = web_page.wait_for_ele_deleted(session_element, 1_000);
        let _ = web_page.wait_for_ele_clickable(session_element, 1_000);
        let _ = web_page.wait_for_ele_displayed(&session_web_element, 1_000);
        let _ = web_page.wait_for_ele_hidden(&session_web_element, 1_000);
        let _ = web_page.wait_for_ele_enabled(&session_web_element, 1_000);
        let _ = web_page.wait_for_ele_deleted(&session_web_element, 1_000);
        let _ = web_page.wait_for_ele_clickable(&session_web_element, 1_000);
    }

    let _ = assert_calls as fn(&Page, &WebPage, &Element, &WebElement, &SessionElement);
}

#[test]
fn page_webpage_and_session_page_set_cookies_accept_supported_inputs() {
    fn assert_calls(page: &Page, web_page: &WebPage, session_page: &Session) {
        let cookie = SessionCookieParam {
            name: "sid".to_string(),
            value: "abc".to_string(),
            url: Some("https://example.test/".to_string()),
            domain: None,
            path: Some("/".to_string()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".to_string()),
        };
        let cookies = vec![cookie.clone()];
        let cookie_json = json!({
            "token": "xyz",
            "domain": ".example.test",
            "path": "/",
            "secure": true,
            "httpOnly": true,
            "sameSite": "Strict"
        });

        let _ = page.set_cookies("sid=abc; domain=.example.test; path=/");
        let _ = page.set_cookies(&cookie);
        let _ = page.set_cookies(&cookies);
        let _ = page.set_cookies(&cookie_json);
        let _ = page.cookie_header();
        let _ = page.set_cookie_header("https://example.test/", "sid=abc");

        let _ = web_page.set_cookies("sid=abc; domain=.example.test; path=/");
        let _ = web_page.set_cookies(&cookie);
        let _ = web_page.set_cookies(&cookies);
        let _ = web_page.set_cookies(&cookie_json);
        let _ = web_page.cookie_header();
        let _ = web_page.set_cookie_header("https://example.test/", "sid=abc");

        let _ = session_page.set_cookies("sid=abc; domain=.example.test; path=/");
        let _ = session_page.set_cookies(&cookie);
        let _ = session_page.set_cookies(&cookies);
        let _ = session_page.set_cookies(&cookie_json);
        let _ = session_page.cookie_header("https://example.test/");
        let _ = session_page.set_cookie_header("https://example.test/", "sid=abc");
    }

    let _ = assert_calls as fn(&Page, &WebPage, &Session);
}

#[test]
fn webpage_cookie_header_wrappers_follow_current_mode() {
    let _settings = scoped_test_settings();
    Settings::reset();

    let (driver_port, driver_server) = spawn_cookie_site();
    let (driver_page, driver_temp_dir) =
        launch_headless_test_webpage("cookie-header-driver", WebMode::Driver)
            .expect("launch driver webpage");
    let driver_result = (|| -> OpenPageResult<()> {
        let url = format!("http://localhost:{driver_port}/");
        driver_page.goto(&url)?;
        assert!(driver_page.wait_for_doc_loaded(5_000)?);

        driver_page.set_cookie_header(&url, "driver_sid=abc")?;
        let cookie_header = driver_page.cookie_header()?.unwrap_or_default();
        assert!(
            cookie_header.contains("driver_sid=abc"),
            "driver cookie header should include driver_sid=abc, got {cookie_header}"
        );
        Ok(())
    })();
    let driver_close_result = driver_page.quit();
    let _ = fs::remove_dir_all(&driver_temp_dir);
    let _ = driver_server.join();
    if let Err(err) = driver_close_result {
        panic!("close driver webpage: {err}");
    }
    driver_result.expect("driver mode cookie header wrapper regression");

    let (session_port, session_server) = spawn_cookie_site();
    let (session_page, session_temp_dir) =
        launch_headless_test_webpage("cookie-header-session", WebMode::Session)
            .expect("launch session webpage");
    let session_result = (|| -> OpenPageResult<()> {
        let url = format!("http://localhost:{session_port}/");
        session_page.get(&url)?;

        session_page.set_cookie_header(&url, "session_sid=abc")?;
        let cookie_header = session_page.cookie_header()?.unwrap_or_default();
        assert!(
            cookie_header.contains("session_sid=abc"),
            "session cookie header should include session_sid=abc, got {cookie_header}"
        );
        Ok(())
    })();
    let session_close_result = session_page.quit();
    let _ = fs::remove_dir_all(&session_temp_dir);
    let _ = session_server.join();
    if let Err(err) = session_close_result {
        panic!("close session webpage: {err}");
    }
    session_result.expect("session mode cookie header wrapper regression");
}

#[test]
fn webpage_session_wait_for_ele_methods_accept_session_element_targets_at_runtime() {
    let (page, temp_dir) =
        launch_headless_test_webpage("session-wait-ele-targets", WebMode::Session)
            .expect("launch headless webpage");
    let html_path = temp_dir.join("session-wait.html");
    let html_path_str = html_path.to_str().expect("html path str");

    let result = (|| -> crate::OpenPageResult<()> {
        write_test_html(
            &html_path,
            r#"
            <html>
              <body>
                <button id="ready">Ready</button>
                <button id="delete-me">Delete me</button>
              </body>
            </html>
            "#,
        )?;
        assert!(page.get(html_path_str)?);

        let ready = page.snapshot_find("#ready")?;
        let ready_web = page.find("#ready")?;
        let delete_me = page.snapshot_find("#delete-me")?;
        let delete_me_web = page.find("#delete-me")?;

        assert!(page.wait_for_ele_displayed(&ready, 1_000)?);
        assert!(page.wait_for_ele_enabled(&ready, 1_000)?);
        assert!(page.wait_for_ele_clickable(&ready, 1_000)?);
        assert!(page.wait_for_ele_displayed(&ready_web, 1_000)?);
        assert!(page.wait_for_ele_enabled(&ready_web, 1_000)?);
        assert!(page.wait_for_ele_clickable(&ready_web, 1_000)?);

        write_test_html(
            &html_path,
            r#"
            <html>
              <body>
                <button id="ready">Ready</button>
              </body>
            </html>
            "#,
        )?;
        assert!(page.get(html_path_str)?);

        assert!(page.wait_for_ele_hidden(&delete_me, 1_000)?);
        assert!(page.wait_for_ele_deleted(&delete_me, 1_000)?);
        assert!(page.wait_for_ele_hidden(&delete_me_web, 1_000)?);
        assert!(page.wait_for_ele_deleted(&delete_me_web, 1_000)?);
        Ok(())
    })();

    let close_result = page.quit();
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(err) = close_result {
        panic!("close headless webpage: {err}");
    }
    result.expect("session element target wait regression");
}

#[test]
fn element_shadow_root_and_webelement_find_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
        let _ = element.find((By::ID, "root"));
        let _ = element.find_all((By::CLASS_NAME, "item"));
        let _ = shadow_root.find((By::ID, "root"));
        let _ = shadow_root.find_all((By::CLASS_NAME, "item"));
        let _ = web_element.find((By::ID, "root"));
        let _ = web_element.find_all((By::CLASS_NAME, "item"));
    }

    let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
}

#[test]
fn elements_one_find_alias_signatures_accept_by_tuples() {
    fn assert_calls(
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        one_session_element: ElementsOne<'_, SessionElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
        owned_session_element: &ElementsOneOwned<SessionElement>,
    ) {
        let _ = one_element.find((By::ID, "root"));
        let _ = one_element.find_all((By::CLASS_NAME, "item"));
        let _ = one_web_element.find((By::ID, "root"));
        let _ = one_web_element.find_all((By::CLASS_NAME, "item"));
        let _ = one_session_element.find((By::ID, "root"));
        let _ = one_session_element.find_by(By::ID, "root");
        let _ = one_session_element.find_all((By::CLASS_NAME, "item"));
        let _ = one_session_element.find_all_by(By::CLASS_NAME, "item");
        let _ = one_session_element.query_xpath(".//div");
        let _ = owned_element.find((By::ID, "root"));
        let _ = owned_element.find_all((By::CLASS_NAME, "item"));
        let _ = owned_web_element.find((By::ID, "root"));
        let _ = owned_web_element.find_all((By::CLASS_NAME, "item"));
        let _ = owned_session_element.find((By::ID, "root"));
        let _ = owned_session_element.find_by(By::ID, "root");
        let _ = owned_session_element.find_all((By::CLASS_NAME, "item"));
        let _ = owned_session_element.find_all_by(By::CLASS_NAME, "item");
        let _ = owned_session_element.query_xpath(".//div");
    }

    let _ = assert_calls
        as fn(
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            ElementsOne<'_, SessionElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
            &ElementsOneOwned<SessionElement>,
        );
}

#[test]
fn elements_one_snapshot_find_signatures_accept_by_tuples() {
    fn assert_calls(
        one_element: ElementsOne<'_, Element>,
        one_web_element: ElementsOne<'_, WebElement>,
        owned_element: &ElementsOneOwned<Element>,
        owned_web_element: &ElementsOneOwned<WebElement>,
    ) {
        let _ = one_element.snapshot_find((By::ID, "root"));
        let _ = one_element.snapshot_find_by(By::ID, "root");
        let _ = one_element.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = one_element.snapshot_find_all_by(By::CLASS_NAME, "item");
        let _ = one_element.snapshot_root();
        let _ = one_element.snapshot_query_xpath(".//div");
        let _ = one_web_element.snapshot_find((By::ID, "root"));
        let _ = one_web_element.snapshot_find_by(By::ID, "root");
        let _ = one_web_element.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = one_web_element.snapshot_find_all_by(By::CLASS_NAME, "item");
        let _ = one_web_element.snapshot_root();
        let _ = one_web_element.snapshot_query_xpath(".//div");
        let _ = owned_element.snapshot_find((By::ID, "root"));
        let _ = owned_element.snapshot_find_by(By::ID, "root");
        let _ = owned_element.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = owned_element.snapshot_find_all_by(By::CLASS_NAME, "item");
        let _ = owned_element.snapshot_root();
        let _ = owned_element.snapshot_query_xpath(".//div");
        let _ = owned_web_element.snapshot_find((By::ID, "root"));
        let _ = owned_web_element.snapshot_find_by(By::ID, "root");
        let _ = owned_web_element.snapshot_find_all((By::CLASS_NAME, "item"));
        let _ = owned_web_element.snapshot_find_all_by(By::CLASS_NAME, "item");
        let _ = owned_web_element.snapshot_root();
        let _ = owned_web_element.snapshot_query_xpath(".//div");
    }

    let _ = assert_calls
        as fn(
            ElementsOne<'_, Element>,
            ElementsOne<'_, WebElement>,
            &ElementsOneOwned<Element>,
            &ElementsOneOwned<WebElement>,
        );
}

#[test]
fn element_shadow_root_and_webelement_parent_child_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
        let _ = element.parent_with((By::ID, "root"), 1);
        let _ = element.child_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = element.children_with(Some((By::CLASS_NAME, "item")));
        let _ = shadow_root.parent_with((By::ID, "root"), 1);
        let _ = shadow_root.child_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = shadow_root.children_with(Some((By::CLASS_NAME, "item")));
        let _ = web_element.parent_with((By::ID, "root"), 1);
        let _ = web_element.child_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_element.children_with(Some((By::CLASS_NAME, "item")));
    }

    let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
}

#[test]
fn element_shadow_root_and_webelement_prev_next_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
        let _ = element.prev_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = element.prevs_with(Some((By::CLASS_NAME, "item")));
        let _ = element.next_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = element.nexts_with(Some((By::CLASS_NAME, "item")));
        let _ = shadow_root.next_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = shadow_root.nexts_with(Some((By::CLASS_NAME, "item")));
        let _ = web_element.prev_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_element.prevs_with(Some((By::CLASS_NAME, "item")));
        let _ = web_element.next_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_element.nexts_with(Some((By::CLASS_NAME, "item")));
    }

    let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
}

#[test]
fn element_shadow_root_and_webelement_before_after_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, shadow_root: &ShadowRoot, web_element: &WebElement) {
        let _ = element.before_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = element.befores_with(Some((By::CLASS_NAME, "item")));
        let _ = element.after_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = element.afters_with(Some((By::CLASS_NAME, "item")));
        let _ = shadow_root.before_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = shadow_root.befores_with(Some((By::CLASS_NAME, "item")));
        let _ = shadow_root.after_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = shadow_root.afters_with(Some((By::CLASS_NAME, "item")));
        let _ = web_element.before_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_element.befores_with(Some((By::CLASS_NAME, "item")));
        let _ = web_element.after_with(Some((By::CLASS_NAME, "item")), 1);
        let _ = web_element.afters_with(Some((By::CLASS_NAME, "item")));
    }

    let _ = assert_calls as fn(&Element, &ShadowRoot, &WebElement);
}

#[test]
fn element_and_webelement_offset_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, web_element: &WebElement) {
        let _ = element.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
        let _ = web_element.offset(Some((By::CLASS_NAME, "item")), Some(1.0), Some(2.0), 100);
    }

    let _ = assert_calls as fn(&Element, &WebElement);
}

#[test]
fn element_and_webelement_visual_direction_signatures_accept_by_tuples() {
    fn assert_calls(element: &Element, web_element: &WebElement) {
        let _ = element.east(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = element.south(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = element.west(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = element.north(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = web_element.east(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = web_element.south(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = web_element.west(Some((By::CLASS_NAME, "item")), None, 1);
        let _ = web_element.north(Some((By::CLASS_NAME, "item")), None, 1);
    }

    let _ = assert_calls as fn(&Element, &WebElement);
}
