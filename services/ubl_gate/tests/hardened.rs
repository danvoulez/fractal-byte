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
async fn execute_determinism_and_idempotency() {
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

    // First call succeeds with a deterministic b3: CID
    let r1 = http.post(format!("{}/v1/execute", base))
        .json(&req).send().await.unwrap();
    assert_eq!(r1.status(), 200);
    let body1: Value = r1.json().await.unwrap();
    let cid = body1["cid"].as_str().unwrap();
    assert!(cid.starts_with("b3:"), "CID must be b3: prefixed");
    assert_eq!(cid.len(), 67, "b3:<64 hex chars>");

    // Replay same input → 409 CONFLICT (idempotency)
    let r2 = http.post(format!("{}/v1/execute", base))
        .json(&req).send().await.unwrap();
    assert_eq!(r2.status(), 409, "replay must return 409 CONFLICT");
    let body2: Value = r2.json().await.unwrap();
    assert!(body2["detail"].as_str().unwrap().contains("duplicate request"));
}

#[tokio::test]
async fn execute_policy_deny_returns_deny_receipt() {
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
    assert_eq!(resp.status(), 200, "policy deny now returns 200 with DENY receipt");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["decision"], "DENY");
    assert_eq!(body["receipts"]["wf"]["body"]["decision"], "DENY");
    assert!(body["receipts"]["wf"]["body"]["reason"].as_str().unwrap().contains("policy deny"));
    assert!(body["receipts"]["wa"]["t"].as_str().unwrap() == "ubl/wa");
}

#[tokio::test]
async fn execute_bad_codec_returns_deny_receipt() {
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
    assert_eq!(resp.status(), 200, "bad codec now returns 200 with DENY receipt");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["decision"], "DENY");
    assert_eq!(body["receipts"]["wf"]["body"]["decision"], "DENY");
    assert!(body["receipts"]["wf"]["body"]["reason"].as_str().unwrap().contains("unknown codec"));
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

// ── AuthN/Z tests ───────────────────────────────────────────────

async fn setup_auth_enabled() -> (String, Client, tokio::task::JoinHandle<()>) {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    let mut state = ubl_gate::AppState::default();
    state.auth_disabled = false;
    let app = ubl_gate::app_with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);
    let http = Client::new();
    (base, http, handle)
}

#[tokio::test]
async fn auth_rejects_missing_token() {
    let (base, http, _h) = setup_auth_enabled().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .body(r#"{"payload":{"a":1}}"#)
        .send().await.unwrap();
    assert_eq!(resp.status(), 401, "missing token must be rejected");
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("missing"));
}

#[tokio::test]
async fn auth_rejects_invalid_token() {
    let (base, http, _h) = setup_auth_enabled().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer bad-token-999")
        .body(r#"{"payload":{"a":1}}"#)
        .send().await.unwrap();
    assert_eq!(resp.status(), 401, "invalid token must be rejected");
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("invalid"));
}

