mod scrapper;
mod bot;

use std::fs;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tokio::sync::RwLock;
use crate::scrapper::Scrapper;
use crate::bot::{Bot};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    steam_login: String,
    steam_password: String,
    webhook_url: String,
    cookies_path: String,
    bot_token: String,
    owner_id: u64,
    role_id: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let scrapper = Scrapper::new(cfg.clone())?;

    let mut bot = Bot::new(cfg, Arc::new(RwLock::new(scrapper))).await;

    bot.run().await?;

    Ok(())
}
