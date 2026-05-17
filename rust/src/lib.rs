pub mod browser;
pub mod download;
pub mod element;
pub mod error;
pub mod listener;
pub mod locator;
pub mod page;
pub mod session;
pub mod webpage;

#[cfg(feature = "python-module")]
pub mod python;

pub use browser::{Browser, LaunchOptions};
pub use download::{DownloadInfo, DownloadMission, DownloadState};
pub use element::Element;
pub use error::{OpenPageError, OpenPageResult};
pub use listener::{Listener, ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerResponse};
pub use locator::{Locator, LocatorKind};
pub use page::Page;
pub use session::{CookieEntry, SessionElement, SessionOptions, SessionPage};
pub use webpage::{WebElement, WebMode, WebPage};

#[cfg(feature = "python-module")]
use pyo3::prelude::*;

#[cfg(feature = "python-module")]
#[pymodule]
fn openpage_rs(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}
