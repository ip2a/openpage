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
pub mod options_manager;
pub mod page;
pub mod screencast;
pub mod session;
pub mod settings;
pub mod shadow_root;
pub mod tools;
pub mod upload;
pub mod webpage;
pub mod window;

#[cfg(feature = "python-module")]
pub mod python;

pub use alert::AlertTracker;
pub use browser::{
    Browser, BrowserTabReference, BrowserTabSelector, BrowserTabTargetsInput, BrowserTabTypeInput,
    DownloadFileExistsMode, LaunchOptions, LoadMode, TabInfo,
};
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
pub use options_manager::OptionsManager;
pub use page::{
    Actions, ActionsDragData, ActionsInput, ActionsTarget, DisconnectedFrame, DisconnectedPage,
    Frame, FrameRect, FrameScroller, FrameSetter, FrameStates, FrameWait, Page, PageElementContent,
    PageElementInfo, PageElementTarget, PageFrameTarget, PageLoadModeSetter, PageSaveContent,
    PageScroller, PageSetter, PageWindowSetter,
};
pub use screencast::{Screencast, ScreencastMode};
pub use session::{
    CookieEntry, CookieInput, SessionAdapter, SessionAdapterMount, SessionCert, SessionCookie,
    SessionCookieParam, SessionDownload, SessionElement, SessionHandle, SessionHookEvent,
    SessionHooks, SessionOptions, SessionPage, SessionRequestOptions, SessionResponseHook,
    SessionResponseInfo, SessionRuntimeInfo, SessionXPathResult,
};
pub use settings::{Settings, SettingsSnapshot};
pub use shadow_root::ShadowRoot;
pub use tools::{
    BlobSource, By, Keys, MakeSessionEleLocator, MakeSessionEleResult, MakeSessionEleSource,
    TreeSource, TreeTextInput, configs_to_here, get_blob, make_session_ele, make_session_ele_by,
    tree, wait_until,
};
pub use upload::UploadTracker;
pub use webpage::{
    DisconnectedWebPage, WebElement, WebElementClicker, WebElementScroller, WebElementSelector,
    WebElementSetter, WebFrame, WebMode, WebPage, WebPageLoadModeSetter, WebPageScroller,
    WebPageSetter, WebPageWindowSetter,
};
pub use window::{activate_app, set_app_visibility};

pub type Chromium = Browser;
pub type ChromiumElement = Element;
pub type ChromiumFrame = Frame;
pub type ChromiumOptions = LaunchOptions;
pub type ChromiumPage = Page;
pub type ChromiumTab = Page;
pub type MixTab = WebPage;

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
        Browser, Chromium, ChromiumElement, ChromiumFrame, ChromiumOptions, ChromiumPage,
        ChromiumTab, Element, Frame, LaunchOptions, MixTab, Page, WebPage,
    };

    #[test]
    fn chromium_aliases_match_core_types() {
        let _ = (|_: &Chromium, _: &Browser| {}) as fn(&Chromium, &Browser);
        let _ = (|_: &ChromiumPage, _: &Page| {}) as fn(&ChromiumPage, &Page);
        let _ = (|_: &ChromiumTab, _: &Page| {}) as fn(&ChromiumTab, &Page);
        let _ = (|_: &MixTab, _: &WebPage| {}) as fn(&MixTab, &WebPage);
        let _ = (|_: &ChromiumFrame, _: &Frame| {}) as fn(&ChromiumFrame, &Frame);
        let _ = (|_: &ChromiumElement, _: &Element| {}) as fn(&ChromiumElement, &Element);
        let _ =
            (|_: &ChromiumOptions, _: &LaunchOptions| {}) as fn(&ChromiumOptions, &LaunchOptions);
    }
}
