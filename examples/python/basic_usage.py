from openpage import Browser


def main() -> None:
    browser = Browser.launch()
    try:
        page = browser.new_page("https://example.com")
        print("title:", page.text("title"))
        print("h1:", page.text("h1"))
    finally:
        browser.close()


if __name__ == "__main__":
    main()
