pub mod bot;
pub mod config;
pub mod error;
pub mod util;

use std::{
    fs::File,
    io::Read,
    sync::{Arc, atomic::AtomicBool},
};
use tracing::info;

#[cfg(target_family = "unix")]
use tokio::signal::unix::SignalKind;

use config::Config;
use serenity::{
    all::ShardManager,
    client::ClientBuilder,
    prelude::{GatewayIntents, TypeMapKey},
};

use crate::{bot::Bot, util::handle_config_error};

pub struct ShardManagerBox;

impl TypeMapKey for ShardManagerBox {
    type Value = Arc<ShardManager>;
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    _ = dotenvy::dotenv();

    tracing_subscriber::fmt().init();

    let mut config = match File::open("./config/config.toml") {
        Ok(config) => config,
        Err(why) => handle_config_error(why),
    };
    let mut string = "".to_owned();
    config.read_to_string(&mut string)?;
    let config = toml::from_str::<Config>(string.as_str())?;

    let database = libsql::Builder::new_remote(
        std::env::var("DATABASE_URL")?,
        std::env::var("DATABASE_TOKEN")?,
    )
    .build()
    .await?;

    let mut cat_video = File::open("./config/cats.mp4")?;

    let cat_meta = cat_video.metadata()?;
    let mut cat_data = Vec::with_capacity(cat_meta.len() as usize);

    _ = cat_video.read_to_end(&mut cat_data)?;

    let config = Box::leak(Box::new(config));

    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config,
        database: Box::leak(Box::new(database)),
        cat: cat_data.leak(),
    };

    let mut client = ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .event_handler(bot)
    .await?;

    let shard_manager = client.shard_manager.clone();

    {
        let mut type_map = client.data.write().await;
        type_map.insert::<ShardManagerBox>(shard_manager.clone());
    }

    #[cfg(target_family = "unix")]
    {
        let mut signal =
            tokio::signal::unix::signal(SignalKind::terminate()).expect("Please work please work");
        let shard_manager1 = shard_manager.clone();
        tokio::spawn(async move {
            _ = signal.recv().await;
            info!("Received shutdown signal.");
            shard_manager1.shutdown_all().await;
        });
    }

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to enable ctrlc handler");
        info!("Received shutdown signal.");
        shard_manager.shutdown_all().await;
    });

    Ok(client.start_autosharded().await?)
}
