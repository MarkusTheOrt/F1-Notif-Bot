use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config<'a> {
    pub discord: DiscordConfig<'a>,
    pub database: DatabaseConfig<'a>,
}

impl<'a> Config<'a> {
    pub fn db_string(&self) -> String {
        format!(
            "mysql://{}:{}@{}/{}",
            self.database.username,
            self.database.password,
            self.database.url,
            self.database.database
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiscordConfig<'a> {
    pub bot_token: Cow<'a, str>,
    pub guild: u64,
    pub f1_channel: u64,
    pub f1_role: u64,
    pub f2_channel: u64,
    pub f2_role: u64,
    pub f3_channel: u64,
    pub f3_role: u64,
    pub f1a_role: u64,
    pub f1a_channel: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DatabaseConfig<'a> {
    pub url: Cow<'a, str>,
    pub username: Cow<'a, str>,
    pub password: Cow<'a, str>,
    pub database: Cow<'a, str>,
}

impl Default for DatabaseConfig<'_> {
    fn default() -> Self {
        Self {
            url: "mysql://127.0.0.1:3306".into(),
            username: "notifbot".into(),
            password: "password".into(),
            database: "notifbot".into(),
        }
    }
}

impl Default for DiscordConfig<'_> {
    fn default() -> Self {
        Self {
            bot_token: "DISCORD_BOT_TOKEN".into(),
            guild: 883847530687913995,
            f1_channel: 1002285400095719524,
            f1_role: 1033311726889861244,
            f2_channel: 1002285400095719524,
            f2_role: 1033311726889861244,
            f3_channel: 1002285400095719524,
            f3_role: 1033311726889861244,
            f1a_channel: 1002285400095719524,
            f1a_role: 1033311726889861244,
        }
    }
}
