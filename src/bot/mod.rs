pub mod calendar;
pub mod notifs;

use crate::{
    config::Config,
    util::{
        check_active_session, check_expired_messages, create_calendar,
        create_new_notifications_msg_db, edit_calendar, mark_session_done,
        send_notification,
    },
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use f1_bot_types::Series;
use serenity::{
    all::{GuildId, Ready},
    async_trait,
    prelude::*,
};

use tracing::{error, info};

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
            let mut last_invocation = Instant::now();
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                // This gives us the ability to abort the task if we want or need to.
                tokio::task::yield_now().await;
                if let Err(why) =
                    check_expired_messages(db_conn.as_mut(), &http).await
                {
                    error!("{why:#?}");
                }

                if Instant::now().duration_since(last_invocation).as_secs()
                    > 60 * 5
                {
                    last_invocation = Instant::now();
                    info!("Doing Calendar");
                    for val in Series::F1.i8()..=Series::F1Academy.i8() {
                        let series: Series = val.into();
                        if let Err(why) = create_calendar(
                            db_conn.as_mut(),
                            &http,
                            val.into(),
                            conf.channel(series),
                        )
                        .await
                        {
                            error!("{why}");
                        } else {
                            info!("Created {series} Calendar");
                        }

                        if let Err(why) =
                            edit_calendar(db_conn.as_mut(), &http, series).await
                        {
                            error!("{why:#?}");
                        }
                    }
                }
                for val in Series::F1.i8()..=Series::F1Academy.i8() {
                    let series: Series = val.into();
                    let role = conf.role(series);
                    let channel = conf.channel(series);
                    let session = match check_active_session(
                        db_conn.as_mut(),
                        series,
                    )
                    .await
                    {
                        Ok(s) => s,
                        Err(why) => {
                            error!("{why:#?}");
                            continue;
                        },
                    };
                    if let Some((w, s)) = session {
                        let msg_id = match send_notification(
                            &http,
                            &w,
                            &s,
                            channel,
                            cat,
                            role
                        )
                        .await
                        {
                            Ok(d) => d,
                            Err(why) => {
                                error!("{why:#?}");
                                continue;
                            },
                        };
                        if let Err(why) =
                            mark_session_done(db_conn.as_mut(), &s).await
                        {
                            error!("{why:#?}");
                        }
                        if let Err(why) = create_new_notifications_msg_db(
                            db_conn.as_mut(),
                            &s,
                            series,
                            channel,
                            msg_id.into(),
                        )
                        .await
                        {
                            error!("{why:#?}");
                        }
                    }
                }
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

