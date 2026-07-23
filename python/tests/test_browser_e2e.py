import os
import tempfile
import unittest
from pathlib import Path

import openpage


CHROME = os.environ.get(
    "OPENPAGE_BROWSER_PATH",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
)


@unittest.skipUnless(Path(CHROME).is_file(), "Chrome/Chromium is not installed")
class BrowserEndToEndTests(unittest.TestCase):
    def test_local_page_navigation_queries_interaction_and_screenshot(self) -> None:
        html = """<!doctype html><html><head><title>OpenPage E2E</title></head><body>
        <input id='name'>
        <button id='submit' onclick="document.querySelector('#title').textContent=document.querySelector('#name').value">Go</button>
        <h1 id='title'>empty</h1><a id='link' href='next'>next</a>
        <div id='content'><span class='child'>child</span></div>
        </body></html>"""
        with tempfile.TemporaryDirectory() as directory:
            document = Path(directory) / "index.html"
            document.write_text(html, encoding="utf-8")
            browser = openpage.Browser.launch(
                browser_path=CHROME,
                headless=True,
                user_data_path=str(Path(directory) / "profile"),
            )
            try:
                page = browser.new_page()
                try:
                    page.goto(document.as_uri())
                    self.assertEqual(page.title(), "OpenPage E2E")
                    self.assertEqual(page.text("#title"), "empty")
                    self.assertTrue(page.attr("#link", "href").endswith("/next"))
                    self.assertEqual(page.find("#content").find(".child").text(), "child")
                    page.input("#name", "hello")
                    page.click("#submit")
                    self.assertEqual(page.text("#title"), "hello")
                    self.assertTrue(page.snapshot())
                    screenshot = Path(directory) / "screenshot.png"
                    page.save_screenshot(str(screenshot))
                    self.assertGreater(screenshot.stat().st_size, 0)
                finally:
                    page.close()
            finally:
                browser.close()


if __name__ == "__main__":
    unittest.main()
