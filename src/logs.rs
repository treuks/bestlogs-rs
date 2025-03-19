use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use crate::Channel;
use crate::UserType;
use crate::config;
use crate::config::Config;
use chrono::Utc;
use futures::{Stream, StreamExt, stream};
use poem_openapi::Object;
use thiserror::Error;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChannelsResponse {
    pub channels: Vec<Channel>,
}

#[derive(Error, Debug)]
pub enum GenericRequestError {
    #[error("Error while making a request: {0:#}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Error while deserializing: {0:#}")]
    DeserError(#[from] serde_json::Error),
}

pub async fn get_all_channels<'a>(
    req_client: &reqwest::Client,
    instances: &'a HashMap<String, config::JustlogsInstance>,
) -> impl Stream<Item = Result<(&'a str, ChannelsResponse), GenericRequestError>> {
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

#[derive(serde::Serialize, serde::Deserialize, Debug, Eq, Hash, PartialEq, Object, Clone)]
pub struct NamehistoryResponse {
    pub user_login: String,
    pub last_timestamp: chrono::DateTime<Utc>,
    pub first_timestamp: chrono::DateTime<Utc>,
}

pub async fn get_name_history<'a>(
    req_client: &reqwest::Client,
    instances: &'a HashMap<String, config::JustlogsInstance>,
    user_id: &str,
) -> impl Stream<Item = Result<Vec<NamehistoryResponse>, GenericRequestError>> {
    stream::iter(instances.keys())
        .map(move |url| async move {
            let res = req_client
                .get(format!("https://{url}/namehistory/{user_id}"))
                .send()
                .await?;

            let res_text = res.text().await?;

            let json = serde_json::from_str::<Vec<NamehistoryResponse>>(&res_text)?;

            Ok(json)
        })
        .buffer_unordered(20)
}

pub struct Params {
    pub force: bool,
    pub pretty: bool,
}

pub async fn get_logs(
    cfg: &Config,
    req_client: &reqwest::Client,
    channel: &'_ UserType<'_>,
    user: &'_ UserType<'_>,
    params: Params,
) {
}
