use chrono::Utc;
use serenity::{
    all::{ChannelId, MessageId},
    builder::{CreateAttachment, CreateMessage},
    http::Http,
};

use crate::{
    error::Error,
    model::{
        NotificationSetting, Series, Session, SessionKind, SessionStatus,
        Weekend, WeekendStatus,
    },
    util::get_current_weekend,
};

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
            NotificationSetting::Notify => {}
        }

        let difference = Utc::now() - session.date;
        if difference.num_minutes() > -5 && difference.num_minutes() < 0{
            return Ok(Some(session));
        }
    }
    Ok(None)
}

pub async fn runner(
    pool: &sqlx::MySqlPool,
    http: &Http,
    channel_id: u64,
    role_id: u64,
    series: Series,
    cat: &[u8]
) {
    let Ok(weekend) = get_current_weekend(pool, series).await else {
        return;
    };

    let session_to_notify = match check_notify_session(&weekend, pool).await {
        Err(why) => {
            eprintln!("Error marking session as done: {why}");
            return;
        },
        Ok(Some(session)) => session,
        // everything is cool but theres no session going on.
        Ok(None) => {
            return;
        }
    };

    // if the session cannot be marked as notified log the error and do not notify!
    if let Err(why) = mark_session_notified(session_to_notify.id, pool).await {
        eprintln!("Error marking session as notified: {why}");
        return;
    }

    println!("send a message");

    let _ = send_message(
        &weekend,
        session_to_notify,
        http,
        channel_id,
        role_id,
        cat
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
    cat: &[u8]
) -> Result<MessageId, Error> {
    let session_name = match session.kind {
        SessionKind::Custom => {
            session.title.clone().unwrap_or("Unnamed Session".to_owned())
        },
        SessionKind::Practice => {
            if let Some(number) = session.number {
                format!("FP{number}")
            } else {
                "Unknown Practice".to_owned()
            }
        },
        SessionKind::Qualifying => "Qualifying".to_owned(),
        SessionKind::Race => "Race".to_owned(),
        SessionKind::SprintRace => "Sprint Race".to_owned(),
        SessionKind::SprintQuali => "Sprint Shootout".to_owned(),
        SessionKind::PreSeasonTest => "Pre-Season Test".to_owned(),
        SessionKind::Unsupported => "Unsupported".to_owned(),
    };
    let attach = CreateAttachment::bytes(cat, "bongocat.mp4");
    let message = ChannelId::new(channel_id)
        .send_message(
            http,
            CreateMessage::new().content(format!(
                "**{} {} - {} starting <t:{}:R>**\n<@&{role_id}>",
                weekend.icon,
                weekend.name,
                session_name,
                session.date.timestamp()
            )).add_file(attach),
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
