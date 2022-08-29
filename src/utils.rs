use anyhow::{Result, anyhow};
use scraper::{Html, Selector};

pub trait Atoi {
    fn atoi<T: atoi::FromRadix10SignedChecked>(self) -> Result<T>;
}

impl Atoi for String {
    fn atoi<T: atoi::FromRadix10SignedChecked>(self) -> Result<T> {
        atoi::atoi::<T>(self.as_bytes()).ok_or_else(|| anyhow!("Could not convert {} to i32", self))
    }
}

pub trait GetElementText {
    fn get_element_text(&self, selector: &str) -> Result<String>;
}

impl GetElementText for Html {
    fn get_element_text(&self, selector: &str) -> Result<String> {
        let net_revenue_selector = Selector::parse(selector).unwrap();
        let el = self.select(&net_revenue_selector).next().ok_or_else(|| anyhow!("element not found"))?;
        let first = el.text().next().ok_or_else(|| anyhow!("text not found"))?;
        Ok(first.to_string())
    }
}

