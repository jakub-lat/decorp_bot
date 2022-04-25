use std::sync::Arc;
use std::time::Duration;
use serenity::async_trait;
use serenity::client::bridge::gateway::GatewayIntents;
use serenity::prelude::*;
use serenity::model::channel::Message;
use serenity::framework::standard::macros::{command, group, hook, check};
use serenity::framework::standard::{StandardFramework, CommandResult, Args, CommandOptions, Reason};
use crate::{Config, Scrapper};
use anyhow::{anyhow, Result};
use serenity::model::id::RoleId;
use serenity::model::Permissions;
use crate::scrapper::LoginResult;

pub struct Bot {
    pub client: Client,
    config: Config,
}
struct Handler;

#[async_trait]
impl EventHandler for Handler {}


#[group]
#[commands(login, stats)]
struct General;

#[check]
#[name = "InProject"]
async fn in_project_check(
    ctx: &Context,
    msg: &Message,
    _: &mut Args,
    _: &CommandOptions,
) -> Result<(), Reason> {
    let lock = ctx.data.read().await;
    let cfg = lock.get::<Config>().unwrap().clone();

    if let Some(member) = &msg.member {
        if member.roles.contains(&RoleId(cfg.role_id)) {
            return Ok(());
        }
    }

    return Err(Reason::User("Forbidden".to_string()));
}


#[check]
#[name = "Owner"]
async fn owner_check(
    ctx: &Context,
    msg: &Message,
    _: &mut Args,
    _: &CommandOptions,
) -> Result<(), Reason> {
    let lock = ctx.data.read().await;
    let cfg = lock.get::<Config>().unwrap().clone();

    if msg.author.id != cfg.owner_id {
        return Err(Reason::User("Not owner".to_string()));
    }

    Ok(())
}

#[hook]
async fn after(ctx: &Context, msg: &Message, command_name: &str, command_result: CommandResult) {
    match command_result {
        Ok(()) => {},
        Err(why) => {
            msg.channel_id.send_message(ctx, |m| {
                m.content(format!("Command `{}` failed: `{:?}`", command_name, why))
            }).await.unwrap();
        },
    }
}

#[command]
#[checks(Owner)]
async fn login(ctx: &Context, msg: &Message) -> CommandResult {
    let lock = ctx.data.read().await;
    let scrapper = lock.get::<Scrapper>().unwrap().clone();
    let mut scrapper = scrapper.write().await;

    msg.channel_id.say(&ctx.http, "Logging in...").await?;

    let res = scrapper.login()?;
    if let LoginResult::AuthCodeNeeded = res {
        msg.channel_id.say(&ctx.http, "Enter Steam Guard auth code:").await?;
        if let Some(answer) = &msg.author.await_reply(&ctx).timeout(Duration::from_secs(30)).await {
            scrapper.provide_auth_code(answer.content.clone())?;
        }
    }

    msg.channel_id.say(&ctx.http, "Login successful").await?;

    Ok(())
}

#[command]
#[checks(InProject)]
async fn stats(ctx: &Context, msg: &Message) -> CommandResult {
    let lock = ctx.data.read().await;
    let scrapper = lock.get::<Scrapper>().unwrap().clone();
    let scrapper = scrapper.read().await;

    let mut msg = msg.channel_id.say(&ctx.http, "Loading...").await?;

    let stats = scrapper.get_stats()?;

    msg.edit(ctx, |m| m.content(format!("```{:#?}```", stats))).await?;

    Ok(())
}

impl TypeMapKey for Scrapper {
    type Value = Arc<RwLock<Scrapper>>;
}

impl TypeMapKey for Config {
    type Value = Config;
}

impl Bot {
    pub async fn new(config: Config, scrapper: Arc<RwLock<Scrapper>>) -> Self {
        let framework = StandardFramework::new()
            .configure(|c| c.prefix("!"))
            .after(after)
            .group(&GENERAL_GROUP);

        // Login with a bot token from the environment
        let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MESSAGES;
        let client = Client::builder(config.bot_token.clone())
            .event_handler(Handler)
            .framework(framework)
            .intents(intents)
            .await
            .expect("Error creating client");

        {
            let mut lock = client.data.write().await;
            lock.insert::<Scrapper>(scrapper);
            lock.insert::<Config>(config.clone());
        }

        Self {
            client,
            config,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("bot started");

        if let Err(why) = self.client.start().await {
            println!("An error occurred while running the client: {:?}", why);
        }

        Ok(())
    }

}

