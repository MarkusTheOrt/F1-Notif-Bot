use std::{
    fs::File,
    future::Future,
    hash::Hasher,
    io::{self, Write},
    process::exit,
};

use f1_bot_types::{Message, MessageKind, Series};
use serenity::all::{CacheHttp, ChannelId, CreateMessage, StatusCode};
use sqlx::MySqlConnection;
use tracing::{error, info};

use crate::{config::Config, error::Error};

use super::*;

pub fn handle_config_error(why: std::io::Error) -> ! {
    if let io::ErrorKind::NotFound = why.kind() {
        info!("Generated default config file, please update settings.");
        if let Err(config_why) = generate_default_config() {
            error!("Error generating config: `{config_why}`")
        }
        exit(0x0100)
    } else {
        info!("Error reading config file: {why}");
        exit(0x0100)
    }
}

fn generate_default_config() -> Result<(), Error> {
    let config = Config::default();
    let str_to_write = toml::to_string_pretty(&config)?;
    let mut config_file = File::create("./config/config.toml")?;
    config_file.write_all(str_to_write.as_bytes())?;
    Ok(())
}

/// Fetches and Deletes all expired messages.
pub async fn check_expired_messages(
    conn: &mut MySqlConnection,
    http: impl CacheHttp,
) -> Result<(), crate::error::Error> {
    let expired_messages = expired_messages(conn).await?;

    for message in expired_messages.into_iter() {
        let delete_result = ChannelId::new(message.channel.parse().unwrap())
            .delete_message(http.http(), message.id)
            .await;
        if let Err(why) = delete_result {
            if let serenity::Error::Http(http_error) = &why {
                if http_error
                    .status_code()
                    .is_some_and(|f| f == StatusCode::NOT_FOUND)
                {
                } else {
                    error!("{why}");
                    continue;
                }
            } else {
                error!("{why}");
                continue;
            }
        }
        delete_message(conn, message.id).await?;
    }
    Ok(())
}

pub async fn create_new_calendar_message(
    conn: &mut MySqlConnection,
    http: impl CacheHttp,
    series: Series,
    channel: u64,
) -> Result<(), crate::error::Error> {
    let new_message = ChannelId::new(channel)
        .send_message(
            http.http(),
            CreateMessage::new().content("*Reserved for Future use.*"),
        )
        .await?;

    sqlx::query!(
        "INSERT INTO messages 
(channel, message, kind, series) 
VALUES (?, ?, ?, ?)",
        channel.to_string(),
        new_message.id.to_string(),
        MessageKind::Calendar.i8(),
        series.i8()
    )
    .execute(conn)
    .await?;

    Ok(())
}

pub async fn delete_latest_calendar_message(
    conn: &mut MySqlConnection,
    http: impl CacheHttp,
    series: Series,
    channel: u64,
) -> Result<(), crate::error::Error> {
    Ok(())
}

pub async fn create_calendar(
    conn: &mut MySqlConnection,
    http: impl CacheHttp,
    series: Series,
    channel: u64,
) -> Result<(), Error> {
    let messages = fetch_calendar_messages(conn, series).await?;
    let weekends = full_weekends_for_series(conn, series).await?;
    match messages.len().cmp(&weekends.len()) {
        std::cmp::Ordering::Less => {
            let diff = weekends.len() - messages.len();
            for _ in 0..diff {
                create_new_calendar_message(conn, &http, series, channel)
                    .await?;
            }
            return Ok(());
        },
        std::cmp::Ordering::Greater => {
            let diff = messages.len() - weekends.len();
            for _ in 0..diff {
                delete_latest_calendar_message(conn, &http, series, channel)
                    .await?;
            }
            return Ok(());
        },
        std::cmp::Ordering::Equal => {},
    }

    for (weekend, message) in weekends.into_iter().zip(messages.into_iter()) {
        use std::hash::Hash;
        match message.hash {
            None => {
                // @TODO:[Markus]: Update message here
            },
            Some(hash) => {
                let mut hasher = std::hash::DefaultHasher::new();
                weekend.hash(&mut hasher);
                let new_hash = hasher.finish();
                if hash != new_hash.to_string() {
                    // @TODO:[Markus]: Update message here
                }
            },
        }
    }

    Ok(())
}
