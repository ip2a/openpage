from __future__ import annotations

import json
import platform
import subprocess
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
from openpage import Keys
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

LISTENER_MULTI_HTML = """
<!doctype html>
<html>
<body>
  <button id="trigger" onclick='Promise.all([
    fetch("/api/one"),
    fetch("/api/two")
  ]).then(() => { document.getElementById("out").textContent = "done"; })'>Send</button>
  <div id="out"></div>
</body>
</html>
"""

INTERCEPT_HTML = """
<!doctype html>
<html>
<body>
  <button id="fetch" onclick='fetch("/api/data").then(async r => { document.getElementById("out").textContent = `${r.status}:${await r.text()}`; }).catch(() => { document.getElementById("out").textContent = "failed"; })'>Fetch</button>
  <button id="rewrite" onclick='fetch("/api/original").then(async r => { document.getElementById("out").textContent = await r.text(); }).catch(() => { document.getElementById("out").textContent = "failed"; })'>Rewrite</button>
  <div id="out"></div>
</body>
</html>
"""

BLOCKED_URL_HTML = """
<!doctype html>
<html>
<head>
  <link rel="stylesheet" href="/style.css">
</head>
<body>
  <h1 id="title">Blocked URL Test</h1>
</body>
</html>
"""

BLOCKED_URL_CSS = """
#title {
  color: rgb(255, 0, 0);
}
"""

HEADER_ECHO_HTML = """
<!doctype html>
<html>
<body>
  <div id="header">{header_value}</div>
</body>
</html>
"""

UPLOAD_HTML = """
<!doctype html>
<html>
<body>
  <input id="picker" type="file" multiple onchange='document.getElementById("out").textContent = Array.from(this.files).map(f => f.name).join(",")' />
  <div id="out"></div>
</body>
</html>
"""

