use chrono::Utc;
use mongodb::Collection;
use serenity::{
    futures::StreamExt,
    prelude::Context,
};

use crate::{
    config::Config,
    error::Error,
    util::database::BotMessageType,
};

use super::database::BotMessage;

pub async fn get_notifications(
    notifications: &Collection<BotMessage>
) -> Result<(), Error> {
    let mut cur = notifications.find(None, None).await?;

    while let Some(doc) = cur.next().await {
        let doc = doc?;
        if let BotMessageType::Notification(notify) = &doc.kind {
            if notify.time_sent.signed_duration_since(Utc::now()).num_minutes()
                < -30
            {
                // At this point we delete the message
            }
        }
    }

    Ok(())
}

pub async fn delete_persistent_message(
    notifications: &Collection<BotMessage>,
    ctx: &Context,
    config: &Config,
) -> Result<(), Error> {
    let mut messages = notifications.find(None, None).await?;
    while let Some(message) = messages.next().await {
        let message = message?;
        if let BotMessageType::Persistent(_) = &message.kind {
            ctx.http
                .delete_message(config.discord.channel, message.discord_id)
                .await?;
        }
    }
    Ok(())
}

pub async fn get_persistent_message(
    notifications: &Collection<BotMessage>
) -> Result<Option<BotMessage>, Error> {
    let mut messages = notifications.find(None, None).await?;
    while let Some(message) = messages.next().await {
        let message = message?;
        if let BotMessageType::Persistent(_) = message.kind {
            return Ok(Some(message));
        }
    }
    Ok(None)
}
