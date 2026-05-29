use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "openpage", about = "OpenPage — Agent-friendly browser automation CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage the browser session
    #[command(subcommand)]
    Browser(BrowserCommand),
    /// Navigate to a URL
    Goto(GotoArgs),
    /// Go back in browser history
    Back(SessionArgs),
    /// Go forward in browser history
    Forward(SessionArgs),
    /// Reload the current page
    Reload(SessionArgs),
    /// Stop loading the current page
    StopLoading(SessionArgs),
    /// Get the current URL
    Url(SessionArgs),
    /// Get the page title
    Title(SessionArgs),
    /// Get the full page HTML
    Html(SessionArgs),
    /// Get a compact agent-friendly snapshot of the page
    Snapshot(SessionArgs),
    /// Take a screenshot
    Screenshot(ScreenshotArgs),
    /// Click an element by locator or @ref
    Click(ElementArgs),
    /// Fill an input element
    Fill(FillArgs),
    /// Focus an element
    Focus(ElementArgs),
    /// Clear an input or textarea element
    Clear(ElementArgs),
    /// Submit a form or form-associated element
    Submit(ElementArgs),
    /// Ensure a checkbox or radio is checked
    Check(ElementArgs),
    /// Ensure a checkbox or radio is unchecked
    Uncheck(ElementArgs),
    /// Right-click an element
    RightClick(ElementArgs),
    /// Middle-click an element
    MiddleClick(ElementArgs),
    /// Double-click an element
    DoubleClick(ElementArgs),
    /// Dispatch a page-level keydown event
    KeyDown(KeyArgs),
    /// Dispatch a page-level keyup event
    KeyUp(KeyArgs),
    /// Send a page-level keyboard shortcut or key sequence
    Shortcut(ShortcutArgs),
    /// Insert text into the currently focused element
    Input(PageTextArgs),
    /// Type text into the currently focused element
    Type(PageTextArgs),
    /// Type text into the currently focused element with an interval between keys
    TypeWithInterval(TypeWithIntervalArgs),
    /// Drag an element by offset
    Drag(DragArgs),
    /// Drag an element onto another element
    DragTo(DragToArgs),
    /// Drag an element to a viewport point
    DragToPoint(DragToPointArgs),
    /// Drop text or files into a target
    DragIn(DragInArgs),
    /// Get element text content
    Text(ElementArgs),
    /// Get an element attribute
    Attr(AttrArgs),
    /// Wait for a condition (element, navigation, etc.)
    Wait(WaitArgs),
    /// Manage network interception
    #[command(subcommand)]
    Intercept(InterceptCommand),
    /// Run JavaScript on the page
    Js(JsArgs),
    /// Download a file
    Download(DownloadArgs),
    /// Manage the browser window
    #[command(subcommand)]
    Window(WindowCommand),
    /// Handle browser dialogs
    #[command(subcommand)]
    Alert(AlertCommand),
    /// Scroll the page
    Scroll(ScrollArgs),
    /// Scroll an element into view
    ScrollIntoView(ScrollIntoViewArgs),
    /// Hover over an element
    Hover(ElementArgs),
    /// Press a key on an element
    Press(PressArgs),
    /// Select options in a <select> element
    Select(SelectArgs),
    /// Upload files to a file input
    Upload(UploadArgs),
    /// Click an element and wait for a download to begin
    ClickToDownload(ClickToDownloadArgs),
    /// Click an element and upload files through the browser picker flow
    ClickToUpload(ClickToUploadArgs),
    /// Click an element and switch to the newly opened tab
    ClickForNewTab(ClickForNewTabArgs),
    /// Check if an element is visible
    IsVisible(ElementArgs),
    /// Check if an element is enabled
    IsEnabled(ElementArgs),
    /// Check if an element is checked
    IsChecked(ElementArgs),
    /// Check if an element is selected
    IsSelected(ElementArgs),
    /// Check if an element is alive in the DOM
    IsAlive(ElementArgs),
    /// Check if an element is in the viewport
    IsInViewport(ElementArgs),
    /// Check if an element is fully in the viewport
    IsWholeInViewport(ElementArgs),
    /// Check if an element is covered by another element
    IsCovered(ElementArgs),
    /// Check if an element is clickable
    IsClickable(ElementArgs),
    /// Find an element and return basic info
    Find(ElementArgs),
    /// Find all matching elements and return basic info
    FindAll(ElementArgs),
    /// Count matching elements
    Count(ElementArgs),
    /// Wait for an element to become visible
    WaitVisible(WaitElementArgs),
    /// Wait for an element to become hidden
    WaitHidden(WaitElementArgs),
    /// Wait for an element to become enabled
    WaitEnabled(WaitElementArgs),
    /// Wait for an element to become disabled
    WaitDisabled(WaitElementArgs),
    /// Wait for an element to be deleted
    WaitDeleted(WaitElementArgs),
    /// Wait for an element to become clickable
    WaitClickable(WaitElementArgs),
    /// Get the currently focused element
    ActiveElement(SessionArgs),
    /// Wait for URL to contain text
    WaitForUrl(WaitForUrlArgs),
    /// Wait for title to contain text
    WaitForTitle(WaitForTitleArgs),
    /// Wait for a JS function to return true
    WaitForFunction(WaitForFunctionArgs),
    /// Wait for an element to contain text
    WaitForText(WaitForTextArgs),
    /// Save page as PDF
    Pdf(PdfArgs),
    /// Manage localStorage and sessionStorage
    #[command(subcommand)]
    Storage(StorageCommand),
    /// Manage cookies
    #[command(subcommand)]
    Cookies(CookiesCommand),
    /// Manage browser tabs
    #[command(subcommand)]
    Tab(TabCommand),
    /// Manage frames
    #[command(subcommand)]
    Frame(FrameCommand),
    /// Diagnose the local OpenPage CLI environment
    Doctor(DoctorArgs),
    /// Execute multiple commands in one invocation
    Batch(BatchArgs),
    /// Start the long-lived NDJSON daemon
    Serve(ServeArgs),
}

