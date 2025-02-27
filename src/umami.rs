use serde_json::json;
use thiserror::Error;

use crate::config::Config;

#[derive(Error, Debug)]
pub enum UmamiError {
    #[error("Error while making a request: {0:#}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Umami is not defined in the config")]
    UmamiUndefined,
    #[error("Response is missing required headers")]
    MissingHeaders,
}

/// Send a payload to a Umami instance defined in the config.
///
/// # Examples
///
/// ```
/// //  cfg        : &crate::config::Config,
/// //  req_client : reqwest::Client,
/// //  req        : &poem::Request
///
/// let umami = async |name: &str, json: serde_json::Value| {
///     let _ = send_to_umami(self.cfg, self.req_client.clone(), req, name, json)
///     .await
///     .map_err(|er| println!("ERROR: {}", er));
/// };
///
/// umami("index", json!({"Hello": "World"})).await;
/// ```
///
///
pub async fn send_to_umami(
    cfg: &Config,
    req_client: reqwest::Client,
    req: &poem::Request,
    name: &str,
    payload: serde_json::Value,
) -> Result<(), UmamiError> {
    let umami = match &cfg.umami_stats {
        Some(c) => c,
        None => return Err(UmamiError::UmamiUndefined),
    };

    let request_headers = req.headers();

    let get_header = |header: &str| match request_headers.get(header) {
        None => Err(UmamiError::MissingHeaders),
        Some(val) => Ok(val.as_bytes()),
    };

    let original_url = req.original_uri().to_string();

    let json_payload = json!({
        "type": "event",
        "payload": {
            "hostname": get_header("host")?,
            "language": get_header("accept-language")?,
            // NOTE: This is not a misspelling
            //                      ↓     ↓
            "referrer": get_header("referer").unwrap_or(b""),
            "url": original_url,
            "website": umami.id,
            "name": name,
            "data": payload,
        }
    });

    req_client
        .post(format!("{}/api/send", umami.url))
        .json(&json_payload)
        .bearer_auth(&umami.token)
        .send()
        .await?;

    Ok(())
}