LOAD_MODE_HTML = """
<!doctype html>
<html>
<head><title>Load Mode</title></head>
<body>
  <div id="status">dom-ready</div>
  <img id="slow" src="/slow-image" onload="document.body.dataset.loaded='yes'; document.getElementById('status').textContent='img-loaded';" />
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
def serve_multi_listener_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = LISTENER_MULTI_HTML.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/api/one":
                time.sleep(0.2)
                payload = json.dumps({"name": "one"}).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/api/two":
                time.sleep(0.4)
                payload = json.dumps({"name": "two"}).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json")
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
def serve_new_tab_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path in {"/", "/new-tab", "/middle-tab"}:
                heading = {"/": "root", "/new-tab": "new-tab", "/middle-tab": "middle-tab"}[self.path]
                payload = f"<html><body><h1>{heading}</h1></body></html>".encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
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
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


@contextmanager
def serve_intercept_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = INTERCEPT_HTML.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/api/data":
                payload = b"server-data"
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/api/original":
                payload = b"original"
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/api/rewritten":
                payload = b"rewritten"
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
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
def serve_header_echo_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            header_value = self.headers.get("X-OpenPage-Test", "")
            payload = HEADER_ECHO_HTML.format(header_value=header_value).encode("utf-8")
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
def serve_upload_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path != "/":
                self.send_error(HTTPStatus.NOT_FOUND)
                return

            payload = UPLOAD_HTML.encode("utf-8")
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


@contextmanager
def serve_load_mode_site():
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = LOAD_MODE_HTML.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/slow-image":
                time.sleep(1.2)
                payload = (
                    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00"
                    b"\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00"
                    b"\x01\x00\x01\x00\x00\x02\x02D\x01\x00;"
                )
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "image/gif")
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


@contextmanager
def serve_blocked_url_site():
    hits = {"style": 0}

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/":
                payload = BLOCKED_URL_HTML.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            if self.path == "/style.css":
                hits["style"] += 1
                payload = BLOCKED_URL_CSS.encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/css; charset=utf-8")
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
    base_url = f"http://127.0.0.1:{server.server_port}/"
    try:
        yield base_url, hits
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


def wait_for_next_download(
    page: ChromiumPage | WebPage,
    previous_count: int,
    previous_guid: str | None = None,
    timeout: float = 10.0,
) -> DownloadMission:
    deadline = time.time() + timeout
    while time.time() < deadline:
        missions = page.download_missions()
        mission = page.last_download()
        if (
            len(missions) > previous_count
            and mission is not None
            and mission.guid != previous_guid
        ):
            return mission
        time.sleep(0.1)
    raise AssertionError("download did not begin in time")


def wait_for_output_text(page: ChromiumPage | WebPage, expected: str, timeout: float = 5.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            text = page.ele("#out").text
        except Exception:
            text = None
        if text == expected:
            return
        time.sleep(0.1)
    raise AssertionError(f"output did not become {expected!r} in time")


def wait_for_condition(check, timeout: float = 5.0, message: str = "condition did not become true") -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if check():
            return
        time.sleep(0.05)
    raise AssertionError(message)


def macos_app_property(pid: int, property_name: str) -> bool | None:
    if platform.system() != "Darwin":
        return None
    script = (
        'tell application "System Events" '
        f'to get {property_name} of first application process whose unix id is {pid}'
    )
    completed = subprocess.run(
        ["osascript", "-e", script],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"failed to query macOS app property {property_name!r} for pid {pid}: "
            f"{completed.stderr.strip() or completed.stdout.strip()}"
        )
    value = completed.stdout.strip().lower()
    if value == "true":
        return True
    if value == "false":
        return False
    raise AssertionError(f"unexpected AppleScript result for {property_name!r}: {value!r}")


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
            page.ele("#name").input(Keys.CTRL_A)
            page.ele("#name").input("rust")
            page.ele("@id=submit").click()
            self.assertEqual(page.ele("@id=out").text, "rust")
            self.assertEqual(len(page.eles(".item")), 2)
            js = page.run_js("({count: document.querySelectorAll('.item').length})")
            self.assertEqual(js["count"], 2)
            self.assertGreaterEqual(page.tabs_count, 1)
            self.assertIn(page.tab_id, page.tab_ids)
            self.assertTrue(page.states.is_headless)
        finally:
            page.quit()

    def test_browser_level_api_and_screenshot(self) -> None:
        browser = Browser.launch(ChromiumOptions())
        try:
            self.assertTrue(browser.states.is_alive)
            self.assertTrue(browser.states.is_headless)
            self.assertTrue(browser.states.is_existed)
            self.assertFalse(browser.states.is_incognito)
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

    def test_browser_waits_for_new_tab_ignores_existing_background_tab(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            current_tab = page.tab_id
            old_page = page.browser.new_page("data:text/html," + quote("<h1>old-tab</h1>"))
            self.assertNotEqual(old_page.tab_id, current_tab)
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
            self.assertNotEqual(new_tab, old_page.tab_id)
            new_page = page.get_tab(new_tab)
            self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
            self.assertIn("new-tab", new_page.html)
        finally:
            page.quit()

    def test_page_element_click_for_new_tab_returns_new_page(self) -> None:
        with serve_new_tab_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            try:
                self.assertTrue(page.get(base_url + "/"))
                new_tab_url = base_url + "/new-tab"
                page.run_js(
                    f"""
                    (() => {{
                        const link = document.createElement('a');
                        link.id = 'open-tab';
                        link.href = {new_tab_url!r};
                        link.target = '_blank';
                        link.textContent = 'Open tab';
                        document.body.appendChild(link);
                        return true;
                    }})()
                    """
                )
                new_page = page.ele("#open-tab").click.for_new_tab(timeout=2.0)
                self.assertNotEqual(new_page, False)
                assert new_page is not False
                self.assertIs(new_page.browser, page.browser)
                self.assertNotEqual(new_page.tab_id, page.tab_id)
                self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
                self.assertIn("new-tab", new_page.html)
            finally:
                page.quit()

    def test_page_element_click_middle_returns_new_page(self) -> None:
        with serve_new_tab_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            try:
                self.assertTrue(page.get(base_url + "/"))
                new_tab_url = base_url + "/middle-tab"
                page.run_js(
                    f"""
                    (() => {{
                        const link = document.createElement('a');
                        link.id = 'middle-open-tab';
                        link.href = {new_tab_url!r};
                        link.textContent = 'Open by middle click';
                        document.body.appendChild(link);
                        return true;
                    }})()
                    """
                )
                new_page = page.ele("#middle-open-tab").click.middle()
                self.assertNotEqual(new_page, False)
                assert new_page is not False
                self.assertIs(new_page.browser, page.browser)
                self.assertNotEqual(new_page.tab_id, page.tab_id)
                self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
                self.assertIn("middle-tab", new_page.html)
                self.assertIn("Open by middle click", page.html)
            finally:
                page.quit()

    def test_page_wait_and_element_states(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            self.assertEqual(page.states.ready_state, "complete")
            self.assertTrue(page.states.is_alive)
            self.assertFalse(page.states.is_loading)
            self.assertTrue(page.states.is_existed)
            self.assertFalse(page.states.is_incognito)

            name = page.ele("#name")
            submit = page.ele("#submit")
            self.assertFalse(name.states.is_selected)
            self.assertFalse(name.states.is_checked)
            self.assertTrue(submit.states.is_displayed)
            self.assertTrue(submit.states.is_enabled)
            rect = submit.states.has_rect
            self.assertNotEqual(rect, False)
            assert isinstance(rect, list)
            self.assertEqual(len(rect), 4)
            self.assertEqual(len(rect[0]), 2)
            self.assertLess(rect[0][0], rect[1][0])
            self.assertLess(rect[0][1], rect[2][1])
            self.assertTrue(submit.states.is_in_viewport)
            self.assertTrue(submit.states.is_whole_in_viewport)
            self.assertFalse(submit.states.is_covered)
            self.assertTrue(submit.states.is_clickable)
            self.assertTrue(page.wait.eles_loaded(["#submit", "#name"], timeout=1.0))
            self.assertIsNot(page.wait.ele_displayed("#submit", timeout=1.0), False)
            self.assertIsNot(page.wait.ele_enabled("#submit", timeout=1.0), False)
            self.assertIsNot(page.wait.ele_clickable("#submit", timeout=1.0), False)
            self.assertIs(submit.wait.displayed(timeout=1.0), submit)
            self.assertIs(submit.wait.clickable(timeout=1.0), submit)
            self.assertIs(submit.wait.has_rect(timeout=1.0), submit)
            self.assertIs(submit.wait.stop_moving(timeout=1.0), submit)

            page.run_js(
                """
                const zero = document.createElement('div');
                zero.id = 'zero';
                zero.style.width = '0px';
                zero.style.height = '0px';
                document.body.appendChild(zero);
                """
            )
            self.assertFalse(page.ele("#zero").states.has_rect)

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

            page.run_js(
                """
                const overlay = document.createElement('div');
                overlay.id = 'overlay';
                overlay.style.position = 'fixed';
                overlay.style.left = '0';
                overlay.style.top = '0';
                overlay.style.width = '100vw';
                overlay.style.height = '100vh';
                overlay.style.zIndex = '9999';
                overlay.style.background = 'transparent';
                document.body.appendChild(overlay);
                """
            )
            self.assertTrue(submit.states.is_covered)
            self.assertIs(submit.wait.covered(timeout=1.0), submit)
            page.run_js(
                "(() => { const overlay = document.getElementById('overlay'); if (overlay) overlay.remove(); return true; })()"
            )
            self.assertIs(submit.wait.not_covered(timeout=1.0), submit)
        finally:
            page.quit()

    def test_page_element_click_at_clicks_submit_button(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            page.ele("#name").input("click-at")
            page.ele("#submit").click.at()
            self.assertEqual(page.ele("#out").text, "click-at")
        finally:
            page.quit()

    def test_page_tracks_and_handles_alert_state(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            self.assertFalse(page.states.has_alert)
            page.run_js("setTimeout(() => alert('hello-alert'), 50)")
            wait_for_condition(lambda: page.states.has_alert, timeout=2.0, message="alert did not open")
            self.assertEqual(page.handle_alert(timeout=2.0), "hello-alert")
            wait_for_condition(
                lambda: not page.states.has_alert,
                timeout=2.0,
                message="alert did not close",
            )
            self.assertFalse(page.states.has_alert)
        finally:
            page.quit()

    def test_webpage_handles_next_alert_and_waits_for_close(self) -> None:
        page = WebPage(mode="d")
        try:
            self.assertTrue(page.get(data_url()))
            self.assertFalse(page.states.has_alert)
            page.handle_alert(send="typed-by-openpage", next_one=True)
            page.run_js(
                """
                setTimeout(() => {
                    document.body.dataset.promptResult = prompt('Your name?', 'default') || '';
                }, 50);
                """
            )
            self.assertIs(page.wait.alert_closed(timeout=2.0), page)
            self.assertEqual(page.run_js("document.body.dataset.promptResult || ''"), "typed-by-openpage")
            self.assertFalse(page.states.has_alert)
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

    def test_load_mode_controls_navigation_waiting(self) -> None:
        with serve_load_mode_site() as base_url:
            normal_page = ChromiumPage(ChromiumOptions().set_load_mode("normal"))
            try:
                start = time.perf_counter()
                self.assertTrue(normal_page.get(base_url))
                normal_elapsed = time.perf_counter() - start
                self.assertGreaterEqual(normal_elapsed, 1.0)
                self.assertEqual(normal_page.states.ready_state, "complete")
                self.assertEqual(normal_page.run_js("document.body.dataset.loaded || ''"), "yes")
            finally:
                normal_page.quit()

            eager_page = ChromiumPage(ChromiumOptions())
            try:
                eager_page.set.load_mode.eager()
                start = time.perf_counter()
                self.assertTrue(eager_page.get(base_url))
                eager_elapsed = time.perf_counter() - start
                self.assertLess(eager_elapsed, 1.0)
                self.assertIn(eager_page.states.ready_state, {"interactive", "complete"})
                self.assertNotEqual(eager_page.run_js("document.body.dataset.loaded || ''"), "yes")
            finally:
                eager_page.quit()

            none_page = ChromiumPage(ChromiumOptions())
            try:
                none_page.set.load_mode.none()
                start = time.perf_counter()
                self.assertTrue(none_page.get(base_url))
                none_elapsed = time.perf_counter() - start
                self.assertLess(none_elapsed, 1.0)
                self.assertTrue(none_page.wait.eles_loaded(["#status"], timeout=1.0))
                self.assertNotEqual(none_page.run_js("document.body.dataset.loaded || ''"), "yes")
                wait_for_condition(
                    lambda: none_page.run_js("document.body.dataset.loaded || ''") == "yes",
                    timeout=3.0,
                    message="none load mode page never finished loading",
                )
                self.assertTrue(none_page.wait.doc_loaded(timeout=1.0))
            finally:
                none_page.quit()

    def test_browser_set_load_mode_applies_to_new_pages(self) -> None:
        with serve_load_mode_site() as base_url:
            browser = Browser.launch(ChromiumOptions())
            try:
                browser.set.load_mode.none()
                page = browser.new_page()
                start = time.perf_counter()
                self.assertTrue(page.get(base_url))
                elapsed = time.perf_counter() - start
                self.assertLess(elapsed, 1.0)
                self.assertTrue(page.wait.eles_loaded(["#status"], timeout=1.0))
                self.assertNotEqual(page.run_js("document.body.dataset.loaded || ''"), "yes")
            finally:
                browser.close()

    def test_page_set_blocked_urls_blocks_and_clears_css_requests(self) -> None:
        with serve_blocked_url_site() as (base_url, hits):
            page = ChromiumPage(ChromiumOptions())
            blocked_pattern = base_url + "style.css"
            try:
                page.set.blocked_urls(blocked_pattern)
                self.assertTrue(page.get(base_url))
                self.assertEqual(
                    page.run_js("getComputedStyle(document.getElementById('title')).color"),
                    "rgb(0, 0, 0)",
                )
                self.assertEqual(hits["style"], 0)

                page.set.blocked_urls(None)
                self.assertTrue(page.get(base_url))
                self.assertEqual(
                    page.run_js("getComputedStyle(document.getElementById('title')).color"),
                    "rgb(255, 0, 0)",
                )
                self.assertEqual(hits["style"], 1)
            finally:
                page.quit()

    def test_page_set_user_agent_overrides_navigator_values(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            page.set.user_agent("OpenPageAgent/1.0", "OpenPageOS")
            self.assertEqual(page.run_js("navigator.userAgent"), "OpenPageAgent/1.0")
            self.assertEqual(page.run_js("navigator.platform"), "OpenPageOS")
        finally:
            page.quit()

    def test_page_set_headers_and_storage(self) -> None:
        with serve_header_echo_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            try:
                page.set.headers({"X-OpenPage-Test": "driver-header"})
                self.assertTrue(page.get(base_url))
                self.assertEqual(page.ele("#header").text, "driver-header")

                page.set.local_storage("openpage-local", "local-value")
                page.set.session_storage("openpage-session", "session-value")
                self.assertEqual(page.run_js("localStorage.getItem('openpage-local')"), "local-value")
                self.assertEqual(page.run_js("sessionStorage.getItem('openpage-session')"), "session-value")
                page.set.local_storage("openpage-local", False)
                page.set.session_storage("openpage-session", False)
                self.assertTrue(page.run_js("localStorage.getItem('openpage-local') === null"))
                self.assertTrue(page.run_js("sessionStorage.getItem('openpage-session') === null"))
            finally:
                page.quit()

    def test_page_set_upload_files_inputs_selected_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_upload_site() as base_url:
            first = Path(tmp_dir) / "first.txt"
            second = Path(tmp_dir) / "second.txt"
            first.write_text("first")
            second.write_text("second")
            page = ChromiumPage(ChromiumOptions())
            try:
                page.set.upload_files([str(first), str(second)])
                self.assertTrue(page.get(base_url))
                page.ele("#picker").click()
                self.assertTrue(page.wait.upload_paths_inputted())
                wait_for_condition(
                    lambda: page.run_js("document.getElementById('picker').files.length") == 2,
                    timeout=2.0,
                    message="upload files were not inputted",
                )
                self.assertEqual(
                    page.run_js(
                        "Array.from(document.getElementById('picker').files).map(f => f.name)"
                    ),
                    ["first.txt", "second.txt"],
                )
                self.assertEqual(page.ele("#out").text, "first.txt,second.txt")
            finally:
                page.quit()

    def test_page_element_click_to_upload_inputs_selected_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_upload_site() as base_url:
            first = Path(tmp_dir) / "first.txt"
            second = Path(tmp_dir) / "second.txt"
            first.write_text("first")
            second.write_text("second")
            page = ChromiumPage(ChromiumOptions())
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#picker").click.to_upload([str(first), str(second)])
                wait_for_condition(
                    lambda: page.run_js("document.getElementById('picker').files.length") == 2,
                    timeout=2.0,
                    message="click.to_upload() did not input files",
                )
                self.assertEqual(
                    page.run_js(
                        "Array.from(document.getElementById('picker').files).map(f => f.name)"
                    ),
                    ["first.txt", "second.txt"],
                )
                self.assertEqual(page.ele("#out").text, "first.txt,second.txt")
            finally:
                page.quit()

    def test_page_window_controls_update_bounds(self) -> None:
        page = ChromiumPage(ChromiumOptions().headless(False).set_window_size(900, 700))
        try:
            self.assertTrue(page.get(data_url()))
            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="window did not enter normal state",
            )

            page.set.window.size(920, 720)
            wait_for_condition(
                lambda: page._inner.window_size() == (920, 720),
                timeout=2.0,
                message="window size did not update",
            )

            page.set.window.location(40, 50)
            wait_for_condition(
                lambda: (
                    abs(page._inner.window_location()[0] - 40) <= 20
                    and abs(page._inner.window_location()[1] - 50) <= 20
                ),
                timeout=2.0,
                message="window location did not update",
            )

            page.set.window.max()
            wait_for_condition(
                lambda: page._inner.window_state() == "maximized",
                timeout=2.0,
                message="window did not maximize",
            )

            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="window did not restore from maximized state",
            )
        finally:
            page.quit()

    def test_page_window_hide_show_and_activate(self) -> None:
        if platform.system() != "Darwin":
            self.skipTest("window visibility assertions are currently verified on macOS only")
        page = ChromiumPage(ChromiumOptions().headless(False).set_window_size(900, 700))
        try:
            self.assertTrue(page.get(data_url()))
            pid = page.browser._inner._browser_pid()
            if pid is None:
                self.skipTest("browser pid is not available for this launch mode")

            page.set.window.hide()
            wait_for_condition(
                lambda: macos_app_property(pid, "visible") is False,
                timeout=2.0,
                message="browser window did not hide",
            )

            page.set.window.show()
            wait_for_condition(
                lambda: macos_app_property(pid, "visible") is True,
                timeout=2.0,
                message="browser window did not show",
            )

            page.set.activate()
            wait_for_condition(
                lambda: macos_app_property(pid, "frontmost") is True,
                timeout=2.0,
                message="browser window did not activate",
            )
        finally:
            try:
                page.set.window.show()
            except Exception:
                pass
            page.quit()

    def test_page_window_min_and_full_states(self) -> None:
        page = ChromiumPage(ChromiumOptions().headless(False).set_window_size(900, 700))
        try:
            self.assertTrue(page.get(data_url()))
            page.set.window.mini()
            wait_for_condition(
                lambda: page._inner.window_state() == "minimized",
                timeout=2.0,
                message="window did not minimize",
            )

            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="window did not restore from minimized state",
            )

            page.set.window.full()
            wait_for_condition(
                lambda: page._inner.window_state() == "fullscreen",
                timeout=2.0,
                message="window did not enter fullscreen",
            )

            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="window did not restore from fullscreen state",
            )
        finally:
            page.quit()

    def test_page_set_auto_handle_alert_closes_dialog(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            page.set.auto_handle_alert()
            page.run_js(
                """
                setTimeout(() => {
                    alert('auto-handled');
                    document.body.dataset.alertDone = 'yes';
                }, 50);
                """
            )
            self.assertIs(page.wait.alert_closed(timeout=2.0), page)
            wait_for_condition(
                lambda: page.run_js("document.body.dataset.alertDone || ''") == "yes",
                timeout=2.0,
                message="auto-handled alert did not resume page execution",
            )
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
                self.assertEqual(mission.id, mission.guid)
                self.assertEqual(mission.tab_id, page.tab_id)
                self.assertEqual(mission.folder, tmp_dir)
                self.assertEqual(mission.name, "openpage.txt")
                self.assertEqual(mission.suggested_filename, "openpage.txt")
                self.assertTrue(mission.tmp_path.endswith(mission.guid))
                self.assertEqual(mission.state, "done")
                self.assertTrue(mission.is_done)
                self.assertGreaterEqual(mission.received_bytes, len(b"openpage-download"))
                self.assertEqual(mission.rate, 100.0)
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

    def test_browser_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                data = page.browser.wait.download_begin(timeout=5.0, cancel_it=True)
                self.assertNotEqual(data, False)
                assert isinstance(data, dict)
                self.assertEqual(data["suggested_filename"], "openpage.txt")
                self.assertEqual(data["name"], "openpage.txt")
                self.assertEqual(data["tab_id"], page.tab_id)
                self.assertEqual(data["id"], data["guid"])
                self.assertEqual(data["folder"], tmp_dir)
                self.assertTrue(data["tmp_path"].endswith(data["guid"]))
                self.assertTrue(page.browser.wait.downloads_done(timeout=10.0))
            finally:
                page.quit()

    def test_page_waits_for_download_begin_and_completion(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                mission = page.wait.download_begin(timeout=5.0)
                self.assertNotEqual(mission, False)
                assert isinstance(mission, DownloadMission)
                self.assertEqual(mission.suggested_filename, "openpage.txt")
                self.assertTrue(page.wait.downloads_done(timeout=10.0))
                self.assertTrue(page.wait.all_downloads_done(timeout=10.0))
            finally:
                page.quit()

    def test_page_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                data = page.wait.download_begin(timeout=5.0, cancel_it=True)
                self.assertNotEqual(data, False)
                assert isinstance(data, dict)
                self.assertEqual(data["suggested_filename"], "openpage.txt")
                self.assertEqual(data["name"], "openpage.txt")
                self.assertEqual(data["tab_id"], page.tab_id)
                self.assertEqual(data["id"], data["guid"])
                self.assertEqual(data["folder"], tmp_dir)
                self.assertTrue(data["tmp_path"].endswith(data["guid"]))
                self.assertTrue(page.wait.downloads_done(timeout=10.0))
            finally:
                page.quit()

    def test_download_conflict_mode_rename_creates_new_name(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            target = Path(tmp_dir) / "openpage.txt"
            target.write_text("existing")
            page = ChromiumPage(
                ChromiumOptions().set_download_path(tmp_dir).set_file_exists("rename")
            )
            try:
                self.assertEqual(page.browser.download_file_exists_mode, "rename")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                download_path = Path(page.wait_for_download("openpage.txt"))
                self.assertEqual(download_path.name, "openpage_1.txt")
                self.assertEqual(download_path.read_text(), "openpage-download")
                self.assertEqual(target.read_text(), "existing")
            finally:
                page.quit()

    def test_download_conflict_mode_overwrite_replaces_existing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            target = Path(tmp_dir) / "openpage.txt"
            target.write_text("existing")
            page = ChromiumPage(
                ChromiumOptions().set_download_path(tmp_dir).set_file_exists("overwrite")
            )
            try:
                self.assertEqual(page.browser.download_file_exists_mode, "overwrite")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                download_path = Path(page.wait_for_download("openpage.txt"))
                self.assertEqual(download_path, target)
                self.assertEqual(target.read_text(), "openpage-download")
            finally:
                page.quit()

    def test_download_conflict_mode_skip_keeps_existing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            target = Path(tmp_dir) / "openpage.txt"
            target.write_text("existing")
            page = ChromiumPage(
                ChromiumOptions().set_download_path(tmp_dir).set_file_exists("skip")
            )
            try:
                self.assertEqual(page.browser.download_file_exists_mode, "skip")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                download_path = Path(page.wait_for_download("openpage.txt"))
                mission = page.last_download()
                self.assertEqual(download_path, target)
                self.assertEqual(target.read_text(), "existing")
                self.assertIsNotNone(mission)
                assert mission is not None
                self.assertEqual(mission.state, "skipped")
                self.assertEqual(mission.final_path, str(target))
            finally:
                page.quit()

    def test_page_set_download_file_name_renames_http_download(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = ChromiumPage(ChromiumOptions().set_download_path(tmp_dir))
            target = Path(tmp_dir) / "renamed.txt"
            try:
                page.set.download_file_name("renamed")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))
                self.assertTrue(target.exists())
                self.assertEqual(target.read_text(), "openpage-download")
            finally:
                page.quit()

    def test_page_set_download_path_scopes_downloads_per_tab(self) -> None:
        with (
            tempfile.TemporaryDirectory() as dir_one,
            tempfile.TemporaryDirectory() as dir_two,
            serve_download_site() as base_url,
        ):
            page = ChromiumPage(ChromiumOptions())
            other_tab = page.new_tab()
            try:
                page.set.download_path(dir_one)
                other_tab.set.download_path(dir_two)
                self.assertTrue(page.get(base_url))
                self.assertTrue(other_tab.get(base_url))

                previous_count = len(page.download_missions())
                page.run_js('(() => { document.getElementById("download").click(); return true; })()')
                mission = wait_for_next_download(page, previous_count, timeout=5.0)
                self.assertEqual(
                    mission.wait(timeout=10.0),
                    str(Path(dir_one) / "openpage.txt"),
                )
                self.assertEqual(mission.final_path, str(Path(dir_one) / "openpage.txt"))

                previous_count = len(page.download_missions())
                previous_guid = mission.guid
                other_tab.run_js('(() => { document.getElementById("download").click(); return true; })()')
                mission = wait_for_next_download(
                    page,
                    previous_count,
                    previous_guid=previous_guid,
                    timeout=5.0,
                )
                self.assertEqual(
                    mission.wait(timeout=10.0),
                    str(Path(dir_two) / "openpage.txt"),
                )
                self.assertEqual(mission.final_path, str(Path(dir_two) / "openpage.txt"))
            finally:
                page.quit()

    def test_page_set_download_file_exists_overrides_browser_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            target = Path(tmp_dir) / "openpage.txt"
            target.write_text("existing")
            page = ChromiumPage(
                ChromiumOptions().set_download_path(tmp_dir).set_file_exists("rename")
            )
            try:
                page.set.download_file_exists("skip")
                self.assertTrue(page.get(base_url))
                previous_count = len(page.download_missions())
                page.ele("#download").click()

                mission = wait_for_next_download(page, previous_count, timeout=5.0)
                self.assertEqual(mission.wait(timeout=10.0), str(target))
                self.assertEqual(target.read_text(), "existing")
                self.assertEqual(mission.state, "skipped")
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

    def test_browser_console_wait_returns_message(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        console = page.console
        try:
            self.assertTrue(page.get(data_url()))
            self.assertFalse(console.listening)
            console.start()
            self.assertTrue(console.listening)
            page.run_js('(setTimeout(() => console.log("openpage-console"), 100), true)')
            message = console.wait(timeout=5.0)
            self.assertNotEqual(message, False)
            assert message is not False
            self.assertEqual(message.text, "openpage-console")
            self.assertEqual(message.body, "openpage-console")
            self.assertEqual(message.level, "log")
            console.stop()
            self.assertFalse(console.listening)
        finally:
            if console.listening:
                console.stop()
            page.quit()

    def test_browser_console_clear_drops_buffered_messages(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        console = page.console
        try:
            self.assertTrue(page.get(data_url()))
            console.start()
            page.run_js('(setTimeout(() => console.log("stale-console"), 100), true)')
            time.sleep(0.3)
            console.clear()
            self.assertFalse(console.wait(timeout=0.5))
        finally:
            if console.listening:
                console.stop()
            page.quit()

    def test_browser_console_messages_drains_buffer(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        console = page.console
        try:
            self.assertTrue(page.get(data_url()))
            console.start()
            page.run_js(
                '(setTimeout(() => console.log("first-console"), 100), '
                'setTimeout(() => console.log("second-console"), 180), true)'
            )
            time.sleep(0.45)
            messages = console.messages
            self.assertEqual([item.text for item in messages], ["first-console", "second-console"])
            self.assertEqual(console.messages, [])
        finally:
            if console.listening:
                console.stop()
            page.quit()

    def test_browser_console_steps_yield_and_stop_on_timeout(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        console = page.console
        try:
            self.assertTrue(page.get(data_url()))
            console.start()
            page.run_js('(setTimeout(() => console.log("step-console"), 100), true)')
            steps = console.steps(timeout=0.5)
            first = next(steps)
            self.assertEqual(first.text, "step-console")
            with self.assertRaises(StopIteration):
                next(steps)
        finally:
            if console.listening:
                console.stop()
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

    def test_listener_pause_and_resume_controls_capture(self) -> None:
        with serve_listener_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            listener = page.listen
            try:
                listener.start(targets="/api/data", method="POST")
                listener.pause()
                self.assertFalse(listener.listening)
                self.assertTrue(page.get(base_url))
                page.ele("#trigger").click()
                with self.assertRaises(RuntimeError):
                    listener.wait(timeout=0.5)

                listener.resume()
                self.assertTrue(listener.listening)
                page.ele("#trigger").click()
                packet = listener.wait(timeout=5.0)
                self.assertEqual(packet.method, "POST")
                self.assertTrue(packet.url.endswith("/api/data"))
                self.assertTrue(listener.wait_silent(timeout=5.0, targets_only=True))
            finally:
                page.quit()

    def test_listener_set_targets_steps_and_wait_silent(self) -> None:
        with serve_multi_listener_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            listener = page.listen
            try:
                self.assertTrue(page.get(base_url))
                listener.start(targets=True, method="GET", res_type="Fetch")
                listener.set_targets("/api/two", method="GET", res_type="Fetch")
                page.ele("#trigger").click()

                packet = next(listener.steps(count=1, timeout=5.0))
                self.assertTrue(packet.url.endswith("/api/two"))
                self.assertEqual(packet.response.body, '{"name": "two"}')
                self.assertTrue(listener.wait_silent(timeout=5.0, targets_only=True))
                self.assertTrue(listener.wait_silent(timeout=5.0, targets_only=False))
            finally:
                page.quit()

    def test_interceptor_can_rewrite_request_url(self) -> None:
        with serve_intercept_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            interceptor = page.intercept
            try:
                self.assertTrue(page.get(base_url))
                interceptor.start(targets="/api/original", method="GET")
                page.ele("#rewrite").click()
                request = interceptor.wait(timeout=5.0)
                self.assertNotEqual(request, False)
                assert request is not False
                request.continue_request(url=base_url + "api/rewritten")
                wait_for_output_text(page, "rewritten")
            finally:
                interceptor.stop()
                page.quit()

    def test_interceptor_can_fail_request(self) -> None:
        with serve_intercept_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            interceptor = page.intercept
            try:
                self.assertTrue(page.get(base_url))
                interceptor.start(targets="/api/data", method="GET")
                page.ele("#fetch").click()
                request = interceptor.wait(timeout=5.0)
                self.assertNotEqual(request, False)
                assert request is not False
                request.fail()
                wait_for_output_text(page, "failed")
            finally:
                interceptor.stop()
                page.quit()

    def test_interceptor_can_fulfill_request(self) -> None:
        with serve_intercept_site() as base_url:
            page = ChromiumPage(ChromiumOptions())
            interceptor = page.intercept
            try:
                self.assertTrue(page.get(base_url))
                interceptor.start(targets="/api/data", method="GET")
                page.ele("#fetch").click()
                request = interceptor.wait(timeout=5.0)
                self.assertNotEqual(request, False)
                assert request is not False
                request.fulfill(
                    response_code=201,
                    body="fulfilled-body",
                    headers={"Content-Type": "text/plain; charset=utf-8"},
                    response_phrase="Created",
                )
                wait_for_output_text(page, "201:fulfilled-body")
            finally:
                interceptor.stop()
                page.quit()

    def test_webpage_interceptor_uses_driver_page(self) -> None:
        with serve_intercept_site() as base_url:
            page = WebPage(mode="d")
            interceptor = page.intercept
            try:
                self.assertTrue(page.get(base_url))
                interceptor.start(targets="/api/data", method="GET")
                page.ele("#fetch").click()
                request = interceptor.wait(timeout=5.0)
                self.assertNotEqual(request, False)
                assert request is not False
                request.fulfill(
                    response_code=202,
                    body="webpage-fulfilled",
                    headers={"Content-Type": "text/plain; charset=utf-8"},
                    response_phrase="Accepted",
                )
                wait_for_output_text(page, "202:webpage-fulfilled")
            finally:
                interceptor.stop()
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
                page.set.user_agent("OpenPageWeb/1.0", "OpenPageOS")
                self.assertEqual(page.user_agent, "OpenPageWeb/1.0")
                self.assertEqual(page.run_js("navigator.platform"), "OpenPageOS")
                self.assertTrue(page.states.is_alive)
                self.assertTrue(page.states.is_headless)
                self.assertTrue(page.states.is_existed)
                self.assertFalse(page.states.is_incognito)
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

    def test_webpage_set_headers_in_session_mode(self) -> None:
        with serve_header_echo_site() as base_url:
            page = WebPage(mode="s")
            try:
                page.set.headers({"X-OpenPage-Test": "session-header"})
                self.assertTrue(page.get(base_url))
                self.assertIn("session-header", page.html)
                self.assertEqual(page.ele("#header").text, "session-header")
            finally:
                page.quit()

    def test_webpage_load_mode_uses_driver_navigation_mode(self) -> None:
        with serve_load_mode_site() as base_url:
            page = WebPage(mode="d", chromium_options=ChromiumOptions().set_load_mode("eager"))
            try:
                start = time.perf_counter()
                self.assertTrue(page.get(base_url))
                elapsed = time.perf_counter() - start
                self.assertLess(elapsed, 1.0)
                self.assertIn(page.states.ready_state, {"interactive", "complete"})
                self.assertNotEqual(page.run_js("document.body.dataset.loaded || ''"), "yes")
            finally:
                page.quit()

    def test_webpage_set_upload_files_uses_driver_page(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_upload_site() as base_url:
            first = Path(tmp_dir) / "first.txt"
            second = Path(tmp_dir) / "second.txt"
            first.write_text("first")
            second.write_text("second")
            page = WebPage(mode="d")
            try:
                page.set.upload_files([str(first), str(second)])
                self.assertTrue(page.get(base_url))
                page.ele("#picker").click()
                self.assertTrue(page.wait.upload_paths_inputted())
                wait_for_condition(
                    lambda: page.run_js("document.getElementById('picker').files.length") == 2,
                    timeout=2.0,
                    message="webpage upload files were not inputted",
                )
                self.assertEqual(
                    page.run_js(
                        "Array.from(document.getElementById('picker').files).map(f => f.name)"
                    ),
                    ["first.txt", "second.txt"],
                )
            finally:
                page.quit()

    def test_webpage_element_click_to_upload_uses_driver_page(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_upload_site() as base_url:
            first = Path(tmp_dir) / "first.txt"
            second = Path(tmp_dir) / "second.txt"
            first.write_text("first")
            second.write_text("second")
            page = WebPage(mode="d")
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#picker").click.to_upload([str(first), str(second)])
                wait_for_condition(
                    lambda: page.run_js("document.getElementById('picker').files.length") == 2,
                    timeout=2.0,
                    message="webpage click.to_upload() did not input files",
                )
                self.assertEqual(
                    page.run_js(
                        "Array.from(document.getElementById('picker').files).map(f => f.name)"
                    ),
                    ["first.txt", "second.txt"],
                )
            finally:
                page.quit()

    def test_webpage_element_click_for_new_tab_returns_new_page(self) -> None:
        page = WebPage(mode="d")
        try:
            self.assertTrue(page.get(data_url()))
            before_tab_ids = list(page.tab_ids)
            new_tab_url = "data:text/html," + quote("<h1>new-tab</h1>")
            page.run_js(
                f"""
                (() => {{
                    const link = document.createElement('a');
                    link.id = 'open-tab';
                    link.href = {new_tab_url!r};
                    link.target = '_blank';
                    link.textContent = 'Open tab';
                    document.body.appendChild(link);
                    return true;
                }})()
                """
            )

            new_page = page.ele("#open-tab").click.for_new_tab(timeout=2.0)

            self.assertNotEqual(new_page, False)
            assert new_page is not False
            self.assertNotIn(new_page.tab_id, before_tab_ids)
            self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
            self.assertIn("new-tab", new_page.html)
        finally:
            page.quit()

    def test_webpage_element_click_middle_returns_new_page(self) -> None:
        page = WebPage(mode="d")
        try:
            self.assertTrue(page.get(data_url()))
            before_tab_ids = list(page.tab_ids)
            new_tab_url = "data:text/html," + quote("<h1>middle-tab</h1>")
            page.run_js(
                f"""
                (() => {{
                    const link = document.createElement('a');
                    link.id = 'middle-open-tab';
                    link.href = {new_tab_url!r};
                    link.textContent = 'Open by middle click';
                    document.body.appendChild(link);
                    return true;
                }})()
                """
            )

            new_page = page.ele("#middle-open-tab").click.middle()

            self.assertNotEqual(new_page, False)
            assert new_page is not False
            self.assertNotIn(new_page.tab_id, before_tab_ids)
            self.assertTrue(new_page.wait.doc_loaded(timeout=2.0))
            self.assertIn("middle-tab", new_page.html)
            self.assertIn("Open by middle click", page.html)
        finally:
            page.quit()

    def test_webpage_window_controls_use_driver_page(self) -> None:
        page = WebPage(mode="d", chromium_options=ChromiumOptions().headless(False).set_window_size(880, 680))
        try:
            self.assertTrue(page.get(data_url()))
            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="webpage window did not enter normal state",
            )
            page.set.window.size(900, 700)
            wait_for_condition(
                lambda: page._inner.window_size() == (900, 700),
                timeout=2.0,
                message="webpage window size did not update",
            )
            page.set.window.max()
            wait_for_condition(
                lambda: page._inner.window_state() == "maximized",
                timeout=2.0,
                message="webpage window did not maximize",
            )
            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="webpage window did not restore",
            )
        finally:
            page.quit()

    def test_webpage_window_hide_show_and_activate(self) -> None:
        if platform.system() != "Darwin":
            self.skipTest("window visibility assertions are currently verified on macOS only")
        page = WebPage(mode="d", chromium_options=ChromiumOptions().headless(False).set_window_size(880, 680))
        try:
            self.assertTrue(page.get(data_url()))
            pid = page._inner._browser_pid()
            if pid is None:
                self.skipTest("browser pid is not available for this launch mode")

            page.set.window.hide()
            wait_for_condition(
                lambda: macos_app_property(pid, "visible") is False,
                timeout=2.0,
                message="webpage browser window did not hide",
            )

            page.set.window.show()
            wait_for_condition(
                lambda: macos_app_property(pid, "visible") is True,
                timeout=2.0,
                message="webpage browser window did not show",
            )

            page.set.activate()
            wait_for_condition(
                lambda: macos_app_property(pid, "frontmost") is True,
                timeout=2.0,
                message="webpage browser window did not activate",
            )
        finally:
            try:
                page.set.window.show()
            except Exception:
                pass
            page.quit()

    def test_webpage_window_min_and_full_states(self) -> None:
        page = WebPage(mode="d", chromium_options=ChromiumOptions().headless(False).set_window_size(880, 680))
        try:
            self.assertTrue(page.get(data_url()))
            page.set.window.mini()
            wait_for_condition(
                lambda: page._inner.window_state() == "minimized",
                timeout=2.0,
                message="webpage window did not minimize",
            )

            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="webpage window did not restore from minimized state",
            )

            page.set.window.full()
            wait_for_condition(
                lambda: page._inner.window_state() == "fullscreen",
                timeout=2.0,
                message="webpage window did not enter fullscreen",
            )

            page.set.window.normal()
            wait_for_condition(
                lambda: page._inner.window_state() == "normal",
                timeout=2.0,
                message="webpage window did not restore from fullscreen state",
            )
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
                self.assertTrue(page.states.is_existed)
                self.assertFalse(page.states.is_incognito)
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
                self.assertEqual(page.download_file_exists_mode, "rename")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))

                mission = page.last_download()
                self.assertIsNotNone(mission)
                assert mission is not None
                self.assertEqual(mission.id, mission.guid)
                self.assertIn(mission.tab_id, page.tab_ids)
                self.assertEqual(mission.folder, tmp_dir)
                self.assertEqual(mission.name, "openpage.txt")
                self.assertTrue(mission.tmp_path.endswith(mission.guid))
                self.assertEqual(mission.state, "done")
                self.assertEqual(mission.rate, 100.0)
                self.assertEqual(mission.final_path, str(target))

                missions = page.download_missions()
                self.assertGreaterEqual(len(missions), 1)
                self.assertEqual(missions[-1].guid, mission.guid)
            finally:
                page.quit()

    def test_webpage_set_blocked_urls_uses_driver_page(self) -> None:
        with serve_blocked_url_site() as (base_url, hits):
            page = WebPage(mode="d")
            blocked_pattern = base_url + "style.css"
            try:
                page.set.blocked_urls(blocked_pattern)
                self.assertTrue(page.get(base_url))
                self.assertEqual(
                    page.run_js("getComputedStyle(document.getElementById('title')).color"),
                    "rgb(0, 0, 0)",
                )
                self.assertEqual(hits["style"], 0)

                page.set.blocked_urls(None)
                self.assertTrue(page.get(base_url))
                self.assertEqual(
                    page.run_js("getComputedStyle(document.getElementById('title')).color"),
                    "rgb(255, 0, 0)",
                )
                self.assertEqual(hits["style"], 1)
            finally:
                page.quit()

    def test_webpage_set_download_path_and_file_exists_use_driver_page(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            target = Path(tmp_dir) / "openpage.txt"
            target.write_text("existing")
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir).set_file_exists("rename"),
            )
            try:
                page.set.download_file_exists("skip")
                page.set.download_path(tmp_dir)
                self.assertTrue(page.get(base_url))
                previous_count = len(page.download_missions())
                page.ele("#download").click()
                mission = wait_for_next_download(page, previous_count, timeout=5.0)
                self.assertEqual(mission.wait(timeout=10.0), str(target))
                self.assertEqual(mission.state, "skipped")
            finally:
                page.quit()

    def test_webpage_set_download_file_name_renames_http_download(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir),
            )
            target = Path(tmp_dir) / "renamed.txt"
            try:
                page.set.download_file_name("renamed")
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()

                self.assertEqual(page.wait_for_download("openpage.txt"), str(target))
                self.assertTrue(target.exists())
                self.assertEqual(target.read_text(), "openpage-download")
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

    def test_webpage_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir, serve_download_site() as base_url:
            page = WebPage(
                mode="d",
                chromium_options=ChromiumOptions().set_download_path(tmp_dir),
            )
            try:
                self.assertTrue(page.get(base_url))
                page.ele("#download").click()
                data = page.wait.download_begin(timeout=5.0, cancel_it=True)
                self.assertNotEqual(data, False)
                assert isinstance(data, dict)
                self.assertEqual(data["suggested_filename"], "openpage.txt")
                self.assertEqual(data["name"], "openpage.txt")
                self.assertIn(data["tab_id"], page.tab_ids)
                self.assertEqual(data["id"], data["guid"])
                self.assertEqual(data["folder"], tmp_dir)
                self.assertTrue(data["tmp_path"].endswith(data["guid"]))
                self.assertTrue(page.wait.all_downloads_done(timeout=10.0))
            finally:
                page.quit()


if __name__ == "__main__":
    unittest.main()
