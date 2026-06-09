pub mod alert;
pub mod browser;
pub mod config;
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

pub use alert::AlertTracker;
pub use browser::{
    Browser, BrowserPageUrlInput, BrowserTabReference, BrowserTabSelector, BrowserTabTargetsInput,
    BrowserTabTypeInput, DownloadFileExistsMode, LaunchOptions, LoadMode,
    OPENPAGE_BROWSER_PATH_ENV, TabInfo, TimeoutConfig,
};
pub use config::{
    ConfigValueSource, OPENPAGE_BROWSER_HEADLESS_ENV, OPENPAGE_BROWSER_HEIGHT_ENV,
    OPENPAGE_BROWSER_NO_SANDBOX_ENV, OPENPAGE_BROWSER_USER_DATA_DIR_ENV,
    OPENPAGE_BROWSER_WIDTH_ENV, OPENPAGE_CONFIG_ENV, OPENPAGE_SESSION_TIMEOUT_SECS_ENV,
    OPENPAGE_SESSION_USER_AGENT_ENV, ResolvedConfig, RuntimeOverrides, browser_exec_candidates,
    load_resolved_config, openpage_home, resolve_browser_executable_path, user_config_path,
    workspace_config_path,
};
pub use console::{Console, ConsoleMessage, ConsoleSteps};
pub use download::{DownloadInfo, DownloadMission, DownloadState};
pub use element::{
    Element, ElementClicker, ElementDragTarget, ElementRect, ElementResource, ElementScroller,
    ElementSelector, ElementSetter, ElementStates, ElementWait, SelectIndexInput,
    SelectOptionInput,
};
pub use element_list::{
    ElementListAttrsItem, ElementListContentItem, ElementListDriverItem, ElementListItem,
    ElementListMetaItem, ElementListSearchItem, ElementListStateItem, ElementsFilter,
    ElementsFilterOne, ElementsGetter, ElementsListExt, ElementsOne, ElementsOneClicker,
    ElementsOneOwned, ElementsOneRect, ElementsOneScroller, ElementsOneSelector, ElementsOneSetter,
    ElementsOneStates, ElementsOneWait, ElementsSearch,
};
pub use error::{OpenPageError, OpenPageResult};
pub use intercept::{InterceptedRequest, InterceptedRequestInfo, Interceptor};
pub use listener::{
    Listener, ListenerAssociatedCookie, ListenerBlockedSetCookie, ListenerExemptedSetCookie,
    ListenerFailInfo, ListenerPacket, ListenerRequest, ListenerRequestExtraInfo, ListenerResponse,
    ListenerResponseExtraInfo, ListenerSteps,
};
pub use locator::{Locator, LocatorBatchInput, LocatorInput, LocatorKind, LocatorMatch};
pub use options_manager::OptionsManager;
pub use page::{
    Actions, ActionsDragData, ActionsInput, ActionsTarget, DisconnectedFrame, DisconnectedPage,
    Frame, FrameCookieSetter, FrameRect, FrameScroller, FrameSetter, FrameStates, FrameWait, Page,
    PageCookieSetter, PageElementContent, PageElementInfo, PageElementTarget, PageFrameTarget,
    PageLoadModeSetter, PageNavigationSnapshot, PageSaveContent, PageScroller, PageSetter,
    PageWindowSetter,
};
pub use screencast::{Screencast, ScreencastMode};
pub use session::{
    CookieEntry, CookieInput, HeadersInput, ParamsInput, SessionAdapter, SessionAdapterMount,
    SessionAuthInput, SessionCert, SessionCertInput, SessionCookie, SessionCookieParam,
    SessionDownload, SessionElement, SessionEncodingInput, SessionHandle, SessionHookEvent,
    SessionHooks, SessionMaxRedirectsInput, SessionOptions, SessionPage, SessionPageSetter,
    SessionProxyInput, SessionRequestOptions, SessionResponseHook, SessionResponseInfo,
    SessionRetryIntervalInput, SessionRetryTimesInput, SessionRuntimeInfo, SessionUserAgentInput,
    SessionXPathResult,
};
pub use settings::{Settings, SettingsChain, SettingsSnapshot};
pub use shadow_root::ShadowRoot;
pub use tools::{
    BlobSource, By, Keys, MakeSessionEleLocator, MakeSessionEleResult, MakeSessionEleSource,
    TreeSource, TreeTextInput, configs_to_here, from_debugger_address, from_playwright,
    from_playwright_debugger_address, from_selenium, from_selenium_debugger_address, get_blob,
    get_blob_bytes, get_blob_text, make_session_ele, make_session_ele_by, print_tree, tree,
    wait_until,
};
pub use upload::{UploadFilesInput, UploadTracker};
pub use webpage::{
    DisconnectedWebFrame, DisconnectedWebPage, WebElement, WebElementClicker, WebElementDragTarget,
    WebElementRect, WebElementScroller, WebElementSelector, WebElementSetter, WebElementStates,
    WebElementWait, WebFrame, WebMode, WebPage, WebPageCookieSetter, WebPageLoadModeSetter,
    WebPageScroller, WebPageSetter, WebPageWindowSetter, WebSelectOptionInput,
};
pub use window::{activate_app, set_app_visibility};

