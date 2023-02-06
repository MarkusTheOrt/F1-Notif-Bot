use std::{
    fmt,
    io,
};

use std::error::Error as StdError;

use std::result::Result as StdResult;

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    Toml(toml::ser::Error),
    Mongo(mongodb::error::Error),
    Serenity(serenity::Error),
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

impl From<mongodb::error::Error> for Error {
    fn from(value: mongodb::error::Error) -> Self {
        Error::Mongo(value)
    }
}

impl From<serenity::Error> for Error {
    fn from(value: serenity::Error) -> Self {
        Error::Serenity(value)
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
            Self::Mongo(inner) => fmt::Display::fmt(&inner, f),
            Self::Serenity(inner) => fmt::Display::fmt(&inner, f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(inner) => Some(inner),
            Self::Toml(inner) => Some(inner),
            Self::Mongo(inner) => Some(inner),
            Self::Serenity(inner) => Some(inner),
        }
    }
}
