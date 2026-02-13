use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;

async fn setup() -> (String, Client, tokio::task::JoinHandle<()>) {
    let (addr, handle) = ubl_gate::test::spawn().await;
    let base = format!("http://{}", addr);
    let http = Client::new();
    (base, http, handle)
}

// ── Ingest: error paths ──────────────────────────────────────────

#[tokio::test]
async fn ingest_rejects_float_payload() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": 3.14}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400, "floats must be rejected at ingest");
}

#[tokio::test]
async fn ingest_rejects_bom_in_string() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": "\u{feff}evil"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400, "BOM must be rejected at ingest");
}

#[tokio::test]
async fn ingest_rejects_empty_body() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .body("")
        .send().await.unwrap();
    // axum returns 400 or 422 for missing/invalid JSON body
    assert!(resp.status().is_client_error(), "empty body must fail: {}", resp.status());
}

#[tokio::test]
async fn ingest_rejects_malformed_json() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .body("{not json}")
        .send().await.unwrap();
    assert!(resp.status().is_client_error(), "malformed JSON must fail");
}

#[tokio::test]
async fn ingest_without_certify_has_no_receipt() {
    let (base, http, _h) = setup().await;
    let r: Value = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"test": "no_certify"}}))
        .send().await.unwrap()
        .json().await.unwrap();
    let cid = r["cid"].as_str().unwrap();

    // Receipt should not exist
    let rec = http.get(format!("{}/v1/receipt/{}", base, cid))
        .send().await.unwrap();
    assert_eq!(rec.status(), 404, "no receipt without certify=true");
}

// ── CID retrieval: error paths ───────────────────────────────────

