import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

import openpage


CHROME = os.environ.get(
    "OPENPAGE_BROWSER_PATH",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
)
HTML = """<!doctype html><html><head><title>OpenPage E2E</title>
<script>
function rerenderDynamic() {
  document.querySelector('#dynamic').innerHTML = `<button class="target" onclick="document.querySelector('#title').textContent='relocated'">Dynamic 2</button>`;
}
function removeDynamic() {
  document.querySelector('#dynamic').innerHTML = '';
}
function rerenderXpath() {
  document.querySelector('#xpath-dynamic').innerHTML = `<button class="xpath-target" onclick="document.querySelector('#title').textContent='xpath-relocated'">XPath 2</button>`;
}
function duplicateTarget() {
  document.querySelector('#ambiguous').innerHTML = `<button class="target">A</button><button class="target">B</button>`;
}
function replaceList() {
  document.querySelector('#listed').innerHTML = `<button class="item">Replacement</button>`;
}
function requestAborted() {
  const image = new Image();
  image.src = '/aborted';
}
window.addEventListener('DOMContentLoaded', () => {
  const root = document.querySelector('#shadow-host').attachShadow({mode: 'open'});
  root.innerHTML = `<button id="shadow-button" onclick="this.textContent='Shadow clicked'">Shadow</button>`;
});
</script></head><body>
<input id='name'>
<label for='email'>Email</label><input id='email'>
<input id='search' placeholder='Search'>
<button id='semantic-submit' aria-label='Submit'>Send</button>
<button data-testid='checkout'>Checkout</button>
<div id='semantic-scope'><button aria-label='Scoped'>Inside</button></div>
<input id='hidden-input' hidden>
<div style='position:relative'><input id='covered-input'><div style='position:absolute;inset:0;z-index:1'></div></div>
<button id='submit' onclick="document.querySelector('#title').textContent=document.querySelector('#name').value">Go</button>
<h1 id='title'>empty</h1><a id='link' href='next'>next</a>
<button id='hidden' hidden onclick="document.querySelector('#title').textContent='hidden-clicked'">Hidden</button>
<div style='position:relative'><button id='covered' onclick="document.querySelector('#title').textContent='covered-clicked'">Covered</button><div style='position:absolute;inset:0;z-index:1'></div></div>
<div id='content'><span class='child'>child</span></div>
<form id='form' onsubmit="event.preventDefault();document.querySelector('#title').textContent='submitted'"><input id='form-input'></form>
<div id='hoverable' onmouseover="document.querySelector('#title').textContent='hovered'">Hover</div>
<div style='position:relative'><div id='covered-hover'>Covered hover</div><div style='position:absolute;inset:0;z-index:1'></div></div>
<div id='dynamic'><button class='target' onclick="document.querySelector('#title').textContent='relocated'">Dynamic</button></div>
<button id='rerender' onclick="rerenderDynamic()">Rerender</button>
<button id='remove-dynamic' onclick="removeDynamic()">Remove dynamic</button>
<div id='xpath-dynamic'><button class='xpath-target' onclick="document.querySelector('#title').textContent='xpath-relocated'">XPath</button></div>
<button id='rerender-xpath' onclick="rerenderXpath()">Rerender XPath</button>
<div id='ambiguous'><button class='target'>One</button></div>
<button id='duplicate' onclick="duplicateTarget()">Duplicate</button>
<div id='listed'><button class='item'>Listed</button></div>
<button id='replace-list' onclick="replaceList()">Replace list</button>
<button id='request-aborted' onclick="requestAborted()">Abort request</button>
<input id='upload' type='file' onchange="document.querySelector('#title').textContent=this.files[0].name">
<button id='popup' onclick="window.open('/popup.html')">Popup</button>
<button id='dialog' onclick="alert('OpenPage dialog')">Dialog</button>
<button id='read-state' onclick="document.querySelector('#title').textContent=[localStorage.getItem('local'),sessionStorage.getItem('session'),document.cookie].join('|')">Read state</button>
<iframe id='child-frame' src='/frame.html'></iframe>
<div id='shadow-host'></div>
</body></html>"""

FRAME_HTML = """<!doctype html><html><head><title>Child Frame</title></head><body>
<button id='frame-button' onclick="this.textContent='Frame clicked'">Frame</button>
</body></html>"""

POPUP_HTML = """<!doctype html><html><head><title>Popup Page</title></head><body>Popup</body></html>"""


