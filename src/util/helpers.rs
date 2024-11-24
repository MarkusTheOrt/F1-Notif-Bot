use std::{
    fs::File,
    io::{self, Write},
    process::exit,
};

use tracing::{error, info};

use crate::{config::Config, error::Error};

pub fn handle_config_error(why: std::io::Error) -> ! {
    if let io::ErrorKind::NotFound = why.kind() {
        info!("Generated default config file, please update settings.");
        if let Err(config_why) = generate_default_config() {
            error!("Error generating config: `{config_why}`")
        }
        exit(0x0100)
    } else {
        info!("Error reading config file: {why}");
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
