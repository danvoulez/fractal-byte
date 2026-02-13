
use axum::{extract::{Path, State}, http::{StatusCode, header}, response::IntoResponse, Json, Extension};
use crate::{AppState, ClientInfo};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use cid::Cid;
use ubl_ai_nrf1::nrf::{self, NrfValue};
use ubl_ai_nrf1::nrf::{encode_to_vec, cid_from_nrf_bytes, json_to_nrf};
use ubl_config::BASE_URL;

#[derive(Debug, Deserialize)]
pub struct IngestReq { pub payload: Value, pub certify: Option<bool> }

pub async fn ingest(Json(req): Json<IngestReq>) -> impl IntoResponse {
    let nrf_val = match json_to_nrf(&req.payload) { Ok(v)=>v, Err(e)=> return (StatusCode::BAD_REQUEST, e.to_string()).into_response() };
    let nrf_bytes = match encode_to_vec(&nrf_val) { Ok(b)=>b, Err(e)=> return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() };
    let cid = cid_from_nrf_bytes(&nrf_bytes);
    if !ubl_ledger::exists(&cid).await { if let Err(e)=ubl_ledger::put(&cid, &nrf_bytes).await { return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(); } }
    if req.certify.unwrap_or(false) { let _ = ubl_receipt::issue_receipt(&cid, nrf_bytes.len()).await; }
    let resp = json!({
        "cid": cid.to_string(),
        "did": format!("did:cid:{}", cid),
        "bytes_len": nrf_bytes.len(),
        "content_type": "application/x-nrf",
        "url": format!("{}/cid/{}", BASE_URL.as_str(), cid),
        "receipt_url": format!("{}/v1/receipt/{}", BASE_URL.as_str(), cid),
    });
    (StatusCode::OK, Json(resp)).into_response()
}

pub async fn get_cid_dispatch(Path(cid_str): Path<String>) -> impl IntoResponse {
    if let Some(bare) = cid_str.strip_suffix(".json") {
        return get_cid_json_inner(bare).await;
    }
    get_cid_inner(&cid_str).await
}

async fn get_cid_inner(cid_str: &str) -> axum::response::Response {
    let cid = match Cid::try_from(cid_str) { Ok(c)=>c, Err(_)=> return (StatusCode::BAD_REQUEST, "invalid CID").into_response() };
    match ubl_ledger::get_raw(&cid).await {
        Some(bytes) => {
            ([
                (header::CONTENT_TYPE, "application/x-nrf"),
            ], bytes).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response()
    }
}

async fn get_cid_json_inner(cid_str: &str) -> axum::response::Response {
    let cid = match Cid::try_from(cid_str) { Ok(c)=>c, Err(_)=> return (StatusCode::BAD_REQUEST, "invalid CID").into_response() };
    let bytes = match ubl_ledger::get_raw(&cid).await { Some(b)=>b, None=> return (StatusCode::NOT_FOUND, "not found").into_response() };
    if let Ok(nrf_val) = nrf::decode_from_slice(&bytes) {
        return (StatusCode::OK, Json(nrf_value_to_json(&nrf_val))).into_response();
    }
    // Fallback: base64 view when NRF decoder can't parse the bytes
    let view = json!({
        "cid": cid.to_string(),
        "content_type": "application/x-nrf",
        "nrf_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        "note": "NRF decode failed; returning base64 view."
    });
    Json(view).into_response()
}

fn nrf_value_to_json(v: &NrfValue) -> Value {
    match v {
        NrfValue::Null => Value::Null,
        NrfValue::Bool(b) => Value::Bool(*b),
        NrfValue::Int(i) => json!(*i),
        NrfValue::String(s) => Value::String(s.clone()),
        NrfValue::Bytes(b) => Value::String(format!("0x{}", hex::encode(b))),
        NrfValue::Array(arr) => Value::Array(arr.iter().map(nrf_value_to_json).collect()),
        NrfValue::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k,v) in map { obj.insert(k.clone(), nrf_value_to_json(v)); }
            Value::Object(obj)
        }
    }
}

pub async fn certify_cid(Json(payload): Json<Value>) -> impl IntoResponse {
    let cid_str = match payload.get("cid").and_then(|v| v.as_str()) { Some(s)=>s, None=> return (StatusCode::BAD_REQUEST, "missing cid").into_response() };
    let cid = match Cid::try_from(cid_str) { Ok(c)=>c, Err(_)=> return (StatusCode::BAD_REQUEST, "invalid CID").into_response() };
    let bytes = match ubl_ledger::get_raw(&cid).await { Some(b)=>b, None=> return (StatusCode::NOT_FOUND, "content not found").into_response() };
    match ubl_receipt::issue_receipt(&cid, bytes.len()).await {
        Ok(jws) => Json(json!({ "receipt": jws })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("certify failed: {}", e)).into_response(),
    }
}

pub async fn get_receipt(Path(cid_str): Path<String>) -> impl IntoResponse {
    let cid = match Cid::try_from(cid_str.as_str()) { Ok(c)=>c, Err(_)=> return (StatusCode::BAD_REQUEST, "invalid CID").into_response() };
    match ubl_receipt::get_receipt(&cid).await {
        Some(jws) => (StatusCode::OK, [(header::CONTENT_TYPE, "application/jose+json")], jws).into_response(),
        None => (StatusCode::NOT_FOUND, "receipt not found").into_response(),
    }
}

pub async fn resolve(Json(payload): Json<Value>) -> impl IntoResponse {
    let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
    Json(ubl_did::resolve_did_or_cid(id, &ubl_config::BASE_URL))
}

pub async fn well_known_did_json() -> impl IntoResponse {
    Json(ubl_did::runtime_did_document())
}

