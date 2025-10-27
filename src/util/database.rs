use std::fmt::Write;
use std::hash::Hash;

use chrono::{DateTime, Utc};
use f1_bot_types::{
    Message, MessageKind, Series, Session, SessionStatus, Weekend,
    WeekendStatus,
};
use serenity::all::CreateMessage;
use sqlx::MySqlConnection;

pub async fn fetch_weekends(
    db_conn: &mut MySqlConnection
) -> Result<Vec<Weekend>, sqlx::Error> {
    sqlx::query_as!(Weekend, "SELECT * FROM weekends ORDER BY start_date ASC")
        .fetch_all(db_conn)
        .await
}

pub async fn fetch_weekend(
    db_conn: &mut MySqlConnection,
    id: u64,
) -> Result<Option<Weekend>, sqlx::Error> {
    sqlx::query_as!(Weekend, "SELECT * FROM weekends WHERE id = ?", id)
        .fetch_optional(db_conn)
        .await
}

pub async fn fetch_weekend_for_series(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Vec<Weekend>, sqlx::Error> {
    sqlx::query_as!(
        Weekend,
        "SELECT * FROM weekends WHERE series = ? ORDER BY start_date ASC",
        series.i8()
    )
    .fetch_all(db_conn)
    .await
}

pub async fn fetch_sessions(
    db_conn: &mut MySqlConnection,
    weekend: &Weekend,
) -> Result<Vec<Session>, sqlx::Error> {
    sqlx::query_as!(
        Session,
        "SELECT * FROM sessions WHERE weekend = ? ORDER BY start_date ASC",
        weekend.id
    )
    .fetch_all(db_conn)
    .await
}

#[derive(Debug)]
pub struct FullWeekend {
    pub weekend: Weekend,
    pub sessions: Vec<Session>,
}

impl FullWeekend {
    pub fn check_is_done(
        &self,
        modified_session: &Session,
    ) -> bool {
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
            matches!(
                f.status,
                SessionStatus::Finished | SessionStatus::Cancelled
            )
        })
    }

    pub fn is_done(&self) -> bool {
        if self.weekend.status == WeekendStatus::Done {
            return true;
        }

        if self.sessions.is_empty() {
            return false;
        }

        self.sessions.iter().all(|f| {
            matches!(
                f.status,
                SessionStatus::Finished | SessionStatus::Cancelled
            )
        })
    }

    pub fn next_session(&self) -> Option<&Session> {
        if matches!(self.weekend.status, WeekendStatus::Done) {
            return None;
        }
        self.sessions.iter().find(|f| {
            matches!(
                f.status,
                f1_bot_types::SessionStatus::Open
                    | f1_bot_types::SessionStatus::Delayed
            ) && matches!(
                f.start_date.signed_duration_since(Utc::now()).num_minutes(),
                0..5
            )
        })
    }

    pub fn weekend_msg_str(
        &self,
        extra: bool,
    ) -> String {
        let mut sessions_str = String::new();
        for session in self.sessions.iter() {
            let tz = session.start_date.timestamp();
            let is_done =
                match Utc::now().timestamp() > tz + session.duration as i64 {
                    true => "~~",
                    false => "",
                };
            sessions_str += &format!(
                "\n> `{:>12}` {2}<t:{}:f> (<t:{1}:R>){2}",
                session.title, tz, is_done
            );
        }
        let extra_str = match extra {
            true => &format!("\n\nUse <id:customize> to get the `{}-notifications` role\n**Times are in your Timezone**", self.weekend.series),
            false => ""
        };
        format!(
            "## Next Event:\n**{} {}**{}{}",
            self.weekend.icon, self.weekend.name, sessions_str, extra_str
        )
    }
}

impl Hash for FullWeekend {
    fn hash<H: std::hash::Hasher>(
        &self,
        state: &mut H,
    ) {
        self.weekend.hash(state);

        for session in self.sessions.iter() {
            session.hash(state)
        }
    }
}

