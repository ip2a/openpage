use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "openpage",
    version,
    about = "OpenPage — Agent-friendly browser automation CLI",
    long_about = "OpenPage — Agent-friendly browser automation CLI.\n\nActive execution modes: TCP-backed daemon for CLI commands and stdio MCP for tool clients. Browser operations remain daemon-backed.",
    after_help = "Use `openpage serve` for long-lived NDJSON TCP control, `openpage mcp` for MCP stdio, `openpage doctor` for local environment checks, and the normal `openpage ...` commands for daemon-backed one-shot control.\n\nBootstrap commands: `browser start` and `goto` may create the daemon-backed session when it is missing. Other `--session` commands require an already active session and fail fast instead of silently starting a fresh browser.\n\nRemoved on purpose and rejected: `serve --stdio`, `page get`, `page url`, `page title`, `page screenshot`."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage the browser session
    #[command(subcommand)]
    Browser(BrowserCommand),
    /// Navigate to a URL, bootstrapping the session if needed
    Goto(GotoArgs),
    /// Go back in browser history
    Back(SessionArgs),
    /// Go forward in browser history
    Forward(SessionArgs),
    /// Reload the current page
    Reload(ReloadArgs),
    /// Stop loading the current page
    StopLoading(SessionArgs),
    /// Get the current URL
    Url(SessionArgs),
    /// Get the page title
    Title(SessionArgs),
    /// Get the current page user agent
    UserAgent(SessionArgs),
    /// Get the current page HTTP status code
    StatusCode(SessionArgs),
    /// Get the current document readyState
    ReadyState(SessionArgs),
    /// Check whether the page is currently loading
    IsLoading(SessionArgs),
    /// Check whether the current page session is headless
    IsHeadless(SessionArgs),
    /// Get the full page HTML
    Html(SessionArgs),
    /// Get a compact agent-friendly snapshot of the page
    Snapshot(SnapshotArgs),
    /// Diff two snapshots or screenshots (no session required, pure computation)
    #[command(subcommand)]
    Diff(DiffCommand),
    /// Take a screenshot
    Screenshot(ScreenshotArgs),
    /// Take a screenshot of a specific element
    ScreenshotElement(ScreenshotElementArgs),
    /// Click an element by locator or @ref
    Click(ClickArgs),
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
    /// Click an element at an optional offset
    ClickAt(ClickAtArgs),
    /// Dispatch a page-level keydown event
    KeyDown(KeyArgs),
    /// Dispatch a page-level keyup event
    KeyUp(KeyArgs),
    /// Send a page-level keyboard shortcut or key sequence
    Shortcut(ShortcutArgs),
    /// Select all content in the currently focused element or active selection
    SelectAll(SessionArgs),
    /// Copy the current selection
    Copy(SessionArgs),
    /// Cut the current selection
    Cut(SessionArgs),
    /// Paste clipboard contents into the focused target
    Paste(SessionArgs),
    /// Read or write clipboard text
    #[command(subcommand)]
    Clipboard(ClipboardCommand),
    /// Undo the last edit action
    Undo(SessionArgs),
    /// Redo the last undone edit action
    Redo(SessionArgs),
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
    /// Get an element's current value
    Value(ElementArgs),
    /// Get raw text content from an element
    RawText(ElementArgs),
    /// Get the resolved href/src-like URL from an element
    Link(ElementArgs),
    /// Open an element's link target in a new tab or window
    OpenLink(OpenLinkArgs),
    /// Get the number of direct child elements
    ChildCount(ElementArgs),
    /// Get the element's CSS path
    CssPath(ElementArgs),
    /// Get the element's XPath
    Xpath(ElementArgs),
    /// Get an element's outer HTML
    ElementHtml(ElementArgs),
    /// Get the currently selected text
    SelectedText(SessionArgs),
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
    /// Inspect browser-managed downloads
    #[command(subcommand)]
    Downloads(DownloadsCommand),
    /// Manage page zoom
    #[command(subcommand)]
    Zoom(ZoomCommand),
    /// Manage the browser window
    #[command(subcommand)]
    Window(WindowCommand),
    /// Handle browser dialogs
    #[command(subcommand)]
    Alert(AlertCommand),
    /// Scroll the page
    Scroll(ScrollArgs),
    /// Get the current page scroll position
    ScrollPosition(SessionArgs),
    /// Scroll a scrollable element or pane
    ScrollElement(ElementScrollArgs),
    /// Get the current scroll position of an element
    ScrollElementPosition(ElementArgs),
    /// Scroll an element into view
    ScrollIntoView(ScrollIntoViewArgs),
    /// Hover over an element
    Hover(ElementArgs),
    /// Hover over an element at an optional offset
    HoverAt(HoverAtArgs),
    /// Press a key on an element
    Press(PressArgs),
    /// Select options in a <select> element
    Select(SelectArgs),
    /// List all option texts from a <select> element
    OptionTexts(ElementArgs),
    /// Get the first selected option text from a <select> element
    SelectedOption(ElementArgs),
    /// Get all selected option texts from a <select> element
    SelectedOptions(ElementArgs),
    /// Select all options in a multi-select element
    SelectAllOptions(ElementArgs),
    /// Clear all selected options in a multi-select element
    ClearSelectedOptions(ElementArgs),
    /// Invert selected options in a multi-select element
    InvertSelectedOptions(ElementArgs),
    /// Select text within an element subtree
    SelectText(SelectTextArgs),
    /// Select a text range within an input or textarea
    SelectRange(SelectRangeArgs),
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
    /// Check if an element has a layout rect
    HasRect(ElementArgs),
    /// Find an element and return basic info
    Find(ElementArgs),
    /// Find text in the current page and jump to a match
    FindInPage(FindInPageArgs),
    /// Find all matching elements and return basic info
    FindAll(ElementArgs),
    /// Resolve a locator chain and return a stable element summary
    Locate(LocateArgs),
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
    /// Wait for an element to gain a layout rect
    WaitHasRect(WaitElementArgs),
    /// Wait for an element to become covered
    WaitCovered(WaitElementArgs),
    /// Wait for an element to become uncovered
    WaitNotCovered(WaitElementArgs),
    /// Wait for an element to stop moving
    WaitStopMoving(WaitElementArgs),
    /// Get the currently focused element
    ActiveElement(SessionArgs),
    /// Wait for a newly opened tab
    WaitForNewTab(WaitTimeoutArgs),
    /// Wait for a download to begin
    WaitForDownloadBegin(WaitTimeoutArgs),
    /// Wait for all downloads to finish
    WaitForDownloadsDone(WaitTimeoutArgs),
    /// Wait for the current alert to close
    WaitForAlertClosed(WaitTimeoutArgs),
    /// Wait for page loading to start
    WaitForLoadStart(WaitTimeoutArgs),
    /// Wait for the current document to finish loading
    WaitForDocLoaded(WaitTimeoutArgs),
    /// Wait until the page is ready for follow-up JS/snapshot commands
    WaitForReady(WaitTimeoutArgs),
    /// Wait until navigation has settled enough for follow-up commands
    WaitForNavigation(WaitForNavigationArgs),
    /// Wait for URL to contain text
    WaitForUrl(WaitForUrlArgs),
    /// Wait for title to contain text
    WaitForTitle(WaitForTitleArgs),
    /// Wait for one or more locators to exist in the DOM
    WaitForElementsLoaded(WaitElementsLoadedArgs),
    /// Wait for a JS function to return true
    WaitForFunction(WaitForFunctionArgs),
    /// Wait for an element to contain text
    WaitForText(WaitForTextArgs),
    /// Wait for an element to become disabled or deleted
    WaitDisabledOrDeleted(WaitElementArgs),
    /// Wait for browser upload paths to be inputted
    WaitUploadPathsInputted(WaitTimeoutArgs),
    /// Save page as an MHTML archive
    Save(SaveArgs),
    /// Save page as PDF
    Pdf(PdfArgs),
    /// Inspect and navigate browser history
    #[command(subcommand)]
    History(HistoryCommand),
    /// Manage localStorage and sessionStorage
    #[command(subcommand)]
    Storage(StorageCommand),
    /// Manage common site permissions for the current page origin
    #[command(subcommand)]
    Permissions(PermissionsCommand),
    /// Clear cache, storage, and/or cookies for the current page
    ClearCache(ClearCacheArgs),
    /// Manage cookies
    #[command(subcommand)]
    Cookies(CookiesCommand),
    /// Manage browser tabs
    #[command(subcommand)]
    Tab(TabCommand),
    /// Manage frames
    #[command(subcommand)]
    Frame(FrameCommand),
    /// Record browser actions into a versioned flow
    #[command(subcommand)]
    Record(RecorderCommand),
    /// Diagnose the local OpenPage CLI environment
    Doctor(DoctorArgs),
    /// Execute multiple commands in one invocation
    Batch(BatchArgs),
    /// Start the long-lived NDJSON TCP daemon
    Serve(ServeArgs),
    /// Start the MCP stdio server
    Mcp(McpArgs),
}

