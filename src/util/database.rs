use std::borrow::Cow;

use chrono::{DateTime, Utc};
use sqlx::MySqlPool;

use crate::{
    error::Error,
    model::{
        BotMessage, MessageKind, NotificationSetting, Series, Session,
        SessionKind, SessionStatus, Weekend, WeekendStatus,
    },
};

pub async fn get_current_weekend<'a>(
    pool: &MySqlPool,
    series: Series,
) -> Result<Weekend<'a>, Error> {
    struct QueryWeekend<'b> {
        id: u32,
        name: Cow<'b, str>,
        year: u16,
        icon: Cow<'b, str>,
        start_date: DateTime<Utc>,
        series: Series,
        status: WeekendStatus,
        session_id: u32,
        session_kind: SessionKind,
        session_start_date: DateTime<Utc>,
        session_duration: i64,
        session_title: Option<String>,
        session_number: Option<u8>,
        session_notify: NotificationSetting,
        session_status: SessionStatus,
    }

    struct TempWeekend<'a> {
        pub id: u32,
        pub series: Series,
        pub title: Cow<'a, str>,
        pub icon: Cow<'a, str>,
        pub year: u16,
        pub start_date: DateTime<Utc>,
        pub status: WeekendStatus,
    }

    impl<'b> TryFrom<Vec<QueryWeekend<'b>>> for Weekend<'b> {
        type Error = Error;
        fn try_from(value: Vec<QueryWeekend<'b>>) -> Result<Self, Error> {
            let mut sessions = Vec::with_capacity(value.len());
            let mut weekend = None;
            for field in value.into_iter() {
                if weekend.is_none() {
                    weekend = Some(TempWeekend {
                        id: field.id,
                        series: field.series,
                        title: field.name,
                        icon: field.icon,
                        year: field.year,
                        start_date: field.start_date,
                        status: field.status,
                    });
                }
                sessions.push(Session {
                    id: field.session_id,
                    date: field.session_start_date,
                    kind: field.session_kind,
                    number: field.session_number,
                    title: field.session_title,
                    notify: field.session_notify,
                    status: field.session_status,
                    weekend: field.id,
                    duration: field.session_duration,
                });
            }
            let Some(weekend) = weekend else {
                return Err(Error::NotFound);
            };
            Ok(Weekend {
                id: weekend.id,
                series: weekend.series,
                name: weekend.title,
                icon: weekend.icon,
                sessions,
                year: weekend.year,
                start_date: weekend.start_date,
                status: weekend.status,
            })
        }
    }

    let result = sqlx::query_as!(
        QueryWeekend,
        "SELECT 
	weekends.*, 
	sessions.id as session_id,
	sessions.kind as session_kind,
	sessions.start_date as session_start_date,
	sessions.duration as session_duration,
	sessions.title as session_title,
	sessions.number as session_number,
    sessions.notify as session_notify,
    sessions.status as session_status
FROM weekends 
JOIN sessions on weekends.id = sessions.weekend
WHERE weekends.id = (
		SELECT id 
		FROM weekends
		WHERE NOT status = \"Done\"
        AND series = ?
		ORDER BY ABS( DATEDIFF(weekends.start_date, now() ))
		LIMIT 1
	)
ORDER BY session_start_date ASC",
        series
    )
    .fetch_all(pool)
    .await?;

    result.try_into()
}

pub async fn get_expired_messages(
    pool: &MySqlPool
) -> Result<Vec<BotMessage>, sqlx::Error> {
    sqlx::query_as!(
        BotMessage,
        "SELECT * FROM messages WHERE kind = ? AND posted < DATE_SUB(NOW(), INTERVAL 30 MINUTE)",
        MessageKind::Notification
    )
    .fetch_all(pool)
    .await
}

