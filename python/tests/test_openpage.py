from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from urllib.parse import quote

from openpage import Browser
from openpage import ChromiumOptions
from openpage import ChromiumPage


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


if __name__ == "__main__":
    unittest.main()
