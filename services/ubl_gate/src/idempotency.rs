//! Idempotency store: prevents duplicate processing of requests.
//!
//! Key: `scope|method|path|idempotency_key`
//! Value: SHA-256 of request body
//!
//! - Same key + same body hash → Replay (409)
//! - Same key + different body hash → KeyReusedDifferentPayload (409)
//! - LRU bounded (deterministic via monotonic `last_touch` + `seq`) + TTL eviction

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Entry {
    body_hash: [u8; 32],
    created_at: Instant,
    seq: u64,
    last_touch: u64,
}

/// Result of checking idempotency.
#[derive(Debug, PartialEq)]
pub enum IdempCheck {
    /// First time seeing this key — proceed with the request.
    New,
    /// Same key + same body hash — replay.
    Replay,
    /// Same key + different body hash — conflict.
    KeyReusedDifferentPayload,
}

struct Inner {
    entries: HashMap<String, Entry>,
    cap: usize,
    ttl: Duration,
    seq_ctr: u64,
    touch_ctr: u64,
}

impl Inner {
    #[inline]
    fn next_seq(&mut self) -> u64 {
        let n = self.seq_ctr;
        self.seq_ctr += 1;
        n
    }
    #[inline]
    fn next_touch(&mut self) -> u64 {
        let n = self.touch_ctr;
        self.touch_ctr += 1;
        n
    }

    fn evict_if_needed(&mut self) {
        if self.entries.len() <= self.cap {
            return;
        }
        if let Some(victim) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| (e.last_touch, e.seq))
            .map(|(k, _)| k.clone())
        {
            self.entries.remove(&victim);
        }
    }
}

#[derive(Clone)]
pub struct IdempotencyStore {
    inner: Arc<Mutex<Inner>>,
}

impl IdempotencyStore {
    pub fn new(cap: usize, ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: HashMap::with_capacity(cap.saturating_mul(2)),
                cap,
                ttl,
                seq_ctr: 0,
                touch_ctr: 0,
            })),
        }
    }

    pub fn from_env() -> Self {
        let cap: usize = std::env::var("IDEMP_MAX_ENTRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);
        let ttl_secs: u64 = std::env::var("IDEMP_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(86_400); // 24h
        Self::new(cap, Duration::from_secs(ttl_secs))
    }

    /// Hash a request body.
    pub fn hash_body(body: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(body);
        hasher.finalize().into()
    }

    /// Check + insert. Returns the idempotency verdict.
    /// On Replay, the entry is "touched" so it stays in the LRU longer.
    pub fn check(
        &self,
        scope_prefix: &str,
        method: &str,
        path: &str,
        idemp_key: &str,
        body_hash: [u8; 32],
    ) -> IdempCheck {
        let k = format!("{scope_prefix}|{method}|{path}|{idemp_key}");
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let ttl = inner.ttl;

        // Lazy TTL sweep
        inner
            .entries
            .retain(|_, e| now.duration_since(e.created_at) < ttl);

        if let Some(e) = inner.entries.get(&k) {
            if e.body_hash == body_hash {
                // Replay — touch to keep alive in LRU
                let touch = inner.next_touch();
                inner.entries.get_mut(&k).unwrap().last_touch = touch;
                return IdempCheck::Replay;
            } else {
                return IdempCheck::KeyReusedDifferentPayload;
            }
        }

        // New entry
        let seq = inner.next_seq();
        let touch = inner.next_touch();
        let entry = Entry {
            body_hash,
            created_at: now,
            seq,
            last_touch: touch,
        };
        inner.entries.insert(k, entry);
        inner.evict_if_needed();

        IdempCheck::New
    }

    /// Record a key after successful processing (for cases where we want to
    /// record without pre-checking, e.g. the existing pipeline-based idempotency).
    pub fn record(
        &self,
        scope_prefix: &str,
        method: &str,
        path: &str,
        idemp_key: &str,
        body_hash: [u8; 32],
    ) {
        let _ = self.check(scope_prefix, method, path, idemp_key, body_hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> [u8; 32] {
        IdempotencyStore::hash_body(s.as_bytes())
    }

    #[test]
    fn new_key_returns_new() {
        let store = IdempotencyStore::new(100, Duration::from_secs(60));
        assert_eq!(
            store.check("default:default", "POST", "/v1/execute", "key1", h("hello")),
            IdempCheck::New
        );
    }

    #[test]
    fn same_key_same_body_returns_replay() {
        let store = IdempotencyStore::new(100, Duration::from_secs(60));
        store.check("default:default", "POST", "/v1/execute", "key1", h("hello"));
        assert_eq!(
            store.check("default:default", "POST", "/v1/execute", "key1", h("hello")),
            IdempCheck::Replay
        );
    }

    #[test]
    fn same_key_different_body_returns_conflict() {
        let store = IdempotencyStore::new(100, Duration::from_secs(60));
        store.check("default:default", "POST", "/v1/execute", "key1", h("hello"));
        assert_eq!(
            store.check("default:default", "POST", "/v1/execute", "key1", h("world")),
            IdempCheck::KeyReusedDifferentPayload
        );
    }

    #[test]
    fn different_scopes_are_independent() {
        let store = IdempotencyStore::new(100, Duration::from_secs(60));
        store.check("app1:tenant1", "POST", "/v1/execute", "key1", h("hello"));
        assert_eq!(
            store.check("app2:tenant2", "POST", "/v1/execute", "key1", h("hello")),
            IdempCheck::New
        );
    }

    #[test]
    fn lru_eviction_is_deterministic() {
        let store = IdempotencyStore::new(2, Duration::from_secs(60));
        // Insert k1, k2 (at capacity)
        assert_eq!(store.check("a:t", "POST", "/x", "k1", h("a")), IdempCheck::New);
        assert_eq!(store.check("a:t", "POST", "/x", "k2", h("b")), IdempCheck::New);
        // Touch k1 → k2 becomes LRU (lowest last_touch)
        assert_eq!(store.check("a:t", "POST", "/x", "k1", h("a")), IdempCheck::Replay);
        // Insert k3 → must evict k2 (lowest last_touch)
        assert_eq!(store.check("a:t", "POST", "/x", "k3", h("c")), IdempCheck::New);
        // k1 still present (was touched)
        assert_eq!(store.check("a:t", "POST", "/x", "k1", h("a")), IdempCheck::Replay);
        // k2 was evicted → New
        assert_eq!(store.check("a:t", "POST", "/x", "k2", h("b")), IdempCheck::New);
    }

    #[test]
    fn ttl_eviction() {
        let store = IdempotencyStore::new(100, Duration::from_millis(1));
        store.check("s", "POST", "/", "k1", h("hello"));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(store.check("s", "POST", "/", "k1", h("hello")), IdempCheck::New);
    }
}
