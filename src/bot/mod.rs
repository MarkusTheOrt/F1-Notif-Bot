pub mod calendar;
pub mod notifs;

use crate::{
    config::Config,
    util::{
        fetch_next_full_weekend_for_series, fetch_weekend_message_for_series,
        insert_weekend_message, post_weekend_message, update_message_hash, update_weekend_message,
    },
};
use std::{
    hash::Hasher,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use f1_bot_types::Series;
use serenity::{
    all::{GuildId, Http, Ready},
    async_trait,
    prelude::*,
};

use tracing::info;

pub struct Bot {
    pub is_mainthread_running: AtomicBool,
    pub config: &'static Config<'static>,
    pub database: &'static libsql::Database,
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
    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        // prevent double-starting threads
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }
        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        set_presence(&ctx);

        let pool = self.database;
        let http = ctx.http.clone();
        let conf = self.config;
        let cat = self.cat;
        tokio::spawn(async move { bot_loop(pool, http, conf, cat) });
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        let user = &ready.user;
        if let Some(discriminator) = user.discriminator {
            info!("Connected as {}#{}", user.name, discriminator);
        } else {
            info!("Connected to discord as {}", user.name);
        }
    }
}

async fn bot_loop(
    db_pool: &'static libsql::Database,
    http: Arc<Http>,
    config: &'static Config<'static>,
    _cat_video: &'static [u8],
) -> Result<(), crate::error::Error> {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut db_conn = db_pool.connect()?;
        let mut _weekends: (u64, (u64, u64, u64)) = (0, (0, 0, 0));

        let Some(next_full_f1_weekend) =
            fetch_next_full_weekend_for_series(&mut db_conn, Series::F1).await?
        else {
            continue;
        };

        let f1_weekend_hash = quick_hash(&next_full_f1_weekend);

        let f1_weekend_message = fetch_weekend_message_for_series(&mut db_conn, Series::F1).await?;
        if f1_weekend_message.is_none() {
            let new_message_id = post_weekend_message(
                &http,
                &next_full_f1_weekend,
                config.channel(Series::F1),
                Series::F1,
            )
            .await?;
            insert_weekend_message(
                &mut db_conn,
                config.channel(Series::F1),
                new_message_id.get(),
                &next_full_f1_weekend,
            )
            .await?;
        }
        if let Some(message) = f1_weekend_message
            && message
                .hash
                .is_some_and(|f| f != f1_weekend_hash.to_string())
        {
            update_weekend_message(
                &http,
                &next_full_f1_weekend,
                config.channel(Series::F1),
                message.message.parse()?,
            )
            .await?;
            update_message_hash(&mut db_conn, message.id, f1_weekend_hash).await?;
        }
    }
}

fn quick_hash(to_hash: impl std::hash::Hash) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    to_hash.hash(&mut hasher);
    hasher.finish()
}
