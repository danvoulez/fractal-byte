//! HTTP Adapter — freezes external HTTP responses by CID.
//!
//! The adapter takes a frozen `HttpParams` request, executes it outside
//! the deterministic boundary, and returns a `PinnedBlob` with the
//! response content-addressed by BLAKE3.
//!
//! Policy enforcement:
//! - URL allowlist (glob matching)
//! - Max response size
//! - Timeout

use crate::error::{AdapterError, Result};
use crate::types::{AdapterPolicy, AdapterResponse, HttpParams};
#[cfg(any(feature = "http", test))]
use crate::types::PinnedBlob;
#[cfg(any(feature = "http", test))]
use std::collections::BTreeMap;

/// Verify that the HTTP request is allowed by the adapter policy.
pub fn check_policy(params: &HttpParams, policy: &AdapterPolicy) -> Result<()> {
    // URL allowlist
    if !policy.allowed_urls.is_empty() {
        let allowed = policy.allowed_urls.iter().any(|pattern| {
            if pattern == "*" {
                return true;
            }
            // Simple glob: "https://api.example.com/*" matches any path
            if let Some(prefix) = pattern.strip_suffix('*') {
                params.url.starts_with(prefix)
            } else {
                params.url == *pattern
            }
        });
        if !allowed {
            return Err(AdapterError::PolicyDeny {
                adapter: format!("http: URL '{}' not in allowlist", params.url),
            });
        }
    }

    // Timeout cap
    if policy.max_timeout_ms > 0 && params.timeout_ms > policy.max_timeout_ms {
        return Err(AdapterError::Timeout {
            adapter: "http".into(),
            timeout_ms: policy.max_timeout_ms,
        });
    }

    Ok(())
}

/// Execute an HTTP request and pin the response by CID.
///
/// This is the IO boundary — it runs OUTSIDE the deterministic runtime.
/// The returned `AdapterResponse` contains a `PinnedBlob` whose CID
/// can be verified independently.
#[cfg(feature = "http")]
pub async fn execute(
    params: &HttpParams,
    policy: &AdapterPolicy,
) -> Result<AdapterResponse> {
    check_policy(params, policy)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(params.timeout_ms))
        .build()
        .map_err(|e| AdapterError::Http(e.to_string()))?;

    let mut req = match params.method.to_uppercase().as_str() {
        "GET" => client.get(&params.url),
        "POST" => client.post(&params.url),
        "PUT" => client.put(&params.url),
        "DELETE" => client.delete(&params.url),
        "PATCH" => client.patch(&params.url),
        "HEAD" => client.head(&params.url),
        other => {
            return Err(AdapterError::Http(format!(
                "unsupported method: {other}"
            )))
        }
    };

    for (k, v) in &params.headers {
        req = req.header(k.as_str(), v.as_str());
    }

    if let Some(body) = &params.body {
        req = req.body(body.clone());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| AdapterError::Http(e.to_string()))?;

    let status = resp.status().as_u16();

    // Capture response headers (subset for audit)
    let mut resp_headers = BTreeMap::new();
    for (k, v) in resp.headers() {
        if let Ok(val) = v.to_str() {
            resp_headers.insert(k.to_string(), val.to_string());
        }
    }

    let body_bytes = resp
        .bytes()
        .await
        .map_err(|e| AdapterError::Http(e.to_string()))?;

    // Enforce max response size
    if policy.max_response_bytes > 0 && body_bytes.len() > policy.max_response_bytes {
        return Err(AdapterError::Http(format!(
            "response too large: {} bytes (max {})",
            body_bytes.len(),
            policy.max_response_bytes
        )));
    }

    let pinned = PinnedBlob::from_bytes(&body_bytes, status, resp_headers);
    let params_cid = params.params_cid();

    Ok(AdapterResponse {
        kind: "http".into(),
        params_cid,
        pinned,
    })
}

/// Verify that a previously pinned response still matches its CID.
/// Used by the runtime to validate cached/replayed adapter responses.
pub fn verify_pinned(response: &AdapterResponse) -> Result<()> {
    if !response.pinned.verify() {
        let actual = crate::cid::cid_b3(response.pinned.data.as_bytes());
        return Err(AdapterError::CidMismatch {
            expected: response.pinned.cid.clone(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(url: &str) -> HttpParams {
        HttpParams {
            url: url.into(),
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: None,
            timeout_ms: 5000,
        }
    }

    #[test]
    fn policy_allows_wildcard() {
        let policy = AdapterPolicy {
            allowed_urls: vec!["*".into()],
            ..Default::default()
        };
        assert!(check_policy(&params("https://anything.com/path"), &policy).is_ok());
    }

    #[test]
    fn policy_allows_prefix_glob() {
        let policy = AdapterPolicy {
            allowed_urls: vec!["https://api.example.com/*".into()],
            ..Default::default()
        };
        assert!(check_policy(&params("https://api.example.com/v1/data"), &policy).is_ok());
        assert!(check_policy(&params("https://evil.com/data"), &policy).is_err());
    }

    #[test]
    fn policy_allows_exact_match() {
        let policy = AdapterPolicy {
            allowed_urls: vec!["https://api.example.com/v1/data".into()],
            ..Default::default()
        };
        assert!(check_policy(&params("https://api.example.com/v1/data"), &policy).is_ok());
        assert!(check_policy(&params("https://api.example.com/v1/other"), &policy).is_err());
    }

    #[test]
    fn policy_empty_allows_all() {
        let policy = AdapterPolicy::default();
        assert!(check_policy(&params("https://anything.com"), &policy).is_ok());
    }

    #[test]
    fn policy_timeout_cap() {
        let policy = AdapterPolicy {
            max_timeout_ms: 3000,
            ..Default::default()
        };
        let mut p = params("https://example.com");
        p.timeout_ms = 5000;
        assert!(check_policy(&p, &policy).is_err());
        p.timeout_ms = 2000;
        assert!(check_policy(&p, &policy).is_ok());
    }

    #[test]
    fn verify_pinned_ok() {
        let pinned = PinnedBlob::from_bytes(b"test data", 200, BTreeMap::new());
        let resp = AdapterResponse {
            kind: "http".into(),
            params_cid: "b3:test".into(),
            pinned,
        };
        assert!(verify_pinned(&resp).is_ok());
    }

    #[test]
    fn verify_pinned_tampered() {
        let mut pinned = PinnedBlob::from_bytes(b"test data", 200, BTreeMap::new());
        pinned.data = "tampered".into();
        let resp = AdapterResponse {
            kind: "http".into(),
            params_cid: "b3:test".into(),
            pinned,
        };
        assert!(verify_pinned(&resp).is_err());
    }
}
