use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub discord: Discord,
    pub mongo: Mongo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Discord {
    pub bot_token: String,
    pub guild: u64,
    pub channel: u64,
    pub role: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Mongo {
    pub url: String,
    pub user: String,
    pub password: String,
    pub database: String,
    pub protocol: String,
    pub suffix: String,
}

impl Default for Discord {
    fn default() -> Self {
        Self {
            bot_token: "DISCORD_BOT_TOKEN".to_owned(),
            guild: 883847530687913995,
            channel: 1002285400095719524,
            role: 1033311726889861244,
        }
    }
}

impl Default for Mongo {
    fn default() -> Self {
        Self {
            url: "localhost:27017".to_owned(),
            user: "notificationsbot".to_owned(),
            password: "YOUR_PASSWORD".to_owned(),
            database: "notifbot".to_owned(),
            protocol: "mongodb".to_owned(),
            suffix: "".to_owned(),
        }
    }
}