#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub manifest: ubl_runtime::Manifest,
    pub vars: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub cid: String,
    pub artifacts: Value,
    pub dimension_stack: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExecRbRequest {
    pub chip_b64: String,
    pub inputs: Vec<Value>,
    pub ghost: Option<bool>,
    pub fuel: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ExecRbResponse {
    pub rc_cid: Option<String>,
    pub steps: u64,
    pub fuel_used: u64,
    pub transition_receipt: Option<Value>,
}

pub async fn execute_rb(State(state): State<AppState>, Json(req): Json<ExecRbRequest>) -> impl IntoResponse {
    let chip = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.chip_b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid base64 chip"}))).into_response(),
    };
    let rb_req = ubl_runtime::ExecuteRbReq {
        chip,
        inputs: req.inputs,
        ghost: req.ghost,
        fuel: req.fuel,
    };
    match ubl_runtime::execute_rb(&rb_req) {
        Ok(res) => {
            // Store transition receipt for GET /v1/transition/:cid
            if let Some(ref tr) = res.transition_receipt {
                if let Some(cid) = tr.get("cid").and_then(|c| c.as_str()) {
                    let mut store = state.transition_receipts.write().unwrap();
                    store.insert(cid.to_string(), tr.clone());
                    // Also index by rho_cid for lookup convenience
                    if let Some(rho) = tr.get("body").and_then(|b| b.get("rho_cid")).and_then(|r| r.as_str()) {
                        store.insert(rho.to_string(), tr.clone());
                    }
                }
            }
            let resp = ExecRbResponse {
                rc_cid: res.rc_cid,
                steps: res.steps,
                fuel_used: res.fuel_used,
                transition_receipt: res.transition_receipt,
            };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
            "error": "execute_rb_failed",
            "detail": e.to_string()
        }))).into_response(),
    }
}

pub async fn get_transition(State(state): State<AppState>, Path(cid): Path<String>) -> impl IntoResponse {
    let store = state.transition_receipts.read().unwrap();
    match store.get(&cid) {
        Some(envelope) => (StatusCode::OK, Json(envelope.clone())).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({
            "error": "transition_not_found",
            "cid": cid
        }))).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ExecRequestFull {
    pub manifest: ubl_runtime::Manifest,
    pub vars: BTreeMap<String, Value>,
    pub ghost: Option<bool>,
}

pub async fn execute_runtime(
    State(state): State<AppState>,
    client: Option<Extension<ClientInfo>>,
    Json(req): Json<ExecRequestFull>,
) -> impl IntoResponse {
    let cfg = ubl_runtime::ExecuteConfig { version: "0.1.0".into() };

    // Kid-scope check: if client has allowed_kids, verify active signing kid
    if let Some(Extension(ref ci)) = client {
        let active_kid = &state.keys.active_kid;
        if !ci.kid_allowed(active_kid) {
            return (StatusCode::FORBIDDEN, Json(json!({
                "error": "kid_scope_denied",
                "detail": format!("client '{}' not authorized for kid '{}'", ci.client_id, active_kid)
            }))).into_response();
        }
    }

    // Read prev_tip and seen_cids for chaining + idempotency
    let prev_tip = state.last_tip.read().unwrap().clone();
    let seen_snapshot = state.seen_cids.read().unwrap().clone();
    let ghost = req.ghost.unwrap_or(false);

    let opts = ubl_runtime::RunOpts {
        prev_tip: prev_tip.as_deref(),
        ghost,
        keys: &state.keys,
        seen: Some(&seen_snapshot),
        logline: None,
    };

    match ubl_runtime::run_with_receipts(&req.manifest, &req.vars, &cfg, &opts) {
        Ok(run) => {
            // Store receipts + update seen_cids + update last_tip (unless ghost)
            if !run.ghost {
                let mut store = state.receipt_chain.write().unwrap();
                store.insert(run.wa.body_cid.clone(), serde_json::to_value(&run.wa).unwrap());
                if let Some(ref tr) = run.transition {
                    store.insert(tr.body_cid.clone(), serde_json::to_value(tr).unwrap());
                }
                store.insert(run.wf.body_cid.clone(), serde_json::to_value(&run.wf).unwrap());
            }

            // Track idempotency key: pipeline:inputs_raw_cid
            {
                let inputs_cid = run.wa.body.get("inputs_raw_cid")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let pipeline = run.wa.body.get("intention")
                    .and_then(|v| v.get("pipeline"))
                    .and_then(|v| v.as_str()).unwrap_or("");
                let key = format!("{}:{}", pipeline, inputs_cid);
                let mut seen = state.seen_cids.write().unwrap();
                seen.insert(key);
            }

            // Update tip
            *state.last_tip.write().unwrap() = Some(run.tip_cid.clone());

            // Get artifacts from the WF body (already computed inside run_with_receipts)
            let decision = run.wf.body.get("decision").cloned().unwrap_or(json!(null));
            let dimension_stack = run.wf.body.get("dimension_stack").cloned().unwrap_or(json!([]));

            let resp = json!({
                "cid": run.tip_cid,
                "tip_cid": run.tip_cid,
                "decision": decision,
                "dimension_stack": dimension_stack,
                "ghost": run.ghost,
                "receipts": {
                    "wa": run.wa,
                    "transition": run.transition,
                    "wf": run.wf,
                },
                "url": format!("{}/v1/receipt/{}", BASE_URL.as_str(), run.tip_cid),
            });
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => {
            let detail = e.to_string();
            let status = if detail.contains("duplicate request") {
                StatusCode::CONFLICT
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            (status, Json(json!({
                "error": "execute_failed",
                "detail": detail
            }))).into_response()
        }
    }
}
