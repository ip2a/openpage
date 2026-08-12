def test_public_facade():
    import openpage

    assert openpage.__all__ == [
        "Browser",
        "Page",
        "Session",
        "open",
        "diff_text",
        "diff_screenshot",
    ]
    assert openpage.Session().__class__.__name__ == "Session"


def test_browser_exposes_cdp_attach_api():
    import openpage

    assert callable(openpage.Browser.connect)
    assert callable(openpage.Browser.attach)