#[derive(Debug, Subcommand)]
pub enum RecorderCommand {
    /// Start recording actions in an active browser session
    Start(SessionArgs),
    /// Stop recording and optionally save the flow JSON
    Stop(RecorderStopArgs),
    /// Replay a flow JSON file
    Replay(RecorderReplayArgs),
    /// Print the current recorded flow
    Steps(SessionArgs),
    /// Print recorder status
    Status(SessionArgs),
    /// Clear recorded steps
    Clear(SessionArgs),
}

#[derive(Debug, Args)]
pub struct RecorderReplayArgs {
    pub flow: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct RecorderStopArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum BrowserCommand {
    /// Start the browser (creates a new session if needed)
    Start(BrowserStartArgs),
    /// Stop the browser session
    Stop(BrowserStopArgs),
    /// Bring the browser window to the front
    Activate(SessionArgs),
    /// Check whether the current browser session is incognito
    IsIncognito(SessionArgs),
    /// Check browser session status
    Status(SessionArgs),
    /// Read the daemon log for a browser session
    Logs(BrowserLogsArgs),
    /// List daemon-backed browser sessions and sidecar audit state
    List,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  openpage browser start --session review --headless https://example.com\n  openpage browser start --session review --headless\n\nFollow-up:\n  If you start with a URL, run `openpage wait-for-ready --session review` and then `openpage snapshot --session review`.\n  If you start without a URL, navigate next with `openpage goto --session review https://example.com`.\n\nReplace semantics:\n  `--replace` restarts an existing named session before launching again.\n  It keeps that session's profile directory unless you explicitly change profile paths.\n\nBootstrap rule:\n  `browser start` may create a missing session. Most other `--session` commands require an already active session."
)]
pub struct BrowserStartArgs {
    /// Optional initial URL to open
    pub url: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
    /// Connect to an already running browser through its CDP debugger URL
    #[arg(
        long,
        value_name = "DEBUGGER_URL",
        conflicts_with_all = [
            "browser_path", "user_data_dir", "port", "head", "headless",
            "width", "height", "no_sandbox", "incognito", "mute", "replace"
        ]
    )]
    pub attach: Option<String>,
    /// Override the browser executable for this launch. If omitted, runtime starts from launch config defaults; OPENPAGE_BROWSER_PATH can still override per-process.
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
    #[arg(long)]
    pub width: Option<u32>,
    #[arg(long)]
    pub height: Option<u32>,
    #[arg(long)]
    pub no_sandbox: bool,
    #[arg(long)]
    pub incognito: bool,
    #[arg(long)]
    pub mute: bool,
    /// Restart an existing named session before launching again; preserves its profile directory
    #[arg(long)]
    pub replace: bool,
}

