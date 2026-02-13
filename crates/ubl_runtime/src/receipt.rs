//! Unified Receipt type for the WA → Transition → WF pipeline.
//!
//! Every receipt has: type tag, parent chain, canonical body, body CID,
//! JWS detached signature, and optional observability metadata.

use serde::{Serialize, Deserialize};
use crate::cid::cid_b3;
use crate::canon::canonical_bytes;
use crate::jws::{sign_detached, JwsDetached};

/// Unified receipt envelope used across all pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Type tag: "ubl/wa", "ubl/transition", "ubl/wf"
    pub t: String,
    /// CIDs of parent receipts (chaining)
    pub parents: Vec<String>,
    /// Typed body (stage-specific content)
    pub body: serde_json::Value,
    /// CID of the canonical body bytes
    pub body_cid: String,
    /// JWS detached proof
    pub proof: JwsDetached,
    /// Optional observability (does NOT affect body_cid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<serde_json::Value>,
}

/// Result of the full receipt-first pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub wa: Receipt,
    pub transition: Option<Receipt>,
    pub wf: Receipt,
    /// CID of the WF receipt body (the "tip" of the chain)
    pub tip_cid: String,
}

/// Build a signed receipt from a type tag, parents, and body value.
pub fn build_receipt(
    t: &str,
    parents: Vec<String>,
    body: serde_json::Value,
    sign_key: &ed25519_dalek::SigningKey,
    kid: &str,
) -> crate::error::Result<Receipt> {
    let body_bytes = canonical_bytes(&body)?;
    let body_cid = cid_b3(&body_bytes);
    let proof = sign_detached(&body_bytes, sign_key, kid);
    Ok(Receipt {
        t: t.into(),
        parents,
        body,
        body_cid,
        proof,
        observability: None,
    })
}

/// Verify a receipt's body_cid matches the canonical body bytes.
pub fn verify_body_cid(receipt: &Receipt) -> crate::error::Result<bool> {
    let body_bytes = canonical_bytes(&receipt.body)?;
    let expected = cid_b3(&body_bytes);
    Ok(expected == receipt.body_cid)
}

