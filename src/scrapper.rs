use std::{fs, io};
use std::borrow::Borrow;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use headless_chrome::{Browser, LaunchOptions, Tab};
use headless_chrome::protocol::cdp::Network::{Cookie, CookieParam, DeleteCookies};
use headless_chrome::protocol::cdp::Page::DeleteCookie;
use scraper::{Html, Selector};

use crate::Config;

pub struct Scrapper {
    browser: Option<Arc<Browser>>,
    tab: Option<Arc<Tab>>,
    cfg: Config,
    is_logged_in: bool,
    client: Option<Arc<reqwest::Client>>,
}

unsafe impl Send for Scrapper {}
unsafe impl Sync for Scrapper {}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
pub struct Stats {
    total_units: i32,
    steam_units: i32,
    units_returned: i32,
    gross_revenue: String,
    net_revenue: String,
    current_players: i32,
    daily_active_users: i32,
    lifetime_unique_users: i32,
    wishlist_count: i32,
}

#[derive(PartialEq)]
pub enum LoginResult {
    Success,
    AuthCodeNeeded,
}

impl Scrapper {
    pub fn new(cfg: Config) -> Result<Self> {
        // let (browser, tab) = Self::open()?;
        Ok(Scrapper {
            browser: None,
            tab: None,
            cfg,
            is_logged_in: false,
            client: None,
        })
    }

    fn is_open(&self) -> bool {
        match self.browser.clone() {
            Some(b) => b.is_open(),
            None => false
        }
    }

    fn open(&mut self) -> Result<()> {
        let browser = Browser::new(
            LaunchOptions::default_builder()
                .headless(true)
                .build()
                .expect("Could not find chrome executable"))?;

        let tab = browser.new_tab()?;

        self.browser = Some(Arc::new(browser));
        self.tab = Some(tab);

        Ok(())
    }

    pub async fn login(&mut self) -> Result<LoginResult> {
        self.client = Some(Arc::new(self.get_client()?));

        if self.check_if_logged_in().await.is_ok() {
            self.is_logged_in = true;
            return Ok(LoginResult::Success);
        }

        if !self.is_open() {
            self.open()?;
        }

        if let Err(why) = self.load_cookies() {
            println!("load cookies failed: {}", why);
        }

        let tab = self.tab.clone().unwrap();

        tab.navigate_to("https://partner.steampowered.com/login/")?;

        let username_input = tab.wait_for_element("input#username");
        if username_input.is_err() {
            println!("already logged in");
            self.is_logged_in = true;
            return Ok(LoginResult::Success);
        }

        let username_input = username_input.unwrap();
        username_input.click()?;
        tab.type_str(&self.cfg.steam_login)?;

        tab.wait_for_element("input#password")?.click()?;
        tab.type_str(&self.cfg.steam_password)?.press_key("Enter")?;

        let auth_el = tab.wait_for_element("input#authcode");

        if auth_el.is_ok() {
            return Ok(LoginResult::AuthCodeNeeded);
        }

        tab.wait_until_navigated()?;
        self.save_cookies()?;

        self.is_logged_in = true;
        self.client = Some(Arc::new(self.get_client()?));
        self.close()?;

        Ok(LoginResult::Success)
    }

    async fn check_if_logged_in(&self) -> Result<()> {
        let text = self.client.clone()
            .ok_or_else(|| anyhow!("client not initialized"))?
            .get("https://partner.steampowered.com/app/details/1968950/?dateStart=2000-01-01&dateEnd=2022-04-24&priorDateStart=1977-09-08&priorDateEnd=1999-12-31")
            .send()
            .await?
            .text()
            .await?;

        let document = &Html::parse_document(&text);
        let title = self.get_element_text(document, "head title")?;
        if title != "Game: Decorporation" {
            return Err(anyhow!("Login failed"));
        }

        Ok(())
    }

    pub fn provide_auth_code(&mut self, auth_code: String) -> Result<()> {
        let tab = self.tab.clone().unwrap();

        let auth_el = tab.wait_for_element("input#authcode")?;

        auth_el.click()?;

        tab.type_str(auth_code.trim())?;

        tab.wait_for_element("input#friendlyname")?.click()?;
        tab.type_str("Decorporation bot")?;


        tab.wait_for_element("#auth_buttonset_entercode > div.auth_button.leftbtn")?.click()?;
        println!("clicked submit btn");
        tab.wait_for_element("#success_continue_btn")?.click()?;

        tab.wait_until_navigated()?;
        self.save_cookies()?;

        self.is_logged_in = true;
        self.client = Some(Arc::new(self.get_client()?));

        self.close()?;

        Ok(())
    }

    fn get_client(&self) -> Result<reqwest::Client> {
        let jar = self.get_reqwest_cookies()?;
        let arc = Arc::new(jar);
        let client = reqwest::Client::builder().cookie_provider(arc).build()?;
        Ok(client)
    }

