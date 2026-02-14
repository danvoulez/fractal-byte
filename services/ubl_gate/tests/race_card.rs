use reqwest::Client;
use serde_json::Value;

#[tokio::test]
async fn race_card_end_to_end() {
    let (addr, _handle) = ubl_gate::test::spawn().await;
    let base = format!("http://{addr}");
    let http = Client::new();

    // 0) healthz
    let health: Value = http
        .get(format!("{base}/healthz"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["ok"], true);

    // 1) ingest + certify
    let r: Value = http
        .post(format!("{base}/v1/ingest"))
        .json(&serde_json::json!({"payload":{"hello":"world","n":42},"certify":true}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid = r["cid"].as_str().unwrap().to_owned();
    assert!(!cid.is_empty());
    assert_eq!(r["bytes_len"], 32);
    assert_eq!(r["content_type"], "application/x-nrf");

    // 2) raw bytes
    let raw = http.get(format!("{base}/cid/{cid}")).send().await.unwrap();
    assert_eq!(raw.status(), 200);
    let bytes = raw.bytes().await.unwrap();
    assert_eq!(bytes.len(), 32);
    assert!(hex::encode(&bytes).starts_with("6e726631")); // NRF magic "nrf1"

    // 3) json view (NRF round-trip)
    let j: Value = http
        .get(format!("{base}/cid/{cid}.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(j["hello"], "world");
    assert_eq!(j["n"], 42);

    // 4) receipt (JWS: 3 dot-separated segments)
    let rec = http
        .get(format!("{base}/v1/receipt/{cid}"))
        .send()
        .await
        .unwrap();
    assert_eq!(rec.status(), 200);
    let jws = rec.text().await.unwrap();
    assert_eq!(jws.split('.').count(), 3, "receipt must be a JWS");

    // 5) DID document
    let did: Value = http
        .get(format!("{base}/.well-known/did.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(did["id"].as_str().unwrap().starts_with("did:key:"));
    assert!(!did["verificationMethod"].as_array().unwrap().is_empty());

    // 6) determinism — same payload ⇒ same CID
    for _ in 0..3 {
        let r2: Value = http
            .post(format!("{base}/v1/ingest"))
            .json(&serde_json::json!({"payload":{"hello":"world","n":42}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(r2["cid"].as_str().unwrap(), cid, "determinism violated");
    }
}
