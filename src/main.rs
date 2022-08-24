mod scrapper;
mod bot;

use std::fs;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use serenity::model::id::ChannelId;
use tokio::sync::RwLock;
use crate::scrapper::{Scrapper, Stats};
use crate::bot::{Bot};
use tokio::{task, time};

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
    updates_channel_id: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let scrapper = Arc::new(RwLock::new(Scrapper::new(cfg.clone())?));

    let mut bot = Bot::new(cfg.clone(), scrapper.clone()).await;

    let ch_id = ChannelId(cfg.updates_channel_id);
    if ch_id != 0 {
        let cache_and_http = bot.client.cache_and_http.http.clone();
        task::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(30));
            let mut last_stats = Stats::default();

            'forever: loop {
                interval.tick().await;
                let res = async {
                    let mut scrapper = scrapper.write().await;
                    scrapper.get_stats().await
                }.await;

                let msg = match res {
                    Ok(stats) => {
                        if stats == last_stats {
                            println!("stats haven't changed");
                            continue 'forever;
                        }

                        last_stats = stats.clone();

                        format!("Stats changed: ```{:#?}```", stats)
                    },
                    Err(why) => {
                        format!("failed to get stats: {:?}", why)
                    }
                };

                if let Err(why) = ch_id.send_message(&cache_and_http, |m| m.content(msg)).await {
                    println!("failed to send message: {:?}", why);
                }
            }
        });
    }

    bot.run().await?;

    Ok(())
}