@unittest.skipUnless(Path(CHROME).is_file(), "Chrome/Chromium is not installed")
class BrowserEndToEndTests(unittest.TestCase):
    def test_local_http_page_navigation_queries_interaction_and_screenshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "index.html").write_text(HTML, encoding="utf-8")
            Path(directory, "frame.html").write_text(FRAME_HTML, encoding="utf-8")
            Path(directory, "popup.html").write_text(POPUP_HTML, encoding="utf-8")
            with socket.socket() as probe:
                probe.bind(("127.0.0.1", 0))
                port = probe.getsockname()[1]
            server = subprocess.Popen(
                [sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1"],
                cwd=directory,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            try:
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                            break
                    except OSError:
                        time.sleep(0.05)
                else:
                    self.fail("local HTTP server did not start")

                browser = openpage.Browser.launch(
                    browser_path=CHROME,
                    headless=True,
                    user_data_path=str(Path(directory, "profile")),
                )
                page = browser.new_page()
                try:
                    listener = page.listen()
                    listener.start(targets=["index.html"], methods=["GET"])
                    page.goto(f"http://127.0.0.1:{port}/index.html")
                    packet = listener.wait(timeout_ms=2_000)
                    self.assertEqual(packet.method, "GET")
                    self.assertEqual(packet.response.status, 200)
                    self.assertIn(b"OpenPage E2E", packet.response.body)
                    listener.stop()
                    self.assertEqual(page.title(), "OpenPage E2E")
                    self.assertEqual(page.text("#title"), "empty")
                    self.assertTrue(page.attr("#link", "href").endswith("/next"))
                    self.assertEqual(page.find("#content").find(".child").text(), "child")
                    self.assertEqual(page.find("text=empty").attr("id"), "title")
                    self.assertEqual(
                        page.find("role=button[name='Submit']").attr("id"),
                        "semantic-submit",
                    )
                    self.assertEqual(page.find("label=Email").attr("id"), "email")
                    self.assertEqual(page.find("placeholder=Search").attr("id"), "search")
                    self.assertEqual(page.find("testid=checkout").text(), "Checkout")
                    self.assertEqual(
                        page.find("#semantic-scope")
                        .find("role=button[name='Scoped']")
                        .text(),
                        "Inside",
                    )
                    page.input("#name", "hello", timeout_ms=1_000)
                    page.find("#name").clear(timeout_ms=1_000)
                    page.find("#name").input("world", timeout_ms=1_000)
                    page.click("#submit", timeout_ms=1_000)
                    self.assertEqual(page.text("#title"), "world")
                    with self.assertRaisesRegex(RuntimeError, "not visible, enabled, or editable"):
                        page.find("#hidden-input").input("ignored")
                    with self.assertRaisesRegex(RuntimeError, "not visible, enabled, or editable"):
                        page.find("#covered-input").input("ignored")
                    with self.assertRaisesRegex(RuntimeError, "has no rect") as failure:
                        page.click("#hidden", timeout_ms=1_000)
                    self.assertEqual(failure.exception.kind, "page_operation")
                    self.assertEqual(failure.exception.operation, "click")
                    self.assertEqual(failure.exception.locator, "#hidden")
                    self.assertEqual(failure.exception.timeout, 1_000)
                    self.assertIn("index.html", failure.exception.url)
                    self.assertEqual(failure.exception.matched_count, 1)
                    self.assertEqual(failure.exception.element_state, "not actionable")
                    self.assertIn("has no rect", failure.exception.failure_reason)
                    self.assertEqual(page.text("#title"), "world")
                    with self.assertRaisesRegex(RuntimeError, "covered"):
                        page.find("#covered").click()
                    self.assertEqual(page.text("#title"), "world")
                    page.find("#hoverable").hover(timeout_ms=1_000)
                    self.assertEqual(page.text("#title"), "hovered")
                    with self.assertRaisesRegex(RuntimeError, "hover failed"):
                        page.find("#covered-hover").hover()
                    page.find("#form-input").submit()
                    self.assertEqual(page.text("#title"), "submitted")
                    with self.assertRaisesRegex(RuntimeError, "not associated with a form"):
                        page.find("#content").submit()

                    dynamic = page.find("#dynamic").find(".target")
                    page.click("#rerender")
                    self.assertEqual(page.text("#dynamic .target"), "Dynamic 2")
                    self.assertFalse(dynamic.is_alive())
                    dynamic.click()
                    self.assertEqual(page.text("#title"), "relocated")

                    missing = page.find("#dynamic").find(".target")
                    page.click("#remove-dynamic")
                    with self.assertRaisesRegex(RuntimeError, "relocation query matched no element"):
                        missing.click()

                    xpath = page.find("xpath://div[@id='xpath-dynamic']").find(
                        "xpath:.//button"
                    )
                    page.click("#rerender-xpath")
                    self.assertFalse(xpath.is_alive())
                    xpath.click()
                    self.assertEqual(page.text("#title"), "xpath-relocated")

                    ambiguous = page.find("#ambiguous").find(".target")
                    page.click("#duplicate")
                    with self.assertRaisesRegex(RuntimeError, "relocation is ambiguous"):
                        ambiguous.click()

                    listed = page.find("#listed").find_all(".item")[0]
                    page.click("#replace-list")
                    with self.assertRaisesRegex(RuntimeError, "detached"):
                        listed.click()

                    frame = page.frame("#child-frame", timeout_ms=2_000)
                    self.assertEqual(frame.title(), "Child Frame")
                    self.assertIn("frame.html", frame.url())
                    frame.find("#frame-button").click()
                    self.assertEqual(frame.find("#frame-button").text(), "Frame clicked")

                    shadow = page.find("#shadow-host").shadow_root()
                    self.assertIsNotNone(shadow)
                    self.assertIn("shadow-button", shadow.html())
                    shadow.find("#shadow-button").click()
                    self.assertEqual(shadow.find("#shadow-button").text(), "Shadow clicked")
                    self.assertEqual(shadow.host().attr("id"), "shadow-host")
                    self.assertTrue(shadow.snapshot())

                    upload = Path(directory, "upload.txt")
                    upload.write_text("upload", encoding="utf-8")
                    self.assertTrue(
                        page.click_to_upload("#upload", [str(upload)], timeout_ms=2_000)
                    )
                    self.assertEqual(page.text("#title"), "upload.txt")

                    popup = page.click_for_new_page("#popup", timeout_ms=2_000)
                    self.assertIsNotNone(popup)
                    self.assertEqual(popup.title(), "Popup Page")
                    popup.close()

                    dialog_errors = []

                    def open_dialog() -> None:
                        try:
                            page.click("#dialog")
                        except Exception as exc:
                            dialog_errors.append(exc)

                    dialog_thread = threading.Thread(target=open_dialog)
                    dialog_thread.start()
                    deadline = time.monotonic() + 2
                    while time.monotonic() < deadline and not page.has_alert():
                        time.sleep(0.02)
                    self.assertTrue(page.has_alert())
                    self.assertEqual(page.alert_text(), "OpenPage dialog")
                    self.assertEqual(page.handle_alert(True), "OpenPage dialog")
                    dialog_thread.join(2)
                    self.assertFalse(dialog_thread.is_alive())
                    self.assertFalse(dialog_errors)

                    page.set_cookie("openpage", "cookie", url=page.url())
                    page.set_local_storage("local", "L")
                    page.set_session_storage("session", "S")
                    page.click("#read-state")
                    self.assertIn("L|S|", page.text("#title"))
                    self.assertIn("openpage=cookie", page.text("#title"))
                    self.assertIn("openpage=cookie", page.cookie_header())

                    page.set_zoom_factor(1.25)
                    self.assertAlmostEqual(page.zoom_factor(), 1.25)
                    page.reset_zoom_factor()
                    self.assertAlmostEqual(page.zoom_factor(), 1.0)
                    downloaded = Path(directory, "downloaded-frame.html")
                    self.assertEqual(
                        Path(page.download_to(f"http://127.0.0.1:{port}/frame.html", str(downloaded))),
                        downloaded,
                    )
                    self.assertIn("Child Frame", downloaded.read_text(encoding="utf-8"))
                    self.assertEqual(page.ready_state(), "complete")
                    self.assertFalse(page.is_loading())
                    self.assertTrue(page.wait_for_doc_loaded(timeout_ms=2_000))

                    mock_page = browser.new_page()
                    interceptor = mock_page.intercept()
                    interceptor.start(targets=["/mock"])
                    mock_errors = []

                    def navigate_mock() -> None:
                        try:
                            mock_page.goto(f"http://127.0.0.1:{port}/mock")
                        except Exception as exc:
                            mock_errors.append(exc)

                    mock_thread = threading.Thread(target=navigate_mock)
                    mock_thread.start()
                    intercepted = interceptor.wait(timeout_ms=2_000)
                    self.assertEqual(intercepted.method, "GET")
                    intercepted.fulfill(
                        200,
                        b"<html><body><h1 id='mocked'>Mocked</h1></body></html>",
                        {"content-type": "text/html; charset=utf-8"},
                    )
                    mock_thread.join(2)
                    self.assertFalse(mock_thread.is_alive())
                    self.assertFalse(mock_errors)
                    self.assertEqual(mock_page.text("#mocked"), "Mocked")
                    interceptor.stop()
                    mock_page.close()

                    continue_page = browser.new_page()
                    interceptor = continue_page.intercept()
                    interceptor.start(targets=["index.html"])
                    continue_errors = []

                    def navigate_continued() -> None:
                        try:
                            continue_page.goto(
                                f"http://127.0.0.1:{port}/index.html"
                            )
                        except Exception as exc:
                            continue_errors.append(exc)

                    continue_thread = threading.Thread(target=navigate_continued)
                    continue_thread.start()
                    intercepted = interceptor.wait(timeout_ms=2_000)
                    intercepted.continue_request()
                    continue_thread.join(2)
                    self.assertFalse(continue_thread.is_alive())
                    self.assertFalse(continue_errors)
                    self.assertEqual(continue_page.title(), "OpenPage E2E")
                    interceptor.stop()
                    continue_page.close()

                    interceptor = page.intercept()
                    interceptor.start(targets=["/aborted"])
                    page.click("#request-aborted")
                    intercepted = interceptor.wait(timeout_ms=2_000)
                    self.assertEqual(intercepted.resource_type, "Image")
                    intercepted.abort()
                    interceptor.stop()

                    self.assertIn("OpenPage E2E", page.html())
                    self.assertTrue(page.snapshot())
                    screenshot = Path(directory) / "screenshot.png"
                    page.save_screenshot(str(screenshot))
                    self.assertGreater(screenshot.stat().st_size, 0)
                finally:
                    page.close()
                    browser.close()
            finally:
                server.terminate()
                server.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
