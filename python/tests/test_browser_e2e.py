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
HTML = """<!doctype html><html><head><title>OpenPage E2E</title></head><body>
<input id='name'>
<input id='hidden-input' hidden>
<div style='position:relative'><input id='covered-input'><div style='position:absolute;inset:0;z-index:1'></div></div>
<button id='submit' onclick="document.querySelector('#title').textContent=document.querySelector('#name').value">Go</button>
<h1 id='title'>empty</h1><a id='link' href='next'>next</a>
<button id='hidden' hidden onclick="document.querySelector('#title').textContent='hidden-clicked'">Hidden</button>
<div style='position:relative'><button id='covered' onclick="document.querySelector('#title').textContent='covered-clicked'">Covered</button><div style='position:absolute;inset:0;z-index:1'></div></div>
<div id='content'><span class='child'>child</span></div>
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
                    page.input("#name", "hello")
                    page.find("#name").clear()
                    page.input("#name", "world")
                    page.click("#submit")
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