    pub async fn get_stats(&mut self) -> Result<Stats> {
        if !self.is_logged_in {
            if let LoginResult::AuthCodeNeeded = self.login().await? {
                return Err(anyhow!("not logged in"));
            }
        }

        let text = self.client.clone()
            .ok_or_else(|| anyhow!("client not initialized"))?
            .get("https://partner.steampowered.com/app/details/1968950/?dateStart=2000-01-01&dateEnd=2022-04-24&priorDateStart=1977-09-08&priorDateEnd=1999-12-31")
            .send()
            .await?
            .text()
            .await?;

        // let mut file = File::create("res.html")?;
        // file.write_all(text.as_bytes())?;

        let document = &Html::parse_document(&text);

        let title = self.get_element_text(document, "head title")?;
        if title != "Game: Decorporation" {
            return Err(anyhow!("not logged in!"));
        }

        Ok(Stats{
            gross_revenue: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(1) > td:nth-child(2)")?,
            net_revenue: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(2) > td:nth-child(2)")?,
            total_units: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(6) > td:nth-child(2)")?.atoi()?,
            steam_units: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(4) > td:nth-child(2)")?.atoi()?,
            units_returned: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(7) > td:nth-child(2)")?.atoi()?,
            current_players: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(9) > td:nth-child(2)")?.atoi()?,
            daily_active_users: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(10) > td:nth-child(2)")?.atoi()?,
            lifetime_unique_users: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(11) > td:nth-child(2)")?.atoi()?,
            wishlist_count: self.get_element_text(document, r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(14) > td:nth-child(2)")?.trim().to_string().atoi()?,
        })


        // Ok(Stats {
        //     net_revenue: self.tab.wait_for_element()?.get_inner_text()?,
        //     current_players: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     daily_active_users: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     lifetime_unique_users: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     wishlist_count: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        // })
    }

    fn get_element_text(&self, document: &Html, selector: &str) -> Result<String> {
        let net_revenue_selector = Selector::parse(selector).unwrap();
        let el = document.select(&net_revenue_selector).next().ok_or_else(|| anyhow!("element not found"))?;
        let first = el.text().next().ok_or_else(|| anyhow!("text not found"))?;
        Ok(first.to_string())
    }

    fn load_cookies_from_file(&self) -> Result<Vec<CookieParam>> {
        let res = fs::read_to_string(&self.cfg.cookies_path);

        if let Err(why) = res {
            return if why.kind() == ErrorKind::NotFound {
                Ok(vec![])
            } else {
                Err(anyhow!(why))
            }
        }

        let str = res.unwrap();

        let cookies: Vec<Cookie> = serde_json::from_str(&str)?;
        let res = cookies.iter().map(|c| CookieParam{
            name: c.name.clone(),
            value: c.value.clone(),
            url: None,
            domain: c.domain.clone().into(),
            path: c.path.clone().into(),
            expires: c.expires.into(),
            priority: None,
            same_party: None,
            source_scheme: None,
            source_port: None,
            http_only: c.http_only.into(),
            secure: c.secure.into(),
            same_site: c.same_site.clone(),
            partition_key: None
        }).collect::<Vec<_>>();
        Ok(res)
    }

    fn load_cookies(&self) -> Result<()> {
        if self.tab.is_none() {
            return Err(anyhow!("not logged in!"));
        }
        self.tab.clone().unwrap().set_cookies(self.load_cookies_from_file()?)?;
        Ok(())
    }

    fn save_cookies(&self) -> Result<()> {
        let cookies = self.tab.clone().unwrap().get_cookies()?;
        let mut file = File::create(&self.cfg.cookies_path)?;
        file.write_all(&serde_json::to_vec(&cookies)?)?;
        Ok(())
    }

    fn get_reqwest_cookies(&self) -> Result<reqwest::cookie::Jar> {
        let jar = reqwest::cookie::Jar::default();
        let url = reqwest::Url::parse("https://partner.steampowered.com")?;

        let cookies = self.load_cookies_from_file()?;
        for c in cookies {
            jar.add_cookie_str(
                &cookie::Cookie::build(c.name, c.value).domain("partner.steampowered.com").finish().to_string(),
                &url);
        }

        Ok(jar)
    }

    pub fn close(&mut self) -> Result<()> {
        if let Some(tab) = self.tab.clone() {
            tab.close(true)?;
        }
        self.browser = None;
        self.tab = None;
        Ok(())
    }

    pub fn logout(&mut self) -> Result<()> {
        if Path::new(&self.cfg.cookies_path).exists() {
            fs::remove_file(&self.cfg.cookies_path)?;
        }

        if let Some(tab) = self.tab.clone() {
            if let Ok(cookies) = tab.get_cookies() {
                tab.delete_cookies(cookies.iter().map(|c| DeleteCookies {
                    name: c.name.clone(),
                    domain: Some(c.domain.clone()),
                    path: None,
                    url: None
                }).collect())?;
            }
        }

        self.close()?;

        Ok(())
    }
}

trait Atoi {
    fn atoi<T: atoi::FromRadix10SignedChecked>(self) -> Result<T>;
}

impl Atoi for String {
    fn atoi<T: atoi::FromRadix10SignedChecked>(self) -> Result<T> {
        atoi::atoi::<T>(self.as_bytes()).ok_or_else(|| anyhow!("Could not convert {} to i32", self))
    }
}