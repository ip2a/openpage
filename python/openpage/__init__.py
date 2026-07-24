import json

from .openpage_rs import Browser, Page, Session
from .openpage_rs import diff_text as _diff_text
from .openpage_rs import diff_screenshot as _diff_screenshot

__all__ = ["Browser", "Page", "Session", "open", "diff_text", "diff_screenshot"]


def diff_text(before: str, after: str) -> dict:
    """Diff two text snapshots (Myers algorithm).

    Returns a dict with: ``identical``, ``changed``, ``additions``,
    ``removals``, ``unchanged``, and the unified ``diff`` text.
    """
    return json.loads(_diff_text(before, after))


def diff_screenshot(baseline: bytes, current: bytes, threshold: float = 0.1) -> dict:
    """Diff two screenshot images pixel-by-pixel.

    ``threshold`` is the per-channel color distance in 0.0..=1.0 (fraction of
    255). Returns a dict with: ``matched``, ``mismatch_percentage``,
    ``different_pixels``, ``total_pixels``, and ``dimension_mismatch`` (only
    present when image sizes differ).
    """
    return json.loads(_diff_screenshot(baseline, current, threshold))


def open(url: str) -> Page:
    browser = Browser.launch()
    return browser.new_page(url)
