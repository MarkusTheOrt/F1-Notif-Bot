#![allow(dead_code)]

use std::{num::NonZeroU64, time::Duration};

use chrono::{DateTime, Utc};
use mongodb::{
    bson::{doc, oid::ObjectId},
    Collection,
};
use serde::{Deserialize, Serialize};

use serenity::futures::StreamExt;

use crate::error::Error;

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct Weekend {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub name: String,
    pub start: DateTime<Utc>,
    pub sessions: Vec<SessionType>,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum SessionType {
    None,
    // Test sessions (usually at the start of a season)
    Test(TestSession),
    // Pratice sessions (FP1, FP2, FP3)
    Practice(PracticeSession),

    SprintQuali(Qualifying),
    Shootout(Qualifying),
    Qualifying(Qualifying),
    Custom(Custom),
    Sprint(Race),
    Race(Race),
}

impl SessionType {
    pub fn is_notified(&self) -> bool {
        match self {
            SessionType::None => true,
            SessionType::Shootout(sess) => sess.notified,
            SessionType::SprintQuali(sess) => sess.notified,
            SessionType::Test(sess) => sess.notified,
            SessionType::Practice(sess) => sess.notified,
            SessionType::Qualifying(sess) => sess.notified,
            SessionType::Sprint(sess) => sess.notified,
            SessionType::Race(sess) => sess.notified,
            SessionType::Custom(sess) => sess.notified,
        }
    }

    pub fn set_modified(&mut self) {
        match self {
            SessionType::None => {},
            SessionType::Shootout(sess) => sess.notified = true,
            SessionType::SprintQuali(sess) => sess.notified = true,
            SessionType::Test(sess) => sess.notified = true,
            SessionType::Practice(sess) => sess.notified = true,
            SessionType::Qualifying(sess) => sess.notified = true,
            SessionType::Sprint(sess) => sess.notified = true,
            SessionType::Race(sess) => sess.notified = true,
            SessionType::Custom(sess) => sess.notified = true,
        }
    }

    pub fn short_name(&self) -> String {
        match self {
            SessionType::SprintQuali(_) => "Deprecated.".to_owned(),
            SessionType::Shootout(_) => "Sprint Shootout".to_owned(),
            SessionType::None => "Unsupported session".to_owned(),
            SessionType::Test(_) => "Test session".to_owned(),
            SessionType::Practice(sess) => format!("FP{}", sess.number),
            SessionType::Qualifying(_) => "Qualifying".to_owned(),
            SessionType::Sprint(_) => "Sprint Race".to_owned(),
            SessionType::Race(_) => "Race".to_owned(),
            SessionType::Custom(sess) => sess.name.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
pub struct Race {
    pub time: DateTime<Utc>,
    pub notified: bool,
    pub duration: Duration,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
pub struct Qualifying {
    pub time: DateTime<Utc>,
    pub notified: bool,
    pub duration: Duration,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct Custom {
    pub time: DateTime<Utc>,
    pub notified: bool,
    pub duration: Duration,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash, Default)]
pub struct TestSession {
    pub time: DateTime<Utc>,
    pub notified: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
pub struct PracticeSession {
    // The start time of the session
    pub time: DateTime<Utc>,
    // Session Number (FP1, FP2, FP3)
    pub number: u8,
    // Estimated duration of a session
    pub duration: Duration,
    // Whether or not a session has been notified
    pub notified: bool,
}

pub trait DiscordString {
    fn to_display(&self) -> String;
}

impl DiscordString for SessionType {
    fn to_display(&self) -> String {
        let (name, timestamp, strikethrough) = match self {
            SessionType::None => ("Unknown".to_owned(), 0, false),
            SessionType::Test(sess) => (
                "Testing session:".to_owned(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::Practice(sess) => (
                format!("FP{}:            ", sess.number),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::Qualifying(sess) => (
                "Qualifying:     ".to_owned(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::SprintQuali(_) => {
                ("Deprecated (Sprint Qualifying)".to_owned(), 0i64, false)
            },
            Self::Shootout(sess) => (
                "Sprint Shootout:".to_owned(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::Sprint(sess) => (
                "Sprint Race:    ".to_owned(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::Race(sess) => (
                "Race:           ".to_owned(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
            SessionType::Custom(sess) => (
                sess.name.clone(),
                sess.time.timestamp(),
                sess.time.signed_duration_since(Utc::now())
                    < -sess.get_duration(),
            ),
        };

        let strikethrough = if strikethrough {
            "~~"
        } else {
            ""
        };
        format!("\n> {strikethrough}**`{name}`**  <t:{timestamp}:f> \t{strikethrough}(<t:{timestamp}:R>)")
    }
}

impl DiscordString for Weekend {
    fn to_display(&self) -> String {
        let name = &self.name;
        let mut content = format!("**Next Event**\n**{name}**");
        for (_, sess) in self.sessions.iter().enumerate() {
            content += sess.to_display().as_str();
        }
        content += "\n\n\nClick :mega: in <#913752470293991424> or use <id:customize> to get a notification when a session is live.\nTimes are in your timezone.";
        content
    }
}

impl Default for Race {
    fn default() -> Self {
        Self {
            time: Default::default(),
            notified: false,
            duration: Duration::from_secs(2 * 60 * 60),
        }
    }
}

impl Default for Qualifying {
    fn default() -> Self {
        Self {
            time: Default::default(),
            notified: false,
            duration: Duration::from_secs(60 * 60),
        }
    }
}

impl Default for PracticeSession {
    fn default() -> Self {
        Self {
            time: Default::default(),
            number: 1,
            duration: Duration::from_secs(60 * 90),
            notified: false,
        }
    }
}

pub trait Sessions {
    fn get_duration(&self) -> chrono::Duration;
}

impl Sessions for PracticeSession {
    fn get_duration(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.duration).unwrap()
    }
}

impl Sessions for TestSession {
    fn get_duration(&self) -> chrono::Duration {
        chrono::Duration::minutes(0)
    }
}

impl Sessions for Qualifying {
    fn get_duration(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.duration).unwrap()
    }
}

impl Sessions for Race {
    fn get_duration(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.duration).unwrap()
    }
}

impl Sessions for Custom {
    fn get_duration(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.duration).unwrap()
    }
}

pub async fn filter_current_weekend(
    weekends: &Collection<Weekend>
) -> Result<Option<Weekend>, Error> {
    let mut cursor = weekends.find(doc! { "done": false }, None).await?;
    let mut best_start = None;
    let mut best_doc = None;
    while let Some(doc) = cursor.next().await {
        let doc = doc?;
        if doc.done {
            continue;
        }
        // Discard any dates older than 4 days, no weekend will make sense at
        // that point.
        if Utc::now().signed_duration_since(doc.start).num_days() > 4 {
            continue;
        }

        best_start = if best_start.is_none()
            || best_start.unwrap() < Utc::now().signed_duration_since(doc.start)
        {
            best_doc = Some(doc.clone());
            Some(Utc::now().signed_duration_since(doc.start))
        } else {
            best_start
        }
    }

    Ok(best_doc)
}

impl SessionType {
    pub fn get_duration(&self) -> Option<chrono::Duration> {
        match self {
            SessionType::None => None,
            SessionType::Test(sess) => Some(sess.get_duration()),
            SessionType::Practice(sess) => Some(sess.get_duration()),
            SessionType::Qualifying(sess)
            | SessionType::SprintQuali(sess)
            | SessionType::Shootout(sess) => Some(sess.get_duration()),
            SessionType::Sprint(sess) | SessionType::Race(sess) => {
                Some(sess.get_duration())
            },
            SessionType::Custom(sess) => Some(sess.get_duration())
        }
    }

    pub fn time_until(&self) -> Option<i64> {
        match self {
            SessionType::None => None,
            SessionType::Test(sess) => {
                Some(Utc::now().signed_duration_since(sess.time).num_minutes())
            },
            SessionType::Practice(sess) => {
                Some(Utc::now().signed_duration_since(sess.time).num_minutes())
            },
            SessionType::Qualifying(sess)
            | SessionType::SprintQuali(sess)
            | SessionType::Shootout(sess) => {
                Some(Utc::now().signed_duration_since(sess.time).num_minutes())
            },
            SessionType::Sprint(sess) | SessionType::Race(sess) => {
                Some(Utc::now().signed_duration_since(sess.time).num_minutes())
            },
            SessionType::Custom(sess) => {
                Some(Utc::now().signed_duration_since(sess.time).num_minutes())
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WeekendState {
    None,
    FutureSession,
    CurrentSession(usize, SessionType),
}

impl Weekend {
    pub fn get_next_session(&mut self) -> WeekendState {
        let mut value = WeekendState::None;

        let mut best_time = None;

        for (i, sess) in self.sessions.iter().enumerate() {
            if sess.is_notified() {
                continue;
            }

            if sess.time_until().is_none() {
                continue;
            }
            let time_until = &sess.time_until().unwrap();

            // Lets mark this weekend as (at least) future session so we don't
            // skip it in the future in case there is a next session another
            // day.
            if let WeekendState::None = value {
                if *time_until < -6 {
                    value = WeekendState::FutureSession
                }
            }

            (value, best_time) = match time_until {
                -6..=0 => {
                    (WeekendState::CurrentSession(i, sess.clone()), sess.time_until())
                },
                _ => (value, best_time),
            }
        }
        value
        // match best_match {
        //     Some(sess) => (best_index, sess),
        //     None => (None, &SessionType::None),
        // }
    }
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone)]
pub enum BotMessageType {
    None,
    Notification(BotNotification),
    Persistent(BotPersistent),
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone)]
pub struct BotNotification {
    pub time_sent: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone)]
pub struct BotPersistent {
    #[serde(with = "string")]
    pub hash: u64,
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone)]
pub struct BotMessage {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    #[serde(with = "string")]
    pub discord_id: NonZeroU64,
    pub kind: BotMessageType,
}

mod string {
    use std::{fmt::Display, str::FromStr};

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<T, S>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

impl Default for BotNotification {
    fn default() -> Self {
        Self {
            time_sent: Utc::now(),
        }
    }
}

impl BotMessage {
    pub fn new_now(
        id: NonZeroU64,
        kind: BotMessageType,
    ) -> Self {
        Self {
            id: ObjectId::new(),
            discord_id: id,
            kind,
        }
    }

    pub fn new_notification(id: NonZeroU64) -> Self {
        Self {
            id: ObjectId::new(),
            discord_id: id,
            kind: BotMessageType::Notification(BotNotification {
                time_sent: Utc::now(),
            }),
        }
    }

    pub fn new_persistent(
        id: NonZeroU64,
        hash: u64,
    ) -> Self {
        Self {
            id: ObjectId::new(),
            discord_id: id,
            kind: BotMessageType::Persistent(BotPersistent {
                hash,
            }),
        }
    }
}
