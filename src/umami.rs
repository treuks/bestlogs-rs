use serde_json::json;
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, serde::Serialize)]
pub struct UmamiSendPayload<'a, T: Send + serde::Serialize> {
    pub hostname: &'a str,
    pub language: &'a str,
    pub referrer: &'a str,
    pub url: &'a str,
    pub website: &'a str,
    pub name: &'a str,

    pub data: Option<&'a T>,
}

#[derive(Error, Debug)]
pub enum UmamiError {
    #[error("Error while making a request")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Umami is not defined in the config")]
    UmamiUndefined,
}

pub async fn send_to_umami<'a, T: Sync + Send + serde::Serialize>(
    cfg: &'static Config,
    req_client: reqwest::Client,
    payload: &'a UmamiSendPayload<'_, &'_ T>,
) -> Result<(), UmamiError> {
    let umami = match &cfg.umami_stats {
        Some(c) => c,
        None => return Err(UmamiError::UmamiUndefined),
    };

    let json_payload = json!({
        "type": "event",
        "payload": &payload,
    });

    req_client
        .post(format!("{}/api/send", umami.url))
        .json(&json_payload)
        .bearer_auth(&umami.token)
        .send()
        .await?;

    Ok(())
}
