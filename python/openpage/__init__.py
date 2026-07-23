from .openpage_rs import Browser, Page, Session

__all__ = ["Browser", "Page", "Session", "open"]


def open(url: str) -> Page:
    browser = Browser.launch()
    return browser.new_page(url)
