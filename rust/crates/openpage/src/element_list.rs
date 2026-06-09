use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::browser::BrowserTabReference;
use crate::element::Element;
use crate::error::{OpenPageError, OpenPageResult};
use crate::session::{SessionElement, SessionXPathResult};
use crate::settings::{
    component_state_lock_poisoned_message, web_element_list_driver_filter_message,
};
use crate::webpage::WebElement;

pub(crate) type ElementsOneRuntimeConfigHandle = Arc<Mutex<ElementsOneRuntimeConfig>>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ElementsOneRuntimeConfig {
    pub return_value_enabled: bool,
    pub return_value: Option<String>,
    pub raise_when_not_found: bool,
}

#[derive(Debug, Clone)]
pub struct ElementsGetter<'a, T> {
    elements: Vec<&'a T>,
}

#[derive(Debug, Clone)]
pub struct ElementsFilter<'a, T> {
    elements: Vec<&'a T>,
    config: Option<&'a ElementsOneRuntimeConfigHandle>,
}

#[derive(Debug, Clone)]
pub struct ElementsFilterOne<'a, T> {
    elements: Vec<&'a T>,
    index: usize,
    config: Option<&'a ElementsOneRuntimeConfigHandle>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ElementsOne<'a, T> {
    element: Option<&'a T>,
    config: Option<&'a ElementsOneRuntimeConfigHandle>,
}

#[derive(Debug, Default)]
pub struct ElementsOneOwned<T> {
    element: Option<T>,
    config: Option<ElementsOneRuntimeConfigHandle>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneClicker<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneScroller<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneSetter<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneStates<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneRect<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneWait<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementsOneSelector<'a, T> {
    element: Option<&'a T>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElementsSearch {
    displayed: Option<bool>,
    checked: Option<bool>,
    selected: Option<bool>,
    enabled: Option<bool>,
    clickable: Option<bool>,
    have_rect: Option<bool>,
    have_text: Option<bool>,
    tag: Option<String>,
}

pub trait ElementsListExt<T> {
    fn get(&self) -> ElementsGetter<'_, T>;
    fn filter(&self) -> ElementsFilter<'_, T>;
    fn filter_one(&self) -> ElementsFilterOne<'_, T>;
    fn filter_one_at(&self, index: usize) -> ElementsFilterOne<'_, T>;

    fn search(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsFilter<'_, T>>
    where
        T: ElementListSearchItem;

    fn search_one(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsOne<'_, T>>
    where
        T: ElementListSearchItem;

    fn search_one_at(
        &self,
        index: usize,
        criteria: &ElementsSearch,
    ) -> OpenPageResult<ElementsOne<'_, T>>
    where
        T: ElementListSearchItem;
}

pub trait ElementListItem {
    fn list_attr(&self, name: &str) -> OpenPageResult<Option<String>>;
    fn list_link(&self) -> OpenPageResult<Option<String>>;
    fn list_text(&self) -> OpenPageResult<Option<String>>;
    fn list_raw_text(&self) -> OpenPageResult<Option<String>>;
    fn list_tag(&self) -> OpenPageResult<String>;
}

pub trait ElementListStateItem {
    fn list_is_displayed(&self) -> OpenPageResult<bool>;
    fn list_is_checked(&self) -> OpenPageResult<bool>;
    fn list_is_selected(&self) -> OpenPageResult<bool>;
    fn list_is_enabled(&self) -> OpenPageResult<bool>;
    fn list_is_clickable(&self) -> OpenPageResult<bool>;
    fn list_has_rect(&self) -> OpenPageResult<bool>;
}

pub trait ElementListDriverItem {
    fn list_style(&self, name: &str) -> OpenPageResult<String>;
    fn list_pseudo_before(&self) -> OpenPageResult<String>;
    fn list_pseudo_after(&self) -> OpenPageResult<String>;
    fn list_property(&self, name: &str) -> OpenPageResult<Option<Value>>;
}

pub trait ElementListContentItem {
    fn list_html(&self) -> OpenPageResult<Option<String>>;
    fn list_inner_html(&self) -> OpenPageResult<Option<String>>;
    fn list_value(&self) -> OpenPageResult<Option<String>>;
}

pub trait ElementListAttrsItem {
    fn list_attrs(&self) -> OpenPageResult<Vec<(String, String)>>;
}

pub trait ElementListMetaItem {
    fn list_child_count(&self) -> OpenPageResult<usize>;
    fn list_css_path(&self) -> OpenPageResult<String>;
    fn list_xpath(&self) -> OpenPageResult<String>;
    fn list_comments(&self) -> OpenPageResult<Vec<String>>;
}

pub trait ElementListSearchItem: ElementListItem + ElementListStateItem {}

impl<T> ElementListSearchItem for T where T: ElementListItem + ElementListStateItem {}

impl ElementsSearch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn displayed(mut self, value: bool) -> Self {
        self.displayed = Some(value);
        self
    }

    pub fn checked(mut self, value: bool) -> Self {
        self.checked = Some(value);
        self
    }

    pub fn selected(mut self, value: bool) -> Self {
        self.selected = Some(value);
        self
    }

    pub fn enabled(mut self, value: bool) -> Self {
        self.enabled = Some(value);
        self
    }

    pub fn clickable(mut self, value: bool) -> Self {
        self.clickable = Some(value);
        self
    }

    pub fn have_rect(mut self, value: bool) -> Self {
        self.have_rect = Some(value);
        self
    }

    pub fn have_text(mut self, value: bool) -> Self {
        self.have_text = Some(value);
        self
    }

    pub fn tag<S>(mut self, value: S) -> Self
    where
        S: Into<String>,
    {
        self.tag = Some(value.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.displayed.is_none()
            && self.checked.is_none()
            && self.selected.is_none()
            && self.enabled.is_none()
            && self.clickable.is_none()
            && self.have_rect.is_none()
            && self.have_text.is_none()
            && self.tag.is_none()
    }
}

fn elements_one_config_lock_error() -> OpenPageError {
    OpenPageError::PageOperation(component_state_lock_poisoned_message(
        "none element runtime config",
        "未找到元素运行时配置",
    ))
}

fn elements_one_missing_config_snapshot(
    config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<Option<ElementsOneRuntimeConfig>> {
    let Some(config) = config else {
        return Ok(None);
    };
    config
        .lock()
        .map(|state| state.clone())
        .map(Some)
        .map_err(|_| elements_one_config_lock_error())
}

pub(crate) fn elements_one_should_raise_when_missing(
    config: Option<&ElementsOneRuntimeConfigHandle>,
) -> OpenPageResult<bool> {
    Ok(elements_one_missing_config_snapshot(config)?
        .is_some_and(|config| config.raise_when_not_found))
}

fn elements_one_runtime_config_from_ref<'a, T: 'static>(
    element: Option<&'a T>,
) -> Option<&'a ElementsOneRuntimeConfigHandle> {
    let element = element?;
    let any = element as &dyn Any;
    if let Some(element) = any.downcast_ref::<Element>() {
        return Some(element.none_element_runtime_config_handle());
    }
    if let Some(element) = any.downcast_ref::<WebElement>() {
        return element.none_element_runtime_config_handle();
    }
    if let Some(element) = any.downcast_ref::<SessionElement>() {
        return element.none_element_runtime_config_handle();
    }
    None
}

fn elements_one_missing_message(method: &str, filters: &[(&str, String)], index: usize) -> String {
    let mut parts = filters
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    parts.push(format!("index={index}"));
    format!("{method} not found ({})", parts.join(", "))
}

fn elements_search_debug(criteria: &ElementsSearch) -> String {
    let mut parts = Vec::new();
    if let Some(value) = criteria.displayed {
        parts.push(format!("displayed={value}"));
    }
    if let Some(value) = criteria.checked {
        parts.push(format!("checked={value}"));
    }
    if let Some(value) = criteria.selected {
        parts.push(format!("selected={value}"));
    }
    if let Some(value) = criteria.enabled {
        parts.push(format!("enabled={value}"));
    }
    if let Some(value) = criteria.clickable {
        parts.push(format!("clickable={value}"));
    }
    if let Some(value) = criteria.have_rect {
        parts.push(format!("have_rect={value}"));
    }
    if let Some(value) = criteria.have_text {
        parts.push(format!("have_text={value}"));
    }
    if let Some(value) = criteria.tag.as_ref() {
        parts.push(format!("tag={value:?}"));
    }
    parts.join(", ")
}

impl<'a, T> ElementsOne<'a, T> {
    pub fn some(element: &'a T) -> Self {
        Self {
            element: Some(element),
            config: None,
        }
    }

    pub fn none() -> Self {
        Self {
            element: None,
            config: None,
        }
    }

    fn some_with_config(
        element: &'a T,
        config: Option<&'a ElementsOneRuntimeConfigHandle>,
    ) -> Self {
        Self {
            element: Some(element),
            config,
        }
    }

    fn none_with_config(config: Option<&'a ElementsOneRuntimeConfigHandle>) -> Self {
        Self {
            element: None,
            config,
        }
    }

    pub fn is_some(&self) -> bool {
        self.element.is_some()
    }

    pub fn is_none(&self) -> bool {
        self.element.is_none()
    }

    pub fn as_option(&self) -> Option<&'a T> {
        self.element
    }

    pub fn into_option(self) -> Option<&'a T> {
        self.element
    }

    pub fn expect(self, message: &str) -> &'a T {
        self.element.expect(message)
    }

    pub fn unwrap(self) -> &'a T {
        self.element.unwrap()
    }

    fn missing_string_value(&self) -> OpenPageResult<Option<String>> {
        Ok(elements_one_missing_config_snapshot(self.config)?
            .and_then(|config| config.return_value_enabled.then_some(config.return_value))
            .flatten())
    }

    fn missing_string_vec_value(&self) -> OpenPageResult<Option<Vec<String>>> {
        Ok(self.missing_string_value()?.map(|value| vec![value]))
    }

    fn missing_json_value(&self) -> OpenPageResult<Option<Value>> {
        Ok(self.missing_string_value()?.map(Value::String))
    }

    fn missing_resource_value(&self) -> OpenPageResult<Option<crate::element::ElementResource>> {
        Ok(self
            .missing_string_value()?
            .map(crate::element::ElementResource::Text))
    }
}

impl<T> ElementsOneOwned<T> {
    pub(crate) fn some_with_config(
        element: T,
        config: Option<ElementsOneRuntimeConfigHandle>,
    ) -> Self {
        Self {
            element: Some(element),
            config,
        }
    }

    pub(crate) fn none_with_config(config: Option<ElementsOneRuntimeConfigHandle>) -> Self {
        Self {
            element: None,
            config,
        }
    }

