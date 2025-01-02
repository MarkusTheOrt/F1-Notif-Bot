pub mod calendar;
pub mod notifs;

use crate::{
    config::Config,
    util::{
        check_expired_messages, check_expired_weekend, create_calendar,
        create_new_notifications_msg_db, edit_calendar,
        fetch_next_full_weekend_for_series, fetch_weekend_message_for_series,
        insert_weekend_message, mark_message_expired, mark_session_done,
        mark_weekend_done, mark_weekend_message_for_series_expired,
        post_weekend_message, send_notification, update_message_hash,
        update_weekend_message,
    },
};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
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
            let mut last_weekend_ids = [0, 0, 0, 0u64];
            let mut last_invocation = Instant::now();
            loop {
                info!("LWIs: {last_weekend_ids:?}");
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
                    #[allow(unused)]
                    let last_weekend_id = &mut last_weekend_ids[val as usize];
                    let full_weekend = match fetch_next_full_weekend_for_series(
                        db_conn.as_mut(),
                        series,
                    )
                    .await
                    {
                        Ok(Some(d)) => d,
                        Ok(None) => {
                            let weekend_msg = match fetch_weekend_message_for_series(db_conn.as_mut(), series).await {
                                Ok(Some(msg)) => msg,
                                Ok(None) => continue,
                                Err(why) => {
                                    error!("{why:#?}");
                                    continue;
                                }
                            };
                            if let Err(why) = mark_message_expired(db_conn.as_mut(), weekend_msg.id, None).await {
                                error!("{why:#?}");
                            }
                            continue;
                        },
                        Err(why) => {
                            error!("{why:#?}");
                            continue;
                        },
                    };
                    if *last_weekend_id == 0 {
                        *last_weekend_id = full_weekend.weekend.id;
                    }
                    if full_weekend.is_done() {
                            if let Err(why) = mark_weekend_done(
                                db_conn.as_mut(),
                                &full_weekend.weekend,
                            )
                            .await
                            {
                                error!("{why:#?}");
                                continue;
                            }
                            if let Err(why) =
                                mark_weekend_message_for_series_expired(
                                    db_conn.as_mut(),
                                    series,
                                )
                                .await
                            {
                                error!("{why:#?}");
                            }
                    }

                    match fetch_weekend_message_for_series(
                        db_conn.as_mut(),
                        series,
                    )
                    .await
                    {
                        Ok(Some(msg)) => {
                            if let Some(hash) = msg.hash {
                                let mut hasher = DefaultHasher::new();
                                full_weekend.hash(&mut hasher);
                                let new_hash = hasher.finish();
                                if new_hash != hash.parse::<u64>().unwrap() {
                                    if *last_weekend_id
                                        != full_weekend.weekend.id
                                    {
                                        if let Err(why) = mark_message_expired(
                                            db_conn.as_mut(),
                                            msg.id,
                                            None,
                                        )
                                        .await
                                        {
                                            error!("{why:#?}");
                                        }
                                        *last_weekend_id =
                                            full_weekend.weekend.id;
                                        continue;
                                    }
                                    if let Err(why) = update_weekend_message(
                                        &http,
                                        &full_weekend,
                                        channel,
                                        msg.message.parse().unwrap(),
                                    )
                                    .await
                                    {
                                        error!("{why:#?}");
                                    }
                                    if let Err(why) = update_message_hash(
                                        db_conn.as_mut(),
                                        msg.id,
                                        new_hash,
                                    )
                                    .await
                                    {
                                        error!("{why:#?}");
                                    }
                                }
                            } else {
                                if *last_weekend_id != full_weekend.weekend.id {
                                    if let Err(why) = mark_message_expired(
                                        db_conn.as_mut(),
                                        msg.id,
                                        None,
                                    )
                                    .await
                                    {
                                        error!("{why:#?}");
                                    }
                                    *last_weekend_id = full_weekend.weekend.id;
                                    continue;
                                }
                                if let Err(why) = update_weekend_message(
                                    &http,
                                    &full_weekend,
                                    channel,
                                    msg.message.parse().unwrap(),
                                )
                                .await
                                {
                                    error!("{why:#?}");
                                }
                            }
                        },
                        Ok(None) => {
                            match post_weekend_message(
                                &http,
                                &full_weekend,
                                channel,
                            )
                            .await
                            {
                                Ok(msg) => {
                                    if let Err(why) = insert_weekend_message(
                                        db_conn.as_mut(),
                                        channel,
                                        msg.into(),
                                        &full_weekend,
                                    )
                                    .await
                                    {
                                        error!("{why:#?}");
                                    }
                                },
                                Err(why) => error!("{why:#?}"),
                            }
                        },
                        Err(why) => {
                            error!("{why:#?}");
                        },
                    }

                    let session = match full_weekend.next_session() {
                        Some(s) => s,
                        None => continue,
                    };
                    let msg_id = match send_notification(
                        &http,
                        &full_weekend.weekend,
                        session,
                        channel,
                        cat,
                        role,
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
                        mark_session_done(db_conn.as_mut(), session).await
                    {
                        error!("{why:#?}");
                    }
                    if let Err(why) = create_new_notifications_msg_db(
                        db_conn.as_mut(),
                        session,
                        series,
                        channel,
                        msg_id.into(),
                    )
                    .await
                    {
                        error!("{why:#?}");
                    }
                    if full_weekend.check_is_done(session) {
                            if let Err(why) = mark_weekend_done(
                                db_conn.as_mut(),
                                &full_weekend.weekend,
                            )
                            .await
                            {
                                error!("{why:#?}");
                                continue;
                            }
                            if let Err(why) =
                                mark_weekend_message_for_series_expired(
                                    db_conn.as_mut(),
                                    series,
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
