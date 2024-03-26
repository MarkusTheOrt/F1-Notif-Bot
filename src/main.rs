pub mod bot;
pub mod config;
pub mod error;
pub mod model;
pub mod util;

use anyhow::anyhow;
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::{fs::File, io::Read, sync::atomic::AtomicBool};

use config::Config;
use serenity::{client::ClientBuilder, prelude::GatewayIntents};

use crate::{bot::Bot, util::handle_config_error};

#[shuttle_runtime::main]
async fn main() -> shuttle_serenity::ShuttleSerenity {
    let mut config = match File::open("./config/config.toml") {
        Ok(config) => config,
        Err(why) => handle_config_error(why),
    };
    let mut string = "".to_owned();
    if let Err(why) = config.read_to_string(&mut string) {
        return Err(anyhow!("Error reading config file: \n\t`{why}`").into());
    }
    let config = match toml::from_str::<Config>(string.as_str()) {
        Ok(config) => config,
        Err(why) => {
            return Err(anyhow!("Error parsing config file:\n\t`{why}`").into())
        },
    };

    let db_options = PgConnectOptions::new()
        .username(&config.database.username)
        .password(&config.database.password)
        .host(&config.database.url);
    let database = match PgPool::connect_with(db_options).await {
        Ok(db) => db,
        Err(why) => {
            return Err(anyhow!("Error creating db client:\n\t`{why}`").into())
        },
    };

    let Ok(mut cat_video) = File::open("./config/cats.mp4") else {
        return Err(anyhow!("Error opening the cat.").into());
    };

    let Ok(cat_meta) = cat_video.metadata() else {
        return Err(anyhow!("No metadata on the cat.").into());
    };
    let mut cat_data = Vec::with_capacity(cat_meta.len() as usize);

    let Ok(_) = cat_video.read_to_end(&mut cat_data) else {
        return Err(anyhow!("Can't see the cats insides.").into());
    };

    let config = Box::leak(Box::new(config));

    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config,
        database,
        cat: cat_data.leak(),
    };

    let client = match ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .event_handler(bot)
    .await
    {
        Ok(client) => client,
        Err(why) => {
            return Err(
                anyhow!("Error creating discord client: \n\t`{why}`").into()
            )
        },
    };

    Ok(client.into())
}
