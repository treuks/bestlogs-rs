use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod config;

use config::Config;
use futures::future;
use poem::{Route, Server, listener::TcpListener};
use poem_openapi::{Object, OpenApi, OpenApiService, payload::PlainText};

#[derive(Debug, serde::Serialize, Object)]
#[serde(rename_all = "camelCase")]
#[oai(rename_all = "camelCase")]
struct ChannelsOutput {
    instances_stats: InstancesStats,
    channels: Arc<HashSet<Channel>>,
}

#[derive(Debug, Object, serde::Serialize)]
#[oai(rename_all = "camelCase")]
struct InstancesOutput {
    instances_stats: InstancesStats,
    instances: Arc<HashMap<String, Vec<Channel>>>,
}

#[derive(Debug, serde::Serialize, Object, Clone)]
struct InstancesStats {
    count: usize,
    down: usize,
}

struct Api<'a> {
    req_client: reqwest::Client,
    cfg: &'a Config,
    channel_map: Arc<HashMap<String, Vec<Channel>>>,
    unique_channels: Arc<HashSet<Channel>>,
    stats: InstancesStats,
}

static REQ_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[OpenApi(prefix_path = "/")]
impl Api<'static> {
    /// Hello world
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<&'static str> {
        PlainText("Hello World")
    }

    /// List all channels
    #[oai(path = "/channels", method = "get")]
    async fn channels(&self) -> poem_openapi::payload::Json<ChannelsOutput> {
        poem_openapi::payload::Json(ChannelsOutput {
            instances_stats: self.stats.clone(),
            channels: self.unique_channels.clone(),
        })
    }

    /// List all instances
    #[oai(path = "/instances", method = "get")]
    async fn instances(&self) -> poem_openapi::payload::Json<InstancesOutput> {
        poem_openapi::payload::Json(InstancesOutput {
            instances_stats: self.stats.clone(),
            instances: self.channel_map.clone(),
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ChannelsResponse {
    channels: Vec<Channel>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Object, Eq, Hash, PartialEq)]
struct Channel {
    name: String,
    #[serde(alias = "userID")]
    user_id: String,
}

async fn get_all_channels(
    req_client: &reqwest::Client,
    instances: &HashMap<String, config::JustlogsInstance>,
) -> Vec<Result<(String, String), reqwest::Error>> {
    future::join_all(instances.keys().map(|url| async move {
        let res = req_client
            .get(format!("https://{url}/channels"))
            .send()
            .await?;

        Ok((url.clone(), res.text().await?))
    }))
    .await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let req_client = reqwest::Client::builder()
        .user_agent(REQ_USER_AGENT)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let cfg = Box::leak(Box::new(config::get_config()?));

    eprintln!("Initiated from config");

    eprintln!("Getting data on all channels.");

    let now = Instant::now();

    let channels = get_all_channels(&req_client, &cfg.justlogs_instances).await;

    eprintln!("Got channels, it took {}ms", now.elapsed().as_millis());

    let instance_data = &channels
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect::<Vec<_>>();

    let mut downed_instances = channels.len() - instance_data.len();

    let mut has: HashMap<String, Vec<Channel>> = HashMap::new();

    let mut all_channels: HashSet<Channel> = HashSet::new();

    for (url, text) in instance_data {
        match serde_json::from_str::<ChannelsResponse>(text) {
            Ok(res) => {
                for channel in &res.channels {
                    all_channels.insert(channel.clone());
                }
                has.insert(url.to_string(), res.channels);
            }
            Err(_) => downed_instances += 1,
        }
    }

    eprintln!("Starting up server: http://localhost:{}", &cfg.port);
    let api_service = OpenApiService::new(
        Api {
            req_client,
            cfg,
            channel_map: Arc::new(has),
            unique_channels: Arc::new(all_channels),
            stats: InstancesStats {
                down: downed_instances,
                count: channels.len(),
            },
        },
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    )
    .server(format!("http://localhost:{}", &cfg.port));
    let ui = api_service.redoc();
    let app = Route::new().nest("/", api_service).nest("/docs", ui);

    let _ = Server::new(TcpListener::bind(format!("127.0.0.1:{}", &cfg.port)))
        .run(app)
        .await;

    Ok(())
}