/// Run the full receipt-first pipeline: WA → Transition(-1→0) → execute → WF.
///
/// Every execution produces exactly 3 receipts (WA, Transition, WF) chained by parent CIDs.
/// The "tip" is the WF receipt's body_cid.
pub fn run_with_receipts(
    manifest: &crate::engine::Manifest,
    vars: &std::collections::BTreeMap<String, serde_json::Value>,
    cfg: &crate::engine::ExecuteConfig,
    prev_tip: Option<&str>,
) -> crate::error::Result<RunResult> {
    let sign_key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let kid = "did:dev#k1";

    // (1) WA — write-ahead (ghost/intention)
    let raw_bytes = serde_json::to_vec(vars)?;
    let inputs_raw_cid = cid_b3(&raw_bytes);
    let wa_body = serde_json::json!({
        "type": "ubl/wa",
        "prev_tip": prev_tip,
        "inputs_raw_cid": inputs_raw_cid,
        "intention": {
            "op": "execute",
            "pipeline": &manifest.pipeline
        }
    });
    let wa = build_receipt("ubl/wa", vec![], wa_body, &sign_key, kid)?;

    // (2) Transition -1→0 (rho.normalize)
    let rho_val = serde_json::to_value(vars)?;
    let rho_bytes = canonical_bytes(&rho_val)?;
    let rho_cid = cid_b3(&rho_bytes);
    let tr_body = serde_json::json!({
        "t": "ubl/transition",
        "from_layer": "-1:rb",
        "to_layer": "0:rho",
        "op": "rho.normalize@ai-nrf1/v1",
        "preimage_raw_cid": inputs_raw_cid,
        "rho_cid": rho_cid,
        "witness": { "vm": "ubl-runtime@0.1.0" }
    });
    let transition = build_receipt(
        "ubl/transition",
        vec![wa.body_cid.clone()],
        tr_body,
        &sign_key,
        kid,
    )?;

    // (3) Execute deterministic pipeline (parse → policy → render)
    let exec_result = crate::engine::execute(manifest, vars, cfg)?;

    // (4) WF — write-final (result)
    let wf_body = serde_json::json!({
        "type": "ubl/wf",
        "rho_cid": rho_cid,
        "outputs_cid": exec_result.cid,
        "decision": if exec_result.dimension_stack.contains(&"policy".to_string()) { "ALLOW" } else { "DENY" },
        "dimension_stack": exec_result.dimension_stack,
    });
    let wf = build_receipt(
        "ubl/wf",
        vec![wa.body_cid.clone(), transition.body_cid.clone()],
        wf_body,
        &sign_key,
        kid,
    )?;

    let tip_cid = wf.body_cid.clone();

    Ok(RunResult {
        wa,
        transition: Some(transition),
        wf,
        tip_cid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn build_and_verify() {
        let rc = build_receipt(
            "ubl/wa",
            vec![],
            json!({"inputs_raw_cid": "b3:abc", "intention": "execute"}),
            &test_key(),
            "did:dev#k1",
        ).unwrap();
        assert_eq!(rc.t, "ubl/wa");
        assert!(rc.body_cid.starts_with("b3:"));
        assert!(verify_body_cid(&rc).unwrap());
    }

    #[test]
    fn parents_chain() {
        let key = test_key();
        let rc1 = build_receipt("ubl/wa", vec![], json!({"a":1}), &key, "did:dev#k1").unwrap();
        let rc2 = build_receipt("ubl/wf", vec![rc1.body_cid.clone()], json!({"b":2}), &key, "did:dev#k1").unwrap();
        assert_eq!(rc2.parents, vec![rc1.body_cid]);
    }

    #[test]
    fn deterministic_receipt() {
        let key = test_key();
        let body = json!({"x": 42});
        let rc1 = build_receipt("ubl/wa", vec![], body.clone(), &key, "did:dev#k1").unwrap();
        let rc2 = build_receipt("ubl/wa", vec![], body, &key, "did:dev#k1").unwrap();
        assert_eq!(rc1.body_cid, rc2.body_cid);
        assert_eq!(rc1.proof.signature, rc2.proof.signature);
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let key = test_key();
        let mut rc = build_receipt("ubl/wa", vec![], json!({"a":1}), &key, "did:dev#k1").unwrap();
        rc.body = json!({"a":2}); // tamper
        assert!(!verify_body_cid(&rc).unwrap());
    }

    #[test]
    fn run_with_receipts_produces_three_chained() {
        use std::collections::BTreeMap;
        use crate::engine::{Manifest, Grammar, Mapping, Policy, ExecuteConfig};

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping { from: "raw_b64".into(), codec: "base64.decode".into(), to: "raw.bytes".into() }],
            output_from: "raw.bytes".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("content".into(), json!(""))]),
            mappings: vec![],
            output_from: "content".into(),
        };
        let manifest = Manifest {
            pipeline: "test".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let cfg = ExecuteConfig { version: "0.1.0".into() };

        let result = run_with_receipts(&manifest, &vars, &cfg, None).unwrap();

        // All three receipts exist
        assert_eq!(result.wa.t, "ubl/wa");
        assert!(result.transition.is_some());
        assert_eq!(result.transition.as_ref().unwrap().t, "ubl/transition");
        assert_eq!(result.wf.t, "ubl/wf");

        // Chain: WA has no parents, transition parents=[wa], wf parents=[wa, transition]
        assert!(result.wa.parents.is_empty());
        assert_eq!(result.transition.as_ref().unwrap().parents, vec![result.wa.body_cid.clone()]);
        assert_eq!(result.wf.parents, vec![
            result.wa.body_cid.clone(),
            result.transition.as_ref().unwrap().body_cid.clone(),
        ]);

        // Tip is the WF body_cid
        assert_eq!(result.tip_cid, result.wf.body_cid);

        // All body_cids verify
        assert!(verify_body_cid(&result.wa).unwrap());
        assert!(verify_body_cid(result.transition.as_ref().unwrap()).unwrap());
        assert!(verify_body_cid(&result.wf).unwrap());
    }

    #[test]
    fn run_with_receipts_deterministic() {
        use std::collections::BTreeMap;
        use crate::engine::{Manifest, Grammar, Mapping, Policy, ExecuteConfig};

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping { from: "raw_b64".into(), codec: "base64.decode".into(), to: "raw.bytes".into() }],
            output_from: "raw.bytes".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("content".into(), json!(""))]),
            mappings: vec![],
            output_from: "content".into(),
        };
        let manifest = Manifest {
            pipeline: "test".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let cfg = ExecuteConfig { version: "0.1.0".into() };

        let r1 = run_with_receipts(&manifest, &vars, &cfg, None).unwrap();
        let r2 = run_with_receipts(&manifest, &vars, &cfg, None).unwrap();
        assert_eq!(r1.tip_cid, r2.tip_cid);
        assert_eq!(r1.wa.body_cid, r2.wa.body_cid);
        assert_eq!(
            r1.transition.as_ref().unwrap().body_cid,
            r2.transition.as_ref().unwrap().body_cid,
        );
    }

    #[test]
    fn wf_body_contains_decision_allow() {
        use std::collections::BTreeMap;
        use crate::engine::{Manifest, Grammar, Mapping, Policy, ExecuteConfig};

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping { from: "raw_b64".into(), codec: "base64.decode".into(), to: "raw.bytes".into() }],
            output_from: "raw.bytes".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("content".into(), json!(""))]),
            mappings: vec![],
            output_from: "content".into(),
        };
        let manifest = Manifest {
            pipeline: "test".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let cfg = ExecuteConfig { version: "0.1.0".into() };

        let result = run_with_receipts(&manifest, &vars, &cfg, None).unwrap();
        assert_eq!(result.wf.body["decision"], "ALLOW");
        assert!(result.wf.body["outputs_cid"].as_str().unwrap().starts_with("b3:"));
    }
}
