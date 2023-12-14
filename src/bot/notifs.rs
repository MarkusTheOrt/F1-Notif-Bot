use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use chrono::Utc;
use serenity::{
    all::{ChannelId, MessageId},
    builder::{CreateAttachment, CreateMessage, EditMessage},
    futures::future::join_all,
    http::{self, Http, StatusCode},
    prelude::{CacheHttp, HttpError},
};
use sqlx::{mysql::MySqlQueryResult, MySqlExecutor};

use crate::{
    error::Error,
    model::{
        BotMessage, MessageKind, NotificationSetting, Series, Session,
        SessionStatus, Weekend, WeekendStatus,
    },
    util::get_current_weekend,
};

pub async fn delete_persistent_message(
    db: impl MySqlExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    channel_id: u64,
    series: Series,
) -> Result<(), Error> {
    let msg = sqlx::query_as!(
        BotMessage,
        "SELECT * from messages WHERE kind = ? and channel = ? AND series = ?",
        MessageKind::Persistent,
        channel_id,
        series
    )
    .fetch_optional(db)
    .await?;

    let Some(msg) = msg else {
        return Ok(());
    };

    match http
        .http()
        .delete_message(channel_id.into(), msg.message.into(), None)
        .await
    {
        Ok(_) => {},
        Err(e) => {
            let serenity::Error::Http(
                serenity::prelude::HttpError::UnsuccessfulRequest(err),
            ) = &e
            else {
                return Err(Error::Serenity(e));
            };
            if err.status_code == StatusCode::NOT_FOUND {
                {}
            } else {
                return Err(Error::Serenity(e));
            }
        },
    }

    sqlx::query!("DELETE FROM messages WHERE id = ?", msg.id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn check_notify_session<'a>(
    weekend: &'a Weekend<'_>,
    pool: &sqlx::MySqlPool,
) -> Result<Option<&'a Session>, Error> {
    for session in weekend.sessions.iter() {
        // Only notify sessions that actually want to be notified!

        // lets not display sessions that are canceled or already notified!
        match session.status {
            SessionStatus::Open => {},
            SessionStatus::Delayed => continue,
            SessionStatus::Cancelled => continue,
            SessionStatus::Done => continue,
            SessionStatus::Unsupported => {
                eprintln!("Found unsupported session in {}", weekend.name);
                continue;
            },
        }

        let difference = Utc::now() - session.date;
        if difference.num_minutes() > -5 && difference.num_minutes() < 0 {
            if matches!(session.notify, NotificationSetting::Notify) {
                return Ok(Some(session));
            } else {
                mark_session_notified(session.id, pool).await?;
            }
        }
    }
    Ok(None)
}

pub async fn get_persistent_message(
    db: impl MySqlExecutor<'_>,
    http: impl CacheHttp,
    channel_id: u64,
    series: Series,
) -> Result<Option<(BotMessage, serenity::all::Message)>, Error> {
    let message: Option<BotMessage> = sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages WHERE kind = ? AND channel = ? AND series = ?",
        MessageKind::Persistent,
        channel_id,
        series
    )
    .fetch_optional(db)
    .await?;

    let Some(message) = message else {
        return Ok(None);
    };

    let discord_msg = match http
        .http()
        .get_message(channel_id.into(), message.message.into())
        .await
    {
        Ok(msg) => msg,
        Err(e) => {
            let serenity::Error::Http(HttpError::UnsuccessfulRequest(err)) = &e
            else {
                return Err(Error::Serenity(e));
            };
            if err.status_code == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            return Err(Error::Serenity(e));
        },
    };

    Ok(Some((message, discord_msg)))
}

pub async fn runner(
    pool: &sqlx::MySqlPool,
    http: &Http,
    channel_id: u64,
    role_id: u64,
    series: Series,
    cat: &[u8],
    hash: &mut u32,
) {
    let weekend = match get_current_weekend(pool, series).await {
        Ok(w) => w,
        Err(why) => {
            if !matches!(why, Error::NotFound) {
                eprintln!("Error getting session: {why}");
            }
            return;
        },
    };

    let mut has_open_sessions = false;
    for session in weekend.sessions.iter() {
        if matches!(session.status, SessionStatus::Open) {
            has_open_sessions = true;
            break;
        }
    }
    if !has_open_sessions {
        if let Err(why) = mark_weekend_done(weekend.id, pool).await {
            eprintln!("Error marking weekend as done: {why}");
        }
    }

    if *hash == 0 {
        *hash = weekend.id;
    } else if *hash != weekend.id {
        if let Err(why) =
            delete_persistent_message(pool, http, channel_id, series).await
        {
            eprintln!("Error deleting persistent message: {why}");
        }
    }

    *hash = weekend.id;

    if let Err(why) =
        create_persistent_message(pool, http, &weekend, channel_id, series)
            .await
    {
        eprintln!("Error creating persistent message: {why}");
    }

    let session_to_notify = match check_notify_session(&weekend, pool).await {
        Err(why) => {
            eprintln!("Error marking session as done: {why}");
            return;
        },
        Ok(Some(session)) => session,
        // everything is cool but theres no session going on.
        Ok(None) => {
            return;
        },
    };

    // if the session cannot be marked as notified log the error and do not notify!
    if let Err(why) = mark_session_notified(session_to_notify.id, pool).await {
        eprintln!("Error marking session as notified: {why}");
        return;
    }

    if let Err(why) = send_message(
        pool,
        &weekend,
        session_to_notify,
        http,
        channel_id,
        role_id,
        series,
        cat,
    )
    .await
    {
        eprintln!("Error sending message: {why}");
    }
}

