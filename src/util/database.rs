use std::fmt::Write;
use std::hash::Hash;

use chrono::{DateTime, Utc};
use f1_bot_types::{Message, MessageKind, Series, Session, SessionStatus, Weekend, WeekendStatus};
use libsql::params;
use serenity::all::CreateMessage;

use crate::error::Error::Io;

pub async fn fetch_all<T>(
    db_conn: &mut libsql::Connection,
    sql: &str,
    params: impl libsql::params::IntoParams,
) -> Result<Vec<T>, crate::error::Error>
where
    T: serde::de::DeserializeOwned,
{
    let mut result = db_conn.query(sql, params).await?;
    let mut return_value = Vec::with_capacity(10);
    while let Ok(Some(row)) = result.next().await {
        return_value.push(libsql::de::from_row::<T>(&row)?);
    }
    Ok(return_value)
}

pub async fn fetch_single<T>(
    db_conn: &mut libsql::Connection,
    sql: &str,
    params: impl libsql::params::IntoParams,
) -> Result<Option<T>, crate::error::Error>
where
    T: serde::de::DeserializeOwned,
{
    let mut result = db_conn.query(sql, params).await?;
    if let Ok(Some(row)) = result.next().await {
        Ok(Some(libsql::de::from_row::<T>(&row)?))
    } else {
        Ok(None)
    }
}

pub async fn fetch_weekends(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Weekend>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM weekends ORDER BY start_date ASC",
        params![],
    )
    .await
}

pub async fn fetch_weekend(
    db_conn: &mut libsql::Connection,
    id: u64,
) -> Result<Option<Weekend>, crate::error::Error> {
    fetch_single(db_conn, "SELECT * FROM weekends WHERE id = ?", params![id]).await
}

pub async fn fetch_weekend_for_series(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Vec<Weekend>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM weekends WHERE series = ? ORDER BY start_date ASC",
        params![series.i8()],
    )
    .await
}

pub async fn fetch_feeder_weekend(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Weekend>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM weekends WHERE series != ? ORDER BY start_date ASC",
        params![Series::F1.i8()],
    )
    .await
}

pub async fn fetch_sessions(
    db_conn: &mut libsql::Connection,
    weekend: &Weekend,
) -> Result<Vec<Session>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM sessions WHERE weekend = ? ORDER BY start_date ASC",
        params![weekend.id],
    )
    .await
}

#[derive(Debug)]
pub struct FullWeekend {
    pub weekend: Weekend,
    pub sessions: Vec<Session>,
}

impl FullWeekend {
    pub fn check_is_done(&self, modified_session: &Session) -> bool {
        if self.weekend.status == WeekendStatus::Done {
            return true;
        }
        if self.sessions.is_empty() {
            return false;
        }
        self.sessions.iter().all(|f| {
            if f.id == modified_session.id {
                return true;
            }
            matches!(f.status, SessionStatus::Finished | SessionStatus::Cancelled)
        })
    }

    pub fn is_done(&self) -> bool {
        if self.weekend.status == WeekendStatus::Done {
            return true;
        }

        if self.sessions.is_empty() {
            return false;
        }

        self.sessions
            .iter()
            .all(|f| matches!(f.status, SessionStatus::Finished | SessionStatus::Cancelled))
    }

    pub fn next_session(&self) -> Option<&Session> {
        if matches!(self.weekend.status, WeekendStatus::Done) {
            return None;
        }
        self.sessions.iter().find(|f| {
            matches!(
                f.status,
                f1_bot_types::SessionStatus::Open | f1_bot_types::SessionStatus::Delayed
            ) && matches!(
                f.start_date.signed_duration_since(Utc::now()).num_minutes(),
                0..5
            )
        })
    }

    pub fn weekend_msg_str(&self, extra: bool) -> String {
        let mut sessions_str = String::new();
        for session in self.sessions.iter() {
            let tz = session.start_date.timestamp();
            let is_done = match Utc::now().timestamp() > tz + session.duration as i64 {
                true => "~~",
                false => "",
            };
            sessions_str += &format!(
                "\n> `{:>12}` {2}<t:{}:f> (<t:{1}:R>){2}",
                session.title, tz, is_done
            );
        }
        let extra_str = match extra {
            true => &format!(
                "\n\nUse <id:customize> to get the `{}-notifications` role\n**Times are in your Timezone**",
                self.weekend.series
            ),
            false => "",
        };
        format!(
            "## Next Event:\n**{} {}**{}{}",
            self.weekend.icon, self.weekend.name, sessions_str, extra_str
        )
    }
}

