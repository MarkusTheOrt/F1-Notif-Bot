use std::{
    fs::File,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
    process::exit,
    time::Duration,
};

use chrono::Utc;
use f1_bot_types::{Message, MessageKind, Series, Session, SessionStatus, Weekend, WeekendStatus};
use libsql::params;
use serenity::all::{
    CacheHttp, ChannelId, CreateAttachment, CreateMessage, EditMessage, MessageId, StatusCode,
};
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
    conn: &mut libsql::Connection,
    http: impl CacheHttp,
) -> Result<(), crate::error::Error> {
    let expired_messages = expired_messages(conn).await?;

    for message in expired_messages.into_iter() {
        let delete_result = ChannelId::new(message.channel.parse()?)
            .delete_message(http.http(), message.message.parse::<u64>()?)
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
    db_conn: &mut libsql::Connection,
    http: impl CacheHttp,
    series: Series,
    channel: u64,
) -> Result<u64, crate::error::Error> {
    let new_message = ChannelId::new(channel)
        .send_message(
            http.http(),
            CreateMessage::new().content("*Reserved for Future use.*"),
        )
        .await?;
    Ok(db_conn
        .execute(
            "INSERT INTO messages (channel, message, kind, series) VALUES (?, ?, ?, ?)",
            params![
                channel.to_string(),
                new_message.id.to_string(),
                MessageKind::Calendar.i8(),
                series.i8()
            ],
        )
        .await?)
}

pub async fn delete_latest_calendar_message(
    db_conn: &mut libsql::Connection,
    http: impl CacheHttp,
    series: Series,
) -> Result<(), crate::error::Error> {
    let messages = fetch_calendar_messages(db_conn, series).await?;
    let last = match messages.last() {
        Some(m) => m,
        None => return Ok(()),
    };

    let channel_u64: u64 = last.channel.parse()?;
    let message_u64: u64 = last.message.parse()?;

    let delete_msg = ChannelId::new(channel_u64)
        .delete_message(http.http(), message_u64)
        .await;
    if let Err(serenity::Error::Http(why)) = delete_msg {
        if why.status_code().is_none_or(|f| f != StatusCode::NOT_FOUND) {
            return Err(Error::Serenity(Box::new(why.into())));
        }
    } else {
        return delete_msg.map_err(|e| e.into());
    }

    delete_message(db_conn, last.id).await?;

    Ok(())
}

pub async fn create_calendar(
    conn: &mut libsql::Connection,
    http: impl CacheHttp,
    series: Series,
    channel: u64,
) -> Result<(), Error> {
    let messages = fetch_calendar_messages(conn, series).await?;
    let weekends = fetch_full_weekends_for_series(conn, series).await?;
    match messages.len().cmp(&weekends.len()) {
        std::cmp::Ordering::Less => {
            let diff = weekends.len() - messages.len();
            for _ in 0..diff {
                create_new_calendar_message(conn, &http, series, channel).await?;
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            return Ok(());
        }
        std::cmp::Ordering::Greater => {
            let diff = messages.len() - weekends.len();
            for _ in 0..diff {
                delete_latest_calendar_message(conn, &http, series).await?;
            }
            return Ok(());
        }
        std::cmp::Ordering::Equal => {}
    }

    Ok(())
}

pub async fn update_calendar_message(
    _http: impl CacheHttp,
    _weekend: &FullWeekend,
    _channel: u64,
    _message: u64,
) -> Result<(), serenity::Error> {
    Ok(())
}

pub async fn edit_calendar(
    db_conn: &mut libsql::Connection,
    http: impl CacheHttp,
    series: Series,
) -> Result<(), crate::error::Error> {
    let msgs = fetch_calendar_messages(db_conn, series).await?;
    let weekends = fetch_full_weekends_for_series(db_conn, series).await?;
    if msgs.len() != weekends.len() {
        return Err(crate::error::Error::NotSameLen);
    }

    for (msg, weekend) in msgs.into_iter().zip(weekends.into_iter()) {
        let mut hasher = std::hash::DefaultHasher::new();
        weekend.hash(&mut hasher);
        let hash = hasher.finish();
        if msg.hash.as_ref().is_some_and(|f| *f == hash.to_string()) {
            continue;
        }

        info!(
            "Updating {} Calendar message for {}...",
            series, weekend.weekend.name
        );

        let channel_u64: u64 = msg.channel.parse()?;
        let message_u64: u64 = msg.message.parse()?;
        let mut sessions_str = String::new();
        for session in weekend.sessions.iter() {
            sessions_str += &format!(
                "\n> `{:>12}` <t:{}:f> (<t:{}:R>)",
                session.title,
                session.start_date.timestamp(),
                session.start_date.timestamp()
            );
        }
        match ChannelId::new(channel_u64)
            .edit_message(
                &http,
                message_u64,
                EditMessage::new().content(format!(
                    "## {} **{}**{}",
                    weekend.weekend.icon, weekend.weekend.name, sessions_str
                )),
            )
            .await
        {
            Ok(_) => {}
            Err(why) => {
                error!("{why:#?}");
                continue;
            }
        }

        if let Err(why) = set_message_hash(db_conn, &msg, hash).await {
            error!("{why:#?}");
        }
    }

    Ok(())
}

pub async fn set_message_hash(
    db_conn: &mut libsql::Connection,
    message: &Message,
    hash: u64,
) -> Result<u64, crate::error::Error> {
    Ok(db_conn
        .execute(
            "UPDATE messages SET HASH = ? WHERE id = ?",
            params![hash.to_string(), message.id],
        )
        .await?)
}

pub async fn check_active_session(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Option<(Weekend, Session)>, crate::error::Error> {
    let weekend = fetch_next_full_weekend_for_series(db_conn, series).await?;
    let Some(weekend) = weekend else {
        return Ok(None);
    };
    let Some(session) = weekend.sessions.into_iter().find(|f| {
        matches!(
            f.status,
            f1_bot_types::SessionStatus::Open | f1_bot_types::SessionStatus::Delayed
        ) && matches!(
            f.start_date.signed_duration_since(Utc::now()).num_minutes(),
            0..5
        )
    }) else {
        return Ok(None);
    };

    Ok(Some((weekend.weekend, session)))
}

pub async fn create_new_notifications_msg_db(
    db_conn: &mut libsql::Connection,
    session: &Session,
    series: Series,
    channel: u64,
    message: u64,
) -> Result<u64, crate::error::Error> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let expiry = (Utc::now() + Duration::from_secs(session.duration as u64))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Ok(db_conn.execute(
        "INSERT INTO messages (channel, message, kind, posted, series, expiry) VALUES (?, ?, ?, ?, ?, ?)", 
        params![
        channel.to_string(),
        message.to_string(),
        MessageKind::Notification.i8(),
        now,
        series.i8(),
        expiry
    ]).await?)
}

pub async fn send_notification(
    http: impl CacheHttp,
    weekend: &Weekend,
    session: &Session,
    channel: u64,
    cat: &[u8],
    role: u64,
) -> Result<MessageId, crate::error::Error> {
    let new_msg = ChannelId::new(channel)
        .send_message(
            http,
            CreateMessage::new()
                .content(format!(
                    "<@&{}>\n{} {} {} is starting: <t:{}:R>",
                    role,
                    weekend.icon,
                    weekend.name,
                    session.title,
                    session.start_date.timestamp()
                ))
                .add_file(CreateAttachment::bytes(cat, "cats.mp4")),
        )
        .await?;
    Ok(new_msg.id)
}

pub async fn check_expired_weekend(
    db_conn: &mut libsql::Connection,
    weekend: &Weekend,
    session: &Session,
) -> Result<Option<Series>, crate::error::Error> {
    let weekend = match fetch_full_weekend(db_conn, weekend.id).await? {
        Some(d) => d,
        None => return Ok(None),
    };

    if weekend.weekend.status == WeekendStatus::Done {
        return Ok(None);
    }

    if weekend.sessions.into_iter().all(|f| {
        matches!(
            if f.id == session.id {
                session.status
            } else {
                f.status
            },
            SessionStatus::Finished | SessionStatus::Cancelled
        )
    }) {
        Ok(Some(weekend.weekend.series))
    } else {
        Ok(None)
    }
}

pub async fn post_weekend_message(
    http: impl CacheHttp,
    weekend: &FullWeekend,
    channel: u64,
    series: Series,
) -> Result<MessageId, serenity::Error> {
    ChannelId::new(channel)
        .send_message(
            http,
            CreateMessage::new()
                .content(weekend.weekend_msg_str(matches!(series, Series::F1 | Series::F1Academy))),
        )
        .await
        .map(|f| f.id)
}

pub async fn insert_weekend_message(
    db_conn: &mut libsql::Connection,
    channel: u64,
    message: u64,
    weekend: &FullWeekend,
) -> Result<u64, crate::error::Error> {
    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    let hash = hasher.finish();
    Ok(db_conn
        .execute(
            "INSERT INTO messages (channel, message, hash, kind, series) VALUES (?, ?, ?, ?, ?)",
            params![
                channel,
                message,
                hash,
                MessageKind::Weekend.i8(),
                weekend.weekend.series.i8()
            ],
        )
        .await?)
}

pub async fn update_weekend_message(
    http: impl CacheHttp,
    weekend: &FullWeekend,
    channel: u64,
    message: u64,
) -> Result<(), crate::error::Error> {
    ChannelId::new(channel)
        .edit_message(
            http,
            message,
            EditMessage::new().content(weekend.weekend_msg_str(true)),
        )
        .await
        .map(|_f| ())?;
    Ok(())
}
