pub mod bot;
pub mod config;
pub mod error;
pub mod model;
pub mod util;

use sqlx::{mysql::MySqlConnectOptions, MySqlPool};
use std::{fs::File, io::Read, sync::atomic::AtomicBool};

use config::Config;
use serenity::{client::ClientBuilder, prelude::GatewayIntents};

use crate::{bot::Bot, util::handle_config_error};

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

    let Ok(mut cat_video) = File::open("./config/cats.mp4") else {
        eprintln!("Error opening the cat.");
        return;
    };

    let Ok(cat_meta) = cat_video.metadata() else {
        eprintln!("No metadata on the cat.");
        return;
    };
    let mut cat_data = Vec::with_capacity(cat_meta.len() as usize);

    let Ok(_) = cat_video.read_to_end(&mut cat_data) else {
        eprintln!("Can't see the cats insides.");
        return;
    };

    let config = Box::leak(Box::new(config));

    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config,
        database,
        cat: cat_data.leak(),
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
