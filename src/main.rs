pub mod config;
pub mod error;
pub mod util;

use error::{Error, Result};
use mongodb::{
    bson::{self, doc},
    Client,
};
use util::{
    database::{BotMessageType, WeekendState},
    helpers::create_or_update_persistent_message,
};

use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::{self, stdout, Read, Write},
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use config::Config;
use serenity::{
    async_trait,
    client::ClientBuilder,
    framework::StandardFramework,
    futures::StreamExt,
    model::prelude::*,
    prelude::{Context, EventHandler},
};

use crate::util::{
    database::{filter_current_weekend, BotMessage, Weekend},
    helpers::{delete_notification, get_persistent_message, notify_session},
};

struct Bot {
    is_mainthread_running: AtomicBool,
    pub config: Arc<Config>,
}

#[cfg(debug_assertions)]
async fn set_presence(ctx: &Context) {
    ctx.set_presence(
        Some(Activity::playing("Debug mode.")),
        OnlineStatus::Online,
    )
    .await;
}

#[cfg(not(debug_assertions))]
async fn set_presence(ctx: &Context) {}

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

            // Contrary to believe this isn't actually waiting for a establshed
            // connection but rather checking if all the options are good.
            if let Err(why) = database {
                println!("Error creating database client: {why}");
                exit(0x0100);
            }

            // Check if we actually are connected to a database server.
            println!("Connecting to database... please wait.");

            let database = database.unwrap();
            // by listing database names we actually have to await a server
            // connection.
            let database_check = database.list_database_names(None, None).await;
            if let Err(why) = database_check {
                println!("Error connecting to database: {why}");
                exit(0x0100);
            }
            println!("Connected to mongodb on {}", mongoconf.database);
            // Great, we are now connected!

            // Database setup, get two collections, one for all the weekends and
            // one for all the messages.
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
                            if let serenity::http::HttpError::UnsuccessfulRequest(why) = why.as_ref() {
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
                        weekend.hash(&mut hasher);
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
                                    if let Ok(Some(new_message)) = res {
                                        let _ = messages
                                            .insert_one(new_message, None)
                                            .await;
                                    }
                                }
                                // now post the f-ing thing.
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
                            }
                        }

                        if h != last_hash {
                            last_hash = h;
                            let error = create_or_update_persistent_message(
                                &messages, &_ctx, &conf, &weekend,
                            )
                            .await;
                            if error.is_err() {
                                println!("message does not exist");
                                exit(0x0100);
                            }
                        }
                    }
                    let messages_to_delete = messages.find(None, None).await;
                    if let Err(why) = messages_to_delete {
                        println!("Error getting messages to delete: {why}");
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
                            println!("Error removing msgs: {why}");
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
        println!(
            "Connected to discord as {}#{}",
            user.name, user.discriminator
        );
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
    let mut config_file = File::create("/config/config.toml")?;
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
                println!("Error generating config: {config_why}")
            }
        } else {
            println!("Error reading config file: {why}")
        }
    }

    let mut config = config.unwrap();
    let mut string = "".to_owned();
    if let Err(why) = config.read_to_string(&mut string) {
        println!("Error reading config file: {why}");
        return;
    }
    let config = toml::from_str::<Config>(string.as_str());
    if let Err(why) = &config {
        println!("Error parsing config file: {why}");
        return;
    }
    let config = config.unwrap();
    let bot = Bot {
        is_mainthread_running: AtomicBool::new(false),
        config: Arc::new(config),
    };
    let framework = StandardFramework::new();
    let client = ClientBuilder::new(
        &bot.config.discord.bot_token,
        GatewayIntents::non_privileged(),
    )
    .framework(framework)
    .event_handler(bot)
    .await;

    if let Err(why) = client {
        println!("Error creating Discord client: {why}");
        return;
    }

    let mut client = client.unwrap();
    let run = client.start().await;

    if let Err(why) = run {
        println!("Error occured while running the client: {why}");
        return;
    }

    println!("Shutting down.");
}
