pub mod browser;
pub mod element;
pub mod error;
pub mod locator;
pub mod page;
pub mod python;
pub mod session;

pub use browser::{Browser, LaunchOptions};
pub use element::Element;
pub use error::{OpenPageError, OpenPageResult};
pub use locator::{Locator, LocatorKind};
pub use page::Page;
pub use session::{SessionElement, SessionOptions, SessionPage};

use pyo3::prelude::*;

#[pymodule]
fn openpage_rs(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}
