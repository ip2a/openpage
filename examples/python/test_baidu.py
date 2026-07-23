from openpage import Browser


def main() -> None:
    browser = Browser.launch()
    try:
        page = browser.new_page("https://www.baidu.com")
        print("title:", page.text("title"))
        print("search input name:", page.attr("#kw", "name"))
        print("search button value:", page.attr("#su", "value"))
    finally:
        browser.close()


if __name__ == "__main__":
    main()