pub type Chromium = Browser;
pub type ChromiumElement = Element;
pub type ChromiumFrame = Frame;
pub type NoneElement = ElementsOneOwned<Element>;
pub type ChromiumOptions = LaunchOptions;
pub type ChromiumPage = Page;
pub type ChromiumTab = Page;
pub type MixTab = WebPage;
pub type SessionNoneElement = ElementsOneOwned<SessionElement>;
pub type WebNoneElement = ElementsOneOwned<WebElement>;

#[cfg(test)]
mod tests {
    use super::{
        Browser, Chromium, ChromiumElement, ChromiumFrame, ChromiumOptions, ChromiumPage,
        ChromiumTab, Element, ElementListAttrsItem, ElementListContentItem, ElementListMetaItem,
        ElementRect, ElementStates, ElementWait, ElementsOneClicker, ElementsOneOwned,
        ElementsOneRect, ElementsOneScroller, ElementsOneSelector, ElementsOneSetter,
        ElementsOneStates, ElementsOneWait, Frame, LaunchOptions, LocatorBatchInput, MixTab,
        NoneElement, OPENPAGE_BROWSER_PATH_ENV, Page, PageNavigationSnapshot, SelectIndexInput,
        SelectOptionInput, SessionElement, SessionNoneElement, TimeoutConfig, WebElement,
        WebElementRect, WebElementStates, WebElementWait, WebNoneElement, WebPage,
        WebSelectOptionInput,
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
        let _ = (|_: &NoneElement, _: &ElementsOneOwned<Element>| {})
            as fn(&NoneElement, &ElementsOneOwned<Element>);
        let _ = (|_: &SessionNoneElement, _: &ElementsOneOwned<SessionElement>| {})
            as fn(&SessionNoneElement, &ElementsOneOwned<SessionElement>);
        let _ = (|_: &WebNoneElement, _: &ElementsOneOwned<WebElement>| {})
            as fn(&WebNoneElement, &ElementsOneOwned<WebElement>);
    }

    #[test]
    fn element_object_wrapper_types_are_exported() {
        let _ = (|_: &ElementStates<'_>,
                  _: &ElementRect<'_>,
                  _: &ElementWait<'_>,
                  _: &WebElementStates<'_>,
                  _: &WebElementRect<'_>,
                  _: &WebElementWait<'_>| {})
            as fn(
                &ElementStates<'_>,
                &ElementRect<'_>,
                &ElementWait<'_>,
                &WebElementStates<'_>,
                &WebElementRect<'_>,
                &WebElementWait<'_>,
            );
    }

    #[test]
    fn elements_one_wrapper_types_are_exported() {
        let _ = (|_: &ElementsOneClicker<'_, Element>,
                  _: &ElementsOneScroller<'_, Element>,
                  _: &ElementsOneSetter<'_, Element>,
                  _: &ElementsOneStates<'_, Element>,
                  _: &ElementsOneRect<'_, Element>,
                  _: &ElementsOneWait<'_, Element>,
                  _: &ElementsOneSelector<'_, Element>| {})
            as fn(
                &ElementsOneClicker<'_, Element>,
                &ElementsOneScroller<'_, Element>,
                &ElementsOneSetter<'_, Element>,
                &ElementsOneStates<'_, Element>,
                &ElementsOneRect<'_, Element>,
                &ElementsOneWait<'_, Element>,
                &ElementsOneSelector<'_, Element>,
            );
    }

    #[test]
    fn element_list_content_traits_are_exported() {
        fn assert_content_item<T: ElementListContentItem>() {}
        fn assert_attrs_item<T: ElementListAttrsItem>() {}
        fn assert_meta_item<T: ElementListMetaItem>() {}

        assert_content_item::<Element>();
        assert_attrs_item::<Element>();
        assert_meta_item::<Element>();
    }

    #[test]
    fn public_input_and_snapshot_types_are_exported() {
        let _ = (|_: &TimeoutConfig,
                  _: &LocatorBatchInput<'_>,
                  _: &SelectIndexInput,
                  _: &SelectOptionInput<'_>,
                  _: &WebSelectOptionInput<'_>,
                  _: &PageNavigationSnapshot| {})
            as fn(
                &TimeoutConfig,
                &LocatorBatchInput<'_>,
                &SelectIndexInput,
                &SelectOptionInput<'_>,
                &WebSelectOptionInput<'_>,
                &PageNavigationSnapshot,
            );
    }

    #[test]
    fn browser_path_env_constant_is_exported() {
        assert_eq!(OPENPAGE_BROWSER_PATH_ENV, "OPENPAGE_BROWSER_PATH");
    }
}
