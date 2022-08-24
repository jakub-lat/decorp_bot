use std::fs;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use serenity::futures::AsyncWriteExt;
use serenity::http::{CacheHttp, Http};
use serenity::model::id::ChannelId;
use serenity::prelude::TypeMap;
use tokio::sync::RwLock;
use crate::scrapper::{Scrapper, Stats};
use crate::bot::{Bot, IntervalStarted};
use tokio::{task, time};
use crate::Config;


pub fn start_interval(cfg: Config, scrapper: Arc<RwLock<Scrapper>>, http: Arc<Http>, data: Arc<RwLock<TypeMap>>) {
    let ch_id = ChannelId(cfg.updates_channel_id);

    if ch_id == 0 {
        return
    }

    let mut interval = time::interval(Duration::from_secs(cfg.updates_interval_secs));

    task::spawn(async move {
        {
            let mut lock = data.write().await;
            lock.insert::<IntervalStarted>(Arc::new(true));
        }

        let mut last_stats = Stats::default();

        'forever: loop {
            interval.tick().await;
            let res = async {
                let mut scrapper = scrapper.write().await;
                scrapper.get_stats().await
            }.await;

            let (msg, err) = match res {
                Ok(stats) => {
                    if stats == last_stats {
                        println!("stats haven't changed");
                        continue 'forever;
                    }

                    last_stats = stats.clone();

                    (format!("Stats changed: ```{:#?}```", stats), false)
                },
                Err(why) => {
                    (format!("failed to get stats: {:?}", why), true)
                }
            };

            if let Err(why) = ch_id.send_message(&http, |m| m.content(msg)).await {
                println!("failed to send message: {:?}", why);
            }

            if err {
                break 'forever;
            }
        }

        {
            let mut lock = data.write().await;
            lock.insert::<IntervalStarted>(Arc::new(false));
        }
    });
}