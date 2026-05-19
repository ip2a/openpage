from __future__ import annotations

import json
import tempfile
import threading
import time
import unittest
from contextlib import contextmanager
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote

from openpage import Browser
from openpage import ChromiumOptions
from openpage import ChromiumPage
from openpage import DownloadMission
from openpage import SessionPage
from openpage import SessionOptions
from openpage import WebPage


HTML = """
<!doctype html>
<html>
<body>
  <h1>OpenPage</h1>
  <input id="name" value="" />
  <button id="submit" onclick="document.getElementById('out').textContent=document.getElementById('name').value">Go</button>
  <div id="out"></div>
  <div class="item">a</div>
  <div class="item">b</div>
</body>
</html>
"""

DOWNLOAD_HTML = """
<!doctype html>
<html>
<body>
  <a id="download" href="data:text/plain;charset=utf-8,openpage-download" download="openpage.txt">Download</a>
</body>
</html>
"""

HTTP_DOWNLOAD_HTML = """
<!doctype html>
<html>
<body>
  <a id="download" href="/download">Download</a>
</body>
</html>
"""

LISTENER_HTML = """
<!doctype html>
<html>
<body>
  <button id="trigger" onclick='fetch("/api/data", {method: "POST", headers: {"Content-Type": "application/json", "X-OpenPage-Request": "enabled"}, body: JSON.stringify({name: "openpage"})}).then(r => r.json()).then(() => { document.getElementById("out").textContent = "done"; })'>Send</button>
  <div id="out"></div>
</body>
</html>
"""


def data_url() -> str:
    return "data:text/html," + quote(HTML)


def download_data_url() -> str:
    return "data:text/html," + quote(DOWNLOAD_HTML)


@contextmanager
def serve_listener_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path != "/":
                self.send_error(HTTPStatus.NOT_FOUND)
                return

            payload = LISTENER_HTML.encode("utf-8")
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            if self.path != "/api/data":
                self.send_error(HTTPStatus.NOT_FOUND)
                return

            length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(length).decode("utf-8")
            payload = json.dumps({"received": body}).encode("utf-8")
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "application/json")
            self.send_header("X-OpenPage-Response", "enabled")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format: str, *args: object) -> None:
            del format, args

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


@contextmanager
def serve_download_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = HTTP_DOWNLOAD_HTML.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/download":
                payload = b"openpage-download"
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/plain")
                self.send_header("Content-Disposition", 'attachment; filename="openpage.txt"')
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            self.send_error(HTTPStatus.NOT_FOUND)

        def log_message(self, format: str, *args: object) -> None:
            del format, args

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


@contextmanager
def serve_load_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = b"""<!doctype html><html><body><h1>Start</h1></body></html>"""
            elif self.path == "/slow":
                time.sleep(0.3)
                payload = b"""<!doctype html><html><head><title>Slow Page</title></head><body><h1>Slow</h1></body></html>"""
            else:
                self.send_error(HTTPStatus.NOT_FOUND)
                return

            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format: str, *args: object) -> None:
            del format, args

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def assert_get_ok(page: SessionPage | WebPage, url: str, attempts: int = 3) -> None:
    last_status: int | str | None = None
    for attempt in range(attempts):
        try:
            if page.get(url):
                return
        except RuntimeError as err:
            last_status = str(err)
        else:
            last_status = getattr(page, "status_code", None)
        if attempt + 1 < attempts:
            time.sleep(1.0)
    raise AssertionError(f"GET {url} failed after {attempts} attempts, last status={last_status}")


