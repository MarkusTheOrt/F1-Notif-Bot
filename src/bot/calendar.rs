use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Duration,
};

use serenity::{
    all::ChannelId,
    builder::{CreateMessage, EditMessage},
    futures::future::join_all,
    http::{CacheHttp, Http},
};
use sqlx::PgExecutor;

use crate::{
    error::Error,
    model::{BotMessage, MessageKind, Series, Weekend},
    util::{get_all_weekends, get_weekends_without_sessions, ID},
};

pub async fn get_calendar_notifs(
    pool: impl PgExecutor<'_>,
    channel: ID,
    series: Series,
) -> Result<Vec<BotMessage>, sqlx::Error> {
    sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages 
WHERE kind = $1 AND channel = $2 AND series = $3
ORDER by posted ASC",
        <crate::model::MessageKind as Into::<&str>>::into(
            MessageKind::Calendar
        ),
        channel.i64(),
        <crate::model::Series as Into::<&str>>::into(series)
    )
    .fetch_all(pool)
    .await
}

pub async fn reserve_calendar_message(
    pool: impl PgExecutor<'_>,
    http: &Http,
    channel: ID,
    series: Series,
) -> Result<(), Error> {
    let channel_id = ChannelId::new(channel.u64());
    let msg = channel_id
        .send_message(
            http,
            CreateMessage::new().content("*Reserved for Calendar*"),
        )
        .await?;
    let id: ID = msg.id.get().into();
    sqlx::query!(
        "INSERT into messages (channel, message, kind, series, hash) VALUES ($1, $2, $3, $4, 0)",
        channel.i64(),
        id.i64(),
        <crate::model::MessageKind as Into::<&str>>::into(MessageKind::Calendar),
        <crate::model::Series as Into::<&str>>::into(series)
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn populate_calendar(
    pool: impl PgExecutor<'_> + Copy,
    http: &Http,
    channel: ID,
    series: Series,
) -> Result<(), Error> {
    let calendar = get_weekends_without_sessions(pool, series).await?;
    let notifs = get_calendar_notifs(pool, channel, series).await?;
    if notifs.len() < calendar.len() {
        for _ in notifs.len()..(calendar.len()) {
            if let Err(why) =
                reserve_calendar_message(pool, http, channel, series).await
            {
                eprintln!("Error posting message: {why}");
                break;
            }

            // sleep for one second so we don't have messed up lists
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    Ok(())
}

pub async fn update_calendar(
    pool: impl PgExecutor<'_> + Copy,
    http: &Http,
    channel: ID,
    series: Series,
) -> Result<(), Error> {
    let mut calendar = get_all_weekends(pool, series).await?;
    let mut weekends = get_weekends_without_sessions(pool, series).await?;
    let notifs = get_calendar_notifs(pool, channel, series).await?;

    for wknd in weekends.iter_mut() {
        let Some(wknd_full) = calendar.iter_mut().find(|f| f.id == wknd.id)
        else {
            continue;
        };
        std::mem::swap(&mut wknd.sessions, &mut wknd_full.sessions);
    }

    let weekends_iter = weekends.iter();
    let mut futures = Vec::with_capacity(weekends_iter.len());
    for (weekend, msg) in weekends_iter.zip(notifs.into_iter()) {
        let mut hasher = DefaultHasher::new();
        weekend.hash(&mut hasher);
        let hash = hasher.finish();
        // skip message if its the same!
        if msg.hash.u64() == hash {
            continue;
        }
        futures.push(update_message(
            channel,
            msg.message,
            http,
            pool,
            weekend,
            hash.into(),
        ));
    }

    let results = join_all(futures).await;
    for result in results.into_iter() {
        if let Err(why) = result {
            eprintln!("Error updating message: \n\t`{why}`")
        }
    }

    Ok(())
}

async fn update_message(
    channel_id: ID,
    message_id: ID,
    http: impl CacheHttp,
    db: impl PgExecutor<'_>,
    weekend: &Weekend<'_>,
    hash: ID,
) -> Result<(), Error> {
    let mut message = http
        .http()
        .get_message(channel_id.u64().into(), message_id.u64().into())
        .await?;

    message
        .edit(http, EditMessage::new().content(format!("{weekend}")))
        .await?;

    sqlx::query!(
        "UPDATE messages SET hash = $1 WHERE message = $2",
        hash.i64(),
        message_id.i64()
    )
    .execute(db)
    .await?;

    Ok(())
}