    fn as_borrowed(&self) -> ElementsOne<'_, T> {
        match self.element.as_ref() {
            Some(element) => ElementsOne::some_with_config(element, self.config.as_ref()),
            None => ElementsOne::none_with_config(self.config.as_ref()),
        }
    }

    pub fn is_some(&self) -> bool {
        self.element.is_some()
    }

    pub fn is_none(&self) -> bool {
        self.element.is_none()
    }

    pub fn as_option(&self) -> Option<&T> {
        self.element.as_ref()
    }

    pub fn into_option(self) -> Option<T> {
        self.element
    }

    pub fn expect(self, message: &str) -> T {
        self.element.expect(message)
    }

    pub fn unwrap(self) -> T {
        self.element.unwrap()
    }

    pub fn map<U, F>(self, f: F) -> ElementsOneOwned<U>
    where
        F: FnOnce(T) -> U,
    {
        ElementsOneOwned {
            element: self.element.map(f),
            config: self.config,
        }
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListItem,
{
    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.as_borrowed().attr(name)
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().link()
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().text()
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().raw_text()
    }

    pub fn tag(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().tag()
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListContentItem,
{
    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().html()
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().inner_html()
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().value()
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListAttrsItem,
{
    pub fn attrs(&self) -> OpenPageResult<Option<Vec<(String, String)>>> {
        self.as_borrowed().attrs()
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListMetaItem,
{
    pub fn child_count(&self) -> OpenPageResult<Option<usize>> {
        self.as_borrowed().child_count()
    }

    pub fn css_path(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().css_path()
    }

    pub fn xpath(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().xpath()
    }

    pub fn comments(&self) -> OpenPageResult<Option<Vec<String>>> {
        self.as_borrowed().comments()
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListStateItem,
{
    pub fn is_displayed(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().is_displayed()
    }

    pub fn is_checked(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().is_checked()
    }

    pub fn is_selected(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().is_selected()
    }

    pub fn is_enabled(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().is_enabled()
    }

    pub fn is_clickable(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().is_clickable()
    }

    pub fn has_rect(&self) -> OpenPageResult<Option<bool>> {
        self.as_borrowed().has_rect()
    }
}

impl<T> ElementsOneOwned<T>
where
    T: ElementListDriverItem,
{
    pub fn style(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.as_borrowed().style(name)
    }

    pub fn pseudo_before(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().pseudo_before()
    }

    pub fn pseudo_after(&self) -> OpenPageResult<Option<String>> {
        self.as_borrowed().pseudo_after()
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.as_borrowed().property(name)
    }
}

impl ElementsOneOwned<Element> {
    fn missing_optional_result<U>(&self, method: &str) -> OpenPageResult<Option<U>> {
        if elements_one_should_raise_when_missing(self.config.as_ref())? {
            return Err(OpenPageError::ElementNotFound(format!(
                "{method} called on missing element"
            )));
        }
        Ok(None)
    }

    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Element>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(element) => Ok(Self::some_with_config(element, self.config.clone())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    fn relative_optional_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Option<Element>>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(Some(element)) => Ok(Self::some_with_config(element, self.config.clone())),
                Ok(None) => Ok(Self::none_with_config(self.config.clone())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Vec<Element>>,
    {
        match self.as_option() {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    pub fn clicker(&self) -> ElementsOneClicker<'_, Element> {
        self.as_borrowed().clicker()
    }

    pub fn sr(&self) -> OpenPageResult<Option<crate::shadow_root::ShadowRoot>> {
        self.shadow_root()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<crate::shadow_root::ShadowRoot>> {
        match self.as_option() {
            Some(element) => element.shadow_root(),
            None => self.missing_optional_result("shadow_root()"),
        }
    }

    pub fn scroll(&self) -> ElementsOneScroller<'_, Element> {
        self.as_borrowed().scroll()
    }

    pub fn set(&self) -> ElementsOneSetter<'_, Element> {
        self.as_borrowed().set()
    }

    pub fn states(&self) -> ElementsOneStates<'_, Element> {
        self.as_borrowed().states()
    }

    pub fn rect(&self) -> ElementsOneRect<'_, Element> {
        self.as_borrowed().rect()
    }

    pub fn wait(&self) -> ElementsOneWait<'_, Element> {
        self.as_borrowed().wait()
    }

    pub fn select(&self) -> ElementsOneSelector<'_, Element> {
        self.as_borrowed().select()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        self.as_borrowed().texts(text_node_only)
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.as_borrowed().size()
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<crate::element::ElementResource>> {
        self.as_borrowed().src(timeout_ms, base64_to_bytes)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<Option<PathBuf>> {
        self.as_borrowed().save(path, name, timeout_ms, rename)
    }

    pub fn click(&self) -> OpenPageResult<bool> {
        self.as_borrowed().click()
    }

    pub fn input(&self, text: &str) -> OpenPageResult<bool> {
        self.as_borrowed().input(text)
    }

    pub fn input_with_options<'a, I>(
        &self,
        text: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'a>>,
    {
        self.as_borrowed().input_with_options(text, clear, by_js)
    }

    pub fn input_keys_with_options<'a, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'a>>,
    {
        self.as_borrowed()
            .input_keys_with_options(values, clear, by_js)
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        self.as_borrowed().clear()
    }

    pub fn focus(&self) -> OpenPageResult<bool> {
        self.as_borrowed().focus()
    }

    pub fn hover(&self) -> OpenPageResult<bool> {
        self.as_borrowed().hover()
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<bool> {
        self.as_borrowed().remove_attr(name)
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<bool> {
        self.as_borrowed().check(uncheck, by_js)
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<bool> {
        self.as_borrowed().uncheck(by_js)
    }

    pub fn set_value(&self, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_value(value)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_attr(name, value)
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_style(name, value)
    }

    pub fn set_inner_html(&self, html: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_inner_html(html)
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.ele(locator),
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.clone(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.clone())),
        }
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'a, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn over(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_optional_element(|element| element.over())
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_optional_element(|element| element.over_with_timeout(timeout_ms))
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.offset(locator, x, y, timeout_ms))
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.east(locator, pixels, index))
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.south(locator, pixels, index))
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.west(locator, pixels, index))
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.north(locator, pixels, index))
    }
}

impl ElementsOneOwned<WebElement> {
    fn missing_optional_result<U>(&self, method: &str) -> OpenPageResult<Option<U>> {
        if elements_one_should_raise_when_missing(self.config.as_ref())? {
            return Err(OpenPageError::ElementNotFound(format!(
                "{method} called on missing element"
            )));
        }
        Ok(None)
    }

    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<WebElement>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(element) => Ok(Self::some_with_config(element, self.config.clone())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    fn relative_optional_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<Option<WebElement>>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(Some(element)) => Ok(Self::some_with_config(element, self.config.clone())),
                Ok(None) => Ok(Self::none_with_config(self.config.clone())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<Vec<WebElement>>,
    {
        match self.as_option() {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    pub fn clicker(&self) -> ElementsOneClicker<'_, WebElement> {
        self.as_borrowed().clicker()
    }

    pub fn sr(&self) -> OpenPageResult<Option<crate::shadow_root::ShadowRoot>> {
        self.shadow_root()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<crate::shadow_root::ShadowRoot>> {
        match self.as_option() {
            Some(element) => element.shadow_root(),
            None => self.missing_optional_result("shadow_root()"),
        }
    }

    pub fn scroll(&self) -> ElementsOneScroller<'_, WebElement> {
        self.as_borrowed().scroll()
    }

    pub fn set(&self) -> ElementsOneSetter<'_, WebElement> {
        self.as_borrowed().set()
    }

    pub fn states(&self) -> ElementsOneStates<'_, WebElement> {
        self.as_borrowed().states()
    }

    pub fn rect(&self) -> ElementsOneRect<'_, WebElement> {
        self.as_borrowed().rect()
    }

    pub fn wait(&self) -> ElementsOneWait<'_, WebElement> {
        self.as_borrowed().wait()
    }

    pub fn select(&self) -> ElementsOneSelector<'_, WebElement> {
        self.as_borrowed().select()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        self.as_borrowed().texts(text_node_only)
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.as_borrowed().size()
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<crate::element::ElementResource>> {
        self.as_borrowed().src(timeout_ms, base64_to_bytes)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<Option<PathBuf>> {
        self.as_borrowed().save(path, name, timeout_ms, rename)
    }

    pub fn click(&self) -> OpenPageResult<bool> {
        self.as_borrowed().click()
    }

    pub fn input(&self, text: &str) -> OpenPageResult<bool> {
        self.as_borrowed().input(text)
    }

    pub fn input_with_options<'a, I>(
        &self,
        text: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'a>>,
    {
        self.as_borrowed().input_with_options(text, clear, by_js)
    }

    pub fn input_keys_with_options<'a, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'a>>,
    {
        self.as_borrowed()
            .input_keys_with_options(values, clear, by_js)
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        self.as_borrowed().clear()
    }

    pub fn focus(&self) -> OpenPageResult<bool> {
        self.as_borrowed().focus()
    }

    pub fn hover(&self) -> OpenPageResult<bool> {
        self.as_borrowed().hover()
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<bool> {
        self.as_borrowed().remove_attr(name)
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<bool> {
        self.as_borrowed().check(uncheck, by_js)
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<bool> {
        self.as_borrowed().uncheck(by_js)
    }

    pub fn set_value(&self, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_value(value)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_attr(name, value)
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_style(name, value)
    }

    pub fn set_inner_html(&self, html: &str) -> OpenPageResult<bool> {
        self.as_borrowed().set_inner_html(html)
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.ele(locator),
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.clone(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.clone())),
        }
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'a, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn over(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_optional_element(|element| element.over())
    }

    pub fn over_with_timeout(
        &self,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_optional_element(|element| element.over_with_timeout(timeout_ms))
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.offset(locator, x, y, timeout_ms))
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.east(locator, pixels, index))
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.south(locator, pixels, index))
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.west(locator, pixels, index))
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.north(locator, pixels, index))
    }
}

impl ElementsOneOwned<SessionElement> {
    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<SessionElement>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(element) => Ok(Self::some_with_config(element, self.config.clone())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<SessionElement>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<Vec<SessionElement>>,
    {
        match self.as_option() {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    fn relative_nodes<F>(&self, f: F) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<Vec<SessionXPathResult>>,
    {
        match self.as_option() {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    fn relative_node<F>(&self, f: F) -> OpenPageResult<Option<SessionXPathResult>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<SessionXPathResult>,
    {
        match self.as_option() {
            Some(element) => match f(element) {
                Ok(node) => Ok(Some(node)),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(None)
                }
                Err(err) => Err(err),
            },
            None => Ok(None),
        }
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        self.as_borrowed().texts(text_node_only)
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.ele(locator),
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.clone(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config.as_ref())? {
                        return Err(err);
                    }
                    Ok(Self::none_with_config(self.config.clone()))
                }
                Err(err) => Err(err),
            },
            None => Ok(Self::none_with_config(self.config.clone())),
        }
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        match self.as_option() {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'a, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn child_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.child_node())
    }

    pub fn child_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_node(|element| element.child_node_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn children_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.children_nodes())
    }

    pub fn children_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_nodes(|element| element.children_nodes_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prev_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.prev_node())
    }

    pub fn prev_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_node(|element| element.prev_node_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn prev_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.prev_nodes())
    }

    pub fn prev_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_nodes(|element| element.prev_nodes_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn next_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.next_node())
    }

    pub fn next_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_node(|element| element.next_node_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn next_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.next_nodes())
    }

    pub fn next_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_nodes(|element| element.next_nodes_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn before_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.before_node())
    }

    pub fn before_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_node(|element| element.before_node_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn before_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.before_nodes())
    }

    pub fn before_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_nodes(|element| element.before_nodes_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn after_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.after_node())
    }

    pub fn after_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_node(|element| element.after_node_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn after_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.after_nodes())
    }

    pub fn after_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'a>>,
    {
        self.relative_nodes(|element| element.after_nodes_with(locator))
    }
}

impl<'a> ElementsOne<'a, Element> {
    pub fn clicker(&self) -> ElementsOneClicker<'a, Element> {
        ElementsOneClicker {
            element: self.element,
        }
    }

    pub fn scroll(&self) -> ElementsOneScroller<'a, Element> {
        ElementsOneScroller {
            element: self.element,
        }
    }

    pub fn set(&self) -> ElementsOneSetter<'a, Element> {
        ElementsOneSetter {
            element: self.element,
        }
    }

    pub fn states(&self) -> ElementsOneStates<'a, Element> {
        ElementsOneStates {
            element: self.element,
        }
    }

    pub fn rect(&self) -> ElementsOneRect<'a, Element> {
        ElementsOneRect {
            element: self.element,
        }
    }

    pub fn wait(&self) -> ElementsOneWait<'a, Element> {
        ElementsOneWait {
            element: self.element,
        }
    }

    pub fn select(&self) -> ElementsOneSelector<'a, Element> {
        ElementsOneSelector {
            element: self.element,
        }
    }
}

impl<'a> ElementsOne<'a, WebElement> {
    pub fn clicker(&self) -> ElementsOneClicker<'a, WebElement> {
        ElementsOneClicker {
            element: self.element,
        }
    }

    pub fn scroll(&self) -> ElementsOneScroller<'a, WebElement> {
        ElementsOneScroller {
            element: self.element,
        }
    }

    pub fn set(&self) -> ElementsOneSetter<'a, WebElement> {
        ElementsOneSetter {
            element: self.element,
        }
    }

    pub fn states(&self) -> ElementsOneStates<'a, WebElement> {
        ElementsOneStates {
            element: self.element,
        }
    }

    pub fn rect(&self) -> ElementsOneRect<'a, WebElement> {
        ElementsOneRect {
            element: self.element,
        }
    }

    pub fn wait(&self) -> ElementsOneWait<'a, WebElement> {
        ElementsOneWait {
            element: self.element,
        }
    }

    pub fn select(&self) -> ElementsOneSelector<'a, WebElement> {
        ElementsOneSelector {
            element: self.element,
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListItem,
{
    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_attr(name),
            None => self.missing_string_value(),
        }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_link(),
            None => self.missing_string_value(),
        }
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_text(),
            None => self.missing_string_value(),
        }
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_raw_text(),
            None => self.missing_string_value(),
        }
    }

    pub fn tag(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_tag().map(Some),
            None => self.missing_string_value(),
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListContentItem,
{
    pub fn html(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_html(),
            None => self.missing_string_value(),
        }
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_inner_html(),
            None => self.missing_string_value(),
        }
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_value(),
            None => self.missing_string_value(),
        }
    }
}

