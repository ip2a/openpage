from openpage import ChromiumOptions, ChromiumPage


def main() -> None:
    opts = ChromiumOptions().set_user_data_path("/Users/yuuu/.openpage/browser/user_data")
    page = ChromiumPage(opts)
    try:
        print("Browser launched with user data from ~/.openpage")
        page.get("https://www.baidu.com")
        print("url:", page.url)
        print("title:", page.title)

        # Try to find chat-textarea by id
        print("\n--- Looking for #chat-textarea ---")
        try:
            ele = page.ele("#chat-textarea")
            print("Found #chat-textarea:")
            print("  tag:", ele.attr("tagName"))
            print("  text:", ele.text)
            print("  html:", ele.html[:200] if ele.html else None)
        except Exception as e:
            print("#chat-textarea not found:", e)

        # List all textarea elements on the page
        print("\n--- All textarea elements ---")
        textareas = page.eles("textarea")
        print(f"Found {len(textareas)} textarea(s)")
        for i, ta in enumerate(textareas):
            print(f"  [{i}] tag={ta.attr('tagName')} id={ta.attr('id')} class={ta.attr('class')} name={ta.attr('name')}")

        # List all elements with id containing 'chat'
        print("\n--- Elements with id containing 'chat' ---")
        import json
        ids = page.run_js(
            """
            Array.from(document.querySelectorAll('[id*="chat"], [class*="chat"]'))
                .map(el => ({id: el.id, class: el.className, tag: el.tagName}))
            """
        )
        print(f"Found {len(ids)} element(s) with id/class containing 'chat':")
        for item in ids:
            print(f"  tag={item.get('tag')} id={item.get('id')} class={item.get('class')}")

        # Try to find any input-like area
        print("\n--- Input-like elements ---")
        inputs = page.eles("input, textarea, [contenteditable='true']")
        print(f"Found {len(inputs)} input-like element(s)")
        for i, inp in enumerate(inputs[:10]):
            print(f"  [{i}] tag={inp.attr('tagName')} id={inp.attr('id')} class={inp.attr('class')}")

        print("\nDone.")
    finally:
        page.quit()


if __name__ == "__main__":
    main()