pub async fn get_all_weekends<'a>(
    pool: &MySqlPool,
    series: Series,
) -> Result<Vec<Weekend<'a>>, Error> {
    struct QueryWeekend<'b> {
        id: u32,
        name: Cow<'b, str>,
        year: u16,
        icon: Cow<'b, str>,
        start_date: DateTime<Utc>,
        series: Series,
        status: WeekendStatus,
        session_id: u32,
        session_kind: SessionKind,
        session_start_date: DateTime<Utc>,
        session_duration: i64,
        session_title: Option<String>,
        session_number: Option<u8>,
        session_notify: NotificationSetting,
        session_status: SessionStatus,
    }

    struct TempWeekend<'a> {
        pub id: u32,
        pub series: Series,
        pub title: Cow<'a, str>,
        pub icon: Cow<'a, str>,
        pub year: u16,
        pub start_date: DateTime<Utc>,
        pub status: WeekendStatus,
    }

    fn weekends(value: Vec<QueryWeekend>) -> Vec<Weekend> {
        let mut weekends = Vec::with_capacity(24);
        let mut sessions = Vec::with_capacity(5);
        let mut weekend: Option<TempWeekend> = None;
        for field in value.into_iter() {
            if weekend.is_none()
                || weekend.as_ref().is_some_and(|f| f.id != field.id)
            {
                if let Some(weekend_u) = weekend.take() {
                    weekends.push(Weekend {
                        id: weekend_u.id,
                        series: weekend_u.series,
                        name: weekend_u.title,
                        icon: weekend_u.icon,
                        sessions,
                        year: weekend_u.year,
                        start_date: weekend_u.start_date,
                        status: weekend_u.status,
                    });
                    sessions = Vec::with_capacity(5);
                }

                weekend = Some(TempWeekend {
                    id: field.id,
                    series: field.series,
                    title: field.name,
                    icon: field.icon,
                    year: field.year,
                    start_date: field.start_date,
                    status: field.status,
                });
            }

            sessions.push(Session {
                id: field.session_id,
                date: field.session_start_date,
                kind: field.session_kind,
                number: field.session_number,
                title: field.session_title,
                notify: field.session_notify,
                status: field.session_status,
                weekend: field.id,
                duration: field.session_duration,
            });
        }
        if let Some(weekend) = weekend.take() {
            weekends.push(Weekend {
                id: weekend.id,
                series: weekend.series,
                name: weekend.title,
                icon: weekend.icon,
                sessions,
                year: weekend.year,
                start_date: weekend.start_date,
                status: weekend.status,
            });
        }
        weekends
    }

    let result = sqlx::query_as!(
        QueryWeekend,
        "SELECT
	weekends.*, 
	sessions.id as session_id,
	sessions.kind as session_kind,
	sessions.start_date as session_start_date,
	sessions.duration as session_duration,
	sessions.title as session_title,
	sessions.number as session_number,
    sessions.notify as session_notify,
    sessions.status as session_status
FROM weekends 
JOIN sessions on weekends.id = sessions.weekend
WHERE weekends.status = \"Open\" AND weekends.series = ?
ORDER BY session_start_date ASC",
        series
    )
    .fetch_all(pool)
    .await?;
    Ok(weekends(result))
}

pub async fn get_weekends_without_sessions<'r, 'e>(
    pool: impl sqlx::MySqlExecutor<'_>,
    series: Series,
) -> Result<Vec<Weekend<'e>>, sqlx::Error> {
    struct QueryData<'a> {
        pub id: u32,
        pub series: Series,
        pub name: Cow<'a, str>,
        pub icon: Cow<'a, str>,
        pub year: u16,
        pub start_date: DateTime<Utc>,
        pub status: WeekendStatus,
    }

    impl<'a, 'b> From<QueryData<'a>> for Weekend<'b>
    where
        'a: 'b,
    {
        fn from(value: QueryData<'a>) -> Self {
            Self {
                id: value.id,
                name: value.name,
                series: value.series,
                icon: value.icon,
                year: value.year,
                start_date: value.start_date,
                status: value.status,
                sessions: Vec::with_capacity(0),
            }
        }
    }

    Ok(sqlx::query_as!(
        QueryData,
        "SELECT * from weekends WHERE series = ?
ORDER BY start_date ASC
        ",
        series
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|f| f.into())
    .collect())
}
