//! Bridge: execute RB-VM chips through ubl_runtime
//!
//! Wires rb_vm::Vm with in-memory CAS and Ed25519 signer,
//! exposing a simple `execute_rb()` function.

use crate::nrf_canon::Nrf1Canon;
use rb_vm::exec::{CasProvider, SignProvider};
use rb_vm::tlv;
use rb_vm::{Cid, ExecError, Vm, VmConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── In-memory CAS (deterministic, no filesystem) ─────────────────

struct MemCas {
    store: HashMap<String, Vec<u8>>,
}

impl MemCas {
    fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl CasProvider for MemCas {
    fn put(&mut self, bytes: &[u8]) -> Cid {
        let hash = blake3::hash(bytes);
        let hex = hex::encode(hash.as_bytes());
        let cid = Cid(format!("b3:{hex}"));
        self.store.insert(cid.0.clone(), bytes.to_vec());
        cid
    }
    fn get(&self, cid: &Cid) -> Option<Vec<u8>> {
        self.store.get(&cid.0).cloned()
    }
}

// ── Deterministic signer (fixed seed) ────────────────────────────

struct FixedSigner {
    key: ed25519_dalek::SigningKey,
}

impl FixedSigner {
    fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            key: ed25519_dalek::SigningKey::from_bytes(&seed),
        }
    }
}

impl SignProvider for FixedSigner {
    fn sign_jws(&self, payload: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer;
        self.key.sign(payload).to_bytes().to_vec()
    }
    fn kid(&self) -> String {
        "did:dev#k1".into()
    }
}

// ── Public API ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecuteRbReq {
    pub chip: Vec<u8>,
    pub inputs: Vec<serde_json::Value>,
    pub ghost: Option<bool>,
    pub fuel: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteRbRes {
    pub rc_cid: Option<String>,
    pub steps: u64,
    pub fuel_used: u64,
    pub transition_receipt: Option<serde_json::Value>,
}

pub fn execute_rb(req: &ExecuteRbReq) -> Result<ExecuteRbRes, crate::error::RuntimeError> {
    let code = tlv::decode_stream(&req.chip)
        .map_err(|e| crate::error::RuntimeError::Engine(format!("TLV decode: {e}")))?;

    let mut cas = MemCas::new();
    let signer = FixedSigner::from_seed([7u8; 32]);
    let canon = Nrf1Canon;

    // (A) Capture raw bytes BEFORE normalization (layer -1)
    let raw_bytes = serde_json::to_vec(&req.inputs)
        .map_err(|e| crate::error::RuntimeError::Engine(format!("serialize inputs: {e}")))?;
    let ghost = req.ghost.unwrap_or(false);

    let input_cids: Vec<Cid> = req
        .inputs
        .iter()
        .map(|v| {
            let bytes = serde_json::to_vec(v).unwrap_or_default();
            cas.put(&bytes)
        })
        .collect();

    // CID of the chip bytecode itself (content-addressed)
    let bytecode_cid = crate::cid::cid_b3(&req.chip);

    let cfg = VmConfig {
        fuel_limit: req.fuel.unwrap_or(50_000),
        ghost,
        trace: false,
    };

    let mut vm = Vm::new(cfg, cas, &signer, canon, input_cids);
    let outcome = vm.run(&code).map_err(|e| match e {
        ExecError::Deny(reason) => crate::error::RuntimeError::PolicyDeny(reason),
        ExecError::FuelExhausted => crate::error::RuntimeError::Engine("fuel exhausted".into()),
        other => crate::error::RuntimeError::Engine(other.to_string()),
    })?;

    // (B) Capture rho bytes AFTER normalization (layer 0)
    // The canonical form of the inputs is the rho layer
    let rho_val = serde_json::to_value(&req.inputs)
        .map_err(|e| crate::error::RuntimeError::Engine(format!("rho serialize: {e}")))?;
    let rho_bytes = crate::canon::canonical_bytes(&rho_val)?;

    // (C) Build Transition Receipt (RB→rho)
    let tr = crate::transition::build_transition(
        &raw_bytes,
        &rho_bytes,
        "rb-vm@0.1.0",
        Some(bytecode_cid),
        Some(outcome.fuel_used),
        ghost,
    );

    let tr_cid = tr.cid()?;
    let tr_body_bytes = tr.canonical_bytes()?;

    // (D) JWS detached signature (b64=false, payload = canonical body bytes)
    let sign_key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let jws = crate::jws::sign_detached(&tr_body_bytes, &sign_key, "did:dev#k1");

    let tr_envelope = serde_json::json!({
        "cid": tr_cid,
        "body": serde_json::to_value(&tr).map_err(|e| crate::error::RuntimeError::Engine(e.to_string()))?,
        "proof": serde_json::to_value(&jws).map_err(|e| crate::error::RuntimeError::Engine(e.to_string()))?,
    });

    Ok(ExecuteRbRes {
        rc_cid: outcome.rc_cid.map(|c| c.0),
        steps: outcome.steps,
        fuel_used: outcome.fuel_used,
        transition_receipt: Some(tr_envelope),
    })
}
