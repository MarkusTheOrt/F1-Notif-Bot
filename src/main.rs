pub mod config;
pub mod error;
pub mod util;

use error::{
    Error,
    Result,
};
use mongodb::{
    bson::{
        self,
        doc,
    },
    Client,
};
use util::{
    database::{
        BotMessageType,
        WeekendState,
    },
    helpers::create_or_update_persistent_message,
};

use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{
        Hash,
        Hasher,
    },
    io::{
        self,
        Read,
        Write,
    },
    process::exit,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
    },
    time::Duration,
};

use config::Config;
use serenity::{
    async_trait,
    client::ClientBuilder,
    futures::StreamExt,
    model::prelude::*,
    prelude::{
        Context,
        EventHandler,
    },
};

use crate::util::*;

struct Bot {
    is_mainthread_running: AtomicBool,
    pub config: Arc<Config>,
}

#[cfg(debug_assertions)]
async fn set_presence(ctx: &Context) {
    use serenity::gateway::ActivityData;

    ctx.set_activity(Some(ActivityData::watching("out for new sessions.")));
}

#[cfg(not(debug_assertions))]
async fn set_presence(_ctx: &Context) {}

#[async_trait]
impl EventHandler for Bot {
    async fn cache_ready(
        &self,
        _ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }
        set_presence(&_ctx).await;

        let conf = self.config.clone();

        self.is_mainthread_running.swap(true, Ordering::Relaxed);
        tokio::spawn(async move {
            println!("Started Watcher thread.");
            let mongoconf = &conf.mongo;
            let database = Client::with_uri_str(format!(
                "{}://{}:{}@{}/{}{}",
                mongoconf.protocol,
                mongoconf.user,
                mongoconf.password,
                mongoconf.url,
                mongoconf.database,
                mongoconf.suffix
            ))
            .await;

            if let Err(why) = database {
                println!("Error creating database client: {why}");
                exit(0x0100);
            }
            let database = database.unwrap();
            // Check if we actually are connected to a database server.
            println!("Connecting to database... please wait.");

            // by listing database names we actually have to await a server,
            // otherwise the driver only connects on the first call
            // to database.
            let database_check = database.list_database_names(None, None).await;
            if let Err(why) = database_check {
                println!("Error connecting to database: {why}");
                exit(0x0100);
            }

            let db = database.database(mongoconf.database.as_str());
            let sessions = db.collection::<Weekend>("weekends");
            let messages = db.collection::<BotMessage>("messages");

            let mut message = get_persistent_message(&messages).await;
            let weekend = filter_current_weekend(&sessions).await;
            if let Ok(weekend) = weekend {
                if let (Ok(None), Some(weekend)) = (&message, weekend) {
                    let res = create_or_update_persistent_message(
                        &messages, &_ctx, &conf, &weekend,
                    )
                    .await;
                    if let Err(why) = &res {
                        println!("Error sending or updating message: {why}");

                        if let Error::Serenity(serenity::Error::Http(why)) = why
                        {
                            if let serenity::http::HttpError::UnsuccessfulRequest(why) = why {
                                println!("{}", why.error.code);
                            }
                        }
                    } else if let Ok(new_or_updated_mesasge) = res {
                        message = Ok(Some(new_or_updated_mesasge));
                    }
                }

                let mut last_hash: u64 = if let Ok(Some(msg)) = message {
                    if let BotMessageType::Persistent(persistent_message) =
                        msg.kind
                    {
                        persistent_message.hash
                    } else {
                        0
                    }
                } else {
                    0
                };
                loop {
                    // We wait for the first time in the loop to make continues
                    // easier.
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let mut hasher = DefaultHasher::new();
                    let weekend = filter_current_weekend(&sessions).await;
                    if let Err(why) = &weekend {
                        println!("Error finding Weekend: {why}");
                        continue;
                    }

                    if let Ok(Some(mut weekend)) = weekend {
                        weekend.to_display().hash(&mut hasher);
                        let h = hasher.finish();
                        let sess = weekend.get_next_session();

                        if let WeekendState::CurrentSession(index, _session) =
                            sess
                        {
                            if let Some(sess) = weekend.sessions.get_mut(index)
                            {
                                sess.set_modified();
                                let update = bson::to_bson(&weekend);
                                if let Ok(doc) = update {
                                    let _ = sessions
                                        .update_one(
                                            doc! { "_id": weekend.id },
                                            doc! { "$set": doc },
                                            None,
                                        )
                                        .await;
                                    let res = notify_session(
                                        &_ctx, &conf, &_session, &weekend,
                                    )
                                    .await;
                                    match res {
                                        Ok(Some(new_message)) => {
                                            let _ = messages
                                                .insert_one(new_message, None)
                                                .await;
                                        },
                                        Ok(None) => {},
                                        Err(why) => {
                                            eprintln!("Error posting message: \n\t`{why}`");
                                        },
                                    }
                                }
                            }
                        } else if let WeekendState::None = sess {
                            weekend.done = true;
                            let update = bson::to_bson(&weekend);
                            if let Ok(doc) = update {
                                let _ = sessions
                                    .update_one(
                                        doc! { "_id": weekend.id },
                                        doc! { "$set": doc },
                                        None,
                                    )
                                    .await;
                                if delete_persistent_message(
                                    &messages, &_ctx, &conf,
                                )
                                .await
                                .is_ok()
                                    && remove_persistent_bot_message(&messages)
                                        .await
                                        .is_ok()
                                {
                                    last_hash = 1337;
                                    continue;
                                }
                            }
                        }

                        if h != last_hash {
                            last_hash = h;
                            let error = create_or_update_persistent_message(
                                &messages, &_ctx, &conf, &weekend,
                            )
                            .await;
                            if let Err(why) = error {
                                eprintln!(
                                    "Error: Message does not exit: \n\t`{why}`"
                                );
                                exit(0x0100);
                            };
                        }
                    }
                    let messages_to_delete = messages.find(None, None).await;
                    if let Err(why) = messages_to_delete {
                        eprintln!(
                            "Error getting messages to delete: \n\t`{why}`"
                        );
                        continue;
                    }
                    let mut messages_to_delete = messages_to_delete.unwrap();
                    while let Some(Ok(message)) =
                        messages_to_delete.next().await
                    {
                        let res = delete_notification(
                            &_ctx, &conf, &message, &messages,
                        )
                        .await;
                        if let Err(why) = res {
                            eprintln!("Error removing msgs: `{why}`");
                        }
                    }
                }
            }
        });
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        _deleted_message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
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

