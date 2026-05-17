use std::io::Error;
use std::thread::sleep;
use std::time::Duration;

use openpage_rs::{LaunchOptions, SessionOptions, WebMode, WebPage};

fn ensure_get(page: &WebPage, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    for attempt in 0..3 {
        if page.get(url)? {
            return Ok(());
        }
        if attempt < 2 {
            sleep(Duration::from_secs(1));
        }
    }

    Err(Error::other(format!("GET {url} failed after 3 attempts")).into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let page = WebPage::new(
        WebMode::Driver,
        LaunchOptions::default(),
        SessionOptions::default(),
    )?;

    ensure_get(&page, "https://httpbin.org/cookies/set?token=openpage")?;
    ensure_get(&page, "https://httpbin.org/cookies")?;
    println!("driver mode body: {:?}", page.find("body")?.text()?);

    page.change_mode(Some(WebMode::Session), true, true)?;
    println!("session mode json: {:?}", page.json()?);

    ensure_get(&page, "https://httpbin.org/cookies/set?token=session")?;
    ensure_get(&page, "https://httpbin.org/cookies")?;
    page.change_mode(Some(WebMode::Driver), true, true)?;
    println!(
        "driver mode after session sync: {:?}",
        page.find("body")?.text()?
    );

    page.quit()?;
    Ok(())
}
