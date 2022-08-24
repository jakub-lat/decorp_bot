mod scrapper;
mod bot;
mod interval;

use std::fs;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use serenity::http::CacheHttp;
use serenity::model::id::ChannelId;
use tokio::sync::RwLock;
use crate::scrapper::{LoginResult, Scrapper, Stats};
use crate::bot::{Bot};
use tokio::{task, time};
use crate::interval::start_interval;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    steam_login: String,
    steam_password: String,
    webhook_url: String,
    cookies_path: String,
    bot_token: String,
    owner_id: u64,
    role_id: u64,
    prefix: String,
    #[serde(default)]
    updates_channel_id: u64,
    #[serde(default)]
    updates_interval_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let scrapper = Arc::new(RwLock::new(Scrapper::new(cfg.clone())?));

    let mut bot = Bot::new(cfg.clone(), scrapper.clone()).await;

    let res = {
        let mut scrapper = scrapper.write().await;
        scrapper.login().await
    };

    if res.map_or(false, |x| x == LoginResult::Success) {
        start_interval(cfg.clone(), scrapper.clone(), bot.client.cache_and_http.http.clone(), bot.client.data.clone());
    } else {
        println!("cannot start interval: not logged in");
    }

    bot.run().await?;

    Ok(())
}
