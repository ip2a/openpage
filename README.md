# openpage

`openpage` is a Rust-first browser automation project with thin Python wrappers.

## Layout

- `rust/`: Rust core, direct examples, and optional PyO3 extension
- `python/`: Python wrappers, examples, and tests
- `参考项目/`: reference code used for API and architecture study

## Current architecture

- Rust owns:
  - browser launch/connect lifecycle
  - CDP-backed page and element operations
  - requests-backed session fetching and snapshot parsing
  - snapshot DOM node identity and relative traversal for session-backed elements
  - `WebPage` mode orchestration across browser and session
  - locator parsing
  - cookie header transfer primitives for browser/session sync
  - screenshots, PDF, DOM querying, JS execution
- Python owns:
  - compatibility-oriented wrappers
  - `ChromiumPage` convenience surface
  - object wrappers and JSON/result adaptation
  - examples and integration tests

The crate now builds as a pure Rust library by default. PyO3 bindings are enabled only with the
`python-module` feature.

## Local development

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

## Pure Rust Build

```bash
cargo check --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml --features python-module
cargo run --manifest-path rust/Cargo.toml --example webpage_modes
```

## Minimal Rust usage

```rust
use openpage_rs::{LaunchOptions, SessionOptions, WebMode, WebPage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let page = WebPage::new(
        WebMode::Driver,
        LaunchOptions::default(),
        SessionOptions::default(),
    )?;
    page.get("https://example.com")?;
    println!("{:?}", page.find("h1")?.text()?);
    page.quit()?;
    Ok(())
}
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
  - snapshot `s_ele / s_eles` queries for browser, session, and `WebPage`
  - snapshot root lookup plus `parent / children / prev / next / tag / inner_html`
  - `post_json`
  - browser/session mode switching
  - current-URL cookie sync between browser and session
- Not yet implemented:
  - advanced network listener parity
  - download manager parity
  - full session-element traversal parity (`before / after / prevs / nexts / befores / afters`)
  - full setter/wait/state parity with the reference project

## Verification

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

Current integration checks cover:

- pure Rust `cargo check` and `cargo test`
- feature-gated `cargo check --features python-module`
- browser flow against `data:` pages
- session flow against `https://example.com` and `https://httpbin.org/json`
- `WebPage` browser -> session cookie sync
- `WebPage` session -> browser cookie sync
- Python `WebPage` thin-wrapper flow over the Rust `WebPage` core
- direct Python examples
- direct Rust `webpage_modes` example
