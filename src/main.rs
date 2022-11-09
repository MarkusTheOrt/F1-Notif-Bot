mod commands;
mod util;

use std::{
    collections::HashSet,
    env,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
    },
    time::Duration,
};

use dotenvy::dotenv;
use serenity::{
    async_trait,
    framework::standard::{
        macros::group,
        StandardFramework,
    },
    http::Http,
    model::{
        application::command::Command,
        prelude::*,
        user::OnlineStatus,
    },
    prelude::*,
};
use util::{
    database::{
        get_database,
        DatabaseHandle,
        DbHandle,
        Weekend,
    },
    tools::{
        best_weekend,
        filter_weekends,
    },
};

use mongodb::bson::doc;

#[group]
struct General;

struct Bot {
    is_watcher_running: AtomicBool,
    is_deleter_running: AtomicBool,
    is_permanent_message_running: AtomicBool,
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(
        &self,
        ctx: Context,
        _ready: Ready,
    ) {
        println!("Connected!");
        ctx.set_presence(
            Some(Activity::watching("out for new sessions.")),
            OnlineStatus::Online,
        )
        .await;
        if let Err(why) =
            Command::set_global_application_commands(&ctx.http, |commands| {
                commands
                    .create_application_command(|f| commands::ping::register(f))
            })
            .await
        {
            println!("Error Registering Global Commands: {}", why);
        }
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
        ctx: Context,
        _guilds: Vec<GuildId>,
    ) {
        println!("Cache built and populated.");

        let ctx = Arc::new(ctx);

        if !self.is_watcher_running.load(Ordering::Relaxed) {
            println!("Notifications service started.");
            let ctx1 = Arc::clone(&ctx);

            tokio::spawn(async move {
                let db = get_database(ctx1.clone()).await;
                println!("dbName: {}", db.db.name());
                loop {
                    if let Ok(mut cur) = db.weekends.find(doc! {}, None).await {
                        let wk = filter_weekends(&mut cur).await;
                        if let Some(weekend) = best_weekend(&wk) {
                            if let Some(sess) = weekend.next_session() {
                                println!(
                                    "Next Session: {} [{}]: {} (in {} minutes)",
                                    weekend.name,
                                    sess.r#type,
                                    sess.start,
                                    sess.time_from_now().num_minutes()
                                );
                            }
                        } else {
                            println!("Couldn't find weekends");
                        }
                    } else {
                        println!("Not OK!");
                    }
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });

            self.is_watcher_running.swap(true, Ordering::Relaxed);
        }

        if !self.is_deleter_running.load(Ordering::Relaxed) {
            println!("Deleter service started.");
            let _ctx1 = Arc::clone(&ctx);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(120)).await;
                }
            });

            self.is_deleter_running.swap(true, Ordering::Relaxed);
        }

        if !self.is_permanent_message_running.load(Ordering::Relaxed) {
            println!("Permanent Message service started.");
            let _ctx1 = Arc::clone(&ctx);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60 * 5)).await;
                }
            });

            self.is_permanent_message_running.swap(true, Ordering::Relaxed);
        }
    }

    async fn resume(
        &self,
        _: Context,
        _: ResumedEvent,
    ) {
    }
}

#[tokio::main]
async fn main() {
    if let Err(why) = dotenv() {
        println!("Couldn't find .env file: {}", why);
        if env::var("DISCORD_TOKEN").is_err() {
            println!("Couldn't read DISCORD_TOKEN env variable.");
            return;
        }
        if env::var("MONGO_URL").is_err() {
            println!("Couldn't read MONGO_URL env variable");
            return;
        }
    }
    let token = env::var("DISCORD_TOKEN").unwrap();
    let http = Http::new(&token);
    let database = mongodb::Client::with_uri_str("mongodb://localhost:27017/")
        .await
        .expect("Error Creating Mongodb Client");
    let conn = Arc::new(database.database("f1-notif-bot"));
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        },
        Err(why) => panic!("Couldn't access application info: {:?}", why),
    };

    let framework = StandardFramework::new()
        .configure(|c| c.owners(owners))
        .group(&GENERAL_GROUP);

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::GUILDS;
    let mut client = Client::builder(token, intents)
        .event_handler(Bot {
            is_deleter_running: AtomicBool::new(false),
            is_permanent_message_running: AtomicBool::new(false),
            is_watcher_running: AtomicBool::new(false),
        })
        .framework(framework)
        .await
        .expect("Error creating Client");
    {
        let mut data = client.data.write().await;
        data.insert::<DatabaseHandle>(Arc::new(DbHandle {
            client: Arc::new(database),
            db: conn.clone(),
            messages: Arc::new(conn.collection("messages")),
            weekends: Arc::new(conn.collection("weekends")),
            settings: Arc::new(conn.collection("settings")),
        }));
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Couldn't register <ctrl><C> handler");
        });
    }
    if let Err(why) = client.start().await {
        println!("Client Error: {:?}", why);
    }
}
