use std::ffi::OsStr;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::browser::Browser;
use crate::cli::args::{
    BrowserCommand, BrowserStartArgs, Command, ElementCommand, PageCommand, SessionArgs,
};
use crate::cli::protocol::simple_ok;
use crate::error::{OpenPageError, OpenPageResult};
use crate::page::Page;

pub fn run(command: Command) -> OpenPageResult<()> {
    match command {
        Command::Browser { command } => run_browser(command),
        Command::Page { command } => run_page(command),
        Command::Ele { command } => run_element(command),
        Command::Js(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(json!({"value": page.run_js(&args.script)?})))
        }
        Command::Serve(_) => unreachable!("serve is handled by cli::serve"),
    }
}

fn run_browser(command: BrowserCommand) -> OpenPageResult<()> {
    match command {
        BrowserCommand::Start(args) => start_browser(args),
        BrowserCommand::Stop(args) => stop_browser(args, false),
        BrowserCommand::Status(args) => {
            let status = match load_session(&args.session) {
                Ok(record) => match debugger_version(&record.debugger_url) {
                    Ok(version) => json!({
                        "session": args.session,
                        "alive": true,
                        "debugger_url": record.debugger_url,
                        "version": version,
                        "target": record.default_target_id,
                    }),
                    Err(err) => json!({
                        "session": args.session,
                        "alive": false,
                        "error": err.to_string(),
                    }),
                },
                Err(_) => json!({"session": args.session, "alive": false}),
            };
            print_json(simple_ok(status))
        }
    }
}

fn run_page(command: PageCommand) -> OpenPageResult<()> {
    match command {
        PageCommand::New(args) => {
            let (browser, _page, mut record) = open_page(&args.session)?;
            let page = browser.new_page(args.url.as_deref())?;
            record.default_target_id = Some(page.target_id());
            save_session(&record)?;
            print_json(simple_ok(json!({"target": record.default_target_id})))
        }
        PageCommand::Get(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            page.goto(&args.url)?;
            print_json(simple_ok(json!({"loaded": true, "url": page.url()?})))
        }
        PageCommand::Url(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(json!({"url": page.url()?})))
        }
        PageCommand::Title(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(json!({"title": page.title()?})))
        }
        PageCommand::Html(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(json!({"html": page.html()?})))
        }
        PageCommand::Screenshot(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            page.save_screenshot(&args.output, args.full_page)?;
            print_json(simple_ok(json!({"saved": true, "output": args.output})))
        }
    }
}

fn run_element(command: ElementCommand) -> OpenPageResult<()> {
    match command {
        ElementCommand::Text(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(json!({"text": page.text(&args.locator)?})))
        }
        ElementCommand::Html(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(
                json!({"html": page.wait_for(&args.locator, 10_000)?.html()?}),
            ))
        }
        ElementCommand::Click(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            page.click(&args.locator)?;
            print_json(simple_ok(json!({"clicked": true})))
        }
        ElementCommand::Input(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            page.fill(&args.locator, &args.text)?;
            print_json(simple_ok(json!({"input": true})))
        }
        ElementCommand::Attr(args) => {
            let (_browser, page, _record) = open_page(&args.session)?;
            print_json(simple_ok(
                json!({"value": page.attr(&args.locator, &args.name)?}),
            ))
        }
    }
}

fn start_browser(args: BrowserStartArgs) -> OpenPageResult<()> {
    if !args.replace {
        if let Ok(record) = load_session(&args.session) {
            if debugger_version(&record.debugger_url).is_ok() {
                return print_json(simple_ok(json!({
                    "session": args.session,
                    "already_running": true,
                    "debugger_url": record.debugger_url,
                    "target": record.default_target_id,
                })));
            }
        }
    }

    if args.replace {
        let stop_args = SessionArgs {
            session: args.session.clone(),
        };
        let _ = stop_browser(stop_args, true);
    }

    let port = match args.port {
        Some(port) => port,
        None => free_port()?,
    };
    let debugger_url = format!("http://127.0.0.1:{port}");
    let user_data_dir = args
        .user_data_dir
        .clone()
        .unwrap_or(session_profile_dir(&args.session)?);
    fs::create_dir_all(&user_data_dir)?;

    let browser_path = args
        .browser_path
        .clone()
        .or_else(default_browser_path)
        .ok_or_else(|| {
            OpenPageError::BrowserLaunch(
                "could not find a Chrome/Chromium executable; pass --browser-path".to_string(),
            )
        })?;

    let mut command = ProcessCommand::new(&browser_path);
    command
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg(format!("--window-size={},{}", args.width, args.height))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-background-networking")
        .arg("--disable-popup-blocking")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let headless = args.headless && !args.head;
    if args.no_sandbox {
        command.arg("--no-sandbox").arg("--disable-setuid-sandbox");
    }
    if headless {
        command
            .arg("--headless=new")
            .arg("--hide-scrollbars")
            .arg("--mute-audio");
    }
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command.arg("about:blank");

    let child = command
        .spawn()
        .map_err(|err| OpenPageError::BrowserLaunch(err.to_string()))?;

    wait_for_debugger(&debugger_url, Duration::from_secs(15))?;
    let record = SessionRecord {
        name: args.session.clone(),
        debugger_url: debugger_url.clone(),
        user_data_dir: Some(user_data_dir),
        default_target_id: current_target_id(&debugger_url)?,
        pid: Some(child.id()),
        created_at_ms: now_ms(),
    };
    save_session(&record)?;
    print_json(simple_ok(json!({
        "session": record.name,
        "debugger_url": record.debugger_url,
        "target": record.default_target_id,
        "pid": record.pid,
        "headless": headless,
    })))
}

