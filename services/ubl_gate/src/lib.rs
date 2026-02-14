pub mod api;
pub mod audit;
pub mod error;
pub mod scope;

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

// ── CORS config: (app, tenant) scoped ──────────────────────────

/// CORS configuration supporting hierarchical origin allowlists.
///
/// Lookup order for a request to `/a/<app>/t/<tenant>/v1/*`:
///   1. `(app, tenant)` specific origins
///   2. `(app, *)` app-level origins
///   3. Global "safe" origins
///
/// Legacy `/v1/*` routes use `(default, default)`.
#[derive(Clone, Debug)]
pub struct CorsConfig {
    /// Origins allowed for all apps/tenants.
    pub global_origins: Vec<String>,
    /// Per-app origin overrides. Key = app_id.
    pub app_origins: HashMap<String, Vec<String>>,
    /// Per-(app, tenant) origin overrides. Key = "app:tenant".
    pub scoped_origins: HashMap<String, Vec<String>>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl CorsConfig {
    /// Build from environment variables:
    /// - `CORS_GLOBAL_ORIGINS`: comma-separated global origins
    /// - `CORS_APP_<APP>_ORIGINS`: per-app origins
    /// - `CORS_APP_<APP>_TENANT_<TENANT>_ORIGINS`: per-(app, tenant) origins
    /// - Legacy: `CORS_TENANT_<TENANT>_ORIGINS` → mapped to (default, <tenant>)
    pub fn from_env() -> Self {
        let global = std::env::var("CORS_GLOBAL_ORIGINS")
            .unwrap_or_else(|_| [
                "https://api.ubl.agency",
                "https://ui.ubl.agency",
                "https://tunnel.ubl.agency",
                "https://ubl.agency",
                "http://localhost:3000",
                "http://localhost:3001",
                "http://localhost:5173",
            ].join(","));
        let global_origins: Vec<String> = global
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut app_origins: HashMap<String, Vec<String>> = HashMap::new();
        let mut scoped_origins: HashMap<String, Vec<String>> = HashMap::new();

        for (key, val) in std::env::vars() {
            let origins: Vec<String> = val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if origins.is_empty() {
                continue;
            }

            // CORS_APP_<APP>_TENANT_<TENANT>_ORIGINS → scoped
            if let Some(rest) = key.strip_prefix("CORS_APP_") {
                if let Some(rest) = rest.strip_suffix("_ORIGINS") {
                    if let Some((app, tenant)) = rest.split_once("_TENANT_") {
                        let scope_key = format!("{}:{}", app.to_lowercase(), tenant.to_lowercase());
                        scoped_origins.insert(scope_key, origins);
                    } else {
                        // CORS_APP_<APP>_ORIGINS → app-level
                        app_origins.insert(rest.to_lowercase(), origins);
                    }
                }
                continue;
            }

            // Legacy: CORS_TENANT_<TENANT>_ORIGINS → (default, <tenant>)
            if let Some(tenant_id) = key
                .strip_prefix("CORS_TENANT_")
                .and_then(|rest| rest.strip_suffix("_ORIGINS"))
            {
                let scope_key = format!("default:{}", tenant_id.to_lowercase());
                scoped_origins.insert(scope_key, origins);
            }
        }

        Self {
            global_origins,
            app_origins,
            scoped_origins,
        }
    }

    /// Check if an origin is allowed for a given scope.
    /// Lookup: (app, tenant) → (app, *) → global.
    pub fn is_origin_allowed(&self, origin: &str, scope: Option<&scope::Scope>) -> bool {
        // 1. (app, tenant) specific
        if let Some(s) = scope {
            let key = format!("{}:{}", s.app, s.tenant);
            if let Some(origins) = self.scoped_origins.get(&key) {
                if origins.iter().any(|o| o == origin) {
                    return true;
                }
            }
            // 2. App-level
            if let Some(origins) = self.app_origins.get(&s.app) {
                if origins.iter().any(|o| o == origin) {
                    return true;
                }
            }
        }
        // 3. Global
        self.global_origins.iter().any(|o| o == origin)
    }

    /// Return all allowed origins for a scope (merged: scoped + app + global).
    pub fn allowed_origins_for(&self, scope: &scope::Scope) -> Vec<String> {
        let mut origins = self.global_origins.clone();
        if let Some(app_specific) = self.app_origins.get(&scope.app) {
            origins.extend(app_specific.iter().cloned());
        }
        let key = format!("{}:{}", scope.app, scope.tenant);
        if let Some(scoped) = self.scoped_origins.get(&key) {
            origins.extend(scoped.iter().cloned());
        }
        origins
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
    pub cors_config: CorsConfig,
    pub metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

impl Default for AppState {
    fn default() -> Self {
        // SEC-3: Auth is ENABLED by default. Set UBL_AUTH_DISABLED=1 to disable.
        let auth_disabled = std::env::var("UBL_AUTH_DISABLED")
            .map(|v| v == "1")
            .unwrap_or(false);
        Self {
            transition_receipts: Default::default(),
            receipt_chain: Default::default(),
            seen_cids: Default::default(),
            keys: Arc::new(ubl_runtime::KeyRing::dev()),
            last_tip: Default::default(),
            token_store: TokenStore::with_dev_token(),
            auth_disabled,
            rate_limiter: RateLimiter::from_env(),
            cors_config: CorsConfig::from_env(),
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

/// Build the shared v1 API routes (no state attached yet).
fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/ingest", post(api::ingest))
        .route("/certify", post(api::certify_cid))
        .route("/receipts", get(api::list_receipts))
        .route("/receipt/:cid", get(api::get_receipt))
        .route("/audit", get(api::audit_report))
        .route("/resolve", post(api::resolve))
        .route("/execute", post(api::execute_runtime))
        .route("/execute/rb", post(api::execute_rb))
        .route("/transition/:cid", get(api::get_transition))
}

/// Middleware: inject Scope from path params :app and :tenant into request extensions.
async fn inject_scope_from_path(req: Request, next: Next) -> Response {
    let mut req = req;
    // Extract :app and :tenant from Axum's matched path params
    // These are available when routes are nested under /a/:app/t/:tenant/v1
    let path = req.uri().path().to_string();
    let scope = parse_scope_from_path(&path).unwrap_or_default();
    req.extensions_mut().insert(scope);
    next.run(req).await
}

/// Parse (app, tenant) from a path like /a/<app>/t/<tenant>/v1/...
fn parse_scope_from_path(path: &str) -> Option<scope::Scope> {
    let parts: Vec<&str> = path.split('/').collect();
    // Expected: ["", "a", "<app>", "t", "<tenant>", "v1", ...]
    if parts.len() >= 6 && parts[1] == "a" && parts[3] == "t" && parts[5] == "v1" {
        Some(scope::Scope::new(parts[2], parts[4]))
    } else {
        None
    }
}

/// Middleware: inject legacy Scope (default, default) into request extensions.
async fn inject_legacy_scope(req: Request, next: Next) -> Response {
    let mut req = req;
    if req.extensions().get::<scope::Scope>().is_none() {
        req.extensions_mut().insert(scope::Scope::legacy());
    }
    next.run(req).await
}

pub fn app_with_state(state: AppState) -> Router {
    let auth_state = state.clone();
    let rl_state = state.clone();
    let cors_config = state.cors_config.clone();

    // Scoped routes: /a/:app/t/:tenant/v1/*
    // The :app and :tenant are parsed by inject_scope_from_path middleware.
    let scoped_v1 = v1_routes()
        .layer(middleware::from_fn(inject_scope_from_path));

    // Legacy routes: /v1/* → Scope(default, default)
    let legacy_v1 = v1_routes()
        .layer(middleware::from_fn(inject_legacy_scope));

    // Layer order: Axum applies layers in REVERSE order.
    // Last .layer() = outermost (runs first).
    // We want: CORS (outermost) → auth → metrics → rate_limit → content-type → timeout → body_limit
    Router::new()
        // Public routes (no auth, no scope)
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics_endpoint))
        .route("/.well-known/did.json", get(api::well_known_did_json))
        // Legacy CID dispatch (outside v1 namespace)
        .route("/cid/:cid", get(api::get_cid_dispatch))
        // Scoped v1 routes: /a/:app/t/:tenant/v1/*
        .nest("/a/:app/t/:tenant/v1", scoped_v1)
        // Legacy v1 routes: /v1/* → (default, default)
        .nest("/v1", legacy_v1)
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
        // CORS must be outermost (last .layer()) so preflight OPTIONS
        // are handled BEFORE auth/rate-limit/content-type checks.
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::predicate(
                    move |origin: &HeaderValue, parts: &axum::http::request::Parts| {
                        let scope = parse_scope_from_path(parts.uri.path());
                        origin
                            .to_str()
                            .map(|o| cors_config.is_origin_allowed(o, scope.as_ref()))
                            .unwrap_or(false)
                    },
                ))
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
                    "idempotency-key".parse().unwrap(),
                ])
                .expose_headers([
                    "x-ratelimit-limit".parse().unwrap(),
                    "x-ratelimit-remaining".parse().unwrap(),
                    "retry-after".parse().unwrap(),
                    "deprecation".parse().unwrap(),
                    "sunset".parse().unwrap(),
                ])
                .max_age(Duration::from_secs(3600)),
        )
        .with_state(state)
}

