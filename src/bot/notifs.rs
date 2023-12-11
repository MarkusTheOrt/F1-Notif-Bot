use chrono::Utc;
use serenity::{
    all::{ChannelId, MessageId},
    builder::{CreateAttachment, CreateMessage},
    http::{self, Http, StatusCode},
    prelude::{CacheHttp, HttpError},
};
use sqlx::MySqlExecutor;

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
) -> Result<(), Error> {
    let msg = sqlx::query_as!(
        BotMessage,
        "SELECT * from messages WHERE kind = ? and channel = ?",
        MessageKind::Persistent,
        channel_id
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

        match session.notify {
            NotificationSetting::Ignore => {
                mark_session_notified(session.id, pool).await?;
                continue;
            },
            NotificationSetting::Notify => {},
        }

        let difference = Utc::now() - session.date;
        if difference.num_minutes() > -5 && difference.num_minutes() < 0 {
            return Ok(Some(session));
        }
    }
    Ok(None)
}

pub async fn get_persistent_message(
    db: impl MySqlExecutor<'_>,
    http: impl CacheHttp,
    channel_id: u64,
) -> Result<Option<(BotMessage, serenity::all::Message)>, Error> {
    let message: Option<BotMessage> = sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages WHERE kind = ? AND channel = ?",
        MessageKind::Persistent,
        channel_id
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
    _hash: &mut u64,
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

    let session_to_notify = match check_notify_session(&weekend, pool).await {
        Err(why) => {
            eprintln!("Error marking session as done: {why}");
            return;
        },
        Ok(Some(session)) => session,
        // everything is cool but theres no session going on.
        Ok(None) => {
            println!("no session found");
            return;
        },
    };

    // if the session cannot be marked as notified log the error and do not notify!
    if let Err(why) = mark_session_notified(session_to_notify.id, pool).await {
        eprintln!("Error marking session as notified: {why}");
        return;
    }
    println!("sending");
    let _ = send_message(
        &weekend,
        session_to_notify,
        http,
        channel_id,
        role_id,
        cat,
    )
    .await;
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

pub async fn send_message(
    weekend: &Weekend<'_>,
    session: &Session,
    http: &Http,
    channel_id: u64,
    role_id: u64,
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

    Ok(message.id)
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
