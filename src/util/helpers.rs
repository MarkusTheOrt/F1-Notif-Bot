use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use chrono::Utc;
use mongodb::{bson::doc, Collection};
use serenity::{self, futures::StreamExt, prelude::Context};

use crate::{config::Config, error::Error, util::database::BotMessageType};

use super::database::{BotMessage, DiscordString, SessionType, Weekend};

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

pub async fn create_persistent_message(
    ctx: &Context,
    config: &Config,
    weekend: &Weekend,
) -> Result<BotMessage, Error> {
    let weekend_as_string = weekend.to_display();
    let channel = ctx.http.get_channel(config.discord.channel).await?;

    let channel = channel.guild().unwrap();
    let message = channel
        .send_message(&ctx.http, |msg| msg.content(weekend_as_string))
        .await?;

    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    Ok(BotMessage::new_persistent(*message.id.as_u64(), hasher.finish()))
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

pub async fn update_persistent_message(
    message: &BotMessage,
    ctx: &Context,
    config: &Config,
    weekend: &Weekend,
) -> Result<(), Error> {
    let mut internal_message = ctx
        .http
        .get_message(config.discord.channel, message.discord_id)
        .await?;

    internal_message
        .edit(&ctx.http, |edit| edit.content(weekend.to_display()))
        .await?;

    Ok(())
}

pub async fn notify_session(
    ctx: &Context,
    config: &Config,
    session: &SessionType,
    weekend: &Weekend,
) -> Result<Option<BotMessage>, Error> {
    let channel = ctx.http.get_channel(config.discord.channel).await?;
    if let Some(channel) = channel.guild() {
        let msg = channel
            .send_message(&ctx, |new_message| {
                new_message.content(format!(
                    "**<@&{}> -- {} {} just started!**",
                    config.discord.role,
                    weekend.name,
                    session.short_name()
                ))
            })
            .await?;
        return Ok(Some(BotMessage::new_notification(msg.id.into())));
    }
    Ok(None)
}

pub async fn delete_notification(
    ctx: &Context,
    config: &Config,
    message: &BotMessage,
    messages: &Collection<BotMessage>,
) -> Result<(), Error> {
    if let BotMessageType::Notification(notification) = &message.kind {
        if Utc::now()
            .signed_duration_since(notification.time_sent)
            .num_minutes()
            < 30
        {
            return Ok(());
        }

        let msg = ctx
            .http
            .get_message(config.discord.channel, message.discord_id)
            .await?;
        msg.delete(ctx).await?;
        messages.delete_one(doc! { "_id": message.id }, None).await?;
    }
    Ok(())
}

pub async fn create_or_update_persistent_message(
    notifications: &Collection<BotMessage>,
    ctx: &Context,
    config: &Config,
    weekend: &Weekend,
) -> Result<BotMessage, Error> {
    let initial_message = get_persistent_message(notifications).await?;
    if let Some(message) = initial_message {
        update_persistent_message(&message, ctx, config, weekend).await?;
        Ok(message)
    } else {
        let new_message =
            create_persistent_message(ctx, config, weekend).await?;
        notifications.insert_one(new_message, None).await?;
        Ok(new_message)
    }
}

pub async fn remove_all_reactions(
    notifications: &Collection<BotMessage>,
    ctx: &Context,
    config: &Config,
) -> Result<(), Error> {
    let botmessage = get_persistent_message(notifications).await?;
    if let Some(botmessage) = botmessage {
        let internal_message = ctx
            .http
            .get_message(config.discord.channel, botmessage.discord_id)
            .await?;

        internal_message.delete_reactions(&ctx).await?;
    }

    Ok(())
}