/// Middleware: reject POST/PUT requests without application/json content-type.
/// OPTIONS requests are always passed through (CORS preflight).
async fn require_json_content_type(req: Request, next: Next) -> Response {
    // Always pass OPTIONS through (CORS preflight)
    if req.method() == axum::http::Method::OPTIONS {
        return next.run(req).await;
    }
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
    // Skip OPTIONS (CORS preflight) — no Bearer token expected
    if req.method() == axum::http::Method::OPTIONS {
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

#[cfg(test)]
mod cors_tests {
    use super::*;

    fn cfg_empty() -> CorsConfig {
        CorsConfig {
            global_origins: vec![],
            app_origins: HashMap::new(),
            scoped_origins: HashMap::new(),
        }
    }

    #[test]
    fn global_origin_allowed() {
        let cfg = CorsConfig {
            global_origins: vec!["https://ubl.agency".into(), "http://localhost:3000".into()],
            ..cfg_empty()
        };
        assert!(cfg.is_origin_allowed("https://ubl.agency", None));
        assert!(cfg.is_origin_allowed("http://localhost:3000", None));
        assert!(!cfg.is_origin_allowed("https://evil.com", None));
    }

    #[test]
    fn scoped_origin_allowed() {
        let mut scoped = HashMap::new();
        scoped.insert("ubl:acme".into(), vec!["https://app.acme.com".into()]);
        let cfg = CorsConfig {
            global_origins: vec!["https://ubl.agency".into()],
            scoped_origins: scoped,
            ..cfg_empty()
        };
        let acme = scope::Scope::new("ubl", "acme");
        let other = scope::Scope::new("ubl", "other");
        // Scoped origin allowed for that (app, tenant)
        assert!(cfg.is_origin_allowed("https://app.acme.com", Some(&acme)));
        // Not allowed for a different tenant in same app
        assert!(!cfg.is_origin_allowed("https://app.acme.com", Some(&other)));
        // Not allowed without scope
        assert!(!cfg.is_origin_allowed("https://app.acme.com", None));
        // Global still works for any scope
        assert!(cfg.is_origin_allowed("https://ubl.agency", Some(&acme)));
    }

    #[test]
    fn app_level_origin_fallback() {
        let mut app = HashMap::new();
        app.insert("ubl".into(), vec!["https://app.ubl.com".into()]);
        let cfg = CorsConfig {
            global_origins: vec!["https://ubl.agency".into()],
            app_origins: app,
            scoped_origins: HashMap::new(),
        };
        let any_tenant = scope::Scope::new("ubl", "whatever");
        // App-level origin works for any tenant in that app
        assert!(cfg.is_origin_allowed("https://app.ubl.com", Some(&any_tenant)));
        // Not for a different app
        let other_app = scope::Scope::new("other", "whatever");
        assert!(!cfg.is_origin_allowed("https://app.ubl.com", Some(&other_app)));
    }

    #[test]
    fn allowed_origins_for_merges() {
        let mut scoped = HashMap::new();
        scoped.insert("ubl:acme".into(), vec!["https://app.acme.com".into()]);
        let mut app = HashMap::new();
        app.insert("ubl".into(), vec!["https://app.ubl.com".into()]);
        let cfg = CorsConfig {
            global_origins: vec!["https://ubl.agency".into()],
            app_origins: app,
            scoped_origins: scoped,
        };
        let acme = scope::Scope::new("ubl", "acme");
        let origins = cfg.allowed_origins_for(&acme);
        assert_eq!(origins.len(), 3);
        assert!(origins.contains(&"https://ubl.agency".into()));
        assert!(origins.contains(&"https://app.ubl.com".into()));
        assert!(origins.contains(&"https://app.acme.com".into()));

        // Unknown tenant in same app gets global + app
        let unknown = scope::Scope::new("ubl", "unknown");
        let origins = cfg.allowed_origins_for(&unknown);
        assert_eq!(origins.len(), 2);
    }

    #[test]
    fn empty_config() {
        let cfg = cfg_empty();
        assert!(!cfg.is_origin_allowed("https://anything.com", None));
        let s = scope::Scope::legacy();
        assert!(cfg.allowed_origins_for(&s).is_empty());
    }

    #[test]
    fn parse_scope_from_path_works() {
        let s = super::parse_scope_from_path("/a/myapp/t/acme/v1/execute");
        assert_eq!(s, Some(scope::Scope::new("myapp", "acme")));

        let s = super::parse_scope_from_path("/v1/execute");
        assert_eq!(s, None);

        let s = super::parse_scope_from_path("/a/x/t/y/v1/receipt/b3:abc");
        assert_eq!(s, Some(scope::Scope::new("x", "y")));
    }
}

pub mod test {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    /// Spawn the server on a random port with auth disabled (for tests).
    /// Returns the address and a JoinHandle that keeps the server alive until dropped.
    pub async fn spawn() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let state = super::AppState {
            auth_disabled: true, // tests run without auth by default
            ..super::AppState::default()
        };
        let app = super::app_with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, handle)
    }

    /// Spawn the server with auth ENABLED and the given token store.
    /// For testing auth flows.
    pub async fn spawn_with_auth(
        token_store: super::TokenStore,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let state = super::AppState {
            auth_disabled: false,
            token_store,
            ..super::AppState::default()
        };
        let app = super::app_with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, handle)
    }
}
