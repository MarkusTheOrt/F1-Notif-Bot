#![allow(dead_code)]
use chrono::{
    DateTime,
    Duration,
    Utc,
};
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    pub r#type: String,
    pub start: String,
    pub name: Option<String>,
    pub notified: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    /// Checks
    pub fn time_from_now(&self) -> Duration {
        self.start_time().signed_duration_since(Utc::now())
    }

    /// Checks if the session is too old to be considered.
    /// This is within a reasonable doubt.
    pub fn prolly_too_old(&self) -> bool {
        self.time_from_now().num_days() < -4
    }

    pub fn start_time(&self) -> DateTime<Utc> {
        self.start.parse::<DateTime<Utc>>().expect("Error Parsing datetime.")
    }

    pub fn next_session(&self) -> Option<&Session> {
        let mut best_session: Option<&Session> = None;
        for (_, session) in self
            .sessions
            .iter()
            .filter(|session| !session.notified.unwrap_or(false))
            .enumerate()
        {
            if session.time_from_now().num_minutes() > 0
                && best_session.is_none()
            {
                best_session = Some(session)
            }
            if best_session.is_some()
                && session.time_from_now()
                    < best_session.unwrap().time_from_now()
            {
                best_session = Some(session)
            }
        }
        best_session
    }
}

impl PartialEq for Session {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.r#type == other.r#type
    }
}

impl Session {
    pub fn time_from_now(&self) -> Duration {
        self.start_time().signed_duration_since(Utc::now())
    }

    pub fn start_time(&self) -> DateTime<Utc> {
        self.start.parse::<DateTime<Utc>>().expect("Error Parsing datetime")
    }
}
