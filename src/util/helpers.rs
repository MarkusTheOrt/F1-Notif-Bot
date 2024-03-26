use std::{
    fs::File,
    io::{self, Write},
    process::exit,
};

use crate::{config::Config, error::Error};

pub fn handle_config_error(why: std::io::Error) -> ! {
    if let io::ErrorKind::NotFound = why.kind() {
        println!("Generated default config file, please update settings.");
        if let Err(config_why) = generate_default_config() {
            eprintln!("Error generating config: `{config_why}`")
        }
        exit(0x0100)
    } else {
        eprintln!("Error reading config file: {why}");
        exit(0x0100)
    }
}

fn generate_default_config() -> Result<(), Error> {
    let config = Config::default();
    let str_to_write = toml::to_string_pretty(&config)?;
    let mut config_file = File::create("./config/config.toml")?;
    config_file.write_all(str_to_write.as_bytes())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ID(pub i64);

impl From<u64> for ID {
    fn from(value: u64) -> Self {
        unsafe { Self(std::mem::transmute(value)) }
    }
}

impl From<i64> for ID {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl ID {
    pub fn i64(self) -> i64 {
        self.0
    }

    pub fn u64(self) -> u64 {
        unsafe { std::mem::transmute(self.0) }
    }
}

impl From<ID> for i64 {
    fn from(val: ID) -> Self {
        val.i64()
    }
}

impl From<ID> for u64 {
    fn from(val: ID) -> Self {
        val.u64()
    }
}

impl From<Option<u64>> for ID {
    fn from(value: Option<u64>) -> Self {
        match value {
            None => Self(0),
            Some(val) => unsafe { Self(std::mem::transmute(val)) },
        }
    }
}

impl From<Option<i64>> for ID {
    fn from(value: Option<i64>) -> Self {
        match value {
            None => Self(0),
            Some(val) => Self(val),
        }
    }
}

impl From<ID> for Option<i64> {
    fn from(value: ID) -> Self {
        Some(value.i64())
    }
}

impl From<ID> for Option<u64> {
    fn from(value: ID) -> Self {
        Some(value.u64())
    }
}
