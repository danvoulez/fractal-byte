//! Per-app KeyRing store with optional tenant overrides.
//!
//! Lookup order:
//!   1. `(app, tenant)` override
//!   2. `app` default
//!   3. Global fallback (dev keyring)

use std::collections::HashMap;
use std::sync::Arc;
use ubl_runtime::KeyRing;

/// Hierarchical keyring store: app → KeyRing, with optional (app, tenant) overrides.
#[derive(Clone)]
pub struct KeyRingStore {
    /// Global fallback keyring (used when no app-specific keyring is configured)
    pub global: Arc<KeyRing>,
    /// Per-app keyrings: app_id → KeyRing
    pub app_keyrings: HashMap<String, Arc<KeyRing>>,
    /// Per-(app, tenant) overrides: "app:tenant" → KeyRing
    pub scoped_keyrings: HashMap<String, Arc<KeyRing>>,
}

impl KeyRingStore {
    /// Create a store with only the global fallback.
    pub fn new(global: KeyRing) -> Self {
        Self {
            global: Arc::new(global),
            app_keyrings: HashMap::new(),
            scoped_keyrings: HashMap::new(),
        }
    }

    /// Create a dev store with the default dev keyring.
    pub fn dev() -> Self {
        Self::new(KeyRing::dev())
    }

    /// Register a keyring for a specific app.
    pub fn set_app(&mut self, app: &str, keyring: KeyRing) {
        self.app_keyrings.insert(app.to_string(), Arc::new(keyring));
    }

    /// Register a keyring override for a specific (app, tenant).
    pub fn set_scoped(&mut self, app: &str, tenant: &str, keyring: KeyRing) {
        let key = format!("{app}:{tenant}");
        self.scoped_keyrings.insert(key, Arc::new(keyring));
    }

    /// Resolve the effective keyring for a given (app, tenant).
    /// Lookup: scoped → app → global.
    pub fn resolve(&self, app: &str, tenant: &str) -> Arc<KeyRing> {
        // 1. Scoped override
        let scoped_key = format!("{app}:{tenant}");
        if let Some(kr) = self.scoped_keyrings.get(&scoped_key) {
            return Arc::clone(kr);
        }
        // 2. App-level
        if let Some(kr) = self.app_keyrings.get(app) {
            return Arc::clone(kr);
        }
        // 3. Global fallback
        Arc::clone(&self.global)
    }

    /// Convenience: resolve for a Scope.
    pub fn resolve_for_scope(&self, scope: &crate::scope::Scope) -> Arc<KeyRing> {
        self.resolve(&scope.app, &scope.tenant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keyring(kid: &str) -> KeyRing {
        let mut kr = KeyRing::dev();
        kr.active_kid = kid.to_string();
        kr
    }

    #[test]
    fn global_fallback() {
        let store = KeyRingStore::new(make_keyring("global#k1"));
        let kr = store.resolve("any", "any");
        assert_eq!(kr.active_kid, "global#k1");
    }

    #[test]
    fn app_level_override() {
        let mut store = KeyRingStore::new(make_keyring("global#k1"));
        store.set_app("ubl", make_keyring("ubl#k1"));
        // App match
        let kr = store.resolve("ubl", "any");
        assert_eq!(kr.active_kid, "ubl#k1");
        // Different app → global
        let kr = store.resolve("other", "any");
        assert_eq!(kr.active_kid, "global#k1");
    }

    #[test]
    fn scoped_override() {
        let mut store = KeyRingStore::new(make_keyring("global#k1"));
        store.set_app("ubl", make_keyring("ubl#k1"));
        store.set_scoped("ubl", "acme", make_keyring("ubl:acme#k1"));
        // Scoped match
        let kr = store.resolve("ubl", "acme");
        assert_eq!(kr.active_kid, "ubl:acme#k1");
        // Different tenant → app level
        let kr = store.resolve("ubl", "beta");
        assert_eq!(kr.active_kid, "ubl#k1");
        // Different app → global
        let kr = store.resolve("other", "acme");
        assert_eq!(kr.active_kid, "global#k1");
    }

    #[test]
    fn resolve_for_scope() {
        let mut store = KeyRingStore::new(make_keyring("global#k1"));
        store.set_app("ubl", make_keyring("ubl#k1"));
        let scope = crate::scope::Scope::new("ubl", "acme");
        let kr = store.resolve_for_scope(&scope);
        assert_eq!(kr.active_kid, "ubl#k1");
    }
}
