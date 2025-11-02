use std::{fmt, io};

use std::error::Error as StdError;

use std::result::Result as StdResult;

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    Toml(toml::ser::Error),
    Serenity(Box<serenity::Error>),
    Libsql(libsql::Error),
    NotFound,
    NotSameLen,
    ParseInt(std::num::ParseIntError),
    NNF(Box<dyn StdError>),
    Fmt(std::fmt::Error),
    De(serde::de::value::Error),
}

impl From<serde::de::value::Error> for Error {
    fn from(value: serde::de::value::Error) -> Self {
        Error::De(value)
    }
}

impl From<libsql::Error> for Error {
    fn from(value: libsql::Error) -> Self {
        Error::Libsql(value)
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
        Error::Serenity(Box::new(value))
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

impl From<std::fmt::Error> for Error {
    fn from(value: std::fmt::Error) -> Self {
        Error::Fmt(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(inner) => fmt::Display::fmt(&inner, f),
            Self::Toml(inner) => fmt::Display::fmt(&inner, f),
            Self::Libsql(inner) => fmt::Display::fmt(&inner, f),
            Self::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Self::NotFound => f.write_str("Not Found (LIB Error)"),
            Self::NotSameLen => f.write_str("Two Iterators are not the same len."),
            Self::ParseInt(inner) => fmt::Display::fmt(&inner, f),
            Self::NNF(inner) => fmt::Display::fmt(&inner, f),
            Self::Fmt(inner) => fmt::Display::fmt(&inner, f),
            Self::De(inner) => fmt::Display::fmt(&inner, f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(inner) => Some(inner),
            Self::Toml(inner) => Some(inner),
            Self::Serenity(inner) => Some(inner),
            Self::Libsql(inner) => Some(inner),
            Self::NotFound => None,
            Self::NotSameLen => None,
            Self::ParseInt(inner) => Some(inner),
            Self::NNF(inner) => inner.source(),
            Self::Fmt(inner) => inner.source(),
            Self::De(inner) => inner.source(),
        }
    }
}
