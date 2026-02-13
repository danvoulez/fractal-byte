pub mod api;

use axum::{routing::{get, post}, Json, Router};
use serde_json::json;

pub fn app() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/ingest", post(api::ingest))
        .route("/v1/certify", post(api::certify_cid))
        .route("/v1/receipt/:cid", get(api::get_receipt))
        .route("/v1/resolve", post(api::resolve))
        .route("/v1/execute", post(api::execute_runtime))
        .route("/cid/:cid", get(api::get_cid_dispatch))
        .route("/.well-known/did.json", get(api::well_known_did_json))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"ok": true}))
}

pub mod test {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    /// Spawn the server on a random port. Returns the address and a
    /// JoinHandle that keeps the server alive until dropped.
    pub async fn spawn() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let app = super::app();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, handle)
    }
}
