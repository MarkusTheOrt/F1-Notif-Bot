use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt::Display};

#[derive(Serialize, Deserialize, Clone, Copy, sqlx::Type)]
pub enum Series {
    F1,
    F2,
    F3,
    F1Academy,
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type)]
pub enum SessionKind {
    Custom,
    Practice,
    Qualifying,
    Race,
    SprintRace,
    SprintQuali,
    PreSeasonTest,
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
        }
    }
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type)]
pub enum SessionStatus {
    Open,
    Delayed,
    Cancelled,
    Done,
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type)]
pub enum NotificationSetting {
    Notify,
    Ignore,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Session<'a> {
    pub id: u32,
    pub weekend: u32,
    pub kind: SessionKind,
    pub status: SessionStatus,
    pub notify: NotificationSetting,
    pub date: DateTime<Utc>,
    pub number: Option<Cow<'a, str>>,
    pub title: Option<Cow<'a, str>>,
}

impl Display for Session<'_> {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.write_str(&format!(
            "> `{}` <t:{}:F> - <t:{}:R>",
            format!("{}{}", self.kind, self.number.as_ref().unwrap_or(&"".into())),
            self.date.timestamp(),
            self.date.timestamp()
        ))
    }
}
