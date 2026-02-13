// ubl_gate handler (snippet) - Axum
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ExecReq {
    pub engine: String,           // "rb"
    pub chip_b64: String,
    pub inputs: Vec<String>,
    pub ghost: Option<bool>,
    pub fuel: Option<u64>,
}

#[derive(Serialize)]
pub struct ExecRes {
    pub rc_cid: String,
    pub fuel_used: u64,
    pub steps: u64,
}

pub async fn post_execute(State(_app): State<()>, Json(req): Json<ExecReq>) -> Json<ExecRes> {
    assert_eq!(req.engine.as_str(), "rb", "engine must be rb");
    // decode chip, call bridge, map response
    // TODO: integrate
    Json(ExecRes{ rc_cid: "cid:TBD".into(), fuel_used: 0, steps: 0 })
}