impl<'a> ElementsOne<'a, Element> {
    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Element>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    fn relative_optional_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Option<Element>>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(Some(element)) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Ok(None) => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<Element>>
    where
        F: FnOnce(&Element) -> OpenPageResult<Vec<Element>>,
    {
        match self.element {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    pub fn ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.ele(locator),
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn s_eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'b, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn over(&self) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_optional_element(|element| element.over())
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<ElementsOneOwned<Element>> {
        self.relative_optional_element(|element| element.over_with_timeout(timeout_ms))
    }

    pub fn offset<'b, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.offset(locator, x, y, timeout_ms))
    }

    pub fn east<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.east(locator, pixels, index))
    }

    pub fn south<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.south(locator, pixels, index))
    }

    pub fn west<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.west(locator, pixels, index))
    }

    pub fn north<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.north(locator, pixels, index))
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        match self.element {
            Some(element) => element.texts(text_node_only).map(Some),
            None => self.missing_string_vec_value(),
        }
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect_size(),
            None => Ok(None),
        }
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<crate::element::ElementResource>> {
        match self.element {
            Some(element) => element.src(timeout_ms, base64_to_bytes),
            None => self.missing_resource_value(),
        }
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<Option<PathBuf>> {
        match self.element {
            Some(element) => element.save(path, name, timeout_ms, rename).map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOne<'a, WebElement> {
    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<WebElement>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    fn relative_optional_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<Option<WebElement>>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(Some(element)) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Ok(None) => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<WebElement>>
    where
        F: FnOnce(&WebElement) -> OpenPageResult<Vec<WebElement>>,
    {
        match self.element {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    pub fn ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.ele(locator),
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn s_eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'b, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<WebElement>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn over(&self) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_optional_element(|element| element.over())
    }

    pub fn over_with_timeout(
        &self,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>> {
        self.relative_optional_element(|element| element.over_with_timeout(timeout_ms))
    }

    pub fn offset<'b, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.offset(locator, x, y, timeout_ms))
    }

    pub fn east<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.east(locator, pixels, index))
    }

    pub fn south<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.south(locator, pixels, index))
    }

    pub fn west<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.west(locator, pixels, index))
    }

    pub fn north<'b, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.north(locator, pixels, index))
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        match self.element {
            Some(element) => element.texts(text_node_only).map(Some),
            None => self.missing_string_vec_value(),
        }
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect_size(),
            None => Ok(None),
        }
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<crate::element::ElementResource>> {
        match self.element {
            Some(element) => element.src(timeout_ms, base64_to_bytes),
            None => self.missing_resource_value(),
        }
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<Option<PathBuf>> {
        match self.element {
            Some(element) => element.save(path, name, timeout_ms, rename).map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOne<'a, SessionElement> {
    fn relative_element<F>(&self, f: F) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<SessionElement>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    fn relative_elements<F>(&self, f: F) -> OpenPageResult<Vec<SessionElement>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<Vec<SessionElement>>,
    {
        match self.element {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    fn relative_nodes<F>(&self, f: F) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<Vec<SessionXPathResult>>,
    {
        match self.element {
            Some(element) => f(element),
            None => Ok(Vec::new()),
        }
    }

    fn relative_node<F>(&self, f: F) -> OpenPageResult<Option<SessionXPathResult>>
    where
        F: FnOnce(&SessionElement) -> OpenPageResult<SessionXPathResult>,
    {
        match self.element {
            Some(element) => match f(element) {
                Ok(node) => Ok(Some(node)),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(None)
                }
                Err(err) => Err(err),
            },
            None => Ok(None),
        }
    }

    pub fn ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.ele(locator),
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn s_ele<'b, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => match element.s_ele(locator) {
                Ok(element) => Ok(ElementsOneOwned::some_with_config(
                    element,
                    self.config.cloned(),
                )),
                Err(err @ OpenPageError::ElementNotFound(_)) => {
                    if elements_one_should_raise_when_missing(self.config)? {
                        return Err(err);
                    }
                    Ok(ElementsOneOwned::none_with_config(self.config.cloned()))
                }
                Err(err) => Err(err),
            },
            None => Ok(ElementsOneOwned::none_with_config(self.config.cloned())),
        }
    }

    pub fn s_eles<'b, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        match self.element {
            Some(element) => element.s_eles(locator),
            None => Ok(Vec::new()),
        }
    }

    pub fn parent(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.parent())
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.parent_level(level))
    }

    pub fn parent_with<'b, L>(
        &self,
        locator: L,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.parent_with(locator, index))
    }

    pub fn child(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.child())
    }

    pub fn child_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.child_with(locator, index))
    }

    pub fn child_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.child_node())
    }

    pub fn child_node_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_node(|element| element.child_node_with(locator, index))
    }

    pub fn children(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.children())
    }

    pub fn children_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.children_with(locator))
    }

    pub fn children_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.children_nodes())
    }

    pub fn children_nodes_with<'b, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_nodes(|element| element.children_nodes_with(locator))
    }

    pub fn prev(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.prev())
    }

    pub fn prev_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.prev_with(locator, index))
    }

    pub fn prev_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.prev_node())
    }

    pub fn prev_node_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_node(|element| element.prev_node_with(locator, index))
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.prevs())
    }

    pub fn prevs_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.prevs_with(locator))
    }

    pub fn prev_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.prev_nodes())
    }

    pub fn prev_nodes_with<'b, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_nodes(|element| element.prev_nodes_with(locator))
    }

    pub fn next(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.next())
    }

    pub fn next_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.next_with(locator, index))
    }

    pub fn next_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.next_node())
    }

    pub fn next_node_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_node(|element| element.next_node_with(locator, index))
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.nexts())
    }

    pub fn nexts_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.nexts_with(locator))
    }

    pub fn next_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.next_nodes())
    }

    pub fn next_nodes_with<'b, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_nodes(|element| element.next_nodes_with(locator))
    }

    pub fn before(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.before())
    }

    pub fn before_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.before_with(locator, index))
    }

    pub fn before_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.before_node())
    }

    pub fn before_node_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_node(|element| element.before_node_with(locator, index))
    }

    pub fn befores(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.befores())
    }

    pub fn befores_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.befores_with(locator))
    }

    pub fn before_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.before_nodes())
    }

    pub fn before_nodes_with<'b, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_nodes(|element| element.before_nodes_with(locator))
    }

    pub fn after(&self) -> OpenPageResult<ElementsOneOwned<SessionElement>> {
        self.relative_element(|element| element.after())
    }

    pub fn after_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<ElementsOneOwned<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_element(|element| element.after_with(locator, index))
    }

    pub fn after_node(&self) -> OpenPageResult<Option<SessionXPathResult>> {
        self.relative_node(|element| element.after_node())
    }

    pub fn after_node_with<'b, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<Option<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_node(|element| element.after_node_with(locator, index))
    }

    pub fn afters(&self) -> OpenPageResult<Vec<SessionElement>> {
        self.relative_elements(|element| element.afters())
    }

    pub fn afters_with<'b, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_elements(|element| element.afters_with(locator))
    }

    pub fn after_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.relative_nodes(|element| element.after_nodes())
    }

    pub fn after_nodes_with<'b, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<crate::locator::LocatorInput<'b>>,
    {
        self.relative_nodes(|element| element.after_nodes_with(locator))
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Option<Vec<String>>> {
        match self.element {
            Some(element) => element.texts(text_node_only).map(Some),
            None => self.missing_string_vec_value(),
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListAttrsItem,
{
    pub fn attrs(&self) -> OpenPageResult<Option<Vec<(String, String)>>> {
        match self.element {
            Some(element) => element.list_attrs().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListMetaItem,
{
    pub fn child_count(&self) -> OpenPageResult<Option<usize>> {
        match self.element {
            Some(element) => element.list_child_count().map(Some),
            None => Ok(None),
        }
    }

    pub fn css_path(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_css_path().map(Some),
            None => self.missing_string_value(),
        }
    }

    pub fn xpath(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_xpath().map(Some),
            None => self.missing_string_value(),
        }
    }

    pub fn comments(&self) -> OpenPageResult<Option<Vec<String>>> {
        match self.element {
            Some(element) => element.list_comments().map(Some),
            None => self.missing_string_vec_value(),
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListStateItem,
{
    pub fn is_displayed(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_is_displayed().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_checked(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_is_checked().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_selected(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_is_selected().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_is_enabled().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_is_clickable().map(Some),
            None => Ok(None),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.list_has_rect().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a, T> ElementsOne<'a, T>
where
    T: ElementListDriverItem,
{
    pub fn style(&self, name: &str) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_style(name).map(Some),
            None => self.missing_string_value(),
        }
    }

    pub fn pseudo_before(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_pseudo_before().map(Some),
            None => self.missing_string_value(),
        }
    }

    pub fn pseudo_after(&self) -> OpenPageResult<Option<String>> {
        match self.element {
            Some(element) => element.list_pseudo_after().map(Some),
            None => self.missing_string_value(),
        }
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        match self.element {
            Some(element) => element.list_property(name),
            None => self.missing_json_value(),
        }
    }
}

impl<'a, T> From<Option<&'a T>> for ElementsOne<'a, T> {
    fn from(value: Option<&'a T>) -> Self {
        Self {
            element: value,
            config: None,
        }
    }
}

impl<'a, T> From<ElementsOne<'a, T>> for Option<&'a T> {
    fn from(value: ElementsOne<'a, T>) -> Self {
        value.element
    }
}

impl<'a> ElementsOne<'a, Element> {
    pub fn click(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.click_with_options(Some(false), None, true),
            None => Ok(false),
        }
    }

    pub fn input(&self, text: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.input(text)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn input_with_options<'b, I>(
        &self,
        text: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => {
                element.input_with_options(text, clear, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn input_keys_with_options<'b, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => {
                element.input_keys_with_options(values, clear, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn focus(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.focus()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn hover(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.hover()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.remove_attr(name)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.check(uncheck, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.uncheck(by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_value(&self, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().value(value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().attr(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().style(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_inner_html(&self, html: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().inner_html(html)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_top()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_bottom()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_location(x, y)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().up(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().down(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().left(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().right(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_see(center)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_center()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text(text),
            None => Ok(false),
        }
    }

    pub fn select_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value(value),
            None => Ok(false),
        }
    }

    pub fn select_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index(index),
            None => Ok(false),
        }
    }

    pub fn select_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn select_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn select_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_option(&self, option: &Element) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_option(option),
            None => Ok(false),
        }
    }

    pub fn select_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_options(options),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_text(text),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_value(value),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_index(index),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_option(&self, option: &Element) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_option(option),
            None => Ok(false),
        }
    }

    pub fn cancel_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_options(options),
            None => Ok(false),
        }
    }

    pub fn select_clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_all(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().all()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_invert(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().invert()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_is_multi(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.select().is_multi().map(Some),
            None => Ok(None),
        }
    }

    pub fn select_options(&self) -> OpenPageResult<Option<Vec<Element>>> {
        match self.element {
            Some(element) => element.select().options().map(Some),
            None => Ok(None),
        }
    }

    pub fn select_selected_option(&self) -> OpenPageResult<Option<Element>> {
        match self.element {
            Some(element) => element.select().selected_option(),
            None => Ok(None),
        }
    }

    pub fn select_selected_options(&self) -> OpenPageResult<Option<Vec<Element>>> {
        match self.element {
            Some(element) => element.select().selected_options().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneClicker<'a, Element> {
    pub fn left(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.clicker().left(),
            None => Ok(false),
        }
    }

    pub fn left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .clicker()
                .left_with_options(by_js, timeout_ms, wait_stop),
            None => Ok(false),
        }
    }

    pub fn right(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().right()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn middle(&self, get_tab: bool) -> OpenPageResult<Option<crate::page::Page>> {
        match self.element {
            Some(element) => element.clicker().middle(get_tab),
            None => Ok(None),
        }
    }

    pub fn multi(&self, times: u32) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().multi(times)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().at(offset_x, offset_y, button, count)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_upload(
        &self,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.clicker().to_upload(files, timeout_ms, by_js),
            None => Ok(false),
        }
    }

    pub fn to_download(
        &self,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<crate::download::DownloadMission>> {
        match self.element {
            Some(element) => element.clicker().to_download(
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            ),
            None => Ok(None),
        }
    }

    pub fn for_new_tab(
        &self,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<crate::page::Page>> {
        match self.element {
            Some(element) => element.clicker().for_new_tab(timeout_ms, by_js),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneScroller<'a, Element> {
    pub fn to_top(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_top()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_bottom(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_bottom()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_half(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_half()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_rightmost(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_rightmost()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_leftmost(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_leftmost()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_location(x, y)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().up(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().down(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().left(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().right(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_see(&self, center: Option<bool>) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_see(center)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_center(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_center()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneSetter<'a, Element> {
    pub fn inner_html(&self, html: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().inner_html(html)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().property(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().style(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().attr(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn value(&self, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().value(value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneStates<'a, Element> {
    pub fn is_in_viewport(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_in_viewport().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_whole_in_viewport().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_alive().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_checked(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_checked().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_selected(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_selected().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_enabled().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_displayed(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_displayed().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_covered(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_covered().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_clickable().map(Some),
            None => Ok(None),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().has_rect().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneRect<'a, Element> {
    pub fn corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self.element {
            Some(element) => element.rect().corners(),
            None => Ok(None),
        }
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self.element {
            Some(element) => element.rect().viewport_corners(),
            None => Ok(None),
        }
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().location(),
            None => Ok(None),
        }
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_location(),
            None => Ok(None),
        }
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_location(),
            None => Ok(None),
        }
    }

    pub fn midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().midpoint(),
            None => Ok(None),
        }
    }

    pub fn viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_midpoint(),
            None => Ok(None),
        }
    }

    pub fn click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().click_point(),
            None => Ok(None),
        }
    }

    pub fn viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_click_point(),
            None => Ok(None),
        }
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().size(),
            None => Ok(None),
        }
    }

    pub fn screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_midpoint(),
            None => Ok(None),
        }
    }

    pub fn screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_click_point(),
            None => Ok(None),
        }
    }

    pub fn scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().scroll_position(),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneWait<'a, Element> {
    pub fn displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().displayed(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().hidden(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().enabled(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().disabled(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().deleted(timeout_ms),
            None => Ok(true),
        }
    }

    pub fn clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().clickable(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().has_rect(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().covered(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().not_covered(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().disabled_or_deleted(timeout_ms),
            None => Ok(true),
        }
    }

    pub fn stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().stop_moving(timeout_ms),
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneSelector<'a, Element> {
    pub fn by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text(text),
            None => Ok(false),
        }
    }

    pub fn by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value(value),
            None => Ok(false),
        }
    }

    pub fn by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index(index),
            None => Ok(false),
        }
    }

    pub fn by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_option<'b, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectOptionInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_option(option),
            None => Ok(false),
        }
    }

    pub fn by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_options(options),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_text(text),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_value(value),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_index(index),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_option<'b, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectOptionInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_option(option),
            None => Ok(false),
        }
    }

    pub fn cancel_by_options(&self, options: &[&Element]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_options(options),
            None => Ok(false),
        }
    }

    pub fn all(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().all()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn invert(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().invert()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn is_multi(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.select().is_multi().map(Some),
            None => Ok(None),
        }
    }

    pub fn options(&self) -> OpenPageResult<Option<Vec<Element>>> {
        match self.element {
            Some(element) => element.select().options().map(Some),
            None => Ok(None),
        }
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<Element>> {
        match self.element {
            Some(element) => element.select().selected_option(),
            None => Ok(None),
        }
    }

    pub fn selected_options(&self) -> OpenPageResult<Option<Vec<Element>>> {
        match self.element {
            Some(element) => element.select().selected_options().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOne<'a, WebElement> {
    pub fn click(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.click_with_options(Some(false), None, true),
            None => Ok(false),
        }
    }

    pub fn input(&self, text: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.input(text)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn input_with_options<'b, I>(
        &self,
        text: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => {
                element.input_with_options(text, clear, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn input_keys_with_options<'b, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => {
                element.input_keys_with_options(values, clear, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn focus(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.focus()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn hover(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.hover()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.remove_attr(name)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.check(uncheck, by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.uncheck(by_js)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_value(&self, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().value(value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().attr(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().style(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_inner_html(&self, html: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().inner_html(html)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_top()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_bottom()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_location(x, y)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().up(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().down(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().left(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().right(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_see(center)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_center()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text(text),
            None => Ok(false),
        }
    }

    pub fn select_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value(value),
            None => Ok(false),
        }
    }

    pub fn select_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index(index),
            None => Ok(false),
        }
    }

    pub fn select_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn select_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn select_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn select_by_option(&self, option: &WebElement) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_option(option),
            None => Ok(false),
        }
    }

    pub fn select_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_options(options),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_text(text),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_value(value),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_index(index),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_option(&self, option: &WebElement) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_option(option),
            None => Ok(false),
        }
    }

    pub fn cancel_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_options(options),
            None => Ok(false),
        }
    }

    pub fn select_clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_all(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().all()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_invert(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().invert()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn select_is_multi(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.select().is_multi().map(Some),
            None => Ok(None),
        }
    }

    pub fn select_options(&self) -> OpenPageResult<Option<Vec<WebElement>>> {
        match self.element {
            Some(element) => element.select().options().map(Some),
            None => Ok(None),
        }
    }

    pub fn select_selected_option(&self) -> OpenPageResult<Option<WebElement>> {
        match self.element {
            Some(element) => element.select().selected_option(),
            None => Ok(None),
        }
    }

    pub fn select_selected_options(&self) -> OpenPageResult<Option<Vec<WebElement>>> {
        match self.element {
            Some(element) => element.select().selected_options().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneClicker<'a, WebElement> {
    pub fn left(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.clicker().left(),
            None => Ok(false),
        }
    }

    pub fn left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .clicker()
                .left_with_options(by_js, timeout_ms, wait_stop),
            None => Ok(false),
        }
    }

    pub fn right(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().right()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn middle(&self, get_tab: bool) -> OpenPageResult<Option<BrowserTabReference>> {
        match self.element {
            Some(element) => element.clicker().middle(get_tab),
            None => Ok(None),
        }
    }

    pub fn multi(&self, times: u32) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().multi(times)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.clicker().at(offset_x, offset_y, button, count)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_upload(
        &self,
        files: &[String],
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.clicker().to_upload(files, timeout_ms, by_js),
            None => Ok(false),
        }
    }

    pub fn to_download(
        &self,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<crate::download::DownloadMission>> {
        match self.element {
            Some(element) => element.clicker().to_download(
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            ),
            None => Ok(None),
        }
    }

    pub fn for_new_tab(
        &self,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<BrowserTabReference>> {
        match self.element {
            Some(element) => element.clicker().for_new_tab(timeout_ms, by_js),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneScroller<'a, WebElement> {
    pub fn to_top(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_top()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_bottom(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_bottom()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_half(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_half()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_rightmost(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_rightmost()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_leftmost(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_leftmost()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_location(x, y)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().up(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().down(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().left(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().right(pixels)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_see(&self, center: Option<bool>) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_see(center)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn to_center(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.scroll().to_center()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneSetter<'a, WebElement> {
    pub fn inner_html(&self, html: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().inner_html(html)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().property(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().style(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().attr(name, value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn value(&self, value: &str) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.set().value(value)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneStates<'a, WebElement> {
    pub fn is_in_viewport(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_in_viewport().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_whole_in_viewport().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_alive(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_alive().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_checked(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_checked().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_selected(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_selected().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_enabled(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_enabled().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_displayed(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_displayed().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_covered(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_covered().map(Some),
            None => Ok(None),
        }
    }

    pub fn is_clickable(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().is_clickable().map(Some),
            None => Ok(None),
        }
    }

    pub fn has_rect(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.states().has_rect().map(Some),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneRect<'a, WebElement> {
    pub fn corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self.element {
            Some(element) => element.rect().corners(),
            None => Ok(None),
        }
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<Vec<(f64, f64)>>> {
        match self.element {
            Some(element) => element.rect().viewport_corners(),
            None => Ok(None),
        }
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().location(),
            None => Ok(None),
        }
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_location(),
            None => Ok(None),
        }
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_location(),
            None => Ok(None),
        }
    }

    pub fn midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().midpoint(),
            None => Ok(None),
        }
    }

    pub fn viewport_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_midpoint(),
            None => Ok(None),
        }
    }

    pub fn click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().click_point(),
            None => Ok(None),
        }
    }

    pub fn viewport_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().viewport_click_point(),
            None => Ok(None),
        }
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().size(),
            None => Ok(None),
        }
    }

    pub fn screen_midpoint(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_midpoint(),
            None => Ok(None),
        }
    }

    pub fn screen_click_point(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().screen_click_point(),
            None => Ok(None),
        }
    }

    pub fn scroll_position(&self) -> OpenPageResult<Option<(f64, f64)>> {
        match self.element {
            Some(element) => element.rect().scroll_position(),
            None => Ok(None),
        }
    }
}

impl<'a> ElementsOneWait<'a, WebElement> {
    pub fn displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().displayed(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().hidden(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().enabled(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().disabled(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().deleted(timeout_ms),
            None => Ok(true),
        }
    }

    pub fn clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().clickable(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().has_rect(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().covered(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().not_covered(timeout_ms),
            None => Ok(false),
        }
    }

    pub fn disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().disabled_or_deleted(timeout_ms),
            None => Ok(true),
        }
    }

    pub fn stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.wait().stop_moving(timeout_ms),
            None => Ok(false),
        }
    }
}

impl<'a> ElementsOneSelector<'a, WebElement> {
    pub fn by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text(text),
            None => Ok(false),
        }
    }

    pub fn by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value(value),
            None => Ok(false),
        }
    }

    pub fn by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index(index),
            None => Ok(false),
        }
    }

    pub fn by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn by_option<'b, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<crate::webpage::WebSelectOptionInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().by_option(option),
            None => Ok(false),
        }
    }

    pub fn by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().by_options(options),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text<'b, I>(&self, text: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_text(text),
            None => Ok(false),
        }
    }

    pub fn cancel_by_text_with_timeout<'b, I>(
        &self,
        text: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_text_with_timeout(text, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value<'b, I>(&self, value: I) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_value(value),
            None => Ok(false),
        }
    }

    pub fn cancel_by_value_with_timeout<'b, I>(
        &self,
        value: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::page::ActionsInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_value_with_timeout(value, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index<I>(&self, index: I) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_index(index),
            None => Ok(false),
        }
    }

    pub fn cancel_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        I: Into<crate::element::SelectIndexInput>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_index_with_timeout(index, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices(&self, indices: &[usize]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_indices(indices),
            None => Ok(false),
        }
    }

    pub fn cancel_by_indices_with_timeout(
        &self,
        indices: &[usize],
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_indices_with_timeout(indices, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator<'b, L>(&self, locator: L) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_locator(locator),
            None => Ok(false),
        }
    }

    pub fn cancel_by_locator_with_timeout<'b, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<bool>
    where
        L: Into<crate::locator::LocatorBatchInput<'b>>,
    {
        match self.element {
            Some(element) => element
                .select()
                .cancel_by_locator_with_timeout(locator, timeout_ms),
            None => Ok(false),
        }
    }

    pub fn cancel_by_option<'b, I>(&self, option: I) -> OpenPageResult<bool>
    where
        I: Into<crate::webpage::WebSelectOptionInput<'b>>,
    {
        match self.element {
            Some(element) => element.select().cancel_by_option(option),
            None => Ok(false),
        }
    }

    pub fn cancel_by_options(&self, options: &[&WebElement]) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => element.select().cancel_by_options(options),
            None => Ok(false),
        }
    }

    pub fn all(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().all()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn clear(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().clear()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn invert(&self) -> OpenPageResult<bool> {
        match self.element {
            Some(element) => {
                element.select().invert()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn is_multi(&self) -> OpenPageResult<Option<bool>> {
        match self.element {
            Some(element) => element.select().is_multi().map(Some),
            None => Ok(None),
        }
    }

    pub fn options(&self) -> OpenPageResult<Option<Vec<WebElement>>> {
        match self.element {
            Some(element) => element.select().options().map(Some),
            None => Ok(None),
        }
    }

    pub fn selected_option(&self) -> OpenPageResult<Option<WebElement>> {
        match self.element {
            Some(element) => element.select().selected_option(),
            None => Ok(None),
        }
    }

    pub fn selected_options(&self) -> OpenPageResult<Option<Vec<WebElement>>> {
        match self.element {
            Some(element) => element.select().selected_options().map(Some),
            None => Ok(None),
        }
    }
}

impl<T> ElementsListExt<T> for Vec<T>
where
    T: 'static,
{
    fn get(&self) -> ElementsGetter<'_, T> {
        ElementsGetter {
            elements: collect_refs(self.iter()),
        }
    }

    fn filter(&self) -> ElementsFilter<'_, T> {
        let elements = collect_refs(self.iter());
        ElementsFilter {
            config: elements_one_runtime_config_from_ref(elements.first().copied()),
            elements,
        }
    }

    fn filter_one(&self) -> ElementsFilterOne<'_, T> {
        self.filter_one_at(1)
    }

    fn filter_one_at(&self, index: usize) -> ElementsFilterOne<'_, T> {
        let elements = collect_refs(self.iter());
        ElementsFilterOne {
            config: elements_one_runtime_config_from_ref(elements.first().copied()),
            elements,
            index: index.max(1),
        }
    }

    fn search(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsFilter<'_, T>>
    where
        T: ElementListSearchItem,
    {
        self.filter().search(criteria)
    }

    fn search_one(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsOne<'_, T>>
    where
        T: ElementListSearchItem,
    {
        self.search_one_at(1, criteria)
    }

    fn search_one_at(
        &self,
        index: usize,
        criteria: &ElementsSearch,
    ) -> OpenPageResult<ElementsOne<'_, T>>
    where
        T: ElementListSearchItem,
    {
        self.filter_one_at(index).search(criteria)
    }
}

impl<'a, T> ElementsGetter<'a, T>
where
    T: ElementListItem,
{
    pub fn attrs(&self, name: &str) -> OpenPageResult<Vec<Option<String>>> {
        self.elements
            .iter()
            .map(|element| element.list_attr(name))
            .collect()
    }

    pub fn links(&self) -> OpenPageResult<Vec<Option<String>>> {
        self.elements
            .iter()
            .map(|element| element.list_link())
            .collect()
    }

    pub fn texts(&self) -> OpenPageResult<Vec<Option<String>>> {
        self.elements
            .iter()
            .map(|element| element.list_text())
            .collect()
    }
}

impl<'a, T> ElementsFilter<'a, T>
where
    T: ElementListItem,
{
    pub fn get(&self) -> ElementsGetter<'a, T> {
        ElementsGetter {
            elements: self.elements.clone(),
        }
    }

    pub fn filter_one(&self) -> ElementsFilterOne<'a, T> {
        self.filter_one_at(1)
    }

    pub fn filter_one_at(&self, index: usize) -> ElementsFilterOne<'a, T> {
        ElementsFilterOne {
            config: self.config,
            elements: self.elements.clone(),
            index: index.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn first(&self) -> Option<&'a T> {
        self.elements.first().copied()
    }

    pub fn nth(&self, index: usize) -> Option<&'a T> {
        if index == 0 {
            return None;
        }
        self.elements.as_slice().get(index - 1).copied()
    }

    pub fn attr(self, name: &str, value: &str, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_option_string(element.list_attr(name)?, value, equal))
    }

    pub fn text(self, text: &str, fuzzy: bool, contain: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| {
            match_text(element.list_raw_text()?.as_deref(), text, fuzzy, contain)
        })
    }

    pub fn tag(self, name: &str, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_tag(element.list_tag()?, name, equal))
    }

    fn filter_matching<F>(self, mut predicate: F) -> OpenPageResult<Self>
    where
        F: FnMut(&T) -> OpenPageResult<bool>,
    {
        let mut elements = Vec::with_capacity(self.elements.len());
        for element in self.elements {
            if predicate(element)? {
                elements.push(element);
            }
        }
        Ok(Self {
            elements,
            config: self.config,
        })
    }
}

impl<'a, T> IntoIterator for ElementsFilter<'a, T> {
    type Item = &'a T;
    type IntoIter = std::vec::IntoIter<&'a T>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.into_iter()
    }
}

impl<'a, T> ElementsFilter<'a, T>
where
    T: ElementListItem + ElementListStateItem,
{
    pub fn displayed(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_is_displayed()?, equal))
    }

    pub fn checked(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_is_checked()?, equal))
    }

    pub fn selected(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_is_selected()?, equal))
    }

    pub fn enabled(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_is_enabled()?, equal))
    }

    pub fn clickable(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_is_clickable()?, equal))
    }

    pub fn have_rect(self, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_bool(element.list_has_rect()?, equal))
    }

    pub fn search(self, criteria: &ElementsSearch) -> OpenPageResult<Self> {
        if criteria.is_empty() {
            return Ok(Self {
                elements: Vec::new(),
                config: self.config,
            });
        }
        self.filter_matching(|element| search_matches(element, criteria))
    }

    pub fn search_one(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsOne<'a, T>> {
        self.search_one_at(1, criteria)
    }

    pub fn search_one_at(
        &self,
        index: usize,
        criteria: &ElementsSearch,
    ) -> OpenPageResult<ElementsOne<'a, T>> {
        self.filter_one_at(index).search(criteria)
    }
}

