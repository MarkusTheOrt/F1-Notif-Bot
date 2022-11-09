use chrono::{DateTime, Utc, Duration};
use mongodb::{
    bson::oid::ObjectId,
    Client,
    Collection,
    Database,
};
use serde::{
    Deserialize,
    Serialize,
};
use serenity::prelude::{
    Context,
    TypeMapKey,
};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
pub struct Session {
    pub r#type: String,
    pub start: String,
    pub name: Option<String>,
    pub notified: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Weekend {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub year: i32,
    pub prefix: String,
    pub start: String,
    pub done: Option<bool>,
    pub sessions: Vec<Session>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub _id: ObjectId,
    pub weekend: ObjectId,
    pub session: i32,
    pub message: String,
    pub channel: String,
    pub date: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Setting<T> {
    pub name: String,
    pub value: Option<T>,
}

pub struct DbHandle {
    pub client: Arc<Client>,
    pub db: Arc<Database>,
    pub weekends: Arc<Collection<Weekend>>,
    pub messages: Arc<Collection<Message>>,
    pub settings: Arc<Collection<Setting<String>>>,
}

pub struct DatabaseHandle {}

impl TypeMapKey for DatabaseHandle {
    type Value = Arc<DbHandle>;
}

pub async fn get_database(ctx: Arc<Context>) -> Arc<DbHandle> {
    let d = ctx.data.read().await;
    d.get::<DatabaseHandle>()
        .expect("Error retrieving Database Handler")
        .clone()
}


impl Weekend {
    pub fn time_from_now(&self) -> Duration {
        self.start_time().signed_duration_since(Utc::now())
    }

    pub fn start_time(&self) -> DateTime<Utc> {
        self.start.parse::<DateTime<Utc>>().expect("Error Parsing datetime.")
    }
}
