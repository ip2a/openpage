# openpage

`openpage` is a Rust-first browser automation project with thin Python wrappers.

## Layout

- `rust/`: Rust core and PyO3 extension
- `python/`: Python wrappers, examples, and tests
- `参考项目/`: reference code used for API and architecture study

## Current architecture

- Rust owns:
  - browser launch/connect lifecycle
  - CDP-backed page and element operations
  - requests-backed session fetching and snapshot parsing
  - `WebPage` mode orchestration across browser and session
  - locator parsing
  - cookie header transfer primitives for browser/session sync
  - screenshots, PDF, DOM querying, JS execution
- Python owns:
  - compatibility-oriented wrappers
  - `ChromiumPage` convenience surface
  - object wrappers and JSON/result adaptation
  - examples and integration tests

## Local development

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

## Minimal Python usage

```python
from openpage import ChromiumPage

page = ChromiumPage()
page.get("https://example.com")
print(page.title)
print(page.ele("h1").text)
page.quit()
```

## Session And Mode-Switch Usage

```python
from openpage import SessionPage, WebPage

session = SessionPage()
session.get("https://example.com")
print(session.title)

page = WebPage(mode="d")
page.get("https://httpbin.org/cookies/set?token=openpage")
page.change_mode("s", copy_cookies=True)
print(page.json)
page.quit()
```

## Status

This first version is intentionally browser-first:

- Implemented:
  - `Browser`
  - `ChromiumOptions`
  - `ChromiumPage`
  - `Page`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage`
  - basic tab info
  - `get / ele / eles / run_js / click / input / clear / screenshot / pdf`
  - `post_json`
  - browser/session mode switching
  - current-URL cookie sync between browser and session
- Not yet implemented:
  - advanced network listener parity
  - download manager parity
  - richer session-element traversal parity
  - full setter/wait/state parity with the reference project

## Verification

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

Current integration checks cover:

- browser flow against `data:` pages
- session flow against `https://example.com` and `https://httpbin.org/json`
- `WebPage` browser -> session cookie sync
- `WebPage` session -> browser cookie sync
- Python `WebPage` thin-wrapper flow over the Rust `WebPage` core
