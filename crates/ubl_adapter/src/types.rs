use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A frozen HTTP request — all parameters are deterministic and content-addressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpParams {
    /// Target URL
    pub url: String,
    /// HTTP method (GET, POST, etc.)
    #[serde(default = "default_method")]
    pub method: String,
    /// Request headers (sorted by key for determinism)
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Optional request body (will be CID-pinned)
    #[serde(default)]
    pub body: Option<String>,
    /// Timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_method() -> String {
    "GET".into()
}
fn default_timeout() -> u64 {
    10_000
}

impl HttpParams {
    /// Compute the CID of the frozen request parameters.
    pub fn params_cid(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        crate::cid::cid_b3(&bytes)
    }
}

/// A content-addressed blob (response body pinned by CID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedBlob {
    /// BLAKE3 CID of the response bytes
    pub cid: String,
    /// The raw response bytes (UTF-8 string for JSON/text responses)
    pub data: String,
    /// HTTP status code (for HTTP adapter)
    #[serde(default)]
    pub status: u16,
    /// Response headers (subset, for audit)
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl PinnedBlob {
    /// Create a new PinnedBlob from raw bytes, computing the CID.
    pub fn from_bytes(data: &[u8], status: u16, headers: BTreeMap<String, String>) -> Self {
        let cid = crate::cid::cid_b3(data);
        Self {
            cid,
            data: String::from_utf8_lossy(data).to_string(),
            status,
            headers,
        }
    }

    /// Verify that the data matches the claimed CID.
    pub fn verify(&self) -> bool {
        let actual = crate::cid::cid_b3(self.data.as_bytes());
        actual == self.cid
    }
}

/// Generic adapter request (kind-tagged).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRequest {
    /// Adapter kind: "http", "llm", etc.
    pub kind: String,
    /// CID of the frozen parameters
    pub params_cid: String,
    /// The frozen parameters (serialized)
    pub params: serde_json::Value,
    /// Policy constraints for this adapter call
    #[serde(default)]
    pub policy: AdapterPolicy,
}

/// Policy constraints on adapter execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterPolicy {
    /// Allowed URL patterns (glob). Empty = allow all.
    #[serde(default)]
    pub allowed_urls: Vec<String>,
    /// Max response size in bytes. 0 = no limit.
    #[serde(default)]
    pub max_response_bytes: usize,
    /// Max timeout in ms. 0 = use adapter default.
    #[serde(default)]
    pub max_timeout_ms: u64,
}

/// Generic adapter response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResponse {
    /// Adapter kind that produced this response
    pub kind: String,
    /// CID of the request parameters
    pub params_cid: String,
    /// The pinned response blob
    pub pinned: PinnedBlob,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_params_cid_deterministic() {
        let p1 = HttpParams {
            url: "https://api.example.com/data".into(),
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: None,
            timeout_ms: 5000,
        };
        let p2 = p1.clone();
        assert_eq!(p1.params_cid(), p2.params_cid());
    }

    #[test]
    fn http_params_cid_changes_with_url() {
        let p1 = HttpParams {
            url: "https://a.com".into(),
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: None,
            timeout_ms: 5000,
        };
        let p2 = HttpParams {
            url: "https://b.com".into(),
            ..p1.clone()
        };
        assert_ne!(p1.params_cid(), p2.params_cid());
    }

    #[test]
    fn pinned_blob_verify() {
        let blob = PinnedBlob::from_bytes(b"hello world", 200, BTreeMap::new());
        assert!(blob.verify());
        assert_eq!(blob.status, 200);
    }

    #[test]
    fn pinned_blob_tamper_detected() {
        let mut blob = PinnedBlob::from_bytes(b"hello world", 200, BTreeMap::new());
        blob.data = "tampered".into();
        assert!(!blob.verify());
    }

    #[test]
    fn adapter_request_serde_roundtrip() {
        let req = AdapterRequest {
            kind: "http".into(),
            params_cid: "b3:abc".into(),
            params: serde_json::json!({"url": "https://example.com"}),
            policy: AdapterPolicy::default(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let req2: AdapterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req2.kind, "http");
        assert_eq!(req2.params_cid, "b3:abc");
    }
}
