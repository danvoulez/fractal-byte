//! Bridge: execute RB-VM chips through ubl_runtime
//!
//! Wires rb_vm::Vm with in-memory CAS and Ed25519 signer,
//! exposing a simple `execute_rb()` function.

use rb_vm::{Vm, VmConfig, ExecError, Cid};
use rb_vm::exec::{CasProvider, SignProvider};
use rb_vm::canon::NaiveCanon;
use rb_vm::tlv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── In-memory CAS (deterministic, no filesystem) ─────────────────

struct MemCas {
    store: HashMap<String, Vec<u8>>,
}

impl MemCas {
    fn new() -> Self { Self { store: HashMap::new() } }
}

impl CasProvider for MemCas {
    fn put(&mut self, bytes: &[u8]) -> Cid {
        let hash = blake3::hash(bytes);
        let hex = hex::encode(hash.as_bytes());
        let cid = Cid(format!("b3:{}", hex));
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
        Self { key: ed25519_dalek::SigningKey::from_bytes(&seed) }
    }
}

impl SignProvider for FixedSigner {
    fn sign_jws(&self, payload: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer;
        self.key.sign(payload).to_bytes().to_vec()
    }
    fn kid(&self) -> String { "did:dev#k1".into() }
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
}

pub fn execute_rb(req: &ExecuteRbReq) -> Result<ExecuteRbRes, crate::error::RuntimeError> {
    let code = tlv::decode_stream(&req.chip)
        .map_err(|e| crate::error::RuntimeError::Engine(format!("TLV decode: {}", e)))?;

    let mut cas = MemCas::new();
    let signer = FixedSigner::from_seed([7u8; 32]);
    let canon = NaiveCanon;

    let input_cids: Vec<Cid> = req.inputs.iter().map(|v| {
        let bytes = serde_json::to_vec(v).unwrap_or_default();
        cas.put(&bytes)
    }).collect();

    let cfg = VmConfig {
        fuel_limit: req.fuel.unwrap_or(50_000),
        ghost: req.ghost.unwrap_or(false),
    };

    let mut vm = Vm::new(cfg, cas, &signer, canon, input_cids);
    let outcome = vm.run(&code).map_err(|e| match e {
        ExecError::Deny(reason) => crate::error::RuntimeError::PolicyDeny(reason),
        ExecError::FuelExhausted => crate::error::RuntimeError::Engine("fuel exhausted".into()),
        other => crate::error::RuntimeError::Engine(other.to_string()),
    })?;

    Ok(ExecuteRbRes {
        rc_cid: outcome.rc_cid.map(|c| c.0),
        steps: outcome.steps,
        fuel_used: outcome.fuel_used,
    })
}
