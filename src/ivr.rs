use thiserror::Error;

use crate::config::Config;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
// #[serde(rename_all = "camelCase")]
pub struct IvrRequestResponse {
    pub id: String,
    // display_name: String,
    // logo: String
}

#[derive(Error, Debug)]
pub enum IvrResponseError {
    #[error("No information for that user")]
    NoInfo,
    #[error("Error while making a request: {0:#}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Deserialization error: {0:#}")]
    DeserError(#[from] serde_json::Error),
}

pub async fn get_ids_from_login(
    cfg: &Config,
    req_client: &reqwest::Client,
    login: &str,
) -> Result<IvrRequestResponse, IvrResponseError> {
    let ivr_link = match &cfg.alternative_ivr_url {
        Some(url) => url,
        None => "https://api.ivr.fi/v2",
    };

    let res = req_client
        .get(format!("{ivr_link}/twitch/user"))
        .query(&[("login", &login)])
        .send()
        .await?;

    let bytes = res.bytes().await?;

    match serde_json::from_slice::<Vec<IvrRequestResponse>>(&bytes) {
        Ok(res) => Ok(res[0].clone()),
        Err(_) => Err(IvrResponseError::NoInfo),
    }
}
