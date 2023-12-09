pub mod calendar;
pub mod notifs;

use crate::{config::Config, model::Series};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serenity::{
    all::{GuildId, Ready},
    async_trait,
    prelude::*,
};

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

#[async_trait]
impl EventHandler for Bot {
    async fn cache_ready(
        &self,
        ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }
        set_presence(&ctx);

        let _ = populate_calendar(
            &self.database,
            &ctx.http,
            self.config.discord.f1_channel,
            Series::F1,
        )
        .await;
        let t = update_calendar(
            &self.database,
            &ctx.http,
            self.config.discord.f1_channel,
            Series::F1,
        )
        .await;

        println!("{:#?}", t);

        let pool_1 = self.database.clone();
        let http = ctx.http.clone();
        let conf = self.config;
        let cat = self.cat;
        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        tokio::spawn(async move {
            loop {
                tokio::join!(
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f1_channel,
                        conf.discord.f1_role,
                        crate::model::Series::F1,
                        cat,
                    ),
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f2_channel,
                        conf.discord.f2_role,
                        crate::model::Series::F2,
                        cat,
                    ),
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f3_channel,
                        conf.discord.f3_role,
                        crate::model::Series::F3,
                        cat,
                    ),
                    runner(
                        &pool_1,
                        &http,
                        conf.discord.f1a_channel,
                        conf.discord.f1a_role,
                        crate::model::Series::F1Academy,
                        cat,
                    )
                );

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
