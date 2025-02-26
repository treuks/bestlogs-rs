use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use thiserror::Error;
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[allow(non_upper_case_globals)]
#[unsafe(export_name = "malloc_conf")]
pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

mod config;

use config::Config;
use futures::{Stream, StreamExt, stream};
use poem::{Route, Server, listener::TcpListener};
use poem_openapi::payload::{self, Binary};
use poem_openapi::{Object, OpenApi, OpenApiService, payload::PlainText};

#[derive(Debug, serde::Serialize, Object)]
#[serde(rename_all = "camelCase")]
struct ChannelsOutput {
    #[serde(alias = "instancesStats")]
    #[oai(rename = "instancesStats")]
    instances_stats: InstancesStats,
    channels: Arc<HashSet<Channel>>,
}

#[derive(Debug, Object, serde::Serialize)]
struct InstancesOutput {
    #[serde(alias = "instancesStats")]
    #[oai(rename = "instancesStats")]
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

#[derive(poem_openapi::ApiResponse)]
enum ProfileResponseError {
    #[oai(status = 409)]
    ProfilingDisabled,
    #[oai(status = 500)]
    DumpFailure(PlainText<String>),
}

#[OpenApi(prefix_path = "/")]
impl Api<'static> {
    /// Hello world
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> payload::PlainText<&'static str> {
        PlainText("Hello World")
    }

    /// Get the prof file
    /// This gives you a binary .pb.gz file which allows you (me) to debug the ram usage.
    #[oai(path = "/heap", method = "get")]
    async fn heap(&self) -> Result<payload::Binary<Vec<u8>>, ProfileResponseError> {
        let mut prof_ctl = jemalloc_pprof::PROF_CTL.as_ref().unwrap().lock().await;

        if !prof_ctl.activated() {
            return Err(ProfileResponseError::ProfilingDisabled);
        }

        let pprof = prof_ctl
            .dump_pprof()
            .map_err(|e| ProfileResponseError::DumpFailure(PlainText(e.to_string())))?;

        Ok(Binary(pprof))
    }

    /// List all channels
    #[oai(path = "/channels", method = "get")]
    async fn channels(&self) -> payload::Json<ChannelsOutput> {
        payload::Json(ChannelsOutput {
            instances_stats: self.stats.clone(),
            channels: self.unique_channels.clone(),
        })
    }

    /// List all instances
    #[oai(path = "/instances", method = "get")]
    async fn instances(&self) -> payload::Json<InstancesOutput> {
        payload::Json(InstancesOutput {
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
    #[oai(rename = "userID")]
    user_id: String,
}

#[derive(Error, Debug)]
enum ChannelsError {
    #[error("Error while making a request")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Error while deserializing")]
    DeserError(#[from] serde_json::Error),
}

async fn get_all_channels<'a>(
    req_client: &reqwest::Client,
    instances: &'a HashMap<String, config::JustlogsInstance>,
) -> impl Stream<Item = Result<(&'a str, ChannelsResponse), ChannelsError>> {
    stream::iter(instances.keys())
        .map(move |url| async move {
            let res = req_client
                .get(format!("https://{url}/channels"))
                .send()
                .await?;

            let res_text = res.text().await?;

            let json = serde_json::from_str::<ChannelsResponse>(&res_text)?;

            Ok((url.as_str(), json))
        })
        .buffer_unordered(20)
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

    let mut instance_data = Vec::with_capacity(cfg.justlogs_instances.len());
    let mut downed_instances = 0;

    let binding = req_client.clone();
    let mut xd = get_all_channels(&binding, &cfg.justlogs_instances).await;
    while let Some(result) = xd.next().await {
        match result {
            Ok((url, channel)) => {
                instance_data.push((url, channel));
                dbg!(&instance_data);
            }
            Err(_err) => {
                // Maybe print log here
                downed_instances += 1;
            }
        }
    }

    let total_channel_count = instance_data.len();

    let mut kv: HashMap<String, Vec<Channel>> = HashMap::new();
    let mut unique_channels = HashSet::new();

    for (url, data) in instance_data {
        for channel in &data.channels {
            unique_channels.insert(channel.clone());
        }

        kv.insert(url.to_string(), data.channels);
    }

    eprintln!("Got channels, it took {}ms", now.elapsed().as_millis());

    eprintln!("Starting up server: http://localhost:{}", &cfg.port);
    let api_service = OpenApiService::new(
        Api {
            req_client,
            cfg,
            channel_map: Arc::new(kv),
            unique_channels: Arc::new(unique_channels),
            stats: InstancesStats {
                down: downed_instances,
                count: total_channel_count,
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