class OpenPageIntegrationTest(unittest.TestCase):
    def test_browser_and_page_flow(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            self.assertEqual(page.title, "")
            self.assertTrue(page.user_agent)
            self.assertEqual(page.ele("h1").text, "OpenPage")
            self.assertEqual(page.s_ele("h1").text, "OpenPage")
            root = page.s_ele()
            self.assertEqual(root.tag, "html")
            snapshot = page.s_ele("body")
            self.assertEqual(snapshot.ele("h1").text, "OpenPage")
            self.assertEqual(snapshot.children()[0].tag, "h1")
            self.assertEqual(snapshot.child(index=2).attr("id"), "name")
            self.assertEqual(page.s_ele("#name").attrs["id"], "name")
            self.assertEqual(page.s_ele("#submit").raw_text, "Go")
            self.assertEqual([item.text for item in snapshot.eles(".item")], ["a", "b"])
            self.assertEqual(page.s_ele("#submit").next().attr("id"), "out")
            self.assertEqual(page.s_ele("#out").prev().attr("id"), "submit")
            self.assertEqual(page.s_ele("#name").parent().tag, "body")
            self.assertEqual([item.attr("id") for item in page.s_ele("#submit").prevs("[id]")], ["name"])
            self.assertEqual(page.s_ele("#submit").before("[id]").attr("id"), "name")
            self.assertEqual(page.s_ele("#submit").after(index=2).text, "a")
            self.assertEqual([item.text for item in page.s_ele("#submit").afters(".item")], ["a", "b"])
            self.assertEqual(len(page.s_eles(".item")), 2)
            page.ele("#name").input("openpage")
            page.ele("@id=submit").click()
            self.assertEqual(page.ele("@id=out").text, "openpage")
            self.assertEqual(len(page.eles(".item")), 2)
            js = page.run_js("({count: document.querySelectorAll('.item').length})")
            self.assertEqual(js["count"], 2)
            self.assertGreaterEqual(page.tabs_count, 1)
            self.assertIn(page.tab_id, page.tab_ids)
        finally:
            page.quit()

    def test_browser_level_api_and_screenshot(self) -> None:
        browser = Browser.launch(ChromiumOptions())
        try:
            self.assertTrue(browser.states.is_alive)
            self.assertTrue(browser.states.is_headless)
            page = browser.new_page(data_url())
            self.assertGreaterEqual(browser.tabs_count, 1)
            self.assertIn(page.tab_id, browser.tab_ids)
            with tempfile.TemporaryDirectory() as tmp_dir:
                shot = Path(tmp_dir) / "page.png"
                page.save_screenshot(str(shot))
                self.assertTrue(shot.exists())
                self.assertGreater(shot.stat().st_size, 0)
        finally:
            browser.close()

    def test_browser_waits_for_new_tab(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            current_tab = page.tab_id
            new_tab_url = "data:text/html," + quote("<h1>new-tab</h1>")
            thread = threading.Thread(
                target=lambda: (time.sleep(0.2), page.browser.new_page(new_tab_url)),
                daemon=True,
            )
            thread.start()
            new_tab = page.browser.wait.new_tab(timeout=2.0, curr_tab=current_tab)
            thread.join(timeout=5)
            self.assertNotEqual(new_tab, False)
            assert isinstance(new_tab, str)
            new_page = page.get_tab(new_tab)
            self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
            self.assertIn("new-tab", new_page.html)
        finally:
            page.quit()

    def test_page_wait_and_element_states(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            self.assertEqual(page.states.ready_state, "complete")
            self.assertTrue(page.states.is_alive)
            self.assertFalse(page.states.is_loading)

            name = page.ele("#name")
            submit = page.ele("#submit")
            self.assertFalse(name.states.is_selected)
            self.assertFalse(name.states.is_checked)
            self.assertTrue(submit.states.is_displayed)
            self.assertTrue(submit.states.is_enabled)
            self.assertTrue(submit.states.has_rect)
            self.assertTrue(submit.states.is_in_viewport)
            self.assertTrue(submit.states.is_clickable)
            self.assertTrue(page.wait.eles_loaded(["#submit", "#name"], timeout=1.0))
            self.assertIsNot(page.wait.ele_displayed("#submit", timeout=1.0), False)
            self.assertIsNot(page.wait.ele_enabled("#submit", timeout=1.0), False)
            self.assertIsNot(page.wait.ele_clickable("#submit", timeout=1.0), False)
            self.assertIs(submit.wait.displayed(timeout=1.0), submit)
            self.assertIs(submit.wait.clickable(timeout=1.0), submit)
            self.assertIs(submit.wait.has_rect(timeout=1.0), submit)

            page.run_js(
                """
                const button = document.getElementById('submit');
                button.disabled = true;
                setTimeout(() => { button.disabled = false; }, 150);
                """
            )
            self.assertIsNot(submit.wait.disabled(timeout=1.0), False)
            self.assertIsNot(submit.wait.disabled_or_deleted(timeout=1.0), False)
            self.assertIsNot(submit.wait.enabled(timeout=2.0), False)
            self.assertIsNot(page.wait.ele_enabled("#submit", timeout=2.0), False)

            page.run_js(
                """
                const temp = document.createElement('div');
                temp.id = 'temp';
                temp.textContent = 'temp';
                document.body.appendChild(temp);
                setTimeout(() => { temp.style.display = 'none'; }, 150);
                """
            )
            temp = page.ele("#temp")
            self.assertTrue(temp.states.is_displayed)
            self.assertIsNot(page.wait.ele_hidden(temp, timeout=2.0), False)

            page.run_js(
                """
                setTimeout(() => {
                    const later = document.createElement('div');
                    later.id = 'later';
                    later.textContent = 'later';
                    document.body.appendChild(later);
                }, 150);
                """
            )
            self.assertTrue(page.wait.eles_loaded(["#missing", "#later"], timeout=2.0, any_one=True))

            page.run_js(
                """
                const doomed = document.createElement('div');
                doomed.id = 'doomed';
                doomed.textContent = 'doomed';
                document.body.appendChild(doomed);
                setTimeout(() => { doomed.remove(); }, 150);
                """
            )
            doomed = page.ele("#doomed")
            self.assertTrue(doomed.states.is_alive)
            self.assertIsNot(page.wait.ele_deleted("#doomed", timeout=2.0), False)
            self.assertIsNot(doomed.wait.deleted(timeout=2.0), False)
            self.assertFalse(doomed.states.is_alive)
        finally:
            page.quit()

    def test_page_wait_detects_load_start_and_doc_loaded(self) -> None:
        with serve_load_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            try:
                self.assertTrue(page.get(base_url))
                page.run_js(f"setTimeout(() => {{ location.href = {base_url!r} + 'slow'; }}, 0)")
                self.assertTrue(page.wait.load_start(timeout=2.0))
                self.assertTrue(page.wait.doc_loaded(timeout=3.0))
                self.assertEqual(page.title, "Slow Page")
            finally:
                page.quit()

    def test_download_path_supports_file_downloads(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            target = Path(tmp_dir) / "openpage.txt"
            try:
                self.assertEqual(page.download_path, tmp_dir)
                self.assertTrue(page.get(download_data_url()))
                page.ele("#download").click()
                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))
                self.assertTrue(target.exists())
                self.assertEqual(target.read_text(), "openpage-download")
            finally:
                page.quit()

    def test_download_missions_track_http_downloads(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            target = Path(tmp_dir) / "openpage.txt"
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))
                mission = page.last_download()
                self.assertIsNotNone(mission)
                assert mission is not None
                self.assertEqual(mission.suggested_filename, "openpage.txt")
                self.assertEqual(mission.state, "completed")
                self.assertTrue(mission.is_done)
                self.assertGreaterEqual(mission.received_bytes, len(b"openpage-download"))
                self.assertEqual(mission.wait(timeout=5.0), str(target))
                self.assertEqual(mission.final_path, str(target))

                missions = page.download_missions()
                self.assertGreaterEqual(len(missions), 1)
                self.assertEqual(missions[-1].guid, mission.guid)
            finally:
                page.quit()

    def test_browser_waits_for_download_begin_and_completion(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                mission = page.browser.wait.download_begin(timeout=5.0)
                self.assertNotEqual(mission, False)
                assert isinstance(mission, DownloadMission)
                self.assertEqual(mission.suggested_filename, "openpage.txt")
                self.assertTrue(page.browser.wait.downloads_done(timeout=10.0))
            finally:
                page.quit()

    def test_browser_listener_captures_completed_post_request(self) -> None:
        with serve_listener_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            listener = page.listen
            try:
                listener.start(targets="/api/data", method="POST")
                self.assertTrue(page.get(base_url))
                page.ele("#trigger").click()

                packet = listener.wait(timeout=5.0)
                self.assertEqual(packet.method, "POST")
                self.assertTrue(packet.url.endswith("/api/data"))
                self.assertIn(packet.resource_type, {"Fetch", "XHR"})
                self.assertFalse(packet.is_failed)
                self.assertIsNone(packet.fail_info)

                response = packet.response
                self.assertIsNotNone(response)
                assert response is not None
                self.assertEqual(response.status, 200)
                self.assertEqual(response.mime_type, "application/json")
                self.assertFalse(response.body_base64)
                self.assertIsNotNone(response.body)
                assert response.body is not None
                self.assertIn('"received"', response.body)

                content_type = (
                    packet.request.headers.get("Content-Type")
                    or packet.request.headers.get("content-type")
                )
                self.assertEqual(content_type, "application/json")

                request_extra_info = packet.request.extra_info
                if request_extra_info is not None:
                    self.assertEqual(
                        request_extra_info.headers.get("X-OpenPage-Request"),
                        "enabled",
                    )

                response_extra_info = response.extra_info
                self.assertIsNotNone(response_extra_info)
                assert response_extra_info is not None
                self.assertEqual(response_extra_info.status_code, 200)
                self.assertEqual(
                    response_extra_info.headers.get("X-OpenPage-Response"),
                    "enabled",
                )
                self.assertIsNotNone(response_extra_info.headers_text)
            finally:
                listener.stop()
                page.quit()

    def test_page_wait_detects_title_and_url_changes(self) -> None:
        with serve_listener_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            try:
                self.assertTrue(page.get(base_url))
                page.run_js(
                    """
                    setTimeout(() => {
                        document.title = 'openpage changed';
                        history.replaceState({}, '', '/changed');
                    }, 150);
                    """
                )
                self.assertIs(page.wait.title_change("openpage changed", timeout=2.0), page)
                self.assertIs(page.wait.url_change("/changed", timeout=2.0), page)
            finally:
                page.quit()

    def test_session_page_flow(self) -> None:
        options = SessionOptions().set_user_agent("openpage-test-agent")
        page = SessionPage(options)
        self.assertTrue(page.get("https://example.com"))
        self.assertEqual(page.title, "Example Domain")
        self.assertEqual(page.ele("h1").text, "Example Domain")
        self.assertEqual(page.s_ele("h1").text, "Example Domain")
        self.assertEqual(page.s_ele().tag, "html")
        self.assertEqual(page.s_ele("body").ele("h1").text, "Example Domain")
        self.assertEqual(page.s_ele("h1").parent().tag, "div")
        self.assertEqual(page.s_ele("h1").parent().parent().tag, "body")
        self.assertEqual(page.s_ele("h1").raw_text, "Example Domain")
        self.assertEqual(page.user_agent, "openpage-test-agent")
        self.assertEqual(page.status_code, 200)
        self.assertIn(b"Example Domain", page.raw_data)
        self.assertEqual(page.encoding, "utf-8")

        assert_get_ok(page, "https://httpbin.org/json")
        self.assertIn("slideshow", page.json)
        self.assertIn(b"slideshow", page.raw_data)
        self.assertEqual(page.encoding, "utf-8")

    def test_webpage_mode_switch_and_cookie_sync(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir),
            )
            try:
                self.assertEqual(page.mode, "d")
                self.assertEqual(page.download_path, tmp_dir)
                assert_get_ok(page, "https://httpbin.org/cookies/set?token=browser")
                assert_get_ok(page, "https://httpbin.org/cookies")
                page.change_mode("s", go=True, copy_cookies=True)
                self.assertEqual(page.mode, "s")
                self.assertEqual(page.status_code, 200)
                self.assertTrue(page.user_agent)
                self.assertEqual(page.json["cookies"]["token"], "browser")
                self.assertIn({"name": "token", "value": "browser", "domain": "httpbin.org"}, page.cookies())
                self.assertIn(b"browser", page.raw_data)
                self.assertEqual(page.encoding, "utf-8")

                assert_get_ok(page, "https://httpbin.org/cookies/set?token=session")
                assert_get_ok(page, "https://httpbin.org/cookies")
                page.change_mode("d", go=True, copy_cookies=True)
                self.assertEqual(page.mode, "d")
                self.assertIsNone(page.status_code)
                self.assertTrue(page.user_agent)
                self.assertIn('"token": "session"', page.ele("body").text or "")
                self.assertIn({"name": "token", "value": "session", "domain": "httpbin.org"}, page.cookies())
                self.assertEqual(page.raw_data, b"")
                self.assertIsNone(page.encoding)
            finally:
                page.quit()

    def test_webpage_wait_and_states_in_driver_mode(self) -> None:
        with serve_load_site() as base_url:
            page = WebPage(mode="d")
            try:
                self.assertTrue(page.get(base_url))
                self.assertTrue(page.states.is_alive)
                self.assertTrue(page.states.is_headless)
                self.assertEqual(page.states.ready_state, "complete")
                self.assertFalse(page.states.is_loading)
                self.assertTrue(page.wait.eles_loaded(["h1"], timeout=1.0))
                self.assertIsNot(page.wait.ele_displayed("h1", timeout=1.0), False)
                self.assertIsNot(page.wait.ele_enabled("h1", timeout=1.0), False)
                self.assertIsNot(page.wait.ele_clickable("h1", timeout=1.0), False)
                self.assertFalse(page.wait.ele_deleted("#missing", timeout=0.1))

                page.run_js(
                    """
                    setTimeout(() => {
                        document.title = 'openpage changed';
                        history.replaceState({}, '', '/changed');
                    }, 150);
                    """
                )
                self.assertIs(page.wait.title_change("openpage changed", timeout=2.0), page)
                self.assertIs(page.wait.url_change("/changed", timeout=2.0), page)

                page.run_js(f"setTimeout(() => {{ location.href = {base_url!r} + 'slow'; }}, 0)")
                self.assertTrue(page.wait.load_start(timeout=2.0))
                self.assertTrue(page.wait.doc_loaded(timeout=3.0))
                self.assertEqual(page.title, "Slow Page")
                self.assertIn("Slow", page.html)
            finally:
                page.quit()

    def test_webpage_wait_and_states_in_session_mode(self) -> None:
        with serve_load_site() as base_url:
            page = WebPage(mode="d")
            try:
                self.assertTrue(page.get(base_url))
                page.change_mode("s", go=True, copy_cookies=False)
                self.assertEqual(page.mode, "s")
                self.assertTrue(page.states.is_alive)
                self.assertTrue(page.states.is_headless)
                self.assertIsNone(page.states.ready_state)
                self.assertFalse(page.states.is_loading)
                self.assertFalse(page.wait.load_start(timeout=0.1))
                self.assertTrue(page.wait.doc_loaded(timeout=0.1))
                self.assertTrue(page.wait.eles_loaded(["h1"], timeout=1.0))
                self.assertTrue(page.wait.eles_loaded(["#missing", "h1"], timeout=1.0, any_one=True))
                self.assertFalse(page.wait.eles_loaded(["#missing"], timeout=0.1))
                self.assertIsNot(page.wait.ele_displayed("h1", timeout=1.0), False)
                self.assertIsNot(page.wait.ele_enabled("h1", timeout=1.0), False)
                self.assertFalse(page.wait.ele_hidden("h1", timeout=0.1))
                self.assertFalse(page.wait.ele_deleted("#missing", timeout=0.1))
                self.assertEqual(page.ele("h1").text, "Start")
            finally:
                page.quit()

    def test_webpage_listener_uses_driver_page(self) -> None:
        with serve_listener_site() as base_url:
            page = WebPage(mode="d")
            listener = page.listen
            try:
                listener.start(targets="/api/data", method="POST")
                self.assertTrue(page.get(base_url))
                page.ele("#trigger").click()

                packet = listener.wait(timeout=5.0)
                self.assertEqual(packet.method, "POST")
                self.assertTrue(packet.url.endswith("/api/data"))
                self.assertFalse(packet.is_failed)

                response = packet.response
                self.assertIsNotNone(response)
                assert response is not None
                self.assertEqual(response.status, 200)
            finally:
                listener.stop()
                page.quit()

    def test_webpage_download_missions_use_browser_core(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir),
            )
            target = Path(tmp_dir) / "openpage.txt"
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))

                mission = page.last_download()
                self.assertIsNotNone(mission)
                assert mission is not None
                self.assertEqual(mission.state, "completed")
                self.assertEqual(mission.final_path, str(target))

                missions = page.download_missions()
                self.assertGreaterEqual(len(missions), 1)
                self.assertEqual(missions[-1].guid, mission.guid)
            finally:
                page.quit()

    def test_webpage_waits_for_download_begin_and_completion(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir),
            )
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                mission = page.wait.download_begin(timeout=5.0)
                self.assertNotEqual(mission, False)
                assert isinstance(mission, DownloadMission)
                self.assertEqual(mission.suggested_filename, "openpage.txt")
                self.assertTrue(page.wait.all_downloads_done(timeout=10.0))
            finally:
                page.quit()


if __name__ == "__main__":
    unittest.main()
