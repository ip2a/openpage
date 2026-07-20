use std::time::Duration;

use openpage_rs::{Browser, LaunchOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch(LaunchOptions {
        user_data_dir: Some("/Users/yuuu/.openpage/browser/user_data".into()),
        headless: false,
        ..LaunchOptions::default()
    })?;

    let page = browser.new_page(None)?;
    page.goto("https://www.baidu.com")?;

    println!("url: {}", page.url()?);
    println!("title: {:?}", page.title()?);

    // Wait for chat-textarea to appear
    let ele = page.wait_for("#chat-textarea", 5000)?;
    println!("Found #chat-textarea");
    println!(
        "  html: {}",
        ele.html()?
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>()
    );
    println!("  is_displayed: {}", ele.is_displayed()?);
    println!("  is_enabled: {}", ele.is_enabled()?);

    // List textareas
    let textareas = page.find_all("textarea")?;
    println!("\nFound {} textarea(s)", textareas.len());
    for (i, ta) in textareas.iter().enumerate() {
        println!(
            "  [{}] id={:?} class={:?}",
            i,
            ta.attr("id")?,
            ta.attr("class")?
        );
    }

    std::thread::sleep(Duration::from_secs(1));
    browser.close()?;
    println!("\nDone.");
    Ok(())
}
