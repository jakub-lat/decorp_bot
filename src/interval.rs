use std::fs;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serenity::futures::AsyncWriteExt;
use serenity::http::{CacheHttp, Http};
use serenity::model::id::ChannelId;
use serenity::prelude::TypeMap;
use tokio::{task, time};
use tokio::sync::RwLock;
use similar::{ChangeTag, TextDiff};

use crate::bot::{Bot, IntervalStarted};
use crate::Config;
use crate::scrapper::{Scrapper, Stats};

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
        let mut last_stats_str = String::new();

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

                    let stats_str = format!("{:#?}", stats);

                    let diff = TextDiff::from_lines(&last_stats_str, &stats_str);
                    let diff_str = diff.iter_all_changes().map(|change| {
                        let sign = match change.tag() {
                            ChangeTag::Delete => "-",
                            ChangeTag::Insert => "+",
                            ChangeTag::Equal => " ",
                        };
                        format!("{}{}", sign, change)
                    }).collect::<Vec<_>>().join("");

                    last_stats = stats.clone();
                    last_stats_str = stats_str;

                    (format!("Stats changed: ```diff\n{}```", diff_str), false)
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