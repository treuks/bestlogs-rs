use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::json;
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
mod ivr;
mod logs;
mod umami;

use config::Config;
use futures::{Stream, StreamExt, stream};
use poem::{EndpointExt, Request, Route, Server, listener::TcpListener};
use poem_openapi::{Object, OpenApi, OpenApiService, payload::PlainText};
use poem_openapi::{
    param::Path,
    payload::{self, Binary},
};
use umami::send_to_umami;

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

pub enum UserType<'a> {
    Id(&'a str),
    Login(&'a str),
}

pub fn parse_id_arg(arg: &str) -> Option<UserType> {
    if arg.starts_with("id:") {
        Some(UserType::Id(arg.strip_prefix("id:")?))
    } else {
        Some(UserType::Login(arg))
    }
}

pub fn parse_name_arg(arg: &str) -> Option<UserType> {
    if arg.starts_with("login:") {
        Some(UserType::Login(arg.strip_prefix("login:")?))
    } else {
        Some(UserType::Id(arg))
    }
}

struct Api {
    req_client: reqwest::Client,
    cfg: &'static Config,
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

#[derive(poem_openapi::ApiResponse)]
enum NamehistoryResponseError {
    // @ZonianMidian FeelsWeirdMan
    // This should be a 400 but it's not so I'm copying the behaviour.
    #[oai(status = 500)]
    InvalidFormat(PlainText<String>),
    #[oai(status = 503)]
    IvrFail,
    #[oai(status = 404)]
    NoSuchUser,
}

#[OpenApi(prefix_path = "/")]
impl Api {
    /// Hello world
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> payload::PlainText<&'static str> {
        PlainText("Hello World")
    }

    /// # Get the prof file
    ///
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

    /// Get user name history
    #[oai(path = "/namehistory/:user", method = "get")]
    async fn namehistory(
        &self,
        user: Path<String>,
    ) -> Result<payload::Json<HashSet<logs::NamehistoryResponse>>, NamehistoryResponseError> {
        let req_client = self.req_client.clone();
        let justlog_instances = &self.cfg.justlogs_instances;

        let name = match parse_name_arg(&user) {
            Some(n) => n,
            None => {
                return Err(NamehistoryResponseError::InvalidFormat(PlainText(
                    "The value must be an ID or use 'login:' to refer to usernames. Example: 754201843 or login:zonianmidian".to_string(),
                )));
            }
        };

        let id = match name {
            UserType::Id(id) => id.to_string(),
            UserType::Login(login) => {
                let ivr_res = ivr::get_ids_from_login(&self.cfg, &req_client, login).await;
                let id = match ivr_res {
                    Ok(res) => res.id,
                    Err(err) => match err {
                        ivr::IvrResponseError::ReqwestError(er) => {
                            eprintln!("ERROR: {er}");
                            return Err(NamehistoryResponseError::IvrFail);
                        }
                        ivr::IvrResponseError::DeserError(er) => {
                            eprintln!("ERROR: {er}");
                            return Err(NamehistoryResponseError::IvrFail);
                        }
                        ivr::IvrResponseError::NoInfo => {
                            return Err(NamehistoryResponseError::NoSuchUser);
                        }
                    },
                };
                id
            }
        };

        let mut namechanges = Vec::new();

        let mut xd = logs::get_name_history(&req_client, &justlog_instances, &id).await;
        while let Some(result) = xd.next().await {
            match result {
                Ok(res) => {
                    namechanges.push(res);
                }
                Err(err) => {
                    eprintln!("ERROR: {err}");
                }
            }
        }

        let mut namechangeset = HashSet::new();

        for v in namechanges {
            for vv in v {
                namechangeset.insert(vv);
            }
        }

        Ok(payload::Json(namechangeset))
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Object, Eq, Hash, PartialEq)]
struct Channel {
    name: String,
    #[serde(alias = "userID")]
    #[oai(rename = "userID")]
    user_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let req_client = reqwest::Client::builder()
        .user_agent(REQ_USER_AGENT)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let cfg = Box::leak(Box::new(config::get_config()?));

    eprintln!("INFO: Initiated from config.");

    eprintln!("INFO: Getting data on all channels.");

    let now = Instant::now();

    let mut instance_data = Vec::with_capacity(cfg.justlogs_instances.len());
    let mut downed_instances = 0;

    let binding = req_client.clone();
    let mut xd = logs::get_all_channels(&binding, &cfg.justlogs_instances).await;
    while let Some(result) = xd.next().await {
        match result {
            Ok((url, channel)) => {
                instance_data.push((url, channel));
            }
            Err(err) => {
                eprintln!("ERROR: {err}");
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

    eprintln!(
        "INFO: Got channels, it took {}ms.",
        now.elapsed().as_millis()
    );

    eprintln!("INFO: Starting up server: http://localhost:{}", &cfg.port);
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
