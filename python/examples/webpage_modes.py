from openpage import ChromiumOptions
from openpage import WebPage


def main() -> None:
    page = WebPage(mode="d", chromium_options=ChromiumOptions())
    try:
        page.get("https://httpbin.org/cookies/set?token=openpage")
        page.get("https://httpbin.org/cookies")
        print("driver mode body:", page.ele("body").text)

        page.change_mode("s", go=True, copy_cookies=True)
        print("session mode json:", page.json)

        page.get("https://httpbin.org/cookies/set?token=session")
        page.get("https://httpbin.org/cookies")
        page.change_mode("d", go=True, copy_cookies=True)
        print("driver mode after session sync:", page.ele("body").text)
    finally:
        page.quit()


if __name__ == "__main__":
    main()
