use axum::{routing::post, Router, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, Level};
use ubl_tdln::{execute, ExecuteConfig, Manifest};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(Level::INFO.into()))
        .with_target(false)
        .compact()
        .init();

    let app = Router::new().route("/v1/execute", post(exec_handler));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3300").await?;
    info!("ubl-tdln-svc listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ExecRequest {
    manifest: Manifest,
    vars: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
struct ExecResponse {
    cid: String,
    artifacts: Value,
    dimension_stack: Vec<String>,
}

async fn exec_handler(Json(req): Json<ExecRequest>) -> Json<ExecResponse> {
    let res = execute(&req.manifest, &req.vars, &ExecuteConfig{ version: "0.1.0".into() })
        .expect("execute failed");
    Json(ExecResponse{
        cid: res.cid,
        artifacts: serde_json::to_value(res.artifacts).unwrap(),
        dimension_stack: res.dimension_stack,
    })
}
