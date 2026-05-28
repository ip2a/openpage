pub mod alert;
pub mod browser;
pub mod cli;
pub mod console;
pub mod download;
pub mod element;
pub mod element_list;
pub mod error;
pub mod intercept;
pub mod listener;
pub mod locator;
pub mod page;
pub mod screencast;
pub mod session;
pub mod shadow_root;
pub mod tools;
pub mod upload;
pub mod webpage;
pub mod window;

#[cfg(feature = "python-module")]
pub mod python;

pub use alert::AlertTracker;
pub use browser::{Browser, DownloadFileExistsMode, LaunchOptions, LoadMode, TabInfo};
pub use console::{Console, ConsoleMessage, ConsoleSteps};
pub use download::{DownloadInfo, DownloadMission, DownloadState};
pub use element::{
    Element, ElementClicker, ElementResource, ElementScroller, ElementSelector, ElementSetter,
};
pub use element_list::{
    ElementListDriverItem, ElementListItem, ElementListSearchItem, ElementListStateItem,
    ElementsFilter, ElementsFilterOne, ElementsGetter, ElementsListExt, ElementsOne,
    ElementsOneOwned, ElementsSearch,
};
pub use error::{OpenPageError, OpenPageResult};
pub use intercept::{InterceptedRequest, InterceptedRequestInfo, Interceptor};
pub use listener::{
    Listener, ListenerAssociatedCookie, ListenerBlockedSetCookie, ListenerExemptedSetCookie,
    ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerRequestExtraInfo, ListenerResponse,
    ListenerResponseExtraInfo, ListenerSteps,
};
pub use locator::{Locator, LocatorInput, LocatorKind, LocatorMatch};
pub use page::{
    Actions, ActionsDragData, ActionsInput, ActionsTarget, Frame, FrameRect, FrameScroller,
    FrameSetter, FrameStates, FrameWait, Page, PageElementContent, PageElementInfo,
    PageElementTarget, PageFrameTarget,
};
pub use screencast::{Screencast, ScreencastMode};
pub use session::{
    CookieEntry, SessionCert, SessionCookie, SessionCookieParam, SessionDownload, SessionElement,
    SessionOptions, SessionPage, SessionRequestOptions, SessionResponseInfo, SessionRuntimeInfo,
    SessionXPathResult,
};
pub use shadow_root::ShadowRoot;
pub use tools::{
    BlobSource, By, Keys, MakeSessionEleLocator, MakeSessionEleResult, MakeSessionEleSource,
    TreeSource, configs_to_here, get_blob, make_session_ele, make_session_ele_by, tree, wait_until,
};
pub use upload::UploadTracker;
pub use webpage::{
    DisconnectedWebPage, WebElement, WebElementClicker, WebElementScroller, WebElementSelector,
    WebElementSetter, WebFrame, WebMode, WebPage,
};
pub use window::{activate_app, set_app_visibility};

pub type Chromium = Browser;
pub type ChromiumElement = Element;
pub type ChromiumFrame = Frame;
pub type ChromiumOptions = LaunchOptions;
pub type ChromiumPage = Page;

#[cfg(feature = "python-module")]
use pyo3::prelude::*;

#[cfg(feature = "python-module")]
#[pymodule]
fn openpage_rs(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}

#[cfg(test)]
mod tests {
    use super::{
        Browser, Chromium, ChromiumElement, ChromiumFrame, ChromiumOptions, ChromiumPage, Element,
        Frame, LaunchOptions, Page,
    };

    #[test]
    fn chromium_aliases_match_core_types() {
        let _ = (|_: &Chromium, _: &Browser| {}) as fn(&Chromium, &Browser);
        let _ = (|_: &ChromiumPage, _: &Page| {}) as fn(&ChromiumPage, &Page);
        let _ = (|_: &ChromiumFrame, _: &Frame| {}) as fn(&ChromiumFrame, &Frame);
        let _ = (|_: &ChromiumElement, _: &Element| {}) as fn(&ChromiumElement, &Element);
        let _ =
            (|_: &ChromiumOptions, _: &LaunchOptions| {}) as fn(&ChromiumOptions, &LaunchOptions);
    }
}
