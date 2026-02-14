pub mod api;

use axum::http::HeaderValue;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use metrics::{counter, histogram};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Max request body size: 1 MiB
const MAX_BODY_BYTES: usize = 1_048_576;
/// Request timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Dev bearer token (only active when UBL_AUTH_DISABLED is not set)
const DEV_TOKEN: &str = "ubl-dev-token-001";

// ── Rate limiting ────────────────────────────────────────────────

/// Per-client token bucket.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token-bucket rate limiter keyed by client_id.
#[derive(Clone)]
pub struct RateLimiter {
    /// Requests per minute (refill rate)
    pub rpm: u32,
    /// Max burst size
    pub burst: u32,
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
}

impl RateLimiter {
    pub fn new(rpm: u32, burst: u32) -> Self {
        Self {
            rpm,
            burst,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_env() -> Self {
        let rpm: u32 = std::env::var("RATE_LIMIT_RPM_DEFAULT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let burst: u32 = std::env::var("RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        Self::new(rpm, burst)
    }

    /// Try to consume one token for the given client_id.
    /// Returns (allowed, remaining, limit, retry_after_secs).
    pub fn check(&self, client_id: &str) -> (bool, u32, u32, f64) {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        let refill_rate = self.rpm as f64 / 60.0; // tokens per second

        let bucket = buckets
            .entry(client_id.to_string())
            .or_insert_with(|| Bucket {
                tokens: self.burst as f64,
                last_refill: now,
            });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_rate).min(self.burst as f64);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            (true, bucket.tokens as u32, self.burst, 0.0)
        } else {
            // Time until next token
            let retry_after = (1.0 - bucket.tokens) / refill_rate;
            (false, 0, self.burst, retry_after)
        }
    }
}

/// Client identity resolved from a bearer token.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub client_id: String,
    /// Tenant namespace for data isolation.
    pub tenant_id: String,
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
        m.insert(
            DEV_TOKEN.to_string(),
            ClientInfo {
                client_id: "dev-client".into(),
                tenant_id: "default".into(),
                allowed_kids: vec![], // empty = unrestricted
            },
        );
        Self {
            tokens: Arc::new(RwLock::new(m)),
        }
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
    pub rate_limiter: RateLimiter,
    pub metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

impl Default for AppState {
    fn default() -> Self {
        let auth_disabled = std::env::var("UBL_AUTH_DISABLED")
            .map(|v| v == "1")
            .unwrap_or(true);
        Self {
            transition_receipts: Default::default(),
            receipt_chain: Default::default(),
            seen_cids: Default::default(),
            keys: Arc::new(ubl_runtime::KeyRing::dev()),
            last_tip: Default::default(),
            token_store: TokenStore::with_dev_token(),
            auth_disabled,
            rate_limiter: RateLimiter::from_env(),
            metrics_handle: init_metrics(),
        }
    }
}

pub fn app() -> Router {
    app_with_state(AppState::default())
}

/// Install the Prometheus recorder and return a handle for the /metrics endpoint.
/// Safe to call multiple times (subsequent calls return None).
pub fn init_metrics() -> Option<metrics_exporter_prometheus::PrometheusHandle> {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder.install_recorder().ok()
}

pub fn app_with_state(state: AppState) -> Router {
    let auth_state = state.clone();
    let rl_state = state.clone();
    let _metrics_state = state.clone();
    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics_endpoint))
        .route("/v1/ingest", post(api::ingest))
        .route("/v1/certify", post(api::certify_cid))
        .route("/v1/receipts", get(api::list_receipts))
        .route("/v1/receipt/:cid", get(api::get_receipt))
        .route("/v1/resolve", post(api::resolve))
        .route("/v1/execute", post(api::execute_runtime))
        .route("/v1/execute/rb", post(api::execute_rb))
        .route("/v1/transition/:cid", get(api::get_transition))
        .route("/cid/:cid", get(api::get_cid_dispatch))
        .route("/.well-known/did.json", get(api::well_known_did_json))
        .layer(
            CorsLayer::new()
                .allow_origin([
                    "https://api.ubl.agency".parse::<HeaderValue>().unwrap(),
                    "https://ui.ubl.agency".parse::<HeaderValue>().unwrap(),
                    "https://tunnel.ubl.agency".parse::<HeaderValue>().unwrap(),
                    "https://ubl.agency".parse::<HeaderValue>().unwrap(),
                    "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                    "http://localhost:3001".parse::<HeaderValue>().unwrap(),
                ])
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    "x-ubl-compat".parse().unwrap(),
                    "x-request-id".parse().unwrap(),
                ])
                .expose_headers([
                    "x-ratelimit-limit".parse().unwrap(),
                    "x-ratelimit-remaining".parse().unwrap(),
                    "retry-after".parse().unwrap(),
                ])
                .max_age(Duration::from_secs(3600)),
        )
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
        .layer(middleware::from_fn(require_json_content_type))
        .layer(middleware::from_fn(move |req, next| {
            let st = rl_state.clone();
            rate_limit_middleware(st, req, next)
        }))
        .layer(middleware::from_fn(metrics_middleware))
        .layer(middleware::from_fn(move |req, next| {
            let st = auth_state.clone();
            require_bearer_auth(st, req, next)
        }))
        .with_state(state)
}

