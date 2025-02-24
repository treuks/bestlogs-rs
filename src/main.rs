use std::{collections::HashMap, time::Duration};

mod config;

use config::Config;
use futures::future;
use poem::{Route, Server, listener::TcpListener, web::Json};
use poem_openapi::{Object, OpenApi, OpenApiService, payload::PlainText};
use serde_json::Value;

#[derive(Debug, serde::Deserialize, serde::Serialize, Object)]
#[serde(rename_all = "camelCase")]
#[oai(rename_all = "camelCase")]
struct ChannelsOutput {
    instances_stats: InstancesStats,
    channels: Vec<Channel>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Object)]
struct InstancesStats {
    count: usize,
    down: usize,
}

struct Api {
    req_client: reqwest::Client,
    cfg: Config,
    channels: Option<HashMap<String, Vec<Channel>>>,
}

static REQ_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[OpenApi(prefix_path = "/")]
impl Api {
    /// Hello world
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<&'static str> {
        PlainText("Hello World")
    }

    #[oai(path = "/channels", method = "get")]
    async fn channels(&self) -> poem_openapi::payload::Json<ChannelsOutput> {
        // TODO: get all of this shit out of here
        let channels = get_all_channels(&self.req_client, &self.cfg.justlogs_instances).await;

        let instance_data = &channels
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

        // TODO: get an actual number
        poem_openapi::payload::Json(ChannelsOutput {
            instances_stats: InstancesStats {
                count: has.len(),
                down: 69,
            },
            channels: has.values().cloned().flat_map(|v| v.into_iter()).collect(),
        })
    }
}

impl Default for Api {
    fn default() -> Self {
        let req_client = reqwest::Client::builder()
            .user_agent(REQ_USER_AGENT)
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let cfg = config::get_config().expect("oops");

        Self {
            req_client,
            cfg,
            channels: None,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ChannelsResponse {
    channels: Vec<Channel>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Object)]
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
    let api_service =
        OpenApiService::new(Api::default(), "Hello World", "1.0").server("http://localhost:4310");
    let ui = api_service.redoc();
    let app = Route::new().nest("/", api_service).nest("/docs", ui);

    let _ = Server::new(TcpListener::bind("127.0.0.1:4310"))
        .run(app)
        .await;

    Ok(())
}
