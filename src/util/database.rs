#![allow(dead_code)]

use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use mongodb::{
    bson::doc,
    Collection,
};
use serde::{
    Deserialize,
    Serialize,
};
use serenity::futures::StreamExt;

use crate::error::Error;

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct Weekend {
    pub name: String,
    pub start: DateTime<Utc>,
    pub sessions: Vec<SessionType>,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
pub enum SessionType {
    None,
    // Test sessions (usually at the start of a season)
    Test(TestSession),
    // Pratice sessions (FP1, FP2, FP3)
    Practice(PracticeSession),
    // Qualifying (Includes both Sprint and Race Quali)
    Qualifying(Qualifying),
    // Sprint Race ()
    Sprint(Race),
    Race(Race),
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

impl DiscordString for Race {
    fn to_display(&self) -> String {
        let timestamp = self.time.timestamp();
        format!("> **`Race:      `** <t:{timestamp}:f> 	(<t:{timestamp}:R>)")
    }
}

impl DiscordString for Qualifying {
    fn to_display(&self) -> String {
        let timestamp = self.time.timestamp();
        let strikethrough = if self.notified {
            "~~"
        } else {
            ""
        };
        format!("> {strikethrough}**`Qualifying:`** <t:{timestamp}:f> 	(<t:{timestamp}:R>){strikethrough}")
    }
}

impl DiscordString for PracticeSession {
    fn to_display(&self) -> String {
        let timestamp = self.time.timestamp();
        let strikethrough = if self.notified {
            "~~"
        } else {
            ""
        };
        format!(
            "> {strikethrough}**`FP{}:       `** <t:{timestamp}:f> 	(<t:{timestamp}:R>){strikethrough}",
            self.number
        )
    }
}

impl DiscordString for SessionType {
    fn to_display(&self) -> String {
        let (name, timestamp, strikethrough) = match self {
            SessionType::None => ("Unsupported:".to_owned(), 0, false),
            SessionType::Test(sess) => (
                "Testing session:".to_owned(),
                sess.time.timestamp(),
                sess.notified,
            ),
            SessionType::Practice(sess) => (
                format!("FP{}:       ", sess.number),
                sess.time.timestamp(),
                sess.notified,
            ),
            SessionType::Qualifying(sess) => (
                "Qualifying: ".to_owned(),
                sess.time.timestamp(),
                sess.notified,
            ),
            SessionType::Sprint(sess) => (
                "Sprint Race:".to_owned(),
                sess.time.timestamp(),
                sess.notified,
            ),
            SessionType::Race(sess) => (
                "Race:       ".to_owned(),
                sess.time.timestamp(),
                sess.notified,
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
        let mut content = format!("**{name}**");
        for (_, sess) in self.sessions.iter().enumerate() {
            content += sess.to_display().as_str();
        }
        content += "\n\n\nClick :mega: in <#913752470293991424> to get a notification when a session is live.";
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
        self.time - Utc::now()
    }
}

impl Sessions for TestSession {
    fn get_duration(&self) -> chrono::Duration {
        self.time - Utc::now()
    }
}

impl Sessions for Qualifying {
    fn get_duration(&self) -> chrono::Duration {
        self.time - Utc::now()
    }
}

impl Sessions for Race {
    fn get_duration(&self) -> chrono::Duration {
        self.time - Utc::now()
    }
}

#[allow(clippy::needless_return)]
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
        if doc.start.signed_duration_since(Utc::now()).num_days() < -4 {
            continue;
        }

        best_start = if best_start.is_none()
            || best_start.unwrap() < doc.start - Utc::now()
        {
            best_doc = Some(doc.clone());
            Some(doc.start - Utc::now())
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
            SessionType::Qualifying(sess) => Some(sess.get_duration()),
            SessionType::Sprint(sess) | SessionType::Race(sess) => {
                Some(sess.get_duration())
            },
        }
    }
}

impl Weekend {
    pub fn get_next_session(&self) -> SessionType {
        let mut best_match = None;
        let mut best_time = None;
        for (_, sess) in self.sessions.iter().enumerate() {
            best_time = if sess.get_duration().is_none()
                || sess.get_duration().unwrap().num_minutes().abs()
                    < best_time.unwrap()
            {
                best_match = Some(sess.to_owned());
                Some(sess.get_duration().unwrap().num_minutes().abs())
            } else {
                best_time
            }
        }
        match best_match {
            None => SessionType::None,
            _ => best_match.unwrap(),
        }
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
    pub hash: u64,
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone)]
pub struct BotMessage {
    pub discord_id: u64,
    pub kind: BotMessageType,
}

impl Default for BotMessage {
    fn default() -> Self {
        Self {
            discord_id: 0,
            kind: BotMessageType::None,
        }
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
        id: u64,
        kind: BotMessageType,
    ) -> Self {
        Self {
            discord_id: id,
            kind,
        }
    }

    pub fn new_notification(id: u64) -> Self {
        Self {
            discord_id: id,
            kind: BotMessageType::Notification(BotNotification {
                time_sent: Utc::now(),
            }),
        }
    }

    pub fn new_persistent(
        id: u64,
        hash: u64,
    ) -> Self {
        Self {
            discord_id: id,
            kind: BotMessageType::Persistent(BotPersistent {
                hash,
            }),
        }
    }
}
