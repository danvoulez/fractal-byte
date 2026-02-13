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
/// Dev bearer token (only active when UBL_AUTH_DISABLED is not set)
const DEV_TOKEN: &str = "ubl-dev-token-001";

/// Client identity resolved from a bearer token.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub client_id: String,
    /// Which key IDs this client is allowed to use. Empty = all.
    pub allowed_kids: Vec<String>,
}

impl ClientInfo {
    /// Check if this client is allowed to use the given kid.
    /// Empty allowed_kids means unrestricted.
    pub fn kid_allowed(&self, kid: &str) -> bool {
        self.allowed_kids.is_empty() || self.allowed_kids.iter().any(|k| k == kid)
    }
}

/// In-memory token store mapping bearer tokens → client info.
#[derive(Clone, Default)]
pub struct TokenStore {
    tokens: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

impl TokenStore {
    /// Create a store pre-loaded with the dev token.
    pub fn with_dev_token() -> Self {
        let mut m = HashMap::new();
        m.insert(DEV_TOKEN.to_string(), ClientInfo {
            client_id: "dev-client".into(),
            allowed_kids: vec![], // empty = unrestricted
        });
        Self { tokens: Arc::new(RwLock::new(m)) }
    }

    /// Register a new token → client mapping.
    pub fn register(&self, token: &str, info: ClientInfo) {
        self.tokens.write().unwrap().insert(token.to_string(), info);
    }

    /// Look up a bearer token. Returns None if not found.
    pub fn lookup(&self, token: &str) -> Option<ClientInfo> {
        self.tokens.read().unwrap().get(token).cloned()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub transition_receipts: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub receipt_chain: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub seen_cids: Arc<RwLock<HashSet<String>>>,
    pub keys: Arc<ubl_runtime::KeyRing>,
    pub last_tip: Arc<RwLock<Option<String>>>,
    pub token_store: TokenStore,
    /// When true, auth middleware is bypassed (for tests / dev)
    pub auth_disabled: bool,
}

impl Default for AppState {
    fn default() -> Self {
        let auth_disabled = std::env::var("UBL_AUTH_DISABLED").map(|v| v == "1").unwrap_or(true);
        Self {
            transition_receipts: Default::default(),
            receipt_chain: Default::default(),
            seen_cids: Default::default(),
            keys: Arc::new(ubl_runtime::KeyRing::dev()),
            last_tip: Default::default(),
            token_store: TokenStore::with_dev_token(),
            auth_disabled,
        }
    }
}

pub fn app() -> Router {
    app_with_state(AppState::default())
}

pub fn app_with_state(state: AppState) -> Router {
    let auth_state = state.clone();
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
        .layer(middleware::from_fn(move |req, next| {
            let st = auth_state.clone();
            require_bearer_auth(st, req, next)
        }))
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

/// Paths that do NOT require authentication.
const PUBLIC_PATHS: &[&str] = &["/healthz", "/.well-known/did.json"];

/// Middleware: require valid Bearer token on non-public paths.
async fn require_bearer_auth(state: AppState, mut req: Request, next: Next) -> Response {
    // Skip auth if disabled (dev/test mode)
    if state.auth_disabled {
        return next.run(req).await;
    }
    // Skip auth for public paths
    let path = req.uri().path().to_string();
    if PUBLIC_PATHS.iter().any(|p| path == *p) {
        return next.run(req).await;
    }
    // Extract Bearer token
    let token = req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match token {
        Some(t) => {
            match state.token_store.lookup(t) {
                Some(client) => {
                    // Inject client info into request extensions for kid-scope checks
                    req.extensions_mut().insert(client);
                    next.run(req).await
                }
                None => (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid bearer token"})),
                ).into_response(),
            }
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "missing Authorization: Bearer <token> header"})),
        ).into_response(),
    }
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