    async fn resume(
        &self,
        _: Context,
        _: ResumedEvent,
    ) {
    }
}

fn generate_default_config() -> Result<()> {
    let config = Config::default();
    let str_to_write = toml::to_string_pretty(&config)?;
    let mut config_file = File::create("./config/config.toml")?;
    config_file.write_all(str_to_write.as_bytes())?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let config = File::open("./config/config.toml");

    if let Err(why) = &config {
        if let io::ErrorKind::NotFound = why.kind() {
            println!("Generated default config file, please update settings.");
            if let Err(config_why) = generate_default_config() {
                eprintln!("Error generating config: `{config_why}`")
            }
            exit(0x0100)
        } else {
            eprintln!("Error reading config file: {why}");
            exit(0x0100)
        }
    }

    let mut config = config.unwrap();
    let mut string = "".to_owned();
    if let Err(why) = config.read_to_string(&mut string) {
        eprintln!("Error reading config file: \n\t`{why}`");
        return;
    }
    let config = toml::from_str::<Config>(string.as_str());
    if let Err(why) = &config {
        eprintln!("Error parsing config file: \n\t`{why}`");
        return;
    }
    let config = config.unwrap();
    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config: Arc::new(config),
    };
    let client = ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .event_handler(bot)
    .await;

    if let Err(why) = client {
        eprintln!("Error creating Discord client: \n\t`{why}`");
        return;
    }

    let mut client = client.unwrap();

    if let Err(why) = client.start().await {
        eprintln!("Error occured while running the client: \n\t`{why}`");
        return;
    }

    println!("Shutting down.");
}
