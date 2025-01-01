use std::{fmt, io};

use std::error::Error as StdError;

use std::result::Result as StdResult;

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    Toml(toml::ser::Error),
    Serenity(serenity::Error),
    Sqlx(sqlx::Error),
    NotFound,
    NotSameLen,
    ParseInt(std::num::ParseIntError),
    NNF(Box<dyn StdError>),
}

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Error::Sqlx(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Error::Toml(value)
    }
}

impl From<serenity::Error> for Error {
    fn from(value: serenity::Error) -> Self {
        Error::Serenity(value)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(value: std::num::ParseIntError) -> Self {
        Error::ParseInt(value)
    }
}

impl From<Box<dyn StdError>> for Error {
    fn from(value: Box<dyn StdError>) -> Self {
        Error::NNF(value)
    }
}

impl fmt::Display for Error {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::Io(inner) => fmt::Display::fmt(&inner, f),
            Self::Toml(inner) => fmt::Display::fmt(&inner, f),
            Self::Sqlx(inner) => fmt::Display::fmt(&inner, f),
            Self::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Self::NotFound => f.write_str("Not Found (LIB Error)"),
            Self::NotSameLen => {
                f.write_str("Two Iterators are not the same len.")
            },
            Self::ParseInt(inner) => fmt::Display::fmt(&inner, f),
            Self::NNF(inner) => fmt::Display::fmt(&inner, f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(inner) => Some(inner),
            Self::Toml(inner) => Some(inner),
            Self::Serenity(inner) => Some(inner),
            Self::Sqlx(inner) => Some(inner),
            Self::NotFound => None,
            Self::NotSameLen => None,
            Self::ParseInt(inner) => Some(inner),
            Self::NNF(inner) => inner.source(),
        }
    }
}