impl<'a, T> ElementsFilter<'a, T>
where
    T: ElementListItem + ElementListDriverItem,
{
    pub fn style(self, name: &str, value: &str, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| match_string(element.list_style(name)?, value, equal))
    }

    pub fn property(self, name: &str, value: &str, equal: bool) -> OpenPageResult<Self> {
        self.filter_matching(|element| {
            match_option_string(
                element.list_property(name)?.map(property_value_to_string),
                value,
                equal,
            )
        })
    }
}

impl<'a, T> ElementsFilterOne<'a, T>
where
    T: ElementListItem,
{
    pub fn attr(&self, name: &str, value: &str, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_option_string(element.list_attr(name)?, value, equal),
            || {
                elements_one_missing_message(
                    "filter_one.attr()",
                    &[
                        ("name", format!("{name:?}")),
                        ("value", format!("{value:?}")),
                        ("equal", equal.to_string()),
                    ],
                    self.index,
                )
            },
        )
    }

    pub fn text(
        &self,
        text: &str,
        fuzzy: bool,
        contain: bool,
    ) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_text(element.list_raw_text()?.as_deref(), text, fuzzy, contain),
            || {
                elements_one_missing_message(
                    "filter_one.text()",
                    &[
                        ("text", format!("{text:?}")),
                        ("fuzzy", fuzzy.to_string()),
                        ("contain", contain.to_string()),
                    ],
                    self.index,
                )
            },
        )
    }

    pub fn tag(&self, name: &str, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_tag(element.list_tag()?, name, equal),
            || {
                elements_one_missing_message(
                    "filter_one.tag()",
                    &[("name", format!("{name:?}")), ("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    fn find_matching<F, M>(
        &self,
        mut predicate: F,
        missing_message: M,
    ) -> OpenPageResult<ElementsOne<'a, T>>
    where
        F: FnMut(&T) -> OpenPageResult<bool>,
        M: FnOnce() -> String,
    {
        let mut current_index = 0usize;
        for element in &self.elements {
            if predicate(element)? {
                current_index += 1;
                if current_index == self.index {
                    return Ok(ElementsOne::some_with_config(element, self.config));
                }
            }
        }
        if elements_one_missing_config_snapshot(self.config)?
            .is_some_and(|config| config.raise_when_not_found)
        {
            return Err(OpenPageError::ElementNotFound(missing_message()));
        }
        Ok(ElementsOne::none_with_config(self.config))
    }
}

impl<'a, T> ElementsFilterOne<'a, T>
where
    T: ElementListItem + ElementListStateItem,
{
    pub fn displayed(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_is_displayed()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.displayed()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn checked(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_is_checked()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.checked()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn selected(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_is_selected()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.selected()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn enabled(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_is_enabled()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.enabled()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn clickable(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_is_clickable()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.clickable()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn have_rect(&self, equal: bool) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_bool(element.list_has_rect()?, equal),
            || {
                elements_one_missing_message(
                    "filter_one.have_rect()",
                    &[("equal", equal.to_string())],
                    self.index,
                )
            },
        )
    }

    pub fn search(&self, criteria: &ElementsSearch) -> OpenPageResult<ElementsOne<'a, T>> {
        if criteria.is_empty() {
            return Ok(ElementsOne::none_with_config(self.config));
        }
        self.find_matching(
            |element| search_matches(element, criteria),
            || {
                elements_one_missing_message(
                    "search_one()",
                    &[("criteria", elements_search_debug(criteria))],
                    self.index,
                )
            },
        )
    }
}

impl<'a, T> ElementsFilterOne<'a, T>
where
    T: ElementListItem + ElementListDriverItem,
{
    pub fn style(
        &self,
        name: &str,
        value: &str,
        equal: bool,
    ) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| match_string(element.list_style(name)?, value, equal),
            || {
                elements_one_missing_message(
                    "filter_one.style()",
                    &[
                        ("name", format!("{name:?}")),
                        ("value", format!("{value:?}")),
                        ("equal", equal.to_string()),
                    ],
                    self.index,
                )
            },
        )
    }

    pub fn property(
        &self,
        name: &str,
        value: &str,
        equal: bool,
    ) -> OpenPageResult<ElementsOne<'a, T>> {
        self.find_matching(
            |element| {
                match_option_string(
                    element.list_property(name)?.map(property_value_to_string),
                    value,
                    equal,
                )
            },
            || {
                elements_one_missing_message(
                    "filter_one.property()",
                    &[
                        ("name", format!("{name:?}")),
                        ("value", format!("{value:?}")),
                        ("equal", equal.to_string()),
                    ],
                    self.index,
                )
            },
        )
    }
}

impl ElementListItem for Element {
    fn list_attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.attr(name)
    }

    fn list_link(&self) -> OpenPageResult<Option<String>> {
        self.link()
    }

    fn list_text(&self) -> OpenPageResult<Option<String>> {
        self.text()
    }

    fn list_raw_text(&self) -> OpenPageResult<Option<String>> {
        self.raw_text()
    }

    fn list_tag(&self) -> OpenPageResult<String> {
        self.tag()
    }
}

impl ElementListStateItem for Element {
    fn list_is_displayed(&self) -> OpenPageResult<bool> {
        self.is_displayed()
    }

    fn list_is_checked(&self) -> OpenPageResult<bool> {
        self.is_checked()
    }

    fn list_is_selected(&self) -> OpenPageResult<bool> {
        self.is_selected()
    }

    fn list_is_enabled(&self) -> OpenPageResult<bool> {
        self.is_enabled()
    }

    fn list_is_clickable(&self) -> OpenPageResult<bool> {
        self.is_clickable()
    }

    fn list_has_rect(&self) -> OpenPageResult<bool> {
        self.has_rect()
    }
}

impl ElementListDriverItem for Element {
    fn list_style(&self, name: &str) -> OpenPageResult<String> {
        self.style(name, None)
    }

    fn list_pseudo_before(&self) -> OpenPageResult<String> {
        self.pseudo_before()
    }

    fn list_pseudo_after(&self) -> OpenPageResult<String> {
        self.pseudo_after()
    }

    fn list_property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.property(name)
    }
}

