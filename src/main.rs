use std::collections::HashMap;

mod config;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let api_service =
    //     OpenApiService::new(Api, "Hello World", "1.0").server("http://localhost:3000");
    // let ui = api_service.swagger_ui();
    // let app = Route::new().nest("/api", api_service).nest("/docs", ui);

    // let _ = Server::new(TcpListener::bind("127.0.0.1:4310"))
    //     .run(app)
    //     .await;

    let cfg = config::get_config()?;

    Ok(())
}
