pub mod calendar;
pub mod notifs;

use crate::{bot::notifs::remove_old_notifs, config::Config, model::Series};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serenity::{
    all::{GuildId, Ready},
    async_trait,
    prelude::*,
};
use sqlx::MySqlPool;

use self::{
    calendar::{populate_calendar, update_calendar},
    notifs::runner,
};

pub struct Bot {
    pub is_mainthread_running: AtomicBool,
    pub config: &'static Config<'static>,
    pub database: sqlx::MySqlPool,
    pub cat: &'static [u8],
}

#[cfg(debug_assertions)]
fn set_presence(ctx: &Context) {
    use serenity::gateway::ActivityData;

    ctx.set_activity(Some(ActivityData::watching("out for new sessions.")));
}

#[cfg(not(debug_assertions))]
async fn set_presence(_ctx: &Context) {}

#[tokio::main]
async fn main_runner(
    pool: &MySqlPool,
    conf: &Config,
    http: impl CacheHttp,
) {
    loop {
        let _ = populate_calendar(
            pool,
            http.http(),
            conf.discord.f1_channel,
            Series::F1,
        )
        .await;
        /*let _ = populate_calendar(
            pool,
            http.http(),
            conf.discord.f2_channel,
            Series::F2,
        )
        .await;

        let _ = populate_calendar(
            pool,
            http.http(),
            conf.discord.f3_channel,
            Series::F3,
        )
        .await;

        let _ = populate_calendar(
            pool,
            http.http(),
            conf.discord.f1a_channel,
            Series::F1Academy,
        )
        .await;*/

        let _ = update_calendar(
            pool,
            http.http(),
            conf.discord.f1_channel,
            Series::F1,
        )
        .await;

        /*  let _ = update_calendar(
            pool,
            http.http(),
            conf.discord.f2_channel,
            Series::F2,
        )
        .await;

        let _ = update_calendar(
            pool,
            http.http(),
            conf.discord.f3_channel,
            Series::F3,
        )
        .await;

        let _ = update_calendar(
            pool,
            http.http(),
            conf.discord.f1a_channel,
            Series::F1Academy,
        )
        .await;*/
        // update calendar every 15 minutes
        std::thread::sleep(Duration::from_secs(60 * 15));
    }
}

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
        set_presence(&ctx);

        let pool_1 = self.database.clone();
        let http = ctx.http.clone();
        let conf = self.config;
        let cat = self.cat;
        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        let pool_2 = self.database.clone();
        std::thread::spawn(move || {
            main_runner(&pool_2, conf, ctx);
        });
        tokio::spawn(async move {
            let mut f1_wknd_id = 0u32;
            let mut f2_wknd_id = 0u32;
            let mut f3_wknd_id = 0u32;
            let mut f1a_wknd_id = 0u32;
            loop {
                tokio::join!(
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f1_channel,
                        conf.discord.f1_role,
                        crate::model::Series::F1,
                        cat,
                        &mut f1_wknd_id
                    ),
                    /*     runner(
                        &pool_1,
                        &http,
                        conf.discord.f2_channel,
                        conf.discord.f2_role,
                        crate::model::Series::F2,
                        cat,
                        &mut f2_wknd_id
                    ),
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f3_channel,
                        conf.discord.f3_role,
                        crate::model::Series::F3,
                        cat,
                        &mut f3_wknd_id
                    ),
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f1a_channel,
                        conf.discord.f1a_role,
                        crate::model::Series::F1Academy,
                        cat,
                        &mut f1a_wknd_id
                    )*/
                );

                if let Err(why) = remove_old_notifs(&pool_1, &http).await {
                    eprintln!("Error removing old notifs: {why}");
                }
                std::thread::sleep(Duration::from_secs(5));
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
            println!("Connected as {}#{}", user.name, discriminator);
        } else {
            println!("Connected to discord as {}", user.name);
        }
    }
}