impl ElementListContentItem for Element {
    fn list_html(&self) -> OpenPageResult<Option<String>> {
        self.html()
    }

    fn list_inner_html(&self) -> OpenPageResult<Option<String>> {
        self.inner_html()
    }

    fn list_value(&self) -> OpenPageResult<Option<String>> {
        self.value()
    }
}

impl ElementListAttrsItem for Element {
    fn list_attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.attrs()
    }
}

impl ElementListMetaItem for Element {
    fn list_child_count(&self) -> OpenPageResult<usize> {
        self.child_count()
    }

    fn list_css_path(&self) -> OpenPageResult<String> {
        self.css_path()
    }

    fn list_xpath(&self) -> OpenPageResult<String> {
        self.xpath()
    }

    fn list_comments(&self) -> OpenPageResult<Vec<String>> {
        self.comments()
    }
}

impl ElementListItem for WebElement {
    fn list_attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.attr(name)
    }

    fn list_link(&self) -> OpenPageResult<Option<String>> {
        self.link()
    }

    fn list_text(&self) -> OpenPageResult<Option<String>> {
        self.text()
    }

    fn list_raw_text(&self) -> OpenPageResult<Option<String>> {
        self.raw_text()
    }

    fn list_tag(&self) -> OpenPageResult<String> {
        self.tag()
    }
}

impl ElementListStateItem for WebElement {
    fn list_is_displayed(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_displayed(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("displayed()"),
            )),
        }
    }

    fn list_is_checked(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_checked(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("checked()"),
            )),
        }
    }

    fn list_is_selected(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_selected(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("selected()"),
            )),
        }
    }

    fn list_is_enabled(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_enabled(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("enabled()"),
            )),
        }
    }

    fn list_is_clickable(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.is_clickable(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("clickable()"),
            )),
        }
    }

    fn list_has_rect(&self) -> OpenPageResult<bool> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.has_rect(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("have_rect()"),
            )),
        }
    }
}

impl ElementListDriverItem for WebElement {
    fn list_style(&self, name: &str) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.style(name, None),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("style()"),
            )),
        }
    }

    fn list_pseudo_before(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.pseudo_before(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("pseudo_before()"),
            )),
        }
    }

    fn list_pseudo_after(&self) -> OpenPageResult<String> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.pseudo_after(),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("pseudo_after()"),
            )),
        }
    }

    fn list_property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        match self {
            Self::Browser(element) | Self::Mix { element, .. } => element.property(name),
            Self::Session(_) => Err(OpenPageError::UnsupportedOperation(
                web_element_list_driver_filter_message("property()"),
            )),
        }
    }
}

impl ElementListContentItem for WebElement {
    fn list_html(&self) -> OpenPageResult<Option<String>> {
        self.html()
    }

    fn list_inner_html(&self) -> OpenPageResult<Option<String>> {
        self.inner_html()
    }

    fn list_value(&self) -> OpenPageResult<Option<String>> {
        self.value()
    }
}

impl ElementListAttrsItem for WebElement {
    fn list_attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.attrs()
    }
}

impl ElementListMetaItem for WebElement {
    fn list_child_count(&self) -> OpenPageResult<usize> {
        self.child_count()
    }

    fn list_css_path(&self) -> OpenPageResult<String> {
        self.css_path()
    }

    fn list_xpath(&self) -> OpenPageResult<String> {
        self.xpath()
    }

    fn list_comments(&self) -> OpenPageResult<Vec<String>> {
        self.comments()
    }
}

impl ElementListItem for SessionElement {
    fn list_attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.attr(name)
    }

    fn list_link(&self) -> OpenPageResult<Option<String>> {
        self.link()
    }

    fn list_text(&self) -> OpenPageResult<Option<String>> {
        self.text()
    }

    fn list_raw_text(&self) -> OpenPageResult<Option<String>> {
        self.raw_text()
    }

    fn list_tag(&self) -> OpenPageResult<String> {
        self.tag()
    }
}

impl ElementListContentItem for SessionElement {
    fn list_html(&self) -> OpenPageResult<Option<String>> {
        self.html()
    }

    fn list_inner_html(&self) -> OpenPageResult<Option<String>> {
        self.inner_html()
    }

    fn list_value(&self) -> OpenPageResult<Option<String>> {
        self.attr("value")
    }
}

impl ElementListAttrsItem for SessionElement {
    fn list_attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.attrs()
    }
}

impl ElementListMetaItem for SessionElement {
    fn list_child_count(&self) -> OpenPageResult<usize> {
        self.child_count()
    }

    fn list_css_path(&self) -> OpenPageResult<String> {
        self.css_path()
    }

    fn list_xpath(&self) -> OpenPageResult<String> {
        self.xpath()
    }

    fn list_comments(&self) -> OpenPageResult<Vec<String>> {
        self.comments()
    }
}

fn collect_refs<'a, T>(iter: impl Iterator<Item = &'a T>) -> Vec<&'a T> {
    iter.collect()
}

