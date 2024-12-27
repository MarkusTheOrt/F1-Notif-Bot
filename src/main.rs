pub mod bot;
pub mod config;
pub mod error;
pub mod util;

use anyhow::anyhow;
use f1_bot_types::Series;
use sqlx::{mysql::MySqlConnectOptions, MySqlPool};
use tracing::info;
use util::fetch_next_full_weekend_for_series;
use std::{fs::File, io::Read, sync::atomic::AtomicBool};

use config::Config;
use serenity::{client::ClientBuilder, prelude::GatewayIntents};

use crate::{bot::Bot, util::handle_config_error};

#[tokio::main]
async fn main() -> Result<(), String> {
    tracing_subscriber::fmt().init();

    let mut config = match File::open("./config/config.toml") {
        Ok(config) => config,
        Err(why) => handle_config_error(why),
    };
    let mut string = "".to_owned();
    if let Err(why) = config.read_to_string(&mut string) {
        return Err(
            anyhow!("Error reading config file: \n\t`{why}`").to_string()
        );
    }
    let config = match toml::from_str::<Config>(string.as_str()) {
        Ok(config) => config,
        Err(why) => {
            return Err(
                anyhow!("Error parsing config file:\n\t`{why}`").to_string()
            )
        },
    };

    let db_options = MySqlConnectOptions::new()
        .username(&config.database.username)
        .password(&config.database.password)
        .host(&config.database.url)
        .port(3306)
        .database("fia-docs");
    let database = match MySqlPool::connect_with(db_options).await {
        Ok(db) => db,
        Err(why) => {
            return Err(
                anyhow!("Error creating db client:\n\t`{why}`").to_string()
            )
        },
    };

    let Ok(mut cat_video) = File::open("./config/cats.mp4") else {
        return Err(anyhow!("Error opening the cat.").to_string());
    };

    let Ok(cat_meta) = cat_video.metadata() else {
        return Err(anyhow!("No metadata on the cat.").to_string());
    };
    let mut cat_data = Vec::with_capacity(cat_meta.len() as usize);

    let Ok(_) = cat_video.read_to_end(&mut cat_data) else {
        return Err(anyhow!("Can't see the cats insides.").to_string());
    };

    let config = Box::leak(Box::new(config));

    let mut db_conn = database.acquire().await.unwrap();

    let next_weekend = fetch_next_full_weekend_for_series(db_conn.as_mut(), Series::F1).await.unwrap();
    info!("Found next weekend: {next_weekend:#?}");

    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config,
        database: Box::leak(Box::new(database)),
        cat: cat_data.leak(),
    };

    _ = bot;
    return Ok(());

    let mut client = match ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .event_handler(bot)
    .await
    {
        Ok(client) => client,
        Err(why) => {
            return Err(anyhow!("Error creating discord client: \n\t`{why}`")
                .to_string())
        },
    };

    client.start_autosharded().await.map_err(|f| f.to_string())
}
