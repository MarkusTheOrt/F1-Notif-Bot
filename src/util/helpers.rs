use std::{
    collections::hash_map::DefaultHasher,
    hash::{
        Hash,
        Hasher,
    },
    io,
};

use chrono::Utc;
use mongodb::{
    bson::doc,
    Collection,
};
use serenity::{
    self,
    all::ChannelId,
    builder::{
        CreateAttachment,
        CreateMessage,
        EditMessage,
    },
    futures::StreamExt,
    prelude::Context,
};

use crate::{
    config::Config,
    error::Error,
    util::database::BotMessageType,
};

use super::database::{
    BotMessage,
    DiscordString,
    SessionType,
    Weekend,
};

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
            let channel = ChannelId::new(config.discord.channel);
            channel.delete_message(ctx, message.discord_id).await?;
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
    let channel = ChannelId::new(config.discord.channel);
    let new_message = CreateMessage::default().content(weekend_as_string);
    let msg = channel.send_message(ctx, new_message).await?;

    let mut hasher = DefaultHasher::new();
    weekend.hash(&mut hasher);
    Ok(BotMessage::new_persistent(msg.id.into(), hasher.finish()))
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
    let channel = ChannelId::new(config.discord.channel);
    let edit_message = EditMessage::default().content(weekend.to_display());
    channel.edit_message(ctx, message.discord_id, edit_message).await?;

    Ok(())
}

pub async fn notify_session(
    ctx: &Context,
    config: &Config,
    session: &SessionType,
    weekend: &Weekend,
) -> Result<Option<BotMessage>, Error> {
    let channel = ChannelId::new(config.discord.channel);
    let bongocat = CreateAttachment::path("./config/cats.mp4").await?;
    let new_message = CreateMessage::default()
        .content(format!(
            "**<@&{}> -- {} {} just started!**",
            config.discord.role,
            weekend.name,
            session.short_name()
        ))
        .add_file(bongocat);

    let notification = channel.send_message(ctx, new_message).await?;
    return Ok(Some(BotMessage::new_notification(notification.id.into())));
}

pub async fn remove_persistent_message(
    ctx: &Context,
    config: &Config,
    messages: &Collection<BotMessage>,
) -> Result<(), Error> {
    let persistent_message = get_persistent_message(messages).await?;
    if let Some(persistent_message) = persistent_message {
        let channel = ChannelId::new(config.discord.channel);
        channel.delete_message(ctx, persistent_message.discord_id).await?;
        return Ok(());
    }

    Err(Error::Io(io::Error::new(io::ErrorKind::InvalidData, "test")))
}

pub async fn remove_persistent_bot_message(
    messages: &Collection<BotMessage>
) -> Result<(), Error> {
    let msg = get_persistent_message(messages).await?;
    if let Some(msg) = msg {
        messages
            .delete_one(
                doc! {
                    "_id": msg.id
                },
                None,
            )
            .await?;
        return Ok(());
    }

    Err(Error::Io(io::Error::new(io::ErrorKind::InvalidData, "test")))
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
        let channel = ChannelId::new(config.discord.channel);
        channel.delete_message(ctx, message.discord_id).await?;
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