fn search_matches<T>(element: &T, criteria: &ElementsSearch) -> OpenPageResult<bool>
where
    T: ElementListSearchItem,
{
    let mut considered = false;
    let mut matched = false;

    if let Some(expected) = criteria.displayed {
        considered = true;
        matched |= element.list_is_displayed()? == expected;
    }
    if let Some(expected) = criteria.checked {
        considered = true;
        matched |= element.list_is_checked()? == expected;
    }
    if let Some(expected) = criteria.selected {
        considered = true;
        matched |= element.list_is_selected()? == expected;
    }
    if let Some(expected) = criteria.enabled {
        considered = true;
        matched |= element.list_is_enabled()? == expected;
    }
    if let Some(expected) = criteria.clickable {
        considered = true;
        matched |= element.list_is_clickable()? == expected;
    }
    if let Some(expected) = criteria.have_rect {
        considered = true;
        matched |= element.list_has_rect()? == expected;
    }
    if let Some(expected) = criteria.have_text {
        considered = true;
        matched |= element
            .list_raw_text()?
            .is_some_and(|value| !value.is_empty())
            == expected;
    }
    if let Some(expected) = criteria.tag.as_deref() {
        considered = true;
        matched |= element.list_tag()?.eq_ignore_ascii_case(expected);
    }

    Ok(considered && matched)
}

fn property_value_to_string(value: Value) -> String {
    match value {
        Value::String(value) => value,
        other => other.to_string(),
    }
}

fn match_bool(actual: bool, expected: bool) -> OpenPageResult<bool> {
    Ok(actual == expected)
}

fn match_string(actual: String, expected: &str, equal: bool) -> OpenPageResult<bool> {
    Ok((actual == expected) == equal)
}

fn match_option_string(
    actual: Option<String>,
    expected: &str,
    equal: bool,
) -> OpenPageResult<bool> {
    Ok((actual.as_deref() == Some(expected)) == equal)
}

fn match_tag(actual: String, expected: &str, equal: bool) -> OpenPageResult<bool> {
    Ok(actual.eq_ignore_ascii_case(expected) == equal)
}

fn match_text(
    actual: Option<&str>,
    expected: &str,
    fuzzy: bool,
    contain: bool,
) -> OpenPageResult<bool> {
    let matches = actual.is_some_and(|value| {
        if fuzzy {
            value.contains(expected)
        } else {
            value == expected
        }
    });
    Ok(matches == contain)
}

#[cfg(test)]
mod tests {
    use super::ElementsListExt;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::session::{SessionXPathResult, snapshot_find, snapshot_find_all};
    use crate::settings::scoped_test_settings;
    use crate::{By, OpenPageError, OpenPageResult, Settings, WebElement};

    #[derive(Debug)]
    struct FakeItem {
        attr: Option<String>,
        text: Option<String>,
        tag: String,
        displayed: bool,
        selected: bool,
        attrs: Vec<(String, String)>,
        child_count: usize,
        css_path: String,
        xpath: String,
        comments: Vec<String>,
        html: Option<String>,
        inner_html: Option<String>,
        value: Option<String>,
        style: String,
        property: Option<serde_json::Value>,
    }

    impl super::ElementListItem for FakeItem {
        fn list_attr(&self, _name: &str) -> OpenPageResult<Option<String>> {
            Ok(self.attr.clone())
        }

        fn list_link(&self) -> OpenPageResult<Option<String>> {
            Ok(self.attr.clone())
        }

        fn list_text(&self) -> OpenPageResult<Option<String>> {
            Ok(self.text.clone())
        }

        fn list_raw_text(&self) -> OpenPageResult<Option<String>> {
            Ok(self.text.clone())
        }

        fn list_tag(&self) -> OpenPageResult<String> {
            Ok(self.tag.clone())
        }
    }

    impl super::ElementListStateItem for FakeItem {
        fn list_is_displayed(&self) -> OpenPageResult<bool> {
            Ok(self.displayed)
        }

        fn list_is_checked(&self) -> OpenPageResult<bool> {
            Ok(false)
        }

        fn list_is_selected(&self) -> OpenPageResult<bool> {
            Ok(self.selected)
        }

        fn list_is_enabled(&self) -> OpenPageResult<bool> {
            Ok(true)
        }

        fn list_is_clickable(&self) -> OpenPageResult<bool> {
            Ok(self.displayed)
        }

        fn list_has_rect(&self) -> OpenPageResult<bool> {
            Ok(self.displayed)
        }
    }

    impl super::ElementListDriverItem for FakeItem {
        fn list_style(&self, _name: &str) -> OpenPageResult<String> {
            Ok(self.style.clone())
        }

        fn list_pseudo_before(&self) -> OpenPageResult<String> {
            Ok("before".to_string())
        }

        fn list_pseudo_after(&self) -> OpenPageResult<String> {
            Ok("after".to_string())
        }

        fn list_property(&self, _name: &str) -> OpenPageResult<Option<serde_json::Value>> {
            Ok(self.property.clone())
        }
    }

    impl super::ElementListContentItem for FakeItem {
        fn list_html(&self) -> OpenPageResult<Option<String>> {
            Ok(self.html.clone())
        }

        fn list_inner_html(&self) -> OpenPageResult<Option<String>> {
            Ok(self.inner_html.clone())
        }

        fn list_value(&self) -> OpenPageResult<Option<String>> {
            Ok(self.value.clone())
        }
    }

