from openpage import ChromiumPage


def main() -> None:
    page = ChromiumPage()
    try:
        page.get("https://example.com")
        print("url:", page.url)
        print("title:", page.title)
        print("h1:", page.ele("h1").text)
        print("js:", page.run_js("({title: document.title, href: location.href})"))
    finally:
        page.quit()


if __name__ == "__main__":
    main()
