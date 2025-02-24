use std::collections::HashMap;

use anyhow::bail;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub port: u16,
    pub justlogs_instances: HashMap<String, JustlogsInstance>,
    pub recentmessages_instances: HashMap<String, JustlogsInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub umami_stats: Option<UmamiStats>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct JustlogsInstance {
    maintainer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    alternate: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct UmamiStats {
    token: String,
    id: String,
    url: String,
}

pub async fn get_config() -> anyhow::Result<Config> {
    const EXAMPLE_STR: &str = "./example_config.json";
    const CONFIG_STR: &str = "./config.json";

    let example_exists = std::fs::exists(EXAMPLE_STR);
    let config_exists = std::fs::exists(CONFIG_STR);

    // TODO: i'm too lazy to do merging so i'm just gonna do this stupid siht
    match (example_exists, config_exists) {
        (_, Ok(true)) => {
            let config_str = std::fs::read_to_string(CONFIG_STR)?;
            let config_json: Config = serde_json::from_str(&config_str)?;

            Ok(config_json)
        }
        (Ok(true), Ok(false)) => {
            bail!("Example config exists but not a real config. too bad.")
        }
        _ => todo!(),
    }
}