/// Middleware: reject POST/PUT requests without application/json content-type.
async fn require_json_content_type(req: Request, next: Next) -> Response {
    let dominated_by_json = match req.method().as_str() {
        "POST" | "PUT" | "PATCH" => req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.starts_with("application/json"))
            .unwrap_or(false),
        _ => true, // GET, DELETE, etc. don't need content-type
    };
    if !dominated_by_json {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error": "content-type must be application/json"})),
        )
            .into_response();
    }
    next.run(req).await
}

/// Paths that do NOT require authentication.
const PUBLIC_PATHS: &[&str] = &["/healthz", "/.well-known/did.json", "/metrics"];

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
    let token = req
        .headers()
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
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "missing Authorization: Bearer <token> header"})),
        )
            .into_response(),
    }
}

/// Middleware: per-client rate limiting. Runs AFTER auth (so ClientInfo is available).
async fn rate_limit_middleware(state: AppState, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    // Skip rate limiting for public/read-only paths
    if PUBLIC_PATHS.iter().any(|p| path == *p) {
        return next.run(req).await;
    }

    // Get client_id from extensions (injected by auth middleware), fallback to "anonymous"
    let client_id = req
        .extensions()
        .get::<ClientInfo>()
        .map(|ci| ci.client_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    let (allowed, remaining, limit, retry_after) = state.rate_limiter.check(&client_id);

    if allowed {
        let mut resp = next.run(req).await;
        let headers = resp.headers_mut();
        headers.insert("x-ratelimit-limit", HeaderValue::from(limit));
        headers.insert("x-ratelimit-remaining", HeaderValue::from(remaining));
        resp
    } else {
        let retry_secs = retry_after.ceil() as u64;
        let body = json!({
            "error": "rate_limit_exceeded",
            "detail": format!("client '{}' exceeded {} rpm", client_id, state.rate_limiter.rpm),
            "receipt": {
                "t": "ubl/wf",
                "body": {
                    "decision": "DENY",
                    "reason": "RATE_LIMIT",
                    "recommended_action": "retry_after",
                    "retry_after_secs": retry_secs
                }
            }
        });
        let mut resp = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
        let headers = resp.headers_mut();
        headers.insert("x-ratelimit-limit", HeaderValue::from(limit));
        headers.insert("x-ratelimit-remaining", HeaderValue::from(0u32));
        headers.insert("retry-after", HeaderValue::from(retry_secs));
        resp
    }
}

/// Middleware: record request count and latency per route/status.
async fn metrics_middleware(req: Request, next: Next) -> Response {
    let route = req.uri().path().to_string();
    let method = req.method().to_string();
    let start = Instant::now();
    let resp = next.run(req).await;
    let status = resp.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();

    counter!("ubl_gate_requests_total", "route" => route.clone(), "status" => status.clone(), "method" => method.clone()).increment(1);
    histogram!("ubl_gate_request_duration_seconds", "route" => route, "method" => method)
        .record(elapsed);

    if resp.status().is_server_error() {
        counter!("ubl_gate_errors_total", "status" => status).increment(1);
    }
    resp
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"ok": true}))
}

async fn metrics_endpoint(State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    // Try to render prometheus metrics; if recorder not installed, return empty
    if let Some(handle) = &state.metrics_handle {
        let body = handle.render();
        (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response()
    } else {
        (StatusCode::OK, "# no metrics recorder installed\n").into_response()
    }
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