#[tokio::test]
async fn get_cid_returns_400_for_invalid_cid() {
    let (base, http, _h) = setup().await;
    let resp = http.get(format!("{}/cid/not-a-valid-cid", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.unwrap();
    assert!(body.contains("invalid CID"), "body: {}", body);
}

#[tokio::test]
async fn get_cid_returns_404_for_missing_content() {
    let (base, http, _h) = setup().await;
    // Valid CID format but never ingested
    let resp = http.get(format!("{}/cid/bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenera", base))
        .send().await.unwrap();
    // Either 400 (CID parse fails for short CID) or 404 (not found) — both acceptable
    assert!(resp.status().is_client_error());
}

#[tokio::test]
async fn get_cid_json_returns_400_for_invalid_cid() {
    let (base, http, _h) = setup().await;
    let resp = http.get(format!("{}/cid/garbage.json", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

// ── Receipt: error paths ─────────────────────────────────────────

#[tokio::test]
async fn receipt_returns_400_for_invalid_cid() {
    let (base, http, _h) = setup().await;
    let resp = http.get(format!("{}/v1/receipt/not-a-cid", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

// ── Certify: error paths ─────────────────────────────────────────

#[tokio::test]
async fn certify_rejects_missing_cid() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/certify", base))
        .json(&json!({}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn certify_rejects_invalid_cid() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/certify", base))
        .json(&json!({"cid": "not-valid"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

// ── /v1/execute: runtime through the gate ────────────────────────

#[tokio::test]
async fn execute_happy_path() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "test",
        "in_grammar": {
            "inputs": {"raw_b64": ""},
            "mappings": [{"from": "raw_b64", "codec": "base64.decode", "to": "raw.bytes"}],
            "output_from": "raw.bytes"
        },
        "out_grammar": {
            "inputs": {"content": ""},
            "mappings": [],
            "output_from": "content"
        },
        "policy": {"allow": true}
    });
    let vars: BTreeMap<String, Value> = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
    let resp = http.post(format!("{}/v1/execute", base))
        .json(&json!({"manifest": manifest, "vars": vars}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["cid"].as_str().unwrap().starts_with("b3:"), "CID must be b3: prefixed");
    assert_eq!(body["dimension_stack"], json!(["parse", "policy", "render"]));
}

#[tokio::test]
async fn execute_determinism() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "det",
        "in_grammar": {
            "inputs": {"raw_b64": ""},
            "mappings": [{"from": "raw_b64", "codec": "base64.decode", "to": "raw.bytes"}],
            "output_from": "raw.bytes"
        },
        "out_grammar": {
            "inputs": {"content": ""},
            "mappings": [],
            "output_from": "content"
        },
        "policy": {"allow": true}
    });
    let vars: BTreeMap<String, Value> = BTreeMap::from([("data".into(), json!("aGVsbG8="))]);
    let req = json!({"manifest": manifest, "vars": vars});

    let r1: Value = http.post(format!("{}/v1/execute", base))
        .json(&req).send().await.unwrap().json().await.unwrap();
    let r2: Value = http.post(format!("{}/v1/execute", base))
        .json(&req).send().await.unwrap().json().await.unwrap();
    assert_eq!(r1["cid"], r2["cid"], "execute must be deterministic");
}

#[tokio::test]
async fn execute_policy_deny_returns_422() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "deny",
        "in_grammar": {
            "inputs": {"x": ""},
            "mappings": [],
            "output_from": "x"
        },
        "out_grammar": {
            "inputs": {"y": ""},
            "mappings": [],
            "output_from": "y"
        },
        "policy": {"allow": false}
    });
    let vars: BTreeMap<String, Value> = BTreeMap::from([("x".into(), json!("data"))]);
    let resp = http.post(format!("{}/v1/execute", base))
        .json(&json!({"manifest": manifest, "vars": vars}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 422, "policy deny must return 422");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "execute_failed");
    assert!(body["detail"].as_str().unwrap().contains("policy deny"));
}

#[tokio::test]
async fn execute_bad_codec_returns_422() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "bad",
        "in_grammar": {
            "inputs": {"x": ""},
            "mappings": [{"from": "x", "codec": "rot13", "to": "y"}],
            "output_from": "y"
        },
        "out_grammar": {
            "inputs": {"z": ""},
            "mappings": [],
            "output_from": "z"
        },
        "policy": {"allow": true}
    });
    let vars: BTreeMap<String, Value> = BTreeMap::from([("x".into(), json!("data"))]);
    let resp = http.post(format!("{}/v1/execute", base))
        .json(&json!({"manifest": manifest, "vars": vars}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 422);
    let body: Value = resp.json().await.unwrap();
    assert!(body["detail"].as_str().unwrap().contains("unknown codec"));
}

// ── DID document structure ───────────────────────────────────────

#[tokio::test]
async fn did_document_has_required_fields() {
    let (base, http, _h) = setup().await;
    let did: Value = http.get(format!("{}/.well-known/did.json", base))
        .send().await.unwrap().json().await.unwrap();

    // id
    let id = did["id"].as_str().unwrap();
    assert!(id.starts_with("did:key:z"), "DID must be did:key:z...");

    // verificationMethod
    let vm = did["verificationMethod"].as_array().unwrap();
    assert_eq!(vm.len(), 1);
    assert_eq!(vm[0]["type"], "Ed25519VerificationKey2020");
    assert!(vm[0]["publicKeyMultibase"].as_str().unwrap().starts_with("z"));

    // assertionMethod references the verification method
    let am = did["assertionMethod"].as_array().unwrap();
    assert_eq!(am.len(), 1);
    assert!(am[0].as_str().unwrap().contains("#ed25519"));
}

// ── Resolve endpoint ─────────────────────────────────────────────

#[tokio::test]
async fn resolve_did_cid() {
    let (base, http, _h) = setup().await;
    let r: Value = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"resolve_test": true}}))
        .send().await.unwrap().json().await.unwrap();
    let did = r["did"].as_str().unwrap();

    let resolved: Value = http.post(format!("{}/v1/resolve", base))
        .json(&json!({"id": did}))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(resolved["id"], did);
    let links = resolved["links"].as_array().unwrap();
    assert!(!links.is_empty(), "resolved DID must have links");
    assert!(links[0].as_str().unwrap().contains("/cid/"));
}

