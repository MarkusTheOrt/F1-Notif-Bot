pub mod calendar;
pub mod notifs;

use crate::{
    config::Config,
    util::{check_expired_messages, delete_message, expired_messages},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use serenity::{
    all::{ChannelId, GuildId, MessageId, Ready, StatusCode},
    async_trait,
    prelude::*,
};

use sqlx::MySqlConnection;
use tracing::{error, info, warn};

pub struct Bot {
    pub is_mainthread_running: AtomicBool,
    pub config: &'static Config<'static>,
    pub database: &'static sqlx::MySqlPool,
    pub cat: &'static [u8],
}

#[cfg(debug_assertions)]
fn set_presence(ctx: &Context) {
    use serenity::gateway::ActivityData;

    ctx.set_activity(Some(ActivityData::watching("out for new sessions.")));
}

#[cfg(not(debug_assertions))]
fn set_presence(_ctx: &Context) {}

#[async_trait]
impl EventHandler for Bot {
    async fn cache_ready(
        &self,
        ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        // prevent double-starting threads
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }
        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        set_presence(&ctx);

        let pool = self.database.clone();
        let http = ctx.http.clone();
        let conf = self.config;
        let cat = self.cat;
        let mut db_conn = pool.acquire().await.unwrap();

        tokio::spawn(async move {
            loop {
                if let Err(why) =
                    check_expired_messages(db_conn.as_mut(), &http).await
                {
                    error!("{why}");
                };
            }
        });
    }

    async fn ready(
        &self,
        _ctx: Context,
        ready: Ready,
    ) {
        let user = &ready.user;
        if let Some(discriminator) = user.discriminator {
            info!("Connected as {}#{}", user.name, discriminator);
        } else {
            info!("Connected to discord as {}", user.name);
        }
    }
}
