use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "openpage", about = "Rust OpenPage browser control CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the long-lived NDJSON protocol over stdin/stdout.
    Serve(ServeArgs),
    /// Manage named browser sessions for one-command-at-a-time control.
    Browser {
        #[command(subcommand)]
        command: BrowserCommand,
    },
    /// Control the current page in a named browser session.
    Page {
        #[command(subcommand)]
        command: PageCommand,
    },
    /// Control elements on the current page in a named browser session.
    Ele {
        #[command(subcommand)]
        command: ElementCommand,
    },
    /// Run JavaScript on the current page in a named browser session.
    Js(JsArgs),
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, default_value_t = true)]
    pub stdio: bool,
}

#[derive(Debug, Subcommand)]
pub enum BrowserCommand {
    Start(BrowserStartArgs),
    Stop(SessionArgs),
    Status(SessionArgs),
}

#[derive(Debug, Args)]
pub struct BrowserStartArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub browser_path: Option<PathBuf>,
    #[arg(long)]
    pub user_data_dir: Option<PathBuf>,
    #[arg(long)]
    pub port: Option<u16>,
    #[arg(long)]
    pub head: bool,
    #[arg(long)]
    pub headless: bool,
    #[arg(long, default_value_t = 1280)]
    pub width: u32,
    #[arg(long, default_value_t = 900)]
    pub height: u32,
    #[arg(long)]
    pub no_sandbox: bool,
    #[arg(long)]
    pub replace: bool,
}

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum PageCommand {
    New(PageNewArgs),
    Get(PageGetArgs),
    Url(SessionArgs),
    Title(SessionArgs),
    Html(SessionArgs),
    Screenshot(PageScreenshotArgs),
}

#[derive(Debug, Args)]
pub struct PageNewArgs {
    pub url: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct PageGetArgs {
    pub url: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct PageScreenshotArgs {
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub full_page: bool,
}

#[derive(Debug, Subcommand)]
pub enum ElementCommand {
    Text(ElementSelectorArgs),
    Html(ElementSelectorArgs),
    Click(ElementSelectorArgs),
    Input(ElementInputArgs),
    Attr(ElementAttrArgs),
}

#[derive(Debug, Args)]
pub struct ElementSelectorArgs {
    pub locator: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ElementInputArgs {
    pub locator: String,
    pub text: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ElementAttrArgs {
    pub locator: String,
    pub name: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct JsArgs {
    pub script: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}
