//! Request scope: (app, tenant) resolved from URL path or defaults.
//!
//! New routes:   `/a/:app/t/:tenant/v1/*`  → Scope from path
//! Legacy routes: `/v1/*`                   → Scope { app: "default", tenant: "default" }

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The (app, tenant) namespace for data isolation, CORS, rate limiting, and key management.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scope {
    pub app: String,
    pub tenant: String,
}

impl Scope {
    pub fn new(app: impl Into<String>, tenant: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            tenant: tenant.into(),
        }
    }

    /// Default scope for legacy `/v1/*` routes.
    pub fn legacy() -> Self {
        Self {
            app: "default".into(),
            tenant: "default".into(),
        }
    }

    /// Storage key prefix for data isolation.
    pub fn key_prefix(&self) -> String {
        format!("{}:{}", self.app, self.tenant)
    }

    /// Scoped storage key for a CID.
    pub fn scoped_cid(&self, cid: &str) -> String {
        format!("{}:{}:{}", self.app, self.tenant, cid)
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::legacy()
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "app={} tenant={}", self.app, self.tenant)
    }
}

/// Axum extractor: resolves Scope from path params `:app` and `:tenant`,
/// falling back to defaults if not present (legacy routes).
#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Scope {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try to extract from extensions (set by scoped-route middleware)
        if let Some(scope) = parts.extensions.get::<Scope>() {
            return Ok(scope.clone());
        }
        // Fallback: legacy route
        Ok(Scope::legacy())
    }
}

/// Authenticated client context, enriched with scope.
#[derive(Debug, Clone)]
pub struct AuthCtx {
    /// The (app, tenant) namespace.
    pub scope: Scope,
    /// Client identifier (from token).
    pub client_id: String,
    /// Which key IDs this client is allowed to use. Empty = all.
    pub allowed_kids: Vec<String>,
}

impl AuthCtx {
    /// Check if this client is allowed to use the given kid.
    pub fn kid_allowed(&self, kid: &str) -> bool {
        self.allowed_kids.is_empty() || self.allowed_kids.iter().any(|k| k == kid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_key_prefix() {
        let s = Scope::new("myapp", "acme");
        assert_eq!(s.key_prefix(), "myapp:acme");
        assert_eq!(s.scoped_cid("b3:abc"), "myapp:acme:b3:abc");
    }

    #[test]
    fn legacy_scope() {
        let s = Scope::legacy();
        assert_eq!(s.app, "default");
        assert_eq!(s.tenant, "default");
    }

    #[test]
    fn scope_display() {
        let s = Scope::new("ubl", "prod");
        assert_eq!(format!("{s}"), "app=ubl tenant=prod");
    }

    #[test]
    fn auth_ctx_kid_allowed() {
        let ctx = AuthCtx {
            scope: Scope::legacy(),
            client_id: "test".into(),
            allowed_kids: vec!["did:dev#k1".into()],
        };
        assert!(ctx.kid_allowed("did:dev#k1"));
        assert!(!ctx.kid_allowed("did:other#k9"));

        let unrestricted = AuthCtx {
            scope: Scope::legacy(),
            client_id: "test".into(),
            allowed_kids: vec![],
        };
        assert!(unrestricted.kid_allowed("anything"));
    }
}