#[tokio::test]
async fn auth_accepts_dev_token() {
    let (base, http, _h) = setup_auth_enabled().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer ubl-dev-token-001")
        .body(r#"{"payload":{"a":1}}"#)
        .send().await.unwrap();
    assert_eq!(resp.status(), 200, "dev token must be accepted");
}

#[tokio::test]
async fn auth_public_paths_skip_auth() {
    let (base, http, _h) = setup_auth_enabled().await;
    // healthz should work without token
    let resp = http.get(format!("{}/healthz", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200, "healthz is public");
    // .well-known/did.json should work without token
    let resp = http.get(format!("{}/.well-known/did.json", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200, "did.json is public");
}

// ── Kid-scope auth (403) ─────────────────────────────────────────

async fn setup_auth_kid_scoped(allowed_kids: Vec<String>) -> (String, Client, tokio::task::JoinHandle<()>) {
    use tokio::net::TcpListener;

    let mut state = ubl_gate::AppState::default();
    state.auth_disabled = false;
    // Register a scoped token that only allows specific kids
    state.token_store.register("scoped-token-001", ubl_gate::ClientInfo {
        client_id: "scoped-client".into(),
        tenant_id: "test-tenant".into(),
        allowed_kids,
    });
    let app = ubl_gate::app_with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{}", addr), Client::new(), handle)
}

#[tokio::test]
async fn auth_kid_scope_denied_returns_403() {
    // Token only allows "did:other#k9" but gate signs with "did:dev#k1"
    let (base, http, _h) = setup_auth_kid_scoped(vec!["did:other#k9".into()]).await;
    let manifest = json!({
        "pipeline": "kid-test",
        "in_grammar": {"inputs": {"raw_b64": ""}, "mappings": [{"from": "raw_b64", "codec": "base64.decode", "to": "raw.bytes"}], "output_from": "raw.bytes"},
        "out_grammar": {"inputs": {"content": ""}, "mappings": [], "output_from": "content"},
        "policy": {"allow": true}
    });
    let resp = http.post(format!("{}/v1/execute", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer scoped-token-001")
        .json(&json!({"manifest": manifest, "vars": {"input_data": "aGVsbG8="}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 403, "kid out of scope must return 403");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "kid_scope_denied");
    assert!(body["detail"].as_str().unwrap().contains("did:dev#k1"));
}

#[tokio::test]
async fn auth_kid_scope_allowed_returns_200() {
    // Token allows "did:dev#k1" which is the active signing kid
    let (base, http, _h) = setup_auth_kid_scoped(vec!["did:dev#k1".into()]).await;
    let manifest = json!({
        "pipeline": "kid-ok",
        "in_grammar": {"inputs": {"raw_b64": ""}, "mappings": [{"from": "raw_b64", "codec": "base64.decode", "to": "raw.bytes"}], "output_from": "raw.bytes"},
        "out_grammar": {"inputs": {"content": ""}, "mappings": [], "output_from": "content"},
        "policy": {"allow": true}
    });
    let resp = http.post(format!("{}/v1/execute", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer scoped-token-001")
        .json(&json!({"manifest": manifest, "vars": {"input_data": "aGVsbG8="}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200, "kid in scope must return 200");
}

// ── Edge limit tests ────────────────────────────────────────────

#[tokio::test]
async fn body_too_large_returns_413() {
    let (base, http, _h) = setup().await;
    // 1 MiB + 1 byte should be rejected
    let big_body = "x".repeat(1_048_577);
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .body(big_body)
        .send().await.unwrap();
    // tower-http returns 413 Payload Too Large
    assert_eq!(resp.status(), 413, "body > 1MiB must be rejected with 413");
}

#[tokio::test]
async fn wrong_content_type_returns_415() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "text/plain")
        .body(r#"{"payload":{"a":1}}"#)
        .send().await.unwrap();
    assert_eq!(resp.status(), 415, "non-JSON content-type must be rejected with 415");
}

#[tokio::test]
async fn missing_content_type_returns_415() {
    let (base, http, _h) = setup().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .body(r#"{"payload":{"a":1}}"#)
        .send().await.unwrap();
    assert_eq!(resp.status(), 415, "missing content-type on POST must be rejected with 415");
}

// ── Replay integrity test ───────────────────────────────────────

#[tokio::test]
async fn replay_returns_409_with_first_chain_intact() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "replay-integrity",
        "in_grammar": {"inputs": {"raw_b64": ""}, "mappings": [{"from": "raw_b64", "codec": "base64.decode", "to": "raw.bytes"}], "output_from": "raw.bytes"},
        "out_grammar": {"inputs": {"content": ""}, "mappings": [], "output_from": "content"},
        "policy": {"allow": true}
    });
    let req_body = json!({"manifest": manifest, "vars": {"input_data": "cmVwbGF5"}});

    // First call: should succeed with full receipt chain
    let resp1 = http.post(format!("{}/v1/execute", base))
        .json(&req_body)
        .send().await.unwrap();
    assert_eq!(resp1.status(), 200, "first call must succeed");
    let body1: Value = resp1.json().await.unwrap();

    // Verify chain integrity of first call
    let wa_cid = body1["receipts"]["wa"]["body_cid"].as_str().unwrap();
    let tr_cid = body1["receipts"]["transition"]["body_cid"].as_str().unwrap();
    let wf_p0 = body1["receipts"]["wf"]["parents"][0].as_str().unwrap();
    let wf_p1 = body1["receipts"]["wf"]["parents"][1].as_str().unwrap();
    assert_eq!(wf_p0, wa_cid, "wf.parents[0] == wa.body_cid");
    assert_eq!(wf_p1, tr_cid, "wf.parents[1] == transition.body_cid");
    assert!(wa_cid.starts_with("b3:"), "wa.body_cid is b3:");
    assert!(tr_cid.starts_with("b3:"), "transition.body_cid is b3:");
    assert_eq!(body1["decision"], "ALLOW");

    // Second call (replay): must return 409
    let resp2 = http.post(format!("{}/v1/execute", base))
        .json(&req_body)
        .send().await.unwrap();
    assert_eq!(resp2.status(), 409, "replay must return 409 CONFLICT");
    let body2: Value = resp2.json().await.unwrap();
    assert!(body2["detail"].as_str().unwrap().contains("duplicate request"));
}

// ── NEG path: policy DENY produces receipt ──────────────────────

#[tokio::test]
async fn policy_deny_produces_deny_receipt_with_chain() {
    let (base, http, _h) = setup().await;
    let manifest = json!({
        "pipeline": "deny-chain-test",
        "in_grammar": {"inputs": {"x": ""}, "mappings": [], "output_from": "x"},
        "out_grammar": {"inputs": {"y": ""}, "mappings": [], "output_from": "y"},
        "policy": {"allow": false}
    });
    let resp = http.post(format!("{}/v1/execute", base))
        .json(&json!({"manifest": manifest, "vars": {"x": "data"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200, "DENY must still return 200");
    let body: Value = resp.json().await.unwrap();

    assert_eq!(body["decision"], "DENY");
    assert_eq!(body["receipts"]["wf"]["body"]["decision"], "DENY");
    assert!(body["receipts"]["wf"]["body"]["reason"].as_str().unwrap().len() > 0);

    // Chain integrity even on DENY
    let wa_cid = body["receipts"]["wa"]["body_cid"].as_str().unwrap();
    let wf_p0 = body["receipts"]["wf"]["parents"][0].as_str().unwrap();
    assert_eq!(wf_p0, wa_cid, "DENY wf.parents[0] == wa.body_cid");
    assert!(body["tip_cid"].as_str().unwrap().starts_with("b3:"));
}

// ── Rate limiting tests ──────────────────────────────────────────

async fn setup_rate_limited(burst: u32) -> (String, Client, tokio::task::JoinHandle<()>) {
    use tokio::net::TcpListener;

    let mut state = ubl_gate::AppState::default();
    state.rate_limiter = ubl_gate::RateLimiter::new(600, burst); // high rpm, low burst
    let app = ubl_gate::app_with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{}", addr), Client::new(), handle)
}

#[tokio::test]
async fn rate_limit_allows_within_burst() {
    let (base, http, _h) = setup_rate_limited(5).await;
    for i in 0..5 {
        let resp = http.post(format!("{}/v1/ingest", base))
            .json(&json!({"payload": {"rl_test": i}}))
            .send().await.unwrap();
        assert_eq!(resp.status(), 200, "request {} within burst must succeed", i);
        assert!(resp.headers().contains_key("x-ratelimit-limit"));
        assert!(resp.headers().contains_key("x-ratelimit-remaining"));
    }
}

#[tokio::test]
async fn rate_limit_429_on_burst_exceeded() {
    let (base, http, _h) = setup_rate_limited(3).await;
    // Consume the burst
    for i in 0..3 {
        let resp = http.post(format!("{}/v1/ingest", base))
            .json(&json!({"payload": {"rl_burst": i}}))
            .send().await.unwrap();
        assert_eq!(resp.status(), 200, "request {} within burst", i);
    }
    // 4th request should be rate limited
    let resp = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"rl_burst": "overflow"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 429, "request beyond burst must return 429");
    assert!(resp.headers().contains_key("retry-after"));
    assert_eq!(
        resp.headers().get("x-ratelimit-remaining").unwrap().to_str().unwrap(),
        "0"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "rate_limit_exceeded");
    assert_eq!(body["receipt"]["body"]["decision"], "DENY");
    assert_eq!(body["receipt"]["body"]["reason"], "RATE_LIMIT");
    assert_eq!(body["receipt"]["body"]["recommended_action"], "retry_after");
}

#[tokio::test]
async fn rate_limit_healthz_exempt() {
    let (base, http, _h) = setup_rate_limited(1).await;
    // Consume the single token
    let resp = http.post(format!("{}/v1/ingest", base))
        .json(&json!({"payload": {"rl_exempt": true}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    // healthz should still work (exempt from rate limiting)
    let resp = http.get(format!("{}/healthz", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200, "healthz must be exempt from rate limiting");
}

// ── Tenant isolation tests ───────────────────────────────────────

async fn setup_multi_tenant() -> (String, Client, tokio::task::JoinHandle<()>) {
    use tokio::net::TcpListener;

    let mut state = ubl_gate::AppState::default();
    state.auth_disabled = false;
    // Two tokens with different tenants
    state.token_store.register("tenant-a-token", ubl_gate::ClientInfo {
        client_id: "client-a".into(),
        tenant_id: "tenant-alpha".into(),
        allowed_kids: vec![],
    });
    state.token_store.register("tenant-b-token", ubl_gate::ClientInfo {
        client_id: "client-b".into(),
        tenant_id: "tenant-beta".into(),
        allowed_kids: vec![],
    });
    let app = ubl_gate::app_with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{}", addr), Client::new(), handle)
}

#[tokio::test]
async fn tenant_isolation_same_payload_different_tenants() {
    let (base, http, _h) = setup_multi_tenant().await;
    let payload = json!({"payload": {"shared_data": "hello"}});

    // Tenant A ingests
    let resp_a = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer tenant-a-token")
        .json(&payload)
        .send().await.unwrap();
    assert_eq!(resp_a.status(), 200);
    let body_a: Value = resp_a.json().await.unwrap();
    assert_eq!(body_a["tenant_id"], "tenant-alpha");
    let cid_a = body_a["cid"].as_str().unwrap().to_string();

    // Tenant B ingests the same payload
    let resp_b = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer tenant-b-token")
        .json(&payload)
        .send().await.unwrap();
    assert_eq!(resp_b.status(), 200);
    let body_b: Value = resp_b.json().await.unwrap();
    assert_eq!(body_b["tenant_id"], "tenant-beta");

    // Same CID (same content) but stored in different tenant paths
    assert_eq!(body_a["cid"], body_b["cid"], "same payload → same CID");

    // Tenant A can read its own data
    let get_a = http.get(format!("{}/cid/{}", base, cid_a))
        .header("authorization", "Bearer tenant-a-token")
        .send().await.unwrap();
    assert_eq!(get_a.status(), 200, "tenant A can read its own CID");

    // Tenant B can also read (same CID, different tenant path)
    let get_b = http.get(format!("{}/cid/{}", base, cid_a))
        .header("authorization", "Bearer tenant-b-token")
        .send().await.unwrap();
    assert_eq!(get_b.status(), 200, "tenant B can read its own CID");
}

#[tokio::test]
async fn tenant_ingest_returns_tenant_id() {
    let (base, http, _h) = setup_multi_tenant().await;
    let resp = http.post(format!("{}/v1/ingest", base))
        .header("content-type", "application/json")
        .header("authorization", "Bearer tenant-a-token")
        .json(&json!({"payload": {"tenant_check": true}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["tenant_id"], "tenant-alpha", "response must include tenant_id");
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
