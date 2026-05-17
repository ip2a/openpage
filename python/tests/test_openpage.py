from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from urllib.parse import quote

from openpage import Browser
from openpage import ChromiumOptions
from openpage import ChromiumPage
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


def data_url() -> str:
    return "data:text/html," + quote(HTML)


class OpenPageIntegrationTest(unittest.TestCase):
    def test_browser_and_page_flow(self) -> None:
        page = ChromiumPage(ChromiumOptions())
        try:
            self.assertTrue(page.get(data_url()))
            self.assertEqual(page.title, "")
            self.assertEqual(page.ele("h1").text, "OpenPage")
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

    def test_session_page_flow(self) -> None:
        page = SessionPage(SessionOptions())
        self.assertTrue(page.get("https://example.com"))
        self.assertEqual(page.title, "Example Domain")
        self.assertEqual(page.ele("h1").text, "Example Domain")
        self.assertEqual(page.status_code, 200)

        self.assertTrue(page.get("https://httpbin.org/json"))
        self.assertIn("slideshow", page.json)

    def test_webpage_mode_switch_and_cookie_sync(self) -> None:
        page = WebPage(mode="d", chromium_options=ChromiumOptions())
        try:
            self.assertEqual(page.mode, "d")
            self.assertTrue(page.get("https://httpbin.org/cookies/set?token=browser"))
            self.assertTrue(page.get("https://httpbin.org/cookies"))
            page.change_mode("s", go=True, copy_cookies=True)
            self.assertEqual(page.mode, "s")
            self.assertEqual(page.json["cookies"]["token"], "browser")

            self.assertTrue(page.get("https://httpbin.org/cookies/set?token=session"))
            self.assertTrue(page.get("https://httpbin.org/cookies"))
            page.change_mode("d", go=True, copy_cookies=True)
            self.assertEqual(page.mode, "d")
            self.assertIn('"token": "session"', page.ele("body").text or "")
        finally:
            page.quit()


if __name__ == "__main__":
    unittest.main()
