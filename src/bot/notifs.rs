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
use sqlx::PgExecutor;

use crate::{
    error::Error,
    model::{
        BotMessage, MessageKind, NotificationSetting, Series, Session,
        SessionStatus, Weekend,
    },
    util::{get_current_weekend, ID},
};

pub async fn delete_persistent_message(
    db: impl PgExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    channel_id: ID,
    series: Series,
) -> Result<(), Error> {
    let msg = sqlx::query_as!(
        BotMessage,
        "SELECT * from messages WHERE kind = $1 and channel = $2 AND series = $3",
        "Persistent",
        channel_id.i64(),
        series.str()
    )
    .fetch_optional(db)
    .await?;

    let Some(msg) = msg else {
        return Ok(());
    };

    match http
        .http()
        .delete_message(channel_id.u64().into(), msg.message.u64().into(), None)
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

    sqlx::query!("DELETE FROM messages WHERE id = $1", msg.id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn check_notify_session<'a>(
    weekend: &'a Weekend<'_>,
    pool: impl PgExecutor<'_> + Copy,
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
    db: impl PgExecutor<'_>,
    http: impl CacheHttp,
    channel_id: ID,
    series: Series,
) -> Result<Option<(BotMessage, serenity::all::Message)>, Error> {
    let message: Option<BotMessage> = sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages WHERE kind = $1 AND channel = $2 AND series = $3",
        "Persistent",
        channel_id.i64(),
        series.str()
    )
    .fetch_optional(db)
    .await?;

    let Some(message) = message else {
        return Ok(None);
    };

    let discord_msg = match http
        .http()
        .get_message(channel_id.u64().into(), message.message.u64().into())
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
    pool: &sqlx::PgPool,
    http: &Http,
    channel_id: ID,
    role_id: u64,
    series: Series,
    cat: &[u8],
    hash: &mut i32,
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
    id: i32,
    pool: impl PgExecutor<'_>,
) -> Result<(), Error> {
    match sqlx::query!(
        "UPDATE weekends set status = $1 WHERE id = $2",
        "Done",
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
    db: impl PgExecutor<'_>,
    weekend: &Weekend<'_>,
    session: &Session,
    http: &Http,
    channel_id: ID,
    role_id: u64,
    series: Series,
    cat: &[u8],
) -> Result<MessageId, Error> {
    let session_name = session.pretty_name();
    let attach = CreateAttachment::bytes(cat, "bongocat.mp4");
    let message = ChannelId::new(channel_id.u64())
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
    let msg_id: ID = message.id.get().into();
    sqlx::query!(
        "INSERT INTO messages 
 (channel, message, kind, series) VALUES ($1, $2, $3, $4)",
        channel_id.i64(),
        msg_id.i64(),
        "Notification",
        series.str()
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
        r#"## Next Event
{weekend}

Use <id:customize> to get a notification when a session is live.
Times are in your timezone"#
    )
}

pub async fn mark_session_notified(
    id: i32,
    pool: impl PgExecutor<'_>,
) -> Result<(), Error> {
    match sqlx::query!(
        "UPDATE sessions set status = $1 WHERE id = $2",
        "Done",
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
    db: impl PgExecutor<'_> + Copy,
    http: impl http::CacheHttp,
) -> Result<(), Error> {
    let messages: Vec<BotMessage> = sqlx::query_as!(
        BotMessage,
        "SELECT * from messages 
 WHERE kind = $1 AND messages.posted > now() + interval '30 minutes'",
        "Notification"
    )
    .fetch_all(db)
    .await?;

    let mut futures = vec![];
    for message in messages.into_iter() {
        futures.push(http.http().delete_message(
            message.channel.u64().into(),
            message.message.u64().into(),
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
WHERE kind = $1 AND posted > now() + interval '30 minutes'",
        "Notification"
    )
    .execute(db)
    .await?;

    Ok(())
}

pub async fn post_new_persistent(
    db: impl PgExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    channel_id: ID,
    series: Series,
) -> Result<(BotMessage, serenity::all::Message), Error> {
    let channel = ChannelId::new(channel_id.u64());
    let message = channel
        .send_message(
            http,
            CreateMessage::new()
                .content(persistent_message_str(weekend, series)),
        )
        .await?;

    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    let hash: ID = hasher.finish().into();

    struct Id {
        id: i64,
    }
    let msg_id: ID = message.id.get().into();
    let new_obj: Id = sqlx::query_as!(
        Id,
        "INSERT INTO messages (
            channel,
            message,
            kind,
            hash,
            series
            ) VALUES ($1, $2, $3, $4, $5) RETURNING id",
        channel_id.i64(),
        msg_id.i64(),
        "Persistent",
        hash.i64(),
        series.str()
    )
    .fetch_one(db)
    .await?;

    let new_msg = BotMessage {
        id: new_obj.id,
        channel: channel_id,
        message: message.id.get().into(),
        hash,
        kind: MessageKind::Persistent,
        posted: Utc::now(),
        series,
    };
    Ok((new_msg, message))
}

pub async fn update_persistent_message(
    db: impl PgExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    db_msg: BotMessage,
    mut dc_msg: serenity::all::Message,
    hash: ID,
) -> Result<(), Error> {
    dc_msg
        .edit(
            http,
            EditMessage::new()
                .content(persistent_message_str(weekend, Series::F1)),
        )
        .await?;
    sqlx::query!(
        "UPDATE messages SET hash = $1 WHERE id = $2",
        hash.i64(),
        db_msg.id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn create_persistent_message(
    db: impl PgExecutor<'_> + Copy,
    http: impl http::CacheHttp,
    weekend: &Weekend<'_>,
    channel_id: ID,
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
    let hash: ID = hasher.finish().into();
    if db_msg.hash == hash {
        return Ok(());
    }
    update_persistent_message(db, http, weekend, db_msg, dc_msg, hash).await?;

    Ok(())
}