pub async fn fetch_full_weekends_for_series(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Vec<FullWeekend>, sqlx::Error> {
    let weekends = fetch_weekend_for_series(db_conn, series).await?;
    let mut return_weekends = Vec::with_capacity(weekends.len());
    for weekend in weekends.into_iter() {
        let sessions = fetch_sessions(db_conn, &weekend).await?;
        return_weekends.push(FullWeekend {
            weekend,
            sessions,
        });
    }
    Ok(return_weekends)
}

pub async fn fetch_full_weekends(
    db_conn: &mut MySqlConnection
) -> Result<Vec<FullWeekend>, sqlx::Error> {
    let weekends = fetch_weekends(db_conn).await?;
    let mut return_weekends = Vec::with_capacity(weekends.len());
    for weekend in weekends.into_iter() {
        let sessions = fetch_sessions(db_conn, &weekend).await?;
        return_weekends.push(FullWeekend {
            weekend,
            sessions,
        });
    }
    Ok(return_weekends)
}

pub async fn fetch_full_weekend(
    db_conn: &mut MySqlConnection,
    id: u64,
) -> Result<Option<FullWeekend>, sqlx::Error> {
    let weekend =
        sqlx::query_as!(Weekend, "SELECT * FROM weekends WHERE id = ?", id)
            .fetch_optional(&mut *db_conn)
            .await?;
    Ok(match weekend {
        None => None,
        Some(weekend) => {
            let sessions = fetch_sessions(db_conn, &weekend).await?;
            Some(FullWeekend {
                weekend,
                sessions,
            })
        },
    })
}

pub async fn fetch_next_weekend_for_series(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Option<Weekend>, sqlx::Error> {
    sqlx::query_as!(
        Weekend,
        "SELECT * FROM weekends WHERE series = ? AND status != ? ORDER BY start_date ASC LIMIT 1",
        series.i8(),
        WeekendStatus::Done.i8(),
    ).fetch_optional(db_conn).await
}

pub async fn fetch_next_full_weekend_for_series(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Option<FullWeekend>, sqlx::Error> {
    let weekend = fetch_next_weekend_for_series(db_conn, series).await?;
    Ok(match weekend {
        None => None,
        Some(weekend) => Some({
            let sessions = fetch_sessions(db_conn, &weekend).await?;
            FullWeekend {
                weekend,
                sessions,
            }
        }),
    })
}

pub async fn fetch_messages(
    db_conn: &mut MySqlConnection
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(Message, "SELECT * FROM messages").fetch_all(db_conn).await
}

pub async fn fetch_weekend_messages(
    db_conn: &mut MySqlConnection
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE kind = ? ORDER BY message ASC",
        MessageKind::Weekend.i8()
    )
    .fetch_all(db_conn)
    .await
}

pub async fn mark_weekend_message_for_series_expired(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE messages SET expiry = ? WHERE kind = ? AND series = ?",
        Utc::now(),
        MessageKind::Weekend.i8(),
        series.i8()
    )
    .execute(db_conn)
    .await
    .map(|_f| ())
}

pub async fn fetch_weekend_message_for_series(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Option<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE kind = ? and series = ?",
        MessageKind::Weekend.i8(),
        series.i8()
    )
    .fetch_optional(db_conn)
    .await
}

pub async fn expired_messages(
    db_conn: &mut MySqlConnection
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE expiry IS NOT NULL AND expiry < now()"
    )
    .fetch_all(db_conn)
    .await
}

pub async fn fetch_calendar_messages(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE kind = ? AND series = ? ORDER BY message ASC",
        MessageKind::Calendar.i8(),
        series.i8()
    )
    .fetch_all(db_conn)
    .await
}

pub async fn fetch_custom_messages(
    db_conn: &mut MySqlConnection
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE kind = ?",
        MessageKind::Custom.i8()
    )
    .fetch_all(db_conn)
    .await
}

pub async fn fetch_series_calendar_messages(
    db_conn: &mut MySqlConnection,
    series: Series,
) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as!(
        Message,
        "SELECT * FROM messages WHERE series = ? ORDER BY message ASC",
        series.i8()
    )
    .fetch_all(db_conn)
    .await
}

/// Sets a [Messages](Message) expiry date.
/// If *date* is set to [None] then the message is set to expire immediately
pub async fn mark_message_expired(
    db_conn: &mut MySqlConnection,
    id: u64,
    date: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    let date = date.unwrap_or(Utc::now());
    let result =
        sqlx::query!("UPDATE messages SET expiry = ? WHERE id = ?", date, id)
            .execute(db_conn)
            .await?;
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Deletes a [Message]
pub async fn delete_message(
    db_conn: &mut MySqlConnection,
    id: u64,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query!("DELETE FROM messages WHERE id = ?", id)
        .execute(db_conn)
        .await?;
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Checks [Weekends](Weekend) and if all [Sessions](Session) are [Finished](SessionStatus)
/// or [Cancelled](SessionStatus), then mark these Weekends as [Done](SessionStatus).
pub async fn check_weekends(
    db_conn: &mut MySqlConnection
) -> Result<(), sqlx::Error> {
    let weekends = fetch_full_weekends(db_conn).await?;
    for weekend in weekends.into_iter().filter(|p| {
        p.sessions.is_empty() && p.weekend.status == WeekendStatus::Open
    }) {
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
    db_conn: &mut MySqlConnection,
    weekend: &Weekend,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE weekends SET status = ? WHERE id = ?",
        WeekendStatus::Done.i8(),
        weekend.id
    )
    .execute(db_conn)
    .await
    .map(|_f| ())
}

pub async fn mark_session_done(
    db_conn: &mut MySqlConnection,
    session: &Session,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE sessions SET STATUS = ? WHERE id = ?",
        SessionStatus::Finished.i8(),
        session.id
    )
    .execute(db_conn)
    .await
    .map(|_f| ())
}

pub async fn update_message_hash(
    db_conn: &mut MySqlConnection,
    msg_id: u64,
    hash: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE messages SET hash = ? WHERE id = ?",
        hash.to_string(),
        msg_id
    )
    .execute(db_conn)
    .await
    .map(|_f| ())
}

pub fn create_multi_message(
    weekends: &[FullWeekend]
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