#[derive(Debug, Args)]
pub struct BrowserStopArgs {
    #[arg(long)]
    pub session: Option<String>,
    /// Stop every active daemon-backed session discovered in the current OPENPAGE_HOME
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SnapshotArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    /// Snapshot mode: interactive for actions, semantic for headings/content, all for broader visible DOM
    #[arg(long, value_enum, default_value_t = SnapshotMode::Interactive)]
    pub mode: SnapshotMode,
    /// Snapshot output shape inside the JSON result
    #[arg(long, value_enum, default_value_t = SnapshotFormat::Json)]
    pub format: SnapshotFormat,
    /// Include raw element entries in addition to the compact text
    #[arg(long)]
    pub raw: bool,
    /// Omit duplicate refs/text fields from the selected output format
    #[arg(long)]
    pub compact: bool,
    /// Limit traversal depth relative to the selected root
    #[arg(long)]
    pub depth: Option<usize>,
    /// Restrict snapshot traversal to a CSS selector subtree
    #[arg(long)]
    pub selector: Option<String>,
    /// Maximum snapshot content characters returned in this page
    #[arg(long, default_value_t = 20_000)]
    pub max_output: usize,
    /// Entry or character offset for paginated snapshot output
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SnapshotMode {
    Interactive,
    Semantic,
    All,
}

impl SnapshotMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Semantic => "semantic",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SnapshotFormat {
    Text,
    Json,
}

impl SnapshotFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum DiffCommand {
    /// Diff two text snapshots (Myers algorithm, unified output)
    Snapshot(DiffTextArgs),
    /// Diff two screenshot images pixel-by-pixel
    Screenshot(DiffScreenshotArgs),
}

#[derive(Debug, Args)]
pub struct DiffTextArgs {
    /// File with the before/baseline text
    #[arg(long)]
    pub before: PathBuf,
    /// File with the after/current text
    #[arg(long)]
    pub after: PathBuf,
}

#[derive(Debug, Args)]
pub struct DiffScreenshotArgs {
    /// Baseline (before) image file
    #[arg(long)]
    pub baseline: PathBuf,
    /// Current (after) image file
    #[arg(long)]
    pub current: PathBuf,
    /// Per-channel color distance threshold in 0.0..=1.0 (fraction of 255)
    #[arg(long, default_value_t = 0.1)]
    pub threshold: f64,
}

#[derive(Debug, Args)]
pub struct BrowserLogsArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    /// Return only the last N log lines
    #[arg(long)]
    pub tail: Option<usize>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Bootstrap behavior:\n  `goto` may create a missing session before navigating.\n\nExamples:\n  openpage goto --session review https://example.com\n  openpage goto --session review --wait https://example.com\n\nFollow-up:\n  If you omit `--wait`, use the returned `wait_for_navigation.command` or run `openpage wait-for-ready --session review` before `snapshot`, `title`, or `click`."
)]
pub struct GotoArgs {
    /// If the session is missing, this command creates it with daemon-backed defaults before navigating.
    pub url: String,
    #[arg(long, default_value = "default")]
    pub session: String,
    /// Wait for navigation to complete
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct ReloadArgs {
    #[arg(long)]
    pub ignore_cache: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ScreenshotArgs {
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long)]
    pub full_page: bool,
}

