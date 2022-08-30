use std::{fmt, fs, io};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use headless_chrome::{Browser, LaunchOptions, Tab};
use headless_chrome::protocol::cdp::Network::{Cookie, CookieParam, DeleteCookies};
use headless_chrome::protocol::cdp::Page::{CaptureScreenshotFormatOption, DeleteCookie};
use scraper::{Html, Selector};
use tokio::time;
use crate::utils::*;

use crate::Config;

pub struct Scrapper {
    login_url: String,
    stats_url: String,
    steam_username: String,
    steam_password: String,
    cookies_path: String,
    browser: Option<Arc<Browser>>,
    tab: Option<Arc<Tab>>,
    is_logged_in: bool,
    client: Option<Arc<reqwest::Client>>,
}

unsafe impl Send for Scrapper {}
unsafe impl Sync for Scrapper {}

#[derive(Default, PartialEq, PartialOrd, Clone)]
struct Percent(f32);

impl Debug for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
pub struct Stats {
    total_units: i32,
    steam_units: i32,
    units_returned: i32,
    return_percent: Percent,
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
            login_url: "https://partner.steampowered.com/login/".to_string(),
            stats_url: cfg.stats_url,
            steam_username: cfg.steam_login,
            steam_password: cfg.steam_password,
            cookies_path: cfg.cookies_path,
            browser: None,
            tab: None,
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

        tab.navigate_to(&self.login_url)?;

        let username_input = tab.wait_for_element("input#username");
        if username_input.is_err() {
            println!("already logged in");
            self.is_logged_in = true;
            return Ok(LoginResult::Success);
        }

        let username_input = username_input.unwrap();
        username_input.click()?;
        tab.type_str(&self.steam_username)?;

        tab.wait_for_element("input#password")?.click()?;
        tab.type_str(&self.steam_password)?.press_key("Enter")?;

        time::sleep(Duration::from_secs(10)).await;

        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, Some(100), None, false)?;
        let mut file = File::create("screenshot.png")?;
        file.write_all(&png)?;


        let auth_el = tab.wait_for_element("input#authcode");

        // let val = tab.wait_for_element("body")?
        //     .call_js_fn("function() { return this.innerHTML; }", vec![], false)?
        //     .value.unwrap();
        //
        // let mut file = File::create("login.html")?;
        // file.write_all(val.as_str().unwrap_or("").as_bytes())?;



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
        let text = self.get_stats_text().await?;

        let document = &Html::parse_document(&text);
        let title = document.get_element_text("head title")?;
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

    async fn get_stats_text(&self) -> Result<String> {
        let text = self.client.clone()
            .ok_or_else(|| anyhow!("client not initialized"))?
            .get(&self.stats_url)
            .send()
            .await?
            .text()
            .await?;
        Ok(text)
    }

    pub async fn get_stats(&mut self) -> Result<Stats> {
        if !self.is_logged_in {
            if let LoginResult::AuthCodeNeeded = self.login().await? {
                return Err(anyhow!("not logged in"));
            }
        }

        let text = self.get_stats_text().await?;
        let document = &Html::parse_document(&text);

        let title = document.get_element_text("head title")?;
        if title != "Game: Decorporation" {
            return Err(anyhow!("not logged in!"));
        }

        let mut res = Stats{
            gross_revenue: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(1) > td:nth-child(2)")?,
            net_revenue: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(2) > td:nth-child(2)")?,
            total_units: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(6) > td:nth-child(2)")?.atoi()?,
            steam_units: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(4) > td:nth-child(2)")?.atoi()?,
            units_returned: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(7) > td:nth-child(2)")?.atoi()?,
            return_percent: Percent(0.0),
            current_players: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(9) > td:nth-child(2)")?.atoi()?,
            daily_active_users: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(10) > td:nth-child(2)")?.atoi()?,
            lifetime_unique_users: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(11) > td:nth-child(2)")?.atoi()?,
            wishlist_count: document.get_element_text(r"#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(14) > td:nth-child(2)")?.trim().to_string().atoi()?,
        };
        res.return_percent = Percent((res.units_returned as f32) / (res.steam_units as f32));

        Ok(res)


        // Ok(Stats {
        //     net_revenue: self.tab.wait_for_element()?.get_inner_text()?,
        //     current_players: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     daily_active_users: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     lifetime_unique_users: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        //     wishlist_count: self.tab.wait_for_element()?.get_inner_text()?.atoi()?,
        // })
    }



    fn load_cookies_from_file(&self) -> Result<Vec<CookieParam>> {
        let res = fs::read_to_string(&self.cookies_path);

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
        let mut file = File::create(&self.cookies_path)?;
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
        // if Path::new(&self.cookies_path).exists() {
        //     fs::remove_file(&self.cookies_path)?;
        // }

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