fn stop_browser(args: SessionArgs, quiet: bool) -> OpenPageResult<()> {
    let record = load_session(&args.session)?;
    match Browser::connect(&record.debugger_url).and_then(|browser| browser.close()) {
        Ok(()) => {}
        Err(_) => {
            if let Some(pid) = record.pid {
                let _ = kill_process(pid);
            }
        }
    }
    let _ = fs::remove_file(session_file(&args.session)?);
    if quiet {
        Ok(())
    } else {
        print_json(simple_ok(json!({"stopped": true, "session": args.session})))
    }
}

fn open_page(session: &str) -> OpenPageResult<(Browser, Page, SessionRecord)> {
    let mut record = load_session(session)?;
    let browser = Browser::connect(&record.debugger_url)?;

    if let Some(target_id) = &record.default_target_id {
        if let Ok(page) = browser.get_page(target_id) {
            return Ok((browser, page, record));
        }
    }

    if let Some(page) = browser.pages()?.into_iter().next() {
        record.default_target_id = Some(page.target_id());
        save_session(&record)?;
        return Ok((browser, page, record));
    }

    let page = browser.new_page(None)?;
    record.default_target_id = Some(page.target_id());
    save_session(&record)?;
    Ok((browser, page, record))
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionRecord {
    name: String,
    debugger_url: String,
    user_data_dir: Option<PathBuf>,
    default_target_id: Option<String>,
    pid: Option<u32>,
    created_at_ms: u128,
}

fn save_session(record: &SessionRecord) -> OpenPageResult<()> {
    let path = session_file(&record.name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(record)
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    fs::write(path, data)?;
    Ok(())
}

fn load_session(session: &str) -> OpenPageResult<SessionRecord> {
    let path = session_file(session)?;
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|err| OpenPageError::Serialization(err.to_string()))
}

fn session_file(session: &str) -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?
        .join("sessions")
        .join(format!("{session}.json")))
}

fn session_profile_dir(session: &str) -> OpenPageResult<PathBuf> {
    Ok(openpage_home()?.join("profiles").join(session))
}

fn openpage_home() -> OpenPageResult<PathBuf> {
    if let Some(value) = std::env::var_os("OPENPAGE_HOME") {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(value).join(".openpage"));
    }
    Err(OpenPageError::Io(
        "OPENPAGE_HOME or HOME must be set".to_string(),
    ))
}

fn free_port() -> OpenPageResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn wait_for_debugger(url: &str, timeout: Duration) -> OpenPageResult<()> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/version", url.trim_end_matches('/'));
    let deadline = Instant::now() + timeout;
    loop {
        if client
            .get(&endpoint)
            .send()
            .is_ok_and(|resp| resp.status().is_success())
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(OpenPageError::Timeout(format!(
                "browser debugger did not become ready: {endpoint}"
            )));
        }
        sleep(Duration::from_millis(100));
    }
}

fn debugger_version(url: &str) -> OpenPageResult<Value> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/version", url.trim_end_matches('/'));
    client
        .get(&endpoint)
        .send()
        .map_err(|err| OpenPageError::Http(err.to_string()))?
        .json()
        .map_err(|err| OpenPageError::Serialization(err.to_string()))
}

#[derive(Debug, Deserialize)]
struct DebugTarget {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

fn current_target_id(url: &str) -> OpenPageResult<Option<String>> {
    let client = reqwest::blocking::ClientBuilder::new()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|err| OpenPageError::Http(err.to_string()))?;
    let endpoint = format!("{}/json/list", url.trim_end_matches('/'));
    let targets: Vec<DebugTarget> = client
        .get(&endpoint)
        .send()
        .map_err(|err| OpenPageError::Http(err.to_string()))?
        .json()
        .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    Ok(targets
        .into_iter()
        .find(|target| target.kind == "page")
        .map(|target| target.id))
}

fn kill_process(pid: u32) -> OpenPageResult<()> {
    #[cfg(unix)]
    {
        let _ = ProcessCommand::new("kill")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| OpenPageError::Io(err.to_string()))?;
    }
    #[cfg(windows)]
    {
        let _ = ProcessCommand::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|err| OpenPageError::Io(err.to_string()))?;
    }
    Ok(())
}

fn default_browser_path() -> Option<PathBuf> {
    for var in ["OPENPAGE_BROWSER_PATH", "CHROME", "CHROME_PATH"] {
        if let Some(path) = std::env::var_os(var).map(PathBuf::from) {
            if path.exists() {
                return Some(path);
            }
        }
    }

    for path in platform_browser_paths() {
        if path.exists() {
            return Some(path);
        }
    }

    find_on_path("google-chrome")
        .or_else(|| find_on_path("chromium"))
        .or_else(|| find_on_path("chromium-browser"))
        .or_else(|| find_on_path("chrome"))
        .or_else(|| find_on_path("msedge"))
}

fn platform_browser_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
    ]
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let path = dir.join(name);
        if is_executable(&path) {
            return Some(path);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    path.is_file() || path.extension() == Some(OsStr::new("exe"))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn print_json(value: Value) -> OpenPageResult<()> {
    println!(
        "{}",
        serde_json::to_string(&value)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::Cli;

    #[test]
    fn parses_browser_start() {
        let cli = Cli::try_parse_from([
            "openpage",
            "browser",
            "start",
            "--session",
            "agent",
            "--head",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            crate::cli::args::Command::Browser { .. }
        ));
    }

    #[test]
    fn parses_page_get() {
        Cli::try_parse_from([
            "openpage",
            "page",
            "get",
            "https://example.com",
            "--session",
            "agent",
        ])
        .unwrap();
    }
}
