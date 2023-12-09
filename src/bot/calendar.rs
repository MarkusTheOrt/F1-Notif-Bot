use std::time::Duration;

use serenity::{
    all::{ChannelId, MessageId},
    builder::{CreateMessage, EditMessage},
    http::{self, Http},
    utils::MessageBuilder,
};
use sqlx::{Acquire, MySqlExecutor, MySqlPool};

use crate::{
    error::Error,
    model::{BotMessage, MessageKind, Series},
    util::get_weekends_without_sessions,
};

pub async fn get_calendar_notifs(
    pool: impl MySqlExecutor<'_>,
    channel: u64,
) -> Result<Vec<BotMessage>, sqlx::Error> {
    sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages 
WHERE kind = ? AND channel = ? 
ORDER by posted ASC",
        MessageKind::Calendar,
        channel
    )
    .fetch_all(pool)
    .await
}

pub async fn reserve_calendar_message(
    pool: impl MySqlExecutor<'_>,
    http: &Http,
    channel: u64,
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
        "INSERT into messages (channel, message, kind) VALUES (?, ?, ?)",
        channel,
        id,
        MessageKind::Calendar
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
        get_calendar_notifs(connection.acquire().await?, channel).await?;

    if notifs.len() < calendar.len() {
        for _ in notifs.len()..(calendar.len() - 1) {
            if let Err(why) = reserve_calendar_message(
                connection.acquire().await?,
                http,
                channel,
            )
            .await
            {
                eprintln!("Error posting message: {why}");
                break;
            }
            // sleep for one second so we don't have messed up lists
            std::thread::sleep(Duration::from_secs(1));
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
    let calendar = get_weekends_without_sessions(pool, series).await?;
    let notifs = get_calendar_notifs(pool, channel).await?;

    if calendar.len() != notifs.len() {
        eprintln!("{}, {}", calendar.len(), notifs.len());
        return Err(Error::NotFound);
    }

    for (weekend, msg) in calendar.into_iter().zip(notifs.into_iter()) {
        let mut message =
            http.get_message(channel.into(), msg.message.into()).await?;
        message
            .edit(
                http,
                EditMessage::new()
                    .content(format!("{} {}", weekend.icon, weekend.name)),
            )
            .await?;
    }

    Ok(())
}