impl Hash for FullWeekend {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.weekend.hash(state);

        for session in self.sessions.iter() {
            session.hash(state)
        }
    }
}

pub async fn fetch_full_weekends_for_series(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Vec<FullWeekend>, crate::error::Error> {
    let weekends = fetch_weekend_for_series(db_conn, series).await?;
    let mut return_weekends = Vec::with_capacity(weekends.len());
    for weekend in weekends.into_iter() {
        let sessions = fetch_sessions(db_conn, &weekend).await?;
        return_weekends.push(FullWeekend { weekend, sessions });
    }
    Ok(return_weekends)
}

pub async fn fetch_full_weekends(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<FullWeekend>, crate::error::Error> {
    let weekends = fetch_weekends(db_conn).await?;
    let mut return_weekends = Vec::with_capacity(weekends.len());
    for weekend in weekends.into_iter() {
        let sessions = fetch_sessions(db_conn, &weekend).await?;
        return_weekends.push(FullWeekend { weekend, sessions });
    }
    Ok(return_weekends)
}

pub async fn fetch_full_weekend(
    db_conn: &mut libsql::Connection,
    id: u64,
) -> Result<Option<FullWeekend>, crate::error::Error> {
    let weekend = fetch_weekend(db_conn, id).await?;
    Ok(match weekend {
        None => None,
        Some(weekend) => {
            let sessions = fetch_sessions(db_conn, &weekend).await?;
            Some(FullWeekend { weekend, sessions })
        }
    })
}

pub async fn fetch_next_weekend_for_series(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Option<Weekend>, crate::error::Error> {
    fetch_single(
        db_conn,
        "SELECT * FROM weekends WHERE series = ? AND status != ? ORDER BY start_date ASC LIMIT 1",
        params![series.i8(), WeekendStatus::Done.i8()],
    )
    .await
}

pub async fn fetch_next_full_weekend_for_series(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Option<FullWeekend>, crate::error::Error> {
    let weekend = fetch_next_weekend_for_series(db_conn, series).await?;
    Ok(match weekend {
        None => None,
        Some(weekend) => Some({
            let sessions = fetch_sessions(db_conn, &weekend).await?;
            FullWeekend { weekend, sessions }
        }),
    })
}

pub async fn fetch_messages(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(db_conn, "SELECT * FROM messages", params![]).await
}

pub async fn fetch_weekend_messages(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM messages WHERE kind = ? ORDER BY message ASC",
        params![MessageKind::Weekend.i8()],
    )
    .await
}

pub async fn mark_weekend_message_for_series_expired(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<u64, crate::error::Error> {
    let now_str = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Ok(db_conn
        .execute(
            "UPDATE messages SET expiry = ? WHERE kind = ? AND series = ?",
            params![now_str, MessageKind::Weekend.i8(), series.i8()],
        )
        .await?)
}

pub async fn fetch_weekend_message_for_series(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Option<Message>, crate::error::Error> {
    fetch_single(
        db_conn,
        "SELECT * FROM messages WHERE kind = ? and series = ?",
        params![MessageKind::Weekend.i8(), series.i8()],
    )
    .await
}

pub async fn expired_messages(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM messages WHERE expiry IS NOT NULL AND expiry < now()",
        params![],
    )
    .await
}

pub async fn fetch_calendar_messages(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM messages WHERE kind = ? AND series = ? ORDER BY message ASC",
        params![MessageKind::Calendar.i8(), series.i8()],
    )
    .await
}

pub async fn fetch_custom_messages(
    db_conn: &mut libsql::Connection,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM messages WHERE kind = ?",
        params![MessageKind::Custom.i8()],
    )
    .await
}

pub async fn fetch_series_calendar_messages(
    db_conn: &mut libsql::Connection,
    series: Series,
) -> Result<Vec<Message>, crate::error::Error> {
    fetch_all(
        db_conn,
        "SELECT * FROM messages WHERE series = ? ORDER BY message ASC",
        params![series.i8()],
    )
    .await
}

/// Sets a [Messages](Message) expiry date.
/// If *date* is set to [None] then the message is set to expire immediately
pub async fn mark_message_expired(
    db_conn: &mut libsql::Connection,
    id: u64,
    date: Option<DateTime<Utc>>,
) -> Result<(), crate::error::Error> {
    let date = date.unwrap_or(Utc::now());
    let date_str = date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let rows_affected = db_conn
        .execute(
            "UPDATE messages SET expiry = ? WHERE id = ?",
            params![date_str, id],
        )
        .await?;
    if rows_affected == 0 {
        return Err(Io(std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
    Ok(())
}

/// Deletes a [Message]
pub async fn delete_message(
    db_conn: &mut libsql::Connection,
    id: u64,
) -> Result<(), crate::error::Error> {
    let rows_affected = db_conn
        .execute("DELETE FROM messages WHERE id = ?", params![id])
        .await?;
    if rows_affected == 0 {
        return Err(Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No Rows to delete found.",
        )));
    }
    Ok(())
}

/// Checks [Weekends](Weekend) and if all [Sessions](Session) are [Finished](SessionStatus)
/// or [Cancelled](SessionStatus), then mark these Weekends as [Done](SessionStatus).
pub async fn check_weekends(db_conn: &mut libsql::Connection) -> Result<(), crate::error::Error> {
    let weekends = fetch_full_weekends(db_conn).await?;
    for weekend in weekends
        .into_iter()
        .filter(|p| p.sessions.is_empty() && p.weekend.status == WeekendStatus::Open)
    {
        if weekend.sessions.into_iter().all(|f| match f.status {
            SessionStatus::Open | SessionStatus::Delayed => false,
            SessionStatus::Finished | SessionStatus::Cancelled => true,
        }) {
            mark_weekend_done(db_conn, &weekend.weekend).await?;
        }
    }

    Ok(())
}

/// Marks a [Weekend] as [Done](WeekendStatus::Done)
pub async fn mark_weekend_done(
    db_conn: &mut libsql::Connection,
    weekend: &Weekend,
) -> Result<u64, crate::error::Error> {
    Ok(db_conn
        .execute(
            "UPDATE weekends SET STATUS = ? WHERE id = ?",
            params![WeekendStatus::Done.i8(), weekend.id],
        )
        .await?)
}

pub async fn mark_session_done(
    db_conn: &mut libsql::Connection,
    session: &Session,
) -> Result<u64, crate::error::Error> {
    Ok(db_conn
        .execute(
            "UPDATE sessions SET STATUS = ? WHERE id = ?",
            params![SessionStatus::Finished.i8(), session.id],
        )
        .await?)
}

pub async fn update_message_hash(
    db_conn: &mut libsql::Connection,
    msg_id: u64,
    hash: u64,
) -> Result<u64, crate::error::Error> {
    Ok(db_conn
        .execute(
            "UPDATE messages SET hash = ? WHERE id = ?",
            params![hash.to_string(), msg_id],
        )
        .await?)
}

pub fn create_multi_message(
    weekends: &[FullWeekend],
) -> Result<CreateMessage, crate::error::Error> {
    let mut string = String::with_capacity(512);
    for weekend in weekends {
        writeln!(
            string,
            "## {} {} {}",
            weekend.weekend.series, weekend.weekend.year, weekend.weekend.name
        )?;
        for session in &weekend.sessions {
            let session_done = session.start_date
                + chrono::Duration::seconds(session.duration.into())
                < Utc::now();

            if session_done {
                string.push_str("~~");
            }

            write!(
                string,
                "> `{0:>12}`: <t:{1}:f> (<t:{1}:r>)",
                session.title,
                session.start_date.timestamp(),
            )?;

            if session_done {
                string.push_str("~~");
            }
            string.push('\n');
        }
        string.push('\n');
    }

    string.push_str("To get a notification once a session goes live, go to <id:customize> and select the series for which you want to be notified.\nTimes are displayed in your timezone.");

    Ok(CreateMessage::new().content(string))
}
