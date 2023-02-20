pub mod config;
pub mod error;
pub mod util;

use error::Result;
use mongodb::Client;
use util::{
    database::BotMessageType,
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
    framework::StandardFramework,
    model::prelude::*,
    prelude::{
        Context,
        EventHandler,
    },
};

use crate::util::{
    database::{
        filter_current_weekend,
        BotMessage,
        Weekend,
    },
    helpers::{
        create_persistent_message,
        get_persistent_message,
    },
};

struct Bot {
    is_mainthread_running: AtomicBool,
    pub config: Arc<Config>,
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(
        &self,
        _ctx: Context,
        ready: Ready,
    ) {
        let user = &ready.user;
        println!("Client connected as {}#{}", user.name, user.discriminator);
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        _deleted_message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
    }

    async fn cache_ready(
        &self,
        _ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        if self.is_mainthread_running.load(Ordering::Relaxed) {
            return;
        }

        self.is_mainthread_running.swap(true, Ordering::Relaxed);

        let conf = self.config.clone();

        tokio::spawn(async move {
            println!("Started Watcher thread.");
            let mongoconf = &conf.mongo;
            let database = Client::with_uri_str(format!(
                "mongodb://{}:{}@{}/{}",
                mongoconf.database_user,
                mongoconf.database_password,
                mongoconf.database_url,
                mongoconf.database_name
            ))
            .await;

            if let Err(why) = database {
                println!("Error connecting to database: {why}");
                return;
            }
            let database = database.unwrap();
            println!("Connected to mongodb on {}", mongoconf.database_url);
            let db = database.database(mongoconf.database_name.as_str());
            let sessions = db.collection::<Weekend>("weekends");
            let messages = db.collection::<BotMessage>("messages");
            let mut message = get_persistent_message(&messages).await;
            let weekend = filter_current_weekend(&sessions).await;
            if let Ok(weekend) = weekend {
                if let (Ok(None), Some(weekend)) = (&message, weekend) {
                    let res =
                        create_persistent_message(&_ctx, &conf, &weekend).await;
                    if let Ok(new_message) = &res {
                        message = Ok(Some(*new_message));
                        let inserted_message =
                            messages.insert_one(new_message, None).await;
                        println!("{inserted_message:#?}")
                    }
                    println!("{res:#?}");
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
                    let mut hasher = DefaultHasher::new();
                    let wknd = filter_current_weekend(&sessions).await;
                    if let Err(why) = &wknd {
                        println!("Error finding Weekend: {why}");
                    }
                    let wknd = wknd.unwrap();

                    if let Some(wknd) = wknd {
                        wknd.hash(&mut hasher);
                        let h = hasher.finish();
                        if h != last_hash {
                            last_hash = h;
                            let error = create_or_update_persistent_message(
                                &messages, &_ctx, &conf, &wknd,
                            )
                            .await;

                            if let Err(why) = error {
                                println!("Error: {why}");
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }
        });
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
    let mut config_file = File::create("./config.toml")?;
    config_file.write_all(str_to_write.as_bytes())?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let config = File::open("./config.toml");

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
