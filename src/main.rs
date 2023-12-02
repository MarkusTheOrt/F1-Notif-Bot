pub mod config;
pub mod error;
pub mod model;
pub mod util;

use error::Error;
use sqlx::{mysql::MySqlConnectOptions, MySqlPool};
use std::{
    fs::File,
    io::{self, Read, Write},
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use config::Config;
use serenity::{
    async_trait,
    client::ClientBuilder,
    model::prelude::*,
    prelude::{Context, EventHandler},
};

struct Bot<'a> {
    is_mainthread_running: AtomicBool,
    pub config: Arc<Config<'a>>,
    pub database: sqlx::MySqlPool,
}

#[cfg(debug_assertions)]
async fn set_presence(ctx: &Context) {
    use serenity::gateway::ActivityData;

    ctx.set_activity(Some(ActivityData::watching("out for new sessions.")));
}

#[cfg(not(debug_assertions))]
async fn set_presence(_ctx: &Context) {}

#[async_trait]
impl<'a> EventHandler for Bot<'a> {
    async fn cache_ready(
        &self,
        _ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }
        set_presence(&_ctx).await;

        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        tokio::spawn(async move {});
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        _deleted_message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
    }

    async fn ready(
        &self,
        _ctx: Context,
        ready: Ready,
    ) {
        let user = &ready.user;
        if let Some(discriminator) = user.discriminator {
            println!("Connected as {}#{}", user.name, discriminator);
        } else {
            println!("Connected to discord as {}", user.name);
        }
    }

    async fn resume(
        &self,
        _: Context,
        _: ResumedEvent,
    ) {
    }
}

fn generate_default_config() -> Result<(), Error> {
    let config = Config::default();
    let str_to_write = toml::to_string_pretty(&config)?;
    let mut config_file = File::create("./config/config.toml")?;
    config_file.write_all(str_to_write.as_bytes())?;
    Ok(())
}

fn handle_config_error(why: std::io::Error) -> ! {
    if let io::ErrorKind::NotFound = why.kind() {
        println!("Generated default config file, please update settings.");
        if let Err(config_why) = generate_default_config() {
            eprintln!("Error generating config: `{config_why}`")
        }
        exit(0x0100)
    } else {
        eprintln!("Error reading config file: {why}");
        exit(0x0100)
    }
}

#[tokio::main]
async fn main() {
    let mut config = match File::open("./config/config.toml") {
        Ok(config) => config,
        Err(why) => handle_config_error(why),
    };

    let mut string = "".to_owned();
    if let Err(why) = config.read_to_string(&mut string) {
        return eprintln!("Error reading config file: \n\t`{why}`");
    }
    let config = match toml::from_str::<Config>(string.as_str()) {
        Ok(config) => config,
        Err(why) => return eprintln!("Error parsing config file:\n\t`{why}`"),
    };

    let db_options = MySqlConnectOptions::new()
        .ssl_mode(sqlx::mysql::MySqlSslMode::VerifyCa)
        .username(&config.database.username)
        .password(&config.database.password)
        .host(&config.database.url);
    let database = match MySqlPool::connect_with(db_options).await {
        Ok(db) => db,
        Err(why) => return eprintln!("Error creating db client:\n\t`{why}`"),
    };

    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config: Arc::new(config),
        database,
    };
    let mut client = match ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .event_handler(bot)
    .await
    {
        Ok(client) => client,
        Err(why) => {
            return eprintln!("Error creating discord client: \n\t`{why}`")
        },
    };

    if let Err(why) = client.start().await {
        eprintln!("Error occured while running the client: \n\t`{why}`");
        return;
    }

    println!("Shutting down.");
}
