mod scrapper;
mod bot;

use std::error::Error;
use std::fs;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use text_io::read;
use anyhow::Result;
use serenity::Client;
use serenity::client::bridge::gateway::GatewayIntents;
use serenity::model::id::{ChannelId, GuildId};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let scrapper = Scrapper::new(cfg.clone())?;

    let mut bot = Bot::new(cfg, Arc::new(RwLock::new(scrapper))).await;

    bot.run().await?;

    Ok(())
}
