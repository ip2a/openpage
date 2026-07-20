from openpage import ChromiumPage


def main() -> None:
    page = ChromiumPage()
    try:
        print("Opening browser...")
        page.get("https://www.baidu.com")

        print("url:", page.url)
        print("title:", page.title)
        assert "百度" in page.title, f"Expected Baidu in title, got: {page.title}"

        html = page.html
        print("html length:", len(html))
        assert len(html) > 1000, "Page HTML too short"

        # Find search input and button
        search_input = page.ele("#kw")
        print("search input found:", search_input is not None)
        print("search input text:", search_input.text)

        search_btn = page.ele("#su")
        print("search button found:", search_btn is not None)
        print("search button attr value:", search_btn.attr("value"))

        # Verify element states
        print("input is displayed:", search_input.states.is_displayed)
        print("input is enabled:", search_input.states.is_enabled)
        print("button has rect:", search_btn.states.has_rect)

        # Run JS
        js_result = page.run_js("({title: document.title, url: location.href})")
        print("js result:", js_result)
        assert js_result["title"] == page.title
        assert "baidu.com" in js_result["url"]

        # Screenshot
        import tempfile
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            path = f.name
        page.save_screenshot(path, full_page=False)
        print("screenshot saved to:", path)
        import os
        assert os.path.getsize(path) > 1000, "Screenshot too small"
        os.remove(path)

        # Wait helpers
        assert page.wait.ele_displayed("#kw", timeout=2.0) is not False
        assert page.wait.ele_enabled("#kw", timeout=2.0) is not False

        print("\nAll checks passed. Base链路 is ready.")
    finally:
        page.quit()


if __name__ == "__main__":
    main()
