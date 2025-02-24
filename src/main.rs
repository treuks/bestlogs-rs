use std::{collections::HashMap, time::Duration};

mod config;

use futures::future;
use poem::{Route, Server, listener::TcpListener};
use poem_openapi::{OpenApi, OpenApiService, payload::PlainText};
use serde_json::Value;

struct Api;

#[OpenApi(prefix_path = "/api")]
impl Api {
    /// Hello world
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<&'static str> {
        PlainText("Hello World")
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ChannelsResponse {
    channels: Vec<Channel>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Channel {
    name: String,
    #[serde(alias = "userID")]
    user_id: String,
}

async fn get_all_channels(
    req_client: &reqwest::Client,
    instances: &HashMap<String, config::JustlogsInstance>,
) -> Vec<Result<(String, String), reqwest::Error>> {
    let reqs = future::join_all(instances.keys().map(|url| async move {
        let res = req_client
            .get(format!("https://{url}/channels"))
            .send()
            .await?;

        Ok((url.clone(), res.text().await?))
    }))
    .await;

    reqs
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let api_service =
    //     OpenApiService::new(Api, "Hello World", "1.0").server("http://localhost:3000");
    // let ui = api_service.swagger_ui();
    // let app = Route::new().nest("/api", api_service).nest("/docs", ui);

    // let _ = Server::new(TcpListener::bind("127.0.0.1:4310"))
    //     .run(app)
    //     .await;

    let cfg = config::get_config().await?;

    static REQ_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

    let req_client = reqwest::Client::builder()
        .user_agent(REQ_USER_AGENT)
        .timeout(Duration::from_secs(5))
        .build()?;

    let channels = get_all_channels(&req_client, &cfg.justlogs_instances).await;

    let instance_data = channels
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect::<Vec<_>>();

    let mut has: HashMap<String, Vec<Channel>> = HashMap::new();

    for (url, text) in instance_data {
        match serde_json::from_str::<ChannelsResponse>(&text) {
            Ok(res) => {
                has.insert(url.to_string(), res.channels);
            }
            Err(_) => (),
        }
    }

    dbg!(has);

    Ok(())
}
