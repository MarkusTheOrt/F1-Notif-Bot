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
use sqlx::{Acquire, MySqlExecutor, MySqlPool};

use crate::{
    error::Error,
    model::{BotMessage, MessageKind, Series, Weekend},
    util::{get_all_weekends, get_weekends_without_sessions},
};

pub async fn get_calendar_notifs(
    pool: impl MySqlExecutor<'_>,
    channel: u64,
    series: Series,
) -> Result<Vec<BotMessage>, sqlx::Error> {
    sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages 
WHERE kind = ? AND channel = ? AND series = ?
ORDER by posted ASC",
        MessageKind::Calendar,
        channel,
        series
    )
    .fetch_all(pool)
    .await
}

pub async fn reserve_calendar_message(
    pool: impl MySqlExecutor<'_>,
    http: &Http,
    channel: u64,
    series: Series,
) -> Result<(), Error> {
    let channel_id = ChannelId::new(channel);
    let msg = channel_id
        .send_message(
            http,
            CreateMessage::new().content("*Reserved for Calendar*"),
        )
        .await?;
    let id = msg.id.get();
    sqlx::query!(
        "INSERT into messages (channel, message, kind, series) VALUES (?, ?, ?, ?)",
        channel,
        id,
        MessageKind::Calendar,
        series
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn populate_calendar(
    pool: &MySqlPool,
    http: &Http,
    channel: u64,
    series: Series,
) -> Result<(), Error> {
    let mut connection = pool.acquire().await?;
    let calendar =
        get_weekends_without_sessions(connection.acquire().await?, series)
            .await?;
    let notifs =
        get_calendar_notifs(connection.acquire().await?, channel, series)
            .await?;
    if notifs.len() < calendar.len() {
        for _ in notifs.len()..(calendar.len()) {
            if let Err(why) = reserve_calendar_message(
                connection.acquire().await?,
                http,
                channel,
                series,
            )
            .await
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
    pool: &MySqlPool,
    http: &Http,
    channel: u64,
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
        if msg.hash.is_some_and(|f| f == hash) {
            continue;
        }
        futures.push(update_message(
            channel,
            msg.message,
            http,
            pool,
            weekend,
            hash,
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
    channel_id: u64,
    message_id: u64,
    http: impl CacheHttp,
    db: impl MySqlExecutor<'_>,
    weekend: &Weekend<'_>,
    hash: u64,
) -> Result<(), Error> {
    let mut message =
        http.http().get_message(channel_id.into(), message_id.into()).await?;

    message
        .edit(http, EditMessage::new().content(format!("{weekend}")))
        .await?;

    sqlx::query!(
        "UPDATE messages SET hash = cast(? as UNSIGNED) WHERE id = ?",
        hash,
        message_id
    )
    .execute(db)
    .await?;

    Ok(())
}
