import time

from openpage import ChromiumOptions
from openpage import WebPage


def ensure_get(page: WebPage, url: str, attempts: int = 3) -> None:
    last_status = None
    for attempt in range(attempts):
        if page.get(url):
            return
        last_status = page.status_code
        if attempt + 1 < attempts:
            time.sleep(1.0)
    raise RuntimeError(f"GET {url} failed after {attempts} attempts, last status={last_status}")


def main() -> None:
    page = WebPage(mode="d", chromium_options=ChromiumOptions())
    try:
        ensure_get(page, "https://httpbin.org/cookies/set?token=openpage")
        ensure_get(page, "https://httpbin.org/cookies")
        print("driver mode body:", page.ele("body").text)

        page.change_mode("s", go=True, copy_cookies=True)
        print("session mode json:", page.json)

        ensure_get(page, "https://httpbin.org/cookies/set?token=session")
        ensure_get(page, "https://httpbin.org/cookies")
        page.change_mode("d", go=True, copy_cookies=True)
        print("driver mode after session sync:", page.ele("body").text)
    finally:
        page.quit()


if __name__ == "__main__":
    main()
