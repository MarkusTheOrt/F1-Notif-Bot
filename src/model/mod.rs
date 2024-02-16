use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt::Display};

#[derive(Serialize, Deserialize, Clone, Copy, sqlx::Type, Debug, Hash)]
pub enum Series {
    F1,
    F2,
    F3,
    F1Academy,
    Unsupported,
}

impl From<String> for Series {
    fn from(value: String) -> Self {
        match value.as_str() {
            "F1" => Self::F1,
            "F2" => Self::F2,
            "F3" => Self::F3,
            "F1Academy" => Self::F1Academy,
            _ => Self::Unsupported,
        }
    }
}

impl From<u8> for Series {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::F1,
            1 => Self::F2,
            2 => Self::F3,
            3 => Self::F1Academy,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, sqlx::Type, Debug, Hash)]
pub enum WeekendStatus {
    Open,
    Cancelled,
    Done,
}

impl From<String> for WeekendStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Open" => Self::Open,
            "Cancelled" => Self::Cancelled,
            "Done" => Self::Done,
            _ => Self::Done,
        }
    }
}

impl From<u8> for WeekendStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Open,
            1 => Self::Cancelled,
            _ => Self::Done,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type, Debug, Hash)]
pub enum SessionKind {
    Custom,
    Practice,
    Qualifying,
    Race,
    SprintRace,
    SprintQuali,
    PreSeasonTest,
    FeatureRace,
    Unsupported,
}

impl From<String> for SessionKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Custom" => Self::Custom,
            "Practice" => Self::Practice,
            "Qualifying" => Self::Qualifying,
            "Race" => Self::Race,
            "SprintRace" => Self::SprintRace,
            "SprintQuali" => Self::SprintQuali,
            "PreSeasonTest" => Self::PreSeasonTest,
            "FeatureRace" => Self::FeatureRace,
            _ => Self::Unsupported,
        }
    }
}

impl Display for SessionKind {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            SessionKind::Custom => f.write_str(""),
            SessionKind::Practice => f.write_str("FP"),
            SessionKind::Qualifying => f.write_str("Qualifying"),
            SessionKind::Race => f.write_str("Race"),
            SessionKind::SprintRace => f.write_str("Sprint Race"),
            SessionKind::SprintQuali => f.write_str("Sprint Shootout"),
            SessionKind::PreSeasonTest => f.write_str("Pre-Season Test"),
            SessionKind::FeatureRace => f.write_str("Feature Race"),
            Self::Unsupported => f.write_str("Unsupported"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type, Debug, Hash)]
pub enum SessionStatus {
    Open,
    Delayed,
    Cancelled,
    Done,
    Unsupported,
}

impl From<SessionStatus> for u8 {
    fn from(value: SessionStatus) -> Self {
        match value {
            SessionStatus::Open => 0,
            SessionStatus::Delayed => 1,
            SessionStatus::Cancelled => 2,
            SessionStatus::Done => 3,
            SessionStatus::Unsupported => 4,
        }
    }
}

impl From<String> for SessionStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Open" => Self::Open,
            "Delayed" => Self::Delayed,
            "Cancelled" => Self::Cancelled,
            "Done" => Self::Done,
            _ => Self::Unsupported,
        }
    }
}

impl From<u8> for SessionStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Open,
            1 => Self::Delayed,
            2 => Self::Cancelled,
            3 => Self::Done,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type, Debug, Hash)]
pub enum NotificationSetting {
    Notify,
    Ignore,
}

impl From<String> for NotificationSetting {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Notify" => Self::Notify,
            _ => Self::Ignore,
        }
    }
}

impl From<u8> for NotificationSetting {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Notify,
            _ => Self::Ignore,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Session {
    pub id: u32,
    pub weekend: u32,
    pub kind: SessionKind,
    pub status: SessionStatus,
    pub notify: NotificationSetting,
    pub duration: i64,
    pub date: DateTime<Utc>,
    pub number: Option<u8>,
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct Weekend<'a> {
    pub id: u32,
    pub series: Series,
    pub name: Cow<'a, str>,
    pub icon: Cow<'a, str>,
    pub sessions: Vec<Session>,
    pub year: u16,
    pub start_date: DateTime<Utc>,
    pub status: WeekendStatus,
}

impl<'a> Display for Weekend<'a> {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.write_fmt(format_args!("{} **{}**", self.icon, self.name))?;
        let now = Utc::now();
        for session in self.sessions.iter() {
            let mut str = "";
            if session.date.signed_duration_since(now).num_seconds()
                < -session.duration
            {
                str = "~~"
            }
            f.write_fmt(format_args!(
                "\n> {str}{session}: <t:{}:f>{str} (<t:{}:R>)",
                session.date.timestamp(),
                session.date.timestamp()
            ))?;
        }
        Ok(())
    }
}

impl PartialEq for Weekend<'_> {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.id == other.id
    }
}

impl Display for Session {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let title = match &self.title {
            None => "No title supplied".to_owned(),
            Some(t) => t.clone(),
        };
        let number = match self.number {
            None => "".to_owned(),
            Some(t) => format!("{t}"),
        };
        match self.kind {
            SessionKind::Custom => f.write_str(&title),
            SessionKind::Practice => {
                f.write_fmt(format_args!("`         FP{number}`"))
            },
            SessionKind::Qualifying => f.write_str("`  Qualifying`"),
            SessionKind::Race => f.write_str("`        Race`"),
            SessionKind::SprintRace => f.write_str("` Sprint Race`"),
            SessionKind::SprintQuali => f.write_str("`Sprint Quali`"),
            SessionKind::FeatureRace => f.write_str("`Feature Race`"),
            SessionKind::PreSeasonTest => f.write_str("`Pre-Season Test`"),
            SessionKind::Unsupported => f.write_str("Unsupported!!"),
        }?;
        Ok(())
    }
}

impl Session {
    pub fn pretty_name(&self) -> String {
        if let Some(title) = &self.title {
            return title.to_owned();
        }
        let number = match self.number {
            None => "".to_owned(),
            Some(t) => format!("{t}"),
        };

        match self.kind {
            SessionKind::Custom => "No title supplied".to_owned(),
            SessionKind::Practice => format!("FP{number}"),
            SessionKind::Qualifying => "Qualifying".to_owned(),
            SessionKind::Race => "Race".to_owned(),
            SessionKind::SprintRace => "Sprint Race".to_owned(),
            SessionKind::SprintQuali => "Sprint Shootout".to_owned(),
            SessionKind::PreSeasonTest => "Pre-Season Test".to_owned(),
            SessionKind::FeatureRace => "Feature Race".to_owned(),
            SessionKind::Unsupported => "Unkown session type".to_owned(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, sqlx::Type, Debug)]
pub enum MessageKind {
    Persistent,
    Notification,
    Calendar,
    Unsupported,
}

impl From<String> for MessageKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Persistent" => Self::Persistent,
            "Notification" => Self::Notification,
            "Calendar" => Self::Calendar,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BotMessage {
    pub id: u32,
    pub channel: u64,
    pub message: u64,
    pub kind: MessageKind,
    pub posted: DateTime<Utc>,
    pub hash: Option<u64>,
    pub series: Series,
}
