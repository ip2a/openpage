import os
import socket
import subprocess
import sys
import tempfile
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
</body></html>"""


@unittest.skipUnless(Path(CHROME).is_file(), "Chrome/Chromium is not installed")
class BrowserEndToEndTests(unittest.TestCase):
    def test_local_http_page_navigation_queries_interaction_and_screenshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "index.html").write_text(HTML, encoding="utf-8")
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
                    page.goto(f"http://127.0.0.1:{port}/")
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
                    with self.assertRaisesRegex(RuntimeError, "has no rect"):
                        page.find("#hidden").click()
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