#[derive(Debug, Subcommand)]
pub enum BrowserCommand {
    /// Start the browser (creates a new session if needed)
    Start(BrowserStartArgs),
    /// Stop the browser session
    Stop(SessionArgs),
    /// Check browser session status
    Status(SessionArgs),
    /// List daemon-backed browser sessions and sidecar audit state
    List,
}

#[derive(Debug, Args)]
pub struct BrowserStartArgs {
    /// Optional initial URL to open
    pub url: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub browser_path: Option<PathBuf>,
    #[arg(long)]
    pub user_data_dir: Option<PathBuf>,
    #[arg(long)]
    pub port: Option<u16>,
    /// Run in headed mode (show browser window)
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
    /// Replace existing session if it exists
    #[arg(long)]
    pub replace: bool,
}

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct GotoArgs {
    pub url: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    /// Wait for navigation to complete
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct ScreenshotArgs {
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub full_page: bool,
}

#[derive(Debug, Args)]
pub struct ElementArgs {
    pub locator: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct FillArgs {
    pub locator: String,
    pub text: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct AttrArgs {
    pub locator: String,
    pub name: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DragArgs {
    pub locator: String,
    #[arg(long)]
    pub dx: f64,
    #[arg(long)]
    pub dy: f64,
    #[arg(long, default_value_t = 0.2)]
    pub duration: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DragToArgs {
    pub source: String,
    pub target: String,
    #[arg(long, default_value_t = 0.2)]
    pub duration: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DragToPointArgs {
    pub locator: String,
    #[arg(long)]
    pub x: f64,
    #[arg(long)]
    pub y: f64,
    #[arg(long, default_value_t = 0.2)]
    pub duration: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DragInArgs {
    pub target: String,
    #[arg(long, conflicts_with = "files")]
    pub text: Option<String>,
    #[arg(long, num_args = 1.., conflicts_with = "text")]
    pub files: Vec<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct KeyArgs {
    pub key: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ShortcutArgs {
    #[arg(required = true, num_args = 1..)]
    pub keys: Vec<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct PageTextArgs {
    pub text: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct TypeWithIntervalArgs {
    pub text: String,
    #[arg(long, default_value_t = 0.1)]
    pub interval: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WaitElementArgs {
    pub locator: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct WaitArgs {
    pub condition: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Subcommand)]
pub enum InterceptCommand {
    Start(SessionArgs),
    Stop(SessionArgs),
    Status(SessionArgs),
}

#[derive(Debug, Args)]
pub struct JsArgs {
    pub script: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DownloadArgs {
    pub url: String,
    pub output: Option<PathBuf>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum WindowCommand {
    State(SessionArgs),
    Location(SessionArgs),
    Max(SessionArgs),
    Min(SessionArgs),
    Fullscreen(SessionArgs),
    Normal(SessionArgs),
    Hide(SessionArgs),
    Show(SessionArgs),
    Size(WindowSizeArgs),
    Move(WindowMoveArgs),
}

#[derive(Debug, Args)]
pub struct WindowSizeArgs {
    pub width: u32,
    pub height: u32,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WindowMoveArgs {
    pub left: i64,
    pub top: i64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum AlertCommand {
    Accept(SessionArgs),
    Dismiss(SessionArgs),
    Text(SessionArgs),
}

#[derive(Debug, Args)]
pub struct ScrollArgs {
    pub direction: String,
    #[arg(long)]
    pub pixels: Option<f64>,
    #[arg(long)]
    pub x: Option<f64>,
    #[arg(long)]
    pub y: Option<f64>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ScrollIntoViewArgs {
    pub locator: String,
    #[arg(long)]
    pub center: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct PressArgs {
    pub locator: String,
    pub key: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SelectArgs {
    pub locator: String,
    #[arg(long)]
    pub text: Option<String>,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long)]
    pub index: Option<usize>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct UploadArgs {
    pub locator: String,
    pub files: Vec<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ClickToDownloadArgs {
    pub locator: String,
    #[arg(long)]
    pub dir: Option<PathBuf>,
    #[arg(long)]
    pub rename: Option<String>,
    #[arg(long)]
    pub suffix: Option<String>,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long)]
    pub js: bool,
    #[arg(long)]
    pub new_tab: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ClickToUploadArgs {
    pub locator: String,
    pub files: Vec<String>,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long)]
    pub js: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ClickForNewTabArgs {
    pub locator: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long)]
    pub js: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WaitForUrlArgs {
    pub text: String,
    #[arg(long)]
    pub exclude: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct WaitForTitleArgs {
    pub text: String,
    #[arg(long)]
    pub exclude: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct WaitForFunctionArgs {
    pub script: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long, default_value_t = 200)]
    pub interval: u64,
}

#[derive(Debug, Args)]
pub struct WaitForTextArgs {
    pub locator: String,
    pub text: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long, default_value_t = 200)]
    pub interval: u64,
}

#[derive(Debug, Args)]
pub struct PdfArgs {
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum StorageCommand {
    Get(StorageGetArgs),
    Set(StorageSetArgs),
}

#[derive(Clone, Debug, ValueEnum)]
pub enum StorageScope {
    Local,
    Session,
}

#[derive(Debug, Args)]
pub struct StorageGetArgs {
    pub key: Option<String>,
    #[arg(long, value_enum)]
    pub scope: StorageScope,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct StorageSetArgs {
    pub key: String,
    pub value: Option<String>,
    #[arg(long, value_enum)]
    pub scope: StorageScope,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum CookiesCommand {
    Get(SessionArgs),
    Set(CookiesSetArgs),
    Delete(CookiesDeleteArgs),
    Clear(SessionArgs),
}

#[derive(Debug, Args)]
pub struct CookiesSetArgs {
    pub name: String,
    pub value: String,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct CookiesDeleteArgs {
    pub name: String,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum TabCommand {
    New(TabNewArgs),
    Close(TabCloseArgs),
    List(SessionArgs),
    Switch(TabSwitchArgs),
}

#[derive(Debug, Args)]
pub struct TabNewArgs {
    pub url: Option<String>,
    #[arg(long)]
    pub window: bool,
    #[arg(long)]
    pub background: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct TabCloseArgs {
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub index: Option<usize>,
    #[arg(long)]
    pub others: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct TabSwitchArgs {
    pub target: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum FrameCommand {
    List(SessionArgs),
    Switch(FrameSwitchArgs),
}

#[derive(Debug, Args)]
pub struct FrameSwitchArgs {
    pub target: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    /// TCP port for NDJSON daemon mode. Use `0` for an OS-assigned port.
    #[arg(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Args)]
pub struct BatchArgs {
    /// Stop on the first failing command
    #[arg(long)]
    pub bail: bool,
    /// Quoted commands to execute. If omitted, reads JSON from stdin as an array of argv arrays.
    pub commands: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Skip the live browser launch smoke test
    #[arg(long)]
    pub quick: bool,
    /// Apply deterministic non-CDP cleanup fixes such as removing legacy session JSON files
    #[arg(long)]
    pub fix: bool,
}