    impl super::ElementListAttrsItem for FakeItem {
        fn list_attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
            Ok(self.attrs.clone())
        }
    }

    impl super::ElementListMetaItem for FakeItem {
        fn list_child_count(&self) -> OpenPageResult<usize> {
            Ok(self.child_count)
        }

        fn list_css_path(&self) -> OpenPageResult<String> {
            Ok(self.css_path.clone())
        }

        fn list_xpath(&self) -> OpenPageResult<String> {
            Ok(self.xpath.clone())
        }

        fn list_comments(&self) -> OpenPageResult<Vec<String>> {
            Ok(self.comments.clone())
        }
    }

    #[test]
    fn session_element_list_getter_returns_attrs_links_and_texts() {
        let items = snapshot_find_all(
            r#"
            <div>
                <a class="item" href="https://example.test/one">One</a>
                <a class="item">Two</a>
                <img class="item" src="https://example.test/three.png" />
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements");

        assert_eq!(
            items.get().attrs("href").expect("href attrs"),
            vec![Some("https://example.test/one".to_string()), None, None,]
        );
        assert_eq!(
            items.get().links().expect("links"),
            vec![
                Some("https://example.test/one".to_string()),
                None,
                Some("https://example.test/three.png".to_string()),
            ]
        );
        assert_eq!(
            items.get().texts().expect("texts"),
            vec![Some("One".to_string()), Some("Two".to_string()), None,]
        );
    }

    #[test]
    fn session_element_list_filter_and_filter_one_support_attr_and_text() {
        let items = snapshot_find_all(
            r#"
            <div>
                <span class="item" data-kind="keep">Alpha keep</span>
                <span class="item" data-kind="drop">Beta</span>
                <span class="item" data-kind="keep">Gamma keep</span>
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements");

        let kept = items
            .filter()
            .attr("data-kind", "keep", true)
            .expect("filter keep");
        assert_eq!(kept.len(), 2);
        assert_eq!(
            kept.get().texts().expect("kept texts"),
            vec![
                Some("Alpha keep".to_string()),
                Some("Gamma keep".to_string()),
            ]
        );

        let dropped = items
            .filter_one()
            .attr("data-kind", "keep", false)
            .expect("filter_one keep false")
            .expect("dropped item");
        assert_eq!(dropped.text().expect("drop text"), Some("Beta".to_string()));

        let second_keep = items
            .filter_one_at(2)
            .text("keep", true, true)
            .expect("filter_one text")
            .expect("second keep");
        assert_eq!(
            second_keep.text().expect("second keep text"),
            Some("Gamma keep".to_string())
        );
    }

    #[test]
    fn session_element_list_filter_supports_tag_and_filtered_getter() {
        let items = snapshot_find_all(
            r#"
            <div>
                <span class="item">Alpha</span>
                <a class="item" href="https://example.test/one">One</a>
                <a class="item" href="https://example.test/two">Two</a>
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements");

        let links = items.filter().tag("a", true).expect("tag filter");
        assert_eq!(links.len(), 2);
        assert_eq!(
            links.get().attrs("href").expect("filtered href attrs"),
            vec![
                Some("https://example.test/one".to_string()),
                Some("https://example.test/two".to_string()),
            ]
        );
    }

    #[test]
    fn web_element_list_driver_filter_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let items: Vec<WebElement> = snapshot_find_all(
            r#"
            <div>
                <button class="item" id="ok">OK</button>
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements")
        .into_iter()
        .map(WebElement::Session)
        .collect();

        let english = items
            .filter()
            .displayed(true)
            .expect_err("session-backed WebElement displayed filter should fail");
        assert!(matches!(
            english,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains(
                    "displayed() filters are only available for driver-backed WebElement lists"
                )
        ));

        Settings::set_language("cn");

        let chinese_style = items
            .filter()
            .style("display", "block", true)
            .expect_err("session-backed WebElement style filter should fail");
        assert!(matches!(
            chinese_style,
            OpenPageError::UnsupportedOperation(ref message)
                if message.contains("style() 过滤器仅适用于 driver-backed WebElement 列表")
        ));
    }

    #[test]
    fn elements_one_wraps_optional_single_match() {
        let items = snapshot_find_all(
            r#"
            <div>
                <span class="item" data-kind="keep">Alpha keep</span>
                <span class="item" data-kind="drop">Beta</span>
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements");

        let found = items
            .filter_one()
            .attr("data-kind", "keep", true)
            .expect("filter one keep");
        assert!(found.is_some());
        assert_eq!(
            found.expect("keep item").text().expect("keep text"),
            Some("Alpha keep".to_string())
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing filter");
        assert!(missing.is_none());
        let missing_option = missing.into_option();
        assert!(missing_option.is_none());
    }

    #[test]
    fn borrowed_session_elements_one_supports_find_aliases() {
        let items = snapshot_find_all(
            r#"
            <main>
                <article class="card" data-kind="primary">
                    <h2 class="title">Alpha</h2>
                    <a class="link" href="/alpha">Open</a>
                </article>
                <article class="card" data-kind="secondary">
                    <h2 class="title">Beta</h2>
                </article>
            </main>
            "#,
            ".card",
        )
        .expect("snapshot elements");

        let found = items
            .filter_one()
            .attr("data-kind", "primary", true)
            .expect("primary card");
        assert_eq!(
            found
                .ele(".title")
                .expect("child element")
                .text()
                .expect("child text"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            found
                .s_ele((By::CLASS_NAME, "link"))
                .expect("static child")
                .attr("href")
                .expect("static child href"),
            Some("/alpha".to_string())
        );
        assert_eq!(found.eles(".title").expect("child elements").len(), 1);
        assert_eq!(
            found
                .s_eles((By::CLASS_NAME, "title"))
                .expect("static children")
                .len(),
            1
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing card");
        assert!(missing.ele(".title").expect("missing child").is_none());
        assert!(
            missing
                .s_ele(".title")
                .expect("missing static child")
                .is_none()
        );
        assert_eq!(missing.eles(".title").expect("missing children").len(), 0);
        assert_eq!(
            missing
                .s_eles(".title")
                .expect("missing static children")
                .len(),
            0
        );
    }

    #[test]
    fn borrowed_browser_elements_one_relative_navigation_handles_missing_owner() {
        let missing = super::ElementsOne::<crate::Element>::none();

        assert!(missing.child().expect("missing child owner").is_none());
        assert!(missing.parent().expect("missing parent owner").is_none());
        assert!(missing.prev().expect("missing prev owner").is_none());
        assert!(missing.next().expect("missing next owner").is_none());
        assert!(missing.before().expect("missing before owner").is_none());
        assert!(missing.after().expect("missing after owner").is_none());
    }

    #[test]
    fn borrowed_browser_elements_one_multi_relative_navigation_handles_missing_owner() {
        let missing = super::ElementsOne::<crate::Element>::none();

        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn borrowed_browser_elements_one_visual_navigation_handles_missing_owner() {
        let missing = super::ElementsOne::<crate::Element>::none();

        assert!(missing.over().expect("missing over owner").is_none());
        assert!(
            missing
                .over_with_timeout(1)
                .expect("missing over timeout owner")
                .is_none()
        );
        assert!(
            missing
                .offset(None::<&str>, None, None, 1)
                .expect("missing offset owner")
                .is_none()
        );
        assert!(
            missing
                .east(None::<&str>, None, 1)
                .expect("missing east owner")
                .is_none()
        );
        assert!(
            missing
                .south(None::<&str>, None, 1)
                .expect("missing south owner")
                .is_none()
        );
        assert!(
            missing
                .west(None::<&str>, None, 1)
                .expect("missing west owner")
                .is_none()
        );
        assert!(
            missing
                .north(None::<&str>, None, 1)
                .expect("missing north owner")
                .is_none()
        );
    }

    #[test]
    fn elements_one_visual_direction_signatures_accept_by_tuples() {
        fn assert_browser_owned(element: &super::ElementsOneOwned<crate::Element>) {
            let _ = element.east(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.south(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.west(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.north(Some((By::CLASS_NAME, "item")), None, 1);
        }

        fn assert_web_owned(element: &super::ElementsOneOwned<WebElement>) {
            let _ = element.east(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.south(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.west(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.north(Some((By::CLASS_NAME, "item")), None, 1);
        }

        fn assert_browser_borrowed(element: &super::ElementsOne<'_, crate::Element>) {
            let _ = element.east(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.south(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.west(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.north(Some((By::CLASS_NAME, "item")), None, 1);
        }

        fn assert_web_borrowed(element: &super::ElementsOne<'_, WebElement>) {
            let _ = element.east(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.south(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.west(Some((By::CLASS_NAME, "item")), None, 1);
            let _ = element.north(Some((By::CLASS_NAME, "item")), None, 1);
        }

        let _ = assert_browser_owned as fn(&super::ElementsOneOwned<crate::Element>);
        let _ = assert_web_owned as fn(&super::ElementsOneOwned<WebElement>);
        let _ = assert_browser_borrowed as fn(&super::ElementsOne<'_, crate::Element>);
        let _ = assert_web_borrowed as fn(&super::ElementsOne<'_, WebElement>);
    }

    #[test]
    fn browser_elements_one_owned_multi_relative_navigation_handles_missing_owner() {
        let missing: super::ElementsOneOwned<crate::Element> =
            super::ElementsOneOwned::none_with_config(None);

        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn borrowed_session_elements_one_supports_relative_navigation() {
        let items = snapshot_find_all(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                    <p class="summary">One</p>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                    <p class="summary">Two</p>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            ".card",
        )
        .expect("snapshot elements");

        let second = items
            .filter_one()
            .attr("data-kind", "second", true)
            .expect("second card");
        assert_eq!(
            second
                .child_with(Some(".title"), 1)
                .expect("title child")
                .text()
                .expect("title text"),
            Some("Beta".to_string())
        );
        assert_eq!(
            second.parent().expect("parent").tag().expect("parent tag"),
            Some("main".to_string())
        );
        assert_eq!(
            second
                .prev()
                .expect("previous card")
                .attr("data-kind")
                .expect("previous kind"),
            Some("first".to_string())
        );
        assert_eq!(
            second
                .next()
                .expect("next card")
                .attr("data-kind")
                .expect("next kind"),
            Some("third".to_string())
        );
        assert!(
            second
                .child_with(Some(".missing"), 1)
                .expect("missing child")
                .is_none()
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing card");
        assert!(missing.child().expect("missing child owner").is_none());
        assert!(missing.parent().expect("missing parent owner").is_none());
        assert!(missing.next().expect("missing next owner").is_none());
    }

    #[test]
    fn borrowed_session_elements_one_supports_multi_relative_navigation() {
        let items = snapshot_find_all(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                    <p class="summary">One</p>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                    <p class="summary">Two</p>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            ".card",
        )
        .expect("snapshot elements");

        let second = items
            .filter_one()
            .attr("data-kind", "second", true)
            .expect("second card");
        assert_eq!(second.children().expect("children").len(), 2);
        assert_eq!(
            second
                .children_with(Some(".title"))
                .expect("title children")
                .len(),
            1
        );
        assert_eq!(second.prevs().expect("previous siblings").len(), 1);
        assert_eq!(second.nexts().expect("next siblings").len(), 1);
        assert_eq!(
            second
                .befores_with(Some(".title"))
                .expect("before title elements")
                .len(),
            1
        );
        assert_eq!(
            second
                .afters_with(Some(".title"))
                .expect("after title elements")
                .len(),
            1
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing card");
        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn borrowed_session_elements_one_supports_node_list_navigation() {
        let items = snapshot_find_all(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
            "#root, #b",
        )
        .expect("snapshot elements");
        let root = items
            .filter_one()
            .attr("id", "root", true)
            .expect("root element");
        let second = items
            .filter_one()
            .attr("id", "b", true)
            .expect("second element");

        let children = root.children_nodes().expect("child nodes");
        assert_eq!(children.len(), 6);
        match &children[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child, got {other:?}"),
        }
        match &children[1] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child, got {other:?}"),
        }
        assert_eq!(
            root.children_nodes_with(Some("xpath:span"))
                .expect("span child nodes")
                .len(),
            2
        );

        let previous_comments = second
            .prev_nodes_with(Some("xpath:comment()"))
            .expect("previous comment nodes");
        assert_eq!(previous_comments.len(), 1);
        match &previous_comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }
        assert_eq!(second.next_nodes().expect("next nodes").len(), 2);
        assert_eq!(
            second
                .before_nodes_with(Some("xpath:span[@id='a']"))
                .expect("before span nodes")
                .len(),
            1
        );
        assert_eq!(
            second
                .after_nodes_with(Some("xpath:strong[@id='c']"))
                .expect("after strong nodes")
                .len(),
            1
        );

        let missing = items
            .filter_one()
            .attr("id", "missing", true)
            .expect("missing element");
        assert_eq!(
            missing.children_nodes().expect("missing child nodes").len(),
            0
        );
        assert_eq!(
            missing.prev_nodes().expect("missing previous nodes").len(),
            0
        );
        assert_eq!(missing.next_nodes().expect("missing next nodes").len(), 0);
        assert_eq!(
            missing.before_nodes().expect("missing before nodes").len(),
            0
        );
        assert_eq!(missing.after_nodes().expect("missing after nodes").len(), 0);
    }

    #[test]
    fn borrowed_session_elements_one_supports_single_node_navigation() {
        let items = snapshot_find_all(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
            "#root, #b",
        )
        .expect("snapshot elements");
        let root = items
            .filter_one()
            .attr("id", "root", true)
            .expect("root element");
        let second = items
            .filter_one()
            .attr("id", "b", true)
            .expect("second element");

        match root
            .child_node()
            .expect("child node")
            .expect("first child node")
        {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child node, got {other:?}"),
        }
        match root
            .child_node_with(Some("xpath:comment()"), 1)
            .expect("comment child node")
            .expect("comment child node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child node, got {other:?}"),
        }
        assert_eq!(
            root.children_nodes_with(Some((By::XPATH, "span")))
                .expect("span child nodes")
                .len(),
            2
        );
        match root
            .child_node_with(Some((By::XPATH, "comment()")), 1)
            .expect("tuple comment child node")
            .expect("tuple comment child node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected tuple comment child node, got {other:?}"),
        }
        match second
            .prev_node_with(Some("xpath:comment()"), 1)
            .expect("previous comment node")
            .expect("previous comment node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }
        match second.next_node().expect("next node").expect("next node") {
            SessionXPathResult::Element(element) => assert_eq!(
                element.attr("id").expect("next element id"),
                Some("c".to_string())
            ),
            other => panic!("expected next element node, got {other:?}"),
        }
        assert!(
            second
                .before_node_with(Some("xpath:span[@id='a']"), 1)
                .expect("before span node")
                .is_some()
        );
        assert!(
            second
                .after_node_with(Some("xpath:strong[@id='c']"), 1)
                .expect("after strong node")
                .is_some()
        );
        assert!(
            second
                .prev_node_with(Some("xpath:strong[@id='missing']"), 1)
                .expect("missing previous node")
                .is_none()
        );

        let missing = items
            .filter_one()
            .attr("id", "missing", true)
            .expect("missing element");
        assert!(missing.child_node().expect("missing child node").is_none());
        assert!(
            missing
                .prev_node()
                .expect("missing previous node")
                .is_none()
        );
        assert!(missing.next_node().expect("missing next node").is_none());
        assert!(
            missing
                .before_node()
                .expect("missing before node")
                .is_none()
        );
        assert!(missing.after_node().expect("missing after node").is_none());

        let config = Arc::new(Mutex::new(super::ElementsOneRuntimeConfig {
            raise_when_not_found: true,
            ..super::ElementsOneRuntimeConfig::default()
        }));
        let raising = super::ElementsOne::some_with_config(
            second.as_option().expect("second element"),
            Some(&config),
        );
        let error = raising
            .prev_node_with(Some("xpath:strong[@id='missing']"), 1)
            .expect_err("missing previous node should raise");
        assert!(matches!(error, crate::OpenPageError::ElementNotFound(_)));
    }

    #[test]
    fn borrowed_web_elements_one_supports_session_relative_navigation() {
        let items: Vec<WebElement> = snapshot_find_all(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            ".card",
        )
        .expect("snapshot elements")
        .into_iter()
        .map(WebElement::Session)
        .collect();

        let second = items
            .filter_one()
            .attr("data-kind", "second", true)
            .expect("second card");
        assert_eq!(
            second
                .child_with(Some(".title"), 1)
                .expect("title child")
                .text()
                .expect("title text"),
            Some("Beta".to_string())
        );
        assert_eq!(
            second.parent().expect("parent").tag().expect("parent tag"),
            Some("main".to_string())
        );
        assert_eq!(
            second
                .prev()
                .expect("previous card")
                .attr("data-kind")
                .expect("previous kind"),
            Some("first".to_string())
        );
        assert_eq!(
            second
                .next()
                .expect("next card")
                .attr("data-kind")
                .expect("next kind"),
            Some("third".to_string())
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing card");
        assert!(missing.child().expect("missing child owner").is_none());
        assert!(missing.parent().expect("missing parent owner").is_none());
        assert!(missing.next().expect("missing next owner").is_none());
    }

    #[test]
    fn borrowed_web_elements_one_supports_session_multi_relative_navigation() {
        let items: Vec<WebElement> = snapshot_find_all(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                    <p class="summary">One</p>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                    <p class="summary">Two</p>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            ".card",
        )
        .expect("snapshot elements")
        .into_iter()
        .map(WebElement::Session)
        .collect();

        let second = items
            .filter_one()
            .attr("data-kind", "second", true)
            .expect("second card");
        assert_eq!(second.children().expect("children").len(), 2);
        assert_eq!(
            second
                .children_with(Some(".title"))
                .expect("title children")
                .len(),
            1
        );
        assert_eq!(second.prevs().expect("previous siblings").len(), 1);
        assert_eq!(second.nexts().expect("next siblings").len(), 1);
        assert_eq!(
            second
                .befores_with(Some(".title"))
                .expect("before title elements")
                .len(),
            1
        );
        assert_eq!(
            second
                .afters_with(Some(".title"))
                .expect("after title elements")
                .len(),
            1
        );

        let missing = items
            .filter_one()
            .attr("data-kind", "missing", true)
            .expect("missing card");
        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn borrowed_web_elements_one_visual_navigation_handles_missing_owner() {
        let missing = super::ElementsOne::<WebElement>::none();

        assert!(missing.over().expect("missing over owner").is_none());
        assert!(
            missing
                .over_with_timeout(1)
                .expect("missing over timeout owner")
                .is_none()
        );
        assert!(
            missing
                .offset(None::<&str>, None, None, 1)
                .expect("missing offset owner")
                .is_none()
        );
        assert!(
            missing
                .east(None::<&str>, None, 1)
                .expect("missing east owner")
                .is_none()
        );
        assert!(
            missing
                .south(None::<&str>, None, 1)
                .expect("missing south owner")
                .is_none()
        );
        assert!(
            missing
                .west(None::<&str>, None, 1)
                .expect("missing west owner")
                .is_none()
        );
        assert!(
            missing
                .north(None::<&str>, None, 1)
                .expect("missing north owner")
                .is_none()
        );
    }

    #[test]
    fn elements_one_exposes_none_element_like_accessors() {
        let items = vec![FakeItem {
            attr: Some("demo".to_string()),
            text: Some("Alpha".to_string()),
            tag: "button".to_string(),
            displayed: true,
            selected: true,
            attrs: vec![
                ("id".to_string(), "root".to_string()),
                ("data-role".to_string(), "demo".to_string()),
            ],
            child_count: 2,
            css_path: "body > button".to_string(),
            xpath: "/html/body/button".to_string(),
            comments: vec!["demo comment".to_string()],
            html: Some("<button>Alpha</button>".to_string()),
            inner_html: Some("Alpha".to_string()),
            value: Some("AlphaValue".to_string()),
            style: "block".to_string(),
            property: Some(json!("root")),
        }];

        let found = items
            .filter_one()
            .attr("data-role", "demo", true)
            .expect("find fake item");
        assert_eq!(
            found.attr("data-role").expect("attr"),
            Some("demo".to_string())
        );
        assert_eq!(found.text().expect("text"), Some("Alpha".to_string()));
        assert_eq!(found.tag().expect("tag"), Some("button".to_string()));
        assert_eq!(
            found.html().expect("html"),
            Some("<button>Alpha</button>".to_string())
        );
        assert_eq!(
            found.inner_html().expect("inner html"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            found.value().expect("value"),
            Some("AlphaValue".to_string())
        );
        assert_eq!(
            found.attrs().expect("attrs"),
            Some(vec![
                ("id".to_string(), "root".to_string()),
                ("data-role".to_string(), "demo".to_string()),
            ])
        );
        assert_eq!(found.child_count().expect("child count"), Some(2));
        assert_eq!(
            found.css_path().expect("css path"),
            Some("body > button".to_string())
        );
        assert_eq!(
            found.xpath().expect("xpath"),
            Some("/html/body/button".to_string())
        );
        assert_eq!(
            found.comments().expect("comments"),
            Some(vec!["demo comment".to_string()])
        );
        assert_eq!(found.is_displayed().expect("displayed"), Some(true));
        assert_eq!(found.is_selected().expect("selected"), Some(true));
        assert_eq!(
            found.style("display").expect("style"),
            Some("block".to_string())
        );
        assert_eq!(
            found.pseudo_before().expect("pseudo before"),
            Some("before".to_string())
        );
        assert_eq!(
            found.pseudo_after().expect("pseudo after"),
            Some("after".to_string())
        );
        assert_eq!(found.property("id").expect("property"), Some(json!("root")));

        let missing = items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect("missing fake item");
        assert!(missing.is_none());
        assert_eq!(missing.attr("data-role").expect("missing attr"), None);
        assert_eq!(missing.text().expect("missing text"), None);
        assert_eq!(missing.tag().expect("missing tag"), None);
        assert_eq!(missing.html().expect("missing html"), None);
        assert_eq!(missing.inner_html().expect("missing inner html"), None);
        assert_eq!(missing.value().expect("missing value"), None);
        assert_eq!(missing.attrs().expect("missing attrs"), None);
        assert_eq!(missing.child_count().expect("missing child count"), None);
        assert_eq!(missing.css_path().expect("missing css path"), None);
        assert_eq!(missing.xpath().expect("missing xpath"), None);
        assert_eq!(missing.comments().expect("missing comments"), None);
        assert_eq!(missing.is_displayed().expect("missing displayed"), None);
        assert_eq!(missing.style("display").expect("missing style"), None);
        assert_eq!(
            missing.pseudo_before().expect("missing pseudo before"),
            None
        );
        assert_eq!(missing.pseudo_after().expect("missing pseudo after"), None);
        assert_eq!(missing.property("id").expect("missing property"), None);
    }

    #[test]
    fn session_elements_one_supports_texts_accessor() {
        let items = snapshot_find_all(
            r#"
            <div>
                <section class="item" id="text-block">
                    alpha
                    <span>beta</span>
                    <em>gamma</em>
                </section>
            </div>
            "#,
            ".item",
        )
        .expect("snapshot elements");

        let found = items
            .filter_one()
            .attr("id", "text-block", true)
            .expect("find text block");
        assert_eq!(
            found.texts(true).expect("direct texts"),
            Some(vec!["alpha".to_string()])
        );
        assert_eq!(
            found.texts(false).expect("all texts"),
            Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ])
        );

        let missing = items
            .filter_one()
            .attr("id", "missing", true)
            .expect("missing text block");
        assert_eq!(missing.texts(false).expect("missing texts"), None);
    }

    #[test]
    fn session_elements_one_owned_supports_static_find_aliases() {
        let root = snapshot_find(
            r#"
            <section id="root">
                <span class="item">alpha</span>
                <span class="item">beta</span>
            </section>
            "#,
            "#root",
        )
        .expect("root element");
        let found = super::ElementsOneOwned::some_with_config(root, None);

        assert_eq!(
            found
                .s_ele((By::CLASS_NAME, "item"))
                .expect("static child")
                .text()
                .expect("static child text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            found
                .s_eles((By::CLASS_NAME, "item"))
                .expect("static children")
                .len(),
            2
        );
        assert!(
            found
                .s_ele("#missing")
                .expect("missing static child")
                .is_none()
        );

        let missing_owner: super::ElementsOneOwned<crate::SessionElement> =
            super::ElementsOneOwned::none_with_config(None);
        assert!(
            missing_owner
                .s_ele("#root")
                .expect("missing owner static child")
                .is_none()
        );
        assert_eq!(
            missing_owner
                .s_eles("#root")
                .expect("missing owner static children")
                .len(),
            0
        );
    }

    #[test]
    fn session_elements_one_owned_supports_multi_relative_navigation() {
        let second = snapshot_find(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                    <p class="summary">One</p>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                    <p class="summary">Two</p>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            "@data-kind=second",
        )
        .expect("second element");
        let found = super::ElementsOneOwned::some_with_config(second, None);

        assert_eq!(found.children().expect("children").len(), 2);
        assert_eq!(
            found
                .children_with(Some(".title"))
                .expect("title children")
                .len(),
            1
        );
        assert_eq!(found.prevs().expect("previous siblings").len(), 1);
        assert_eq!(found.nexts().expect("next siblings").len(), 1);
        assert_eq!(
            found
                .befores_with(Some(".title"))
                .expect("before title elements")
                .len(),
            1
        );
        assert_eq!(
            found
                .afters_with(Some(".title"))
                .expect("after title elements")
                .len(),
            1
        );

        let missing: super::ElementsOneOwned<crate::SessionElement> =
            super::ElementsOneOwned::none_with_config(None);
        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn session_elements_one_owned_supports_node_list_navigation() {
        let root = snapshot_find(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
            "#root",
        )
        .expect("root element");
        let second = root.find("#b").expect("second element");
        let root = super::ElementsOneOwned::some_with_config(root, None);
        let second = super::ElementsOneOwned::some_with_config(second, None);

        let children = root.children_nodes().expect("child nodes");
        assert_eq!(children.len(), 6);
        match &children[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child, got {other:?}"),
        }
        match &children[1] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child, got {other:?}"),
        }
        assert_eq!(
            root.children_nodes_with(Some("xpath:span"))
                .expect("span child nodes")
                .len(),
            2
        );

        let previous_comments = second
            .prev_nodes_with(Some("xpath:comment()"))
            .expect("previous comment nodes");
        assert_eq!(previous_comments.len(), 1);
        match &previous_comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }
        assert_eq!(second.next_nodes().expect("next nodes").len(), 2);
        assert_eq!(
            second
                .before_nodes_with(Some("xpath:span[@id='a']"))
                .expect("before span nodes")
                .len(),
            1
        );
        assert_eq!(
            second
                .after_nodes_with(Some("xpath:strong[@id='c']"))
                .expect("after strong nodes")
                .len(),
            1
        );

        let missing: super::ElementsOneOwned<crate::SessionElement> =
            super::ElementsOneOwned::none_with_config(None);
        assert_eq!(
            missing.children_nodes().expect("missing child nodes").len(),
            0
        );
        assert_eq!(
            missing.prev_nodes().expect("missing previous nodes").len(),
            0
        );
        assert_eq!(missing.next_nodes().expect("missing next nodes").len(), 0);
        assert_eq!(
            missing.before_nodes().expect("missing before nodes").len(),
            0
        );
        assert_eq!(missing.after_nodes().expect("missing after nodes").len(), 0);
    }

    #[test]
    fn session_elements_one_owned_supports_single_node_navigation() {
        let root = snapshot_find(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
            "#root",
        )
        .expect("root element");
        let second = root.find("#b").expect("second element");
        let root = super::ElementsOneOwned::some_with_config(root, None);
        let second = super::ElementsOneOwned::some_with_config(second, None);

        match root
            .child_node()
            .expect("child node")
            .expect("first child node")
        {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child node, got {other:?}"),
        }
        match root
            .child_node_with(Some("xpath:comment()"), 1)
            .expect("comment child node")
            .expect("comment child node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child node, got {other:?}"),
        }
        assert_eq!(
            root.children_nodes_with(Some((By::XPATH, "span")))
                .expect("span child nodes")
                .len(),
            2
        );
        match root
            .child_node_with(Some((By::XPATH, "comment()")), 1)
            .expect("tuple comment child node")
            .expect("tuple comment child node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected tuple comment child node, got {other:?}"),
        }
        match second
            .prev_node_with(Some("xpath:comment()"), 1)
            .expect("previous comment node")
            .expect("previous comment node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }
        match second.next_node().expect("next node").expect("next node") {
            SessionXPathResult::Element(element) => assert_eq!(
                element.attr("id").expect("next element id"),
                Some("c".to_string())
            ),
            other => panic!("expected next element node, got {other:?}"),
        }
        assert!(
            second
                .before_node_with(Some("xpath:span[@id='a']"), 1)
                .expect("before span node")
                .is_some()
        );
        assert!(
            second
                .after_node_with(Some("xpath:strong[@id='c']"), 1)
                .expect("after strong node")
                .is_some()
        );
        assert!(
            second
                .prev_node_with(Some("xpath:strong[@id='missing']"), 1)
                .expect("missing previous node")
                .is_none()
        );

        let missing: super::ElementsOneOwned<crate::SessionElement> =
            super::ElementsOneOwned::none_with_config(None);
        assert!(missing.child_node().expect("missing child node").is_none());
        assert!(
            missing
                .prev_node()
                .expect("missing previous node")
                .is_none()
        );
        assert!(missing.next_node().expect("missing next node").is_none());
        assert!(
            missing
                .before_node()
                .expect("missing before node")
                .is_none()
        );
        assert!(missing.after_node().expect("missing after node").is_none());

        let config = Arc::new(Mutex::new(super::ElementsOneRuntimeConfig {
            raise_when_not_found: true,
            ..super::ElementsOneRuntimeConfig::default()
        }));
        let raising = super::ElementsOneOwned::some_with_config(
            second.as_option().expect("second element").clone(),
            Some(config),
        );
        let error = raising
            .prev_node_with(Some("xpath:strong[@id='missing']"), 1)
            .expect_err("missing previous node should raise");
        assert!(matches!(error, crate::OpenPageError::ElementNotFound(_)));
    }

    #[test]
    fn web_elements_one_owned_supports_session_multi_relative_navigation() {
        let second = snapshot_find(
            r#"
            <main>
                <article class="card" data-kind="first">
                    <h2 class="title">Alpha</h2>
                    <p class="summary">One</p>
                </article>
                <article class="card" data-kind="second">
                    <h2 class="title">Beta</h2>
                    <p class="summary">Two</p>
                </article>
                <article class="card" data-kind="third">
                    <h2 class="title">Gamma</h2>
                </article>
            </main>
            "#,
            "@data-kind=second",
        )
        .expect("second element");
        let found = super::ElementsOneOwned::some_with_config(WebElement::Session(second), None);

        assert_eq!(found.children().expect("children").len(), 2);
        assert_eq!(
            found
                .children_with(Some(".title"))
                .expect("title children")
                .len(),
            1
        );
        assert_eq!(found.prevs().expect("previous siblings").len(), 1);
        assert_eq!(found.nexts().expect("next siblings").len(), 1);
        assert_eq!(
            found
                .befores_with(Some(".title"))
                .expect("before title elements")
                .len(),
            1
        );
        assert_eq!(
            found
                .afters_with(Some(".title"))
                .expect("after title elements")
                .len(),
            1
        );

        let missing: super::ElementsOneOwned<WebElement> =
            super::ElementsOneOwned::none_with_config(None);
        assert_eq!(missing.children().expect("missing children").len(), 0);
        assert_eq!(missing.prevs().expect("missing previous siblings").len(), 0);
        assert_eq!(missing.nexts().expect("missing next siblings").len(), 0);
        assert_eq!(missing.befores().expect("missing before elements").len(), 0);
        assert_eq!(missing.afters().expect("missing after elements").len(), 0);
    }

    #[test]
    fn elements_one_runtime_config_can_return_values_and_raise() {
        let config = Arc::new(Mutex::new(super::ElementsOneRuntimeConfig {
            return_value_enabled: true,
            return_value: Some("missing".to_string()),
            raise_when_not_found: false,
        }));
        let missing: super::ElementsOne<'_, FakeItem> =
            super::ElementsOne::none_with_config(Some(&config));
        assert_eq!(
            missing.text().expect("missing text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.attr("id").expect("missing attr"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.html().expect("missing html"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.comments().expect("missing comments"),
            Some(vec!["missing".to_string()])
        );
        assert_eq!(
            missing.pseudo_before().expect("missing pseudo before"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.pseudo_after().expect("missing pseudo after"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.property("id").expect("missing property"),
            Some(json!("missing"))
        );
        assert_eq!(missing.child_count().expect("missing child count"), None);

        let items = vec![FakeItem {
            attr: Some("demo".to_string()),
            text: Some("Alpha".to_string()),
            tag: "button".to_string(),
            displayed: true,
            selected: true,
            attrs: vec![],
            child_count: 0,
            css_path: "body > button".to_string(),
            xpath: "/html/body/button".to_string(),
            comments: vec![],
            html: Some("<button>Alpha</button>".to_string()),
            inner_html: Some("Alpha".to_string()),
            value: Some("AlphaValue".to_string()),
            style: "block".to_string(),
            property: Some(json!("root")),
        }];
        let filter = super::ElementsFilterOne {
            elements: items.iter().collect(),
            index: 1,
            config: Some(&config),
        };
        assert_eq!(
            filter
                .attr("data-role", "missing", true)
                .expect("missing filter attr")
                .text()
                .expect("fallback text"),
            Some("missing".to_string())
        );

        config.lock().expect("config lock").raise_when_not_found = true;
        let error = filter
            .attr("data-role", "missing", true)
            .expect_err("missing filter should raise");
        assert!(
            matches!(error, crate::OpenPageError::ElementNotFound(_)),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn elements_one_config_lock_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let config = Arc::new(Mutex::new(super::ElementsOneRuntimeConfig::default()));
        let missing: super::ElementsOne<'_, FakeItem> =
            super::ElementsOne::none_with_config(Some(&config));

        let config_for_thread = Arc::clone(&config);
        let join = thread::spawn(move || {
            let _guard = config_for_thread
                .lock()
                .expect("elements one config lock should be available");
            panic!("poison elements one config lock");
        })
        .join();
        assert!(join.is_err(), "poison helper thread should panic");

        let english = missing
            .text()
            .expect_err("poisoned config should fail in English")
            .to_string();
        assert!(english.contains("none element runtime config lock poisoned"));

        Settings::set_language("cn");

        let chinese = missing
            .text()
            .expect_err("poisoned config should localize after language switch")
            .to_string();
        assert!(chinese.contains("未找到元素运行时配置锁已损坏"));
    }
}
