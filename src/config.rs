use std::collections::HashMap;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    port: u16,
    justlogs_instances: HashMap<String, JustlogsInstance>,
    recentmessages_instances: HashMap<String, JustlogsInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    umami_stats: Option<UmamiStats>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct JustlogsInstance {
    maintainer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    alternate: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct UmamiStats {
    token: String,
    id: String,
    url: String,
}

pub fn get_config() -> anyhow::Result<Config> {
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
        _ => todo!(),
    }
}