pub async fn mark_weekend_done(
    id: u32,
    pool: &sqlx::MySqlPool,
) -> Result<(), Error> {
    match sqlx::query!(
        "UPDATE weekends set status = ? WHERE id = ?",
        WeekendStatus::Done,
        id
    )
    .execute(pool)
    .await
    {
        Ok(res) => {
            if res.rows_affected() == 0 {
                Err(Error::NotFound)
            } else {
                Ok(())
            }
        },
        Err(why) => Err(Error::Sqlx(why)),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn send_message(
    db: impl MySqlExecutor<'_>,
    weekend: &Weekend<'_>,
    session: &Session,
    http: &Http,
    channel_id: u64,
    role_id: u64,
    series: Series,
    cat: &[u8],
) -> Result<MessageId, Error> {
    let session_name = session.pretty_name();
    let attach = CreateAttachment::bytes(cat, "bongocat.mp4");
    let message = ChannelId::new(channel_id)
        .send_message(
            http,
            CreateMessage::new()
                .content(format!(
                    "**{} {} - {} starting <t:{}:R>**\n<@&{role_id}>",
                    weekend.icon,
                    weekend.name,
                    session_name,
                    session.date.timestamp()
                ))
                .add_file(attach),
        )
        .await?;
    sqlx::query!(
        "INSERT INTO messages 
 (channel, message, kind, series) VALUES (?, ?, ?, ?)",
        channel_id,
        message.id.get(),
        MessageKind::Notification,
        series
    )
    .execute(db)
    .await?;
    Ok(message.id)
}

fn persistent_message_str(
    weekend: &Weekend<'_>,
    _series: Series,
) -> String {
    format!(
        r#"**Next Event**
{weekend}

Use <#913752470293991424> or <id:customize> to get a notification when a session is live.
Times are in your timezone"#
    )
}

pub async fn mark_session_notified(
    id: u32,
    pool: &sqlx::MySqlPool,
) -> Result<(), Error> {
    match sqlx::query!(
        "UPDATE sessions set status = ? WHERE id = ?",
        SessionStatus::Done,
        id
    )
    .execute(pool)
    .await
    {
        Ok(res) => {
            if res.rows_affected() == 0 {
                Err(Error::NotFound)
            } else {
                Ok(())
            }
        },
        Err(why) => Err(Error::Sqlx(why)),
    }
}

pub async fn remove_old_notifs(
    db: impl MySqlExecutor<'_> + Copy,
    http: impl http::CacheHttp,
) -> Result<(), Error> {
    let messages: Vec<BotMessage> = sqlx::query_as!(
        BotMessage,
        "SELECT * from messages 
 WHERE kind = ? AND TIMESTAMPDIFF(Minute, messages.posted, NOW()) > 30",
        MessageKind::Notification
    )
    .fetch_all(db)
    .await?;

    let mut futures = vec![];
    for message in messages.into_iter() {
        futures.push(http.http().delete_message(
            message.channel.into(),
            message.message.into(),
            None,
        ));
    }
    let futures = join_all(futures).await;

    for future in futures.into_iter() {
        let Err(why) = future else {
            continue;
        };
        eprintln!("Error removing message: {why}");
    }

    sqlx::query!(
        "DELETE FROM messages 
WHERE kind = ? AND TIMESTAMPDIFF(Minute, messages.posted, NOW()) > 30",
        MessageKind::Notification
    )
    .execute(db)
    .await?;

    Ok(())
}

pub async fn post_new_persistent(
    db: impl MySqlExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    channel_id: u64,
    series: Series,
) -> Result<(BotMessage, serenity::all::Message), Error> {
    let channel = ChannelId::new(channel_id);
    let message = channel
        .send_message(
            http,
            CreateMessage::new()
                .content(persistent_message_str(weekend, series)),
        )
        .await?;

    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    let hash = hasher.finish();

    let new_obj: MySqlQueryResult = sqlx::query!(
        "INSERT INTO messages (
            channel,
            message,
            kind,
            hash,
            series
            ) VALUES (?, ?, ?, cast(? as UNSIGNED), ?)",
        channel_id,
        message.id.get(),
        MessageKind::Persistent,
        hash,
        series
    )
    .execute(db)
    .await?;

    let new_msg = BotMessage {
        id: new_obj.last_insert_id() as u32,
        channel: channel_id,
        message: message.id.get(),
        hash: Some(hash),
        kind: MessageKind::Persistent,
        posted: Utc::now(),
        series,
    };
    Ok((new_msg, message))
}

pub async fn update_persistent_message(
    db: impl MySqlExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    db_msg: BotMessage,
    mut dc_msg: serenity::all::Message,
    hash: u64,
) -> Result<(), Error> {
    dc_msg
        .edit(
            http,
            EditMessage::new()
                .content(persistent_message_str(weekend, Series::F1)),
        )
        .await?;
    sqlx::query!(
        "UPDATE messages SET hash = cast(? as UNSIGNED) WHERE id = ?",
        hash,
        db_msg.id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn create_persistent_message(
    db: impl MySqlExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    channel_id: u64,
    series: Series,
) -> Result<(), Error> {
    let msg = get_persistent_message(db, &http, channel_id, series).await?;

    let (db_msg, dc_msg) = match msg {
        None => {
            post_new_persistent(db, &http, weekend, channel_id, series).await?
        },
        Some(d) => d,
    };

    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    let hash = hasher.finish();
    if db_msg.hash.is_some_and(|h| h == hash) {
        return Ok(());
    }
    update_persistent_message(db, http, weekend, db_msg, dc_msg, hash).await?;

    Ok(())
}
