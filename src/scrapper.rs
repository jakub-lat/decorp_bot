use std::sync::Arc;
use headless_chrome::{Browser, LaunchOptions, Tab};
use anyhow::{anyhow, Result};
use std::fs;
use std::fs::File;
use std::io::Write;
use headless_chrome::protocol::cdp::Network::{Cookie, CookieParam};

use crate::Config;

pub struct Scrapper {
    browser: Browser,
    tab: Arc<Tab>,
    cfg: Config,
    is_logged_in: bool,
}

unsafe impl Send for Scrapper {}
unsafe impl Sync for Scrapper {}

#[derive(Debug)]
pub struct Stats {
    net_revenue: String,
    current_players: i32,
    daily_active_users: i32,
    lifetime_unique_users: i32,
    wishlist_count: i32,
}

pub enum LoginResult {
    Success,
    AuthCodeNeeded,
}

impl Scrapper {
    pub fn new(cfg: Config) -> Result<Self> {
        // let x = Stats{};
        let browser = Browser::new(
            LaunchOptions::default_builder()
                .headless(true)
                .build()
                .expect("Could not find chrome executable"))?;

        let tab = browser.new_tab()?;
        Ok(Scrapper {
            browser,
            tab,
            cfg,
            is_logged_in: false,
        })
    }
    pub fn login(&mut self) -> Result<LoginResult> {
        if let Err(why) = self.load_cookies() {
            println!("load cookies failed: {}", why);
        }

        self.tab.navigate_to("https://partner.steampowered.com/login/")?;

        let username_input = self.tab.wait_for_element("input#username");
        if username_input.is_err() {
            println!("already logged in");
            self.is_logged_in = true;
            return Ok(LoginResult::Success);
        }

        let username_input = username_input.unwrap();
        username_input.click()?;
        self.tab.type_str(&self.cfg.steam_login)?;

        self.tab.wait_for_element("input#password")?.click()?;
        self.tab.type_str(&self.cfg.steam_password)?.press_key("Enter")?;

        let auth_el = self.tab.wait_for_element("input#authcode");

        if let Ok(auth_el) = auth_el {
            return Ok(LoginResult::AuthCodeNeeded);
        }


        self.tab.wait_until_navigated()?;
        self.save_cookies()?;

        self.is_logged_in = true;

        Ok(LoginResult::Success)
    }

    pub fn provide_auth_code(&mut self, auth_code: String) -> Result<()> {
        let auth_el = self.tab.wait_for_element("input#authcode")?;

        auth_el.click()?;

        self.tab.type_str(auth_code.trim())?;

        self.tab.wait_for_element("input#friendlyname")?.click()?;
        self.tab.type_str("Decorporation bot")?;


        self.tab.wait_for_element("#auth_buttonset_entercode > div.auth_button.leftbtn")?.click()?;
        println!("clicked submit btn");
        self.tab.wait_for_element("#success_continue_btn")?.click()?;

        self.tab.wait_until_navigated()?;
        self.save_cookies()?;

        self.is_logged_in = true;

        Ok(())
    }

    pub fn get_stats(&self) -> Result<Stats> {
        if !self.is_logged_in {
            return Err(anyhow!("not logged in"));
        }

        self.tab.navigate_to("https://partner.steampowered.com/app/details/1968950/?dateStart=2000-01-01&dateEnd=2022-04-24&priorDateStart=1977-09-08&priorDateEnd=1999-12-31")?;

        Ok(Stats {
            net_revenue: self.tab.wait_for_element("#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(2) > td:nth-child(2)")?.get_inner_text()?,
            current_players: self.tab.wait_for_element("#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(7) > td:nth-child(2)")?.get_inner_text()?.atoi()?,
            daily_active_users: self.tab.wait_for_element("#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(8) > td:nth-child(2)")?.get_inner_text()?.atoi()?,
            lifetime_unique_users: self.tab.wait_for_element("#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(9) > td:nth-child(2)")?.get_inner_text()?.atoi()?,
            wishlist_count: self.tab.wait_for_element("#gameDataLeft > div.lifetimeSummaryCtn > table > tbody > tr:nth-child(11) > td:nth-child(2)")?.get_inner_text()?.atoi()?,
        })
    }

    fn load_cookies(&self) -> Result<()> {
        let str = fs::read_to_string(&self.cfg.cookies_path)?;
        let cookies: Vec<Cookie> = serde_json::from_str(&str)?;
        self.tab.set_cookies(cookies.iter().map(|c| CookieParam{
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
        }).collect())?;
        Ok(())
    }

    fn save_cookies(&self) -> Result<()> {
        let cookies = self.tab.get_cookies()?;
        let mut file = File::create(&self.cfg.cookies_path)?;
        file.write_all(&serde_json::to_vec(&cookies)?)?;
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