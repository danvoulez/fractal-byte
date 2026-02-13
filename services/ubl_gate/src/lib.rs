pub mod api;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Max request body size: 1 MiB
const MAX_BODY_BYTES: usize = 1_048_576;
/// Request timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct AppState {
    pub transition_receipts: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub receipt_chain: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub seen_cids: Arc<RwLock<HashSet<String>>>,
    pub keys: Arc<ubl_runtime::KeyRing>,
    pub last_tip: Arc<RwLock<Option<String>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            transition_receipts: Default::default(),
            receipt_chain: Default::default(),
            seen_cids: Default::default(),
            keys: Arc::new(ubl_runtime::KeyRing::dev()),
            last_tip: Default::default(),
        }
    }
}

pub fn app() -> Router {
    let state = AppState::default();
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/ingest", post(api::ingest))
        .route("/v1/certify", post(api::certify_cid))
        .route("/v1/receipt/:cid", get(api::get_receipt))
        .route("/v1/resolve", post(api::resolve))
        .route("/v1/execute", post(api::execute_runtime))
        .route("/v1/execute/rb", post(api::execute_rb))
        .route("/v1/transition/:cid", get(api::get_transition))
        .route("/cid/:cid", get(api::get_cid_dispatch))
        .route("/.well-known/did.json", get(api::well_known_did_json))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
        .layer(middleware::from_fn(require_json_content_type))
        .with_state(state)
}

/// Middleware: reject POST/PUT requests without application/json content-type.
async fn require_json_content_type(req: Request, next: Next) -> Response {
    let dominated_by_json = match req.method().as_str() {
        "POST" | "PUT" | "PATCH" => {
            req.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|ct| ct.starts_with("application/json"))
                .unwrap_or(false)
        }
        _ => true, // GET, DELETE, etc. don't need content-type
    };
    if !dominated_by_json {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error": "content-type must be application/json"})),
        ).into_response();
    }
    next.run(req).await
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