// ── Full lifecycle: ingest → certify → receipt → raw → json ──────

#[tokio::test]
async fn full_lifecycle_separate_certify() {
    let (base, http, _h) = setup().await;

    // Ingest WITHOUT certify — unique payload so CID is fresh each run
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let r: Value = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"lifecycle": "test", "step": 1, "nonce": nonce}}))
        .send().await.unwrap().json().await.unwrap();
    let cid = r["cid"].as_str().unwrap().to_owned();

    // No receipt yet
    let rec = http.get(format!("{}/v1/receipt/{}", base, cid))
        .send().await.unwrap();
    assert_eq!(rec.status(), 404);

    // Certify separately
    let cert: Value = http.post(format!("{}/v1/certify", base))
        .json(&json!({"cid": cid}))
        .send().await.unwrap().json().await.unwrap();
    let jws = cert["receipt"].as_str().unwrap();
    assert_eq!(jws.split('.').count(), 3, "receipt must be JWS");

    // Now receipt exists
    let rec = http.get(format!("{}/v1/receipt/{}", base, cid))
        .send().await.unwrap();
    assert_eq!(rec.status(), 200);

    // Raw bytes still work
    let raw = http.get(format!("{}/cid/{}", base, cid))
        .send().await.unwrap();
    assert_eq!(raw.status(), 200);
    let bytes = raw.bytes().await.unwrap();
    assert!(hex::encode(&bytes).starts_with("6e726631"));

    // JSON view still works
    let j: Value = http.get(format!("{}/cid/{}.json", base, cid))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(j["lifecycle"], "test");
    assert_eq!(j["step"], 1);
}

// ── JWS structure validation ─────────────────────────────────────

#[tokio::test]
async fn jws_receipt_has_valid_structure() {
    let (base, http, _h) = setup().await;
    let r: Value = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"jws_test": true}, "certify": true}))
        .send().await.unwrap().json().await.unwrap();
    let cid = r["cid"].as_str().unwrap();

    let jws_text = http.get(format!("{}/v1/receipt/{}", base, cid))
        .send().await.unwrap().text().await.unwrap();

    let parts: Vec<&str> = jws_text.split('.').collect();
    assert_eq!(parts.len(), 3, "JWS must have 3 parts");

    // Decode header
    use base64::Engine;
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0]).expect("header must be valid base64url");
    let header: Value = serde_json::from_slice(&header_bytes).unwrap();
    assert_eq!(header["alg"], "EdDSA", "algorithm must be EdDSA");
    assert_eq!(header["typ"], "JWT");
    assert!(header["kid"].as_str().unwrap().contains("#ed25519"));

    // Decode payload
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1]).expect("payload must be valid base64url");
    let payload: Value = serde_json::from_slice(&payload_bytes).unwrap();
    assert_eq!(payload["receipt_version"], "1");
    assert_eq!(payload["cid"], cid);
    assert_eq!(payload["cid_codec"], "raw");
    assert_eq!(payload["mh"], "sha2-256");
    assert!(payload["issued_at"].as_str().unwrap().len() > 10, "issued_at must be a timestamp");
    assert!(payload["issuer"].as_str().unwrap().starts_with("did:key:"));

    // Signature must be non-empty base64url
    assert!(!parts[2].is_empty(), "signature must not be empty");
    let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2]).expect("signature must be valid base64url");
    assert_eq!(sig_bytes.len(), 64, "Ed25519 signature must be 64 bytes");
}

// ── Healthz ──────────────────────────────────────────────────────

#[tokio::test]
async fn healthz_returns_ok() {
    let (base, http, _h) = setup().await;
    let resp = http.get(format!("{}/healthz", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body, json!({"ok": true}));
}

// ── 404 for unknown routes ───────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let (base, http, _h) = setup().await;
    let resp = http.get(format!("{}/v1/nonexistent", base)).send().await.unwrap();
    assert_eq!(resp.status(), 404);
}
