use super::*;

impl WebPage {
    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<WebElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .find(locator)
                .map(|element| self.with_driver_element(element)),
            WebMode::Session => self.session.find(locator).map(WebElement::Session),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.find_all(locator).map(|elements| {
                elements
                    .into_iter()
                    .map(|element| self.with_driver_element(element))
                    .collect()
            }),
            WebMode::Session => self
                .session
                .find_all(locator)
                .map(|elements| elements.into_iter().map(WebElement::Session).collect()),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .ele(locator.raw())
                .map(|element| element.map(|element| self.with_driver_element(element))),
            WebMode::Session => self
                .session
                .ele(locator.raw())
                .map(|element| element.map(WebElement::Session)),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<WebElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<WebElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self
                .driver
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item
                                .elements
                                .into_iter()
                                .map(|element| self.with_driver_element(element))
                                .collect(),
                        })
                        .collect()
                }),
            WebMode::Session => self
                .session
                .find_locators(locators, any_one, first_match_only)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| LocatorMatch {
                            locator: item.locator,
                            elements: item.elements.into_iter().map(WebElement::Session).collect(),
                        })
                        .collect()
                }),
        }
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find(locator),
            WebMode::Session => self.session.find(locator),
        }
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_all(locator),
            WebMode::Session => self.session.find_all(locator),
        }
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_by(by, value),
            WebMode::Session => self.session.find_by(by, value),
        }
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_find_all_by(by, value),
            WebMode::Session => self.session.find_all_by(by, value),
        }
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_query_xpath(expression),
            WebMode::Session => self.session.query_xpath(expression),
        }
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        match self.mode()? {
            WebMode::Driver => self.driver.snapshot_root(),
            WebMode::Session => self.session.root(),
        }
    }
}