#[derive(Debug, Args)]
pub struct ScreenshotElementArgs {
    pub locator: String,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ElementArgs {
    pub locator: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ClickArgs {
    pub locator: String,
    /// Wait for the navigation triggered by this click to finish
    #[arg(long)]
    pub wait_navigation: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct OpenLinkArgs {
    pub locator: String,
    #[arg(long)]
    pub window: bool,
    #[arg(long)]
    pub background: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct FillArgs {
    pub locator: String,
    #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
    pub text: Option<String>,
    /// Read the value from stdin instead of exposing it in process arguments
    #[arg(long)]
    pub stdin: bool,
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
pub struct FindInPageArgs {
    pub text: String,
    #[arg(long)]
    pub backward: bool,
    #[arg(long)]
    pub case_sensitive: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct LocateArgs {
    /// Locator chain, e.g. '@e2 >> parent >> child a'
    #[arg(required = true, num_args = 1..)]
    pub chain: Vec<String>,
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
pub struct ClickAtArgs {
    pub locator: String,
    #[arg(long)]
    pub x: Option<f64>,
    #[arg(long)]
    pub y: Option<f64>,
    #[arg(long, default_value = "left")]
    pub button: String,
    #[arg(long, default_value_t = 1)]
    pub count: u32,
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
    #[arg(long)]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct WaitTimeoutArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct WaitForNavigationArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
    #[arg(long)]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct WaitElementsLoadedArgs {
    #[arg(required = true, num_args = 1..)]
    pub locators: Vec<String>,
    #[arg(long)]
    pub any_one: bool,
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
pub enum DownloadsCommand {
    /// List tracked browser downloads
    List(SessionArgs),
    /// Show the most recent browser download
    Last(SessionArgs),
    /// Clear tracked finished browser downloads
    Clear(SessionArgs),
    /// Cancel a tracked browser download by guid
    Cancel(DownloadsCancelArgs),
    /// Open a downloaded file with the OS default app
    Open(DownloadsOpenArgs),
    /// Reveal a downloaded file in its containing folder
    Reveal(DownloadsOpenArgs),
    /// Show the current tab download directory
    Path(SessionArgs),
    /// Set the current tab download directory
    SetPath(DownloadsPathArgs),
    /// Show the current download same-name file handling mode
    Mode(SessionArgs),
    /// Set the current download same-name file handling mode
    SetMode(DownloadsModeArgs),
    /// Wait for a browser download to finish
    Wait(WaitForDownloadArgs),
}

#[derive(Debug, Args)]
pub struct DownloadsPathArgs {
    pub path: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DownloadsCancelArgs {
    pub guid: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DownloadsOpenArgs {
    pub guid: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct DownloadsModeArgs {
    pub mode: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WaitForDownloadArgs {
    pub filename: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
    #[arg(long, default_value_t = 10000)]
    pub timeout: u64,
}

#[derive(Debug, Subcommand)]
pub enum WindowCommand {
    /// List open browser windows
    List(SessionArgs),
    /// Switch to a browser window by index or window id
    Switch(WindowSwitchArgs),
    /// Close a browser window by index or window id; defaults to the current window
    Close(WindowCloseArgs),
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

#[derive(Debug, Subcommand)]
pub enum ZoomCommand {
    /// Get the current page zoom factor
    Get(SessionArgs),
    /// Increase the current page zoom factor
    In(ZoomStepArgs),
    /// Decrease the current page zoom factor
    Out(ZoomStepArgs),
    /// Set the current page zoom factor
    Set(ZoomSetArgs),
    /// Reset page zoom to the browser default
    Reset(SessionArgs),
}

#[derive(Debug, Args)]
pub struct ZoomSetArgs {
    pub factor: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct ZoomStepArgs {
    #[arg(long, default_value_t = 0.1)]
    pub step: f64,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum ClipboardCommand {
    /// Read text from the browser clipboard
    Read(SessionArgs),
    /// Write text into the browser clipboard
    Write(ClipboardWriteArgs),
}

#[derive(Debug, Args)]
pub struct ClipboardWriteArgs {
    pub text: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WindowSwitchArgs {
    pub target: String,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct WindowCloseArgs {
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub index: Option<usize>,
    #[arg(long, default_value = "default")]
    pub session: String,
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
    Accept(AlertHandleArgs),
    Dismiss(AlertHandleArgs),
    Has(SessionArgs),
    Text(SessionArgs),
}

#[derive(Debug, Args)]
pub struct AlertHandleArgs {
    #[arg(long)]
    pub prompt_text: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
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
pub struct ElementScrollArgs {
    pub locator: String,
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
pub struct HoverAtArgs {
    pub locator: String,
    #[arg(long)]
    pub x: Option<f64>,
    #[arg(long)]
    pub y: Option<f64>,
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
    #[arg(long, conflicts_with_all = ["value", "index"])]
    pub text: Vec<String>,
    #[arg(long, conflicts_with_all = ["text", "index"])]
    pub value: Vec<String>,
    #[arg(long, conflicts_with_all = ["text", "value"])]
    pub index: Vec<usize>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SelectTextArgs {
    pub locator: String,
    #[arg(long)]
    pub start: Option<usize>,
    #[arg(long)]
    pub end: Option<usize>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct SelectRangeArgs {
    pub locator: String,
    pub start: usize,
    pub end: usize,
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
pub struct SaveArgs {
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct PdfArgs {
    pub output: PathBuf,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// List navigation history entries for the current tab
    List(SessionArgs),
    /// Jump to a specific history entry by 1-based index
    Go(HistoryGoArgs),
    /// Clear navigation history for the current tab
    Clear(SessionArgs),
}

#[derive(Debug, Args)]
pub struct HistoryGoArgs {
    pub index: usize,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Subcommand)]
pub enum StorageCommand {
    Get(StorageGetArgs),
    Set(StorageSetArgs),
}

#[derive(Debug, Subcommand)]
pub enum PermissionsCommand {
    /// Override a common site permission for the current page origin
    Set(PermissionSetArgs),
    /// Reset permission overrides in the current browser context
    Reset(SessionArgs),
}

#[derive(Debug, Args)]
pub struct PermissionSetArgs {
    #[arg(value_enum)]
    pub name: PermissionName,
    #[arg(value_enum)]
    pub setting: PermissionSettingValue,
    #[arg(long)]
    pub origin: Option<String>,
    #[arg(long)]
    pub embedded_origin: Option<String>,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionName {
    ClipboardRead,
    ClipboardWrite,
    Geolocation,
    Notifications,
    Camera,
    Microphone,
}

impl PermissionName {
    pub fn as_descriptor_name(self) -> &'static str {
        match self {
            Self::ClipboardRead => "clipboard-read",
            Self::ClipboardWrite => "clipboard-write",
            Self::Geolocation => "geolocation",
            Self::Notifications => "notifications",
            Self::Camera => "camera",
            Self::Microphone => "microphone",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionSettingValue {
    Granted,
    Denied,
    Prompt,
}

impl PermissionSettingValue {
    pub fn as_cdp_value(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Prompt => "prompt",
        }
    }
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

#[derive(Debug, Args)]
pub struct ClearCacheArgs {
    #[arg(long)]
    pub session_storage: bool,
    #[arg(long)]
    pub local_storage: bool,
    #[arg(long)]
    pub cache: bool,
    #[arg(long)]
    pub cookies: bool,
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
    Duplicate(TabDuplicateArgs),
    Reopen(TabReopenArgs),
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
pub struct TabDuplicateArgs {
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub index: Option<usize>,
    #[arg(long)]
    pub window: bool,
    #[arg(long)]
    pub background: bool,
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
pub struct TabReopenArgs {
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
pub struct McpArgs {
    /// Browser session used by MCP tool calls
    #[arg(long, default_value = "default")]
    pub session: String,
}

#[derive(Debug, Args)]
#[command(
    after_help = "TCP-only daemon mode. Use `--port 0` for an OS-assigned port. The active OpenPage daemon protocol remains unique; `serve --stdio` was removed. MCP stdio is exposed separately by `openpage mcp`."
)]
pub struct ServeArgs {
    #[arg(long, default_value = "default")]
    pub session: String,
    /// TCP port for NDJSON daemon mode. Use `0` for an OS-assigned port.
    #[arg(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  openpage batch --bail \"browser start --session review --headless https://example.com\" \"wait-for-ready --session review\" \"snapshot --session review\" \"browser stop --session review\"\n\n  printf '%s\\n' '[ [\"browser\",\"start\",\"--session\",\"review\",\"--headless\",\"https://example.com\"], [\"title\",\"--session\",\"review\"], [\"browser\",\"stop\",\"--session\",\"review\"] ]' | openpage batch\n\nOutput:\n  Returns one JSON envelope with a `result.commands` array. `--bail` stops after the first failing command."
)]
pub struct BatchArgs {
    /// Stop on the first failing command
    #[arg(long)]
    pub bail: bool,
    /// Return one structured entry per command (enabled by default)
    #[arg(long, default_value_t = true)]
    pub per_command: bool,
    /// Quoted commands to execute. If omitted, reads JSON from stdin as an array of argv arrays.
    pub commands: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Skip the live browser launch smoke test. OPENPAGE_BROWSER_PATH still affects browser-path resolution in doctor checks.
    #[arg(long)]
    pub quick: bool,
    /// Apply deterministic daemon cleanup fixes
    #[arg(long)]
    pub fix: bool,
}
