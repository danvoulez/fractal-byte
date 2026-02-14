//! Unified Receipt type for the WA → Transition → WF pipeline.
//!
//! Every receipt has: type tag, parent chain, canonical body, body CID,
//! JWS detached signature, and optional observability metadata.
//!
//! Invariants enforced:
//! - Schema: t must be a known tag, parents/body/body_cid/proof required
//! - Chaining: parents[0] == prev_tip when prev_tip is provided
//! - Idempotency: duplicate body_cid is rejected
//! - Ghost: ghost=true ⇒ observability.ghost=true, ledger skip signaled

use crate::canon::canonical_bytes;
use crate::cid::cid_b3;
use crate::jws::{sign_detached, JwsDetached};
use serde::{Deserialize, Serialize};

const VALID_TYPES: &[&str] = &["ubl/wa", "ubl/transition", "ubl/wf", "ubl/attestation"];

/// LLM-first observability logline.
/// Attached to `observability.logline` on every receipt.
/// Does NOT affect body_cid — purely for "look and narrate".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logline {
    pub who: String,
    pub actor_did: String,
    pub what: String,
    pub r#where: String,
    pub when_iso: String,
    pub why: String,
    pub context_id: String,
    pub version: String,
}

impl Logline {
    pub fn now(
        who: &str,
        actor_did: &str,
        what: &str,
        where_: &str,
        why: &str,
        context_id: &str,
    ) -> Self {
        Self {
            who: who.into(),
            actor_did: actor_did.into(),
            what: what.into(),
            r#where: where_.into(),
            when_iso: chrono_now_iso(),
            why: why.into(),
            context_id: context_id.into(),
            version: "0.1.0".into(),
        }
    }
}

fn chrono_now_iso() -> String {
    // Simple ISO 8601 timestamp without chrono dependency
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    // Rough UTC: good enough for observability, not for crypto
    let (s, m, h) = (secs % 60, (secs / 60) % 60, (secs / 3600) % 24);
    let days = secs / 86400;
    // Approximate date from epoch days (not leap-second accurate, fine for logs)
    let y = 1970 + days / 365;
    let doy = days % 365;
    let mo = doy / 30 + 1;
    let day = doy % 30 + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        mo.min(12),
        day.min(31),
        h,
        m,
        s
    )
}

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
    /// Whether this run was in ghost mode (ledger should NOT persist)
    pub ghost: bool,
}

/// Signing context: active key + optional next key for rotation.
#[derive(Clone)]
pub struct KeyRing {
    pub active: ed25519_dalek::SigningKey,
    pub active_kid: String,
    pub next: Option<ed25519_dalek::SigningKey>,
    pub next_kid: Option<String>,
}

impl KeyRing {
    pub fn dev() -> Self {
        Self {
            active: ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]),
            active_kid: "did:dev#k1".into(),
            next: None,
            next_kid: None,
        }
    }
}

/// Pipeline options for run_with_receipts.
pub struct RunOpts<'a> {
    pub prev_tip: Option<&'a str>,
    pub ghost: bool,
    pub keys: &'a KeyRing,
    /// Already-seen keys for idempotency (caller provides)
    pub seen: Option<&'a std::collections::HashSet<String>>,
    /// Optional logline context for observability
    pub logline: Option<LoglineContext<'a>>,
}

/// Minimal context for generating loglines per receipt.
pub struct LoglineContext<'a> {
    pub who: &'a str,
    pub actor_did: &'a str,
    pub where_: &'a str,
    pub why: &'a str,
    pub context_id: &'a str,
}

impl<'a> Default for RunOpts<'a> {
    fn default() -> Self {
        Self {
            prev_tip: None,
            ghost: false,
            keys: &DEVKEYS,
            seen: None,
            logline: None,
        }
    }
}

static DEVKEYS: once_cell::sync::Lazy<KeyRing> = once_cell::sync::Lazy::new(KeyRing::dev);

/// Validate a receipt against the canonical schema.
pub fn validate_receipt(rc: &Receipt) -> crate::error::Result<()> {
    if !VALID_TYPES.contains(&rc.t.as_str()) {
        return Err(crate::error::RuntimeError::Validation(format!(
            "invalid receipt type '{}', expected one of {:?}",
            rc.t, VALID_TYPES
        )));
    }
    if rc.body_cid.is_empty() || !rc.body_cid.starts_with("b3:") {
        return Err(crate::error::RuntimeError::Validation(
            "body_cid must be non-empty and start with 'b3:'".into(),
        ));
    }
    if rc.proof.signature.is_empty() {
        return Err(crate::error::RuntimeError::Validation(
            "proof.signature must not be empty".into(),
        ));
    }
    if rc.proof.kid.is_empty() {
        return Err(crate::error::RuntimeError::Validation(
            "proof.kid must not be empty".into(),
        ));
    }
    // body_cid must match canonical body
    let body_bytes = canonical_bytes(&rc.body)?;
    let expected_cid = cid_b3(&body_bytes);
    if expected_cid != rc.body_cid {
        return Err(crate::error::RuntimeError::Validation(format!(
            "body_cid mismatch: expected {}, got {}",
            expected_cid, rc.body_cid
        )));
    }
    Ok(())
}

/// Build a signed receipt from a type tag, parents, and body value.
/// Validates the receipt against the schema before returning.
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
    let rc = Receipt {
        t: t.into(),
        parents,
        body,
        body_cid,
        proof,
        observability: None,
    };
    validate_receipt(&rc)?;
    Ok(rc)
}

/// Verify a receipt's body_cid matches the canonical body bytes.
pub fn verify_body_cid(receipt: &Receipt) -> crate::error::Result<bool> {
    let body_bytes = canonical_bytes(&receipt.body)?;
    let expected = cid_b3(&body_bytes);
    Ok(expected == receipt.body_cid)
}

/// Build the observability JSON for a receipt, merging ghost flag and logline.
fn make_observability(
    ghost: bool,
    logline_ctx: &Option<LoglineContext>,
    what_suffix: &str,
) -> Option<serde_json::Value> {
    let has_ghost = ghost;
    let has_logline = logline_ctx.is_some();
    if !has_ghost && !has_logline {
        return None;
    }
    let mut obs = serde_json::Map::new();
    if ghost {
        obs.insert("ghost".into(), serde_json::Value::Bool(true));
    }
    if let Some(ctx) = logline_ctx {
        let ll = Logline::now(
            ctx.who,
            ctx.actor_did,
            what_suffix,
            ctx.where_,
            ctx.why,
            ctx.context_id,
        );
        obs.insert("logline".into(), serde_json::to_value(&ll).unwrap());
    }
    Some(serde_json::Value::Object(obs))
}

/// Run the full receipt-first pipeline: WA → Transition(-1→0) → execute → WF.
///
/// Every execution produces exactly 3 receipts (WA, Transition, WF) chained by parent CIDs.
/// The "tip" is the WF receipt's body_cid.
///
/// Invariants:
/// - Schema validated on every receipt before returning
/// - parents[0] == prev_tip when provided
/// - Duplicate body_cid rejected (idempotency)
/// - ghost=true ⇒ observability.ghost=true on all receipts
pub fn run_with_receipts(
    manifest: &crate::engine::Manifest,
    vars: &std::collections::BTreeMap<String, serde_json::Value>,
    cfg: &crate::engine::ExecuteConfig,
    opts: &RunOpts,
) -> crate::error::Result<RunResult> {
    let sign_key = &opts.keys.active;
    let kid = opts.keys.active_kid.as_str();
    let ghost = opts.ghost;

    // (1) WA — write-ahead (ghost/intention)
    let wa_parents = match opts.prev_tip {
        Some(tip) => vec![tip.to_string()],
        None => vec![],
    };
    let raw_bytes = serde_json::to_vec(vars)?;
    let inputs_raw_cid = cid_b3(&raw_bytes);
    let wa_body = serde_json::json!({
        "type": "ubl/wa",
        "prev_tip": opts.prev_tip,
        "inputs_raw_cid": inputs_raw_cid,
        "intention": {
            "op": "execute",
            "pipeline": &manifest.pipeline
        }
    });
    // Idempotency check: same inputs + pipeline = replay
    let idempotency_key = format!("{}:{}", manifest.pipeline, inputs_raw_cid);
    if let Some(seen) = opts.seen {
        if seen.contains(&idempotency_key) {
            return Err(crate::error::RuntimeError::Validation(format!(
                "duplicate request (replay): pipeline={} inputs_cid={}",
                manifest.pipeline, inputs_raw_cid
            )));
        }
    }

    let mut wa = build_receipt("ubl/wa", wa_parents, wa_body, sign_key, kid)?;
    wa.observability = make_observability(ghost, &opts.logline, "wa:write-ahead");

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
    let mut transition = build_receipt(
        "ubl/transition",
        vec![wa.body_cid.clone()],
        tr_body,
        sign_key,
        kid,
    )?;
    transition.observability = make_observability(ghost, &opts.logline, "transition:normalize");

    // (3) Execute deterministic pipeline (parse → policy → render)
    // On failure → produce DENY WF receipt, never 500
    let exec_result = match crate::engine::execute(manifest, vars, cfg) {
        Ok(r) => r,
        Err(e) => {
            // DENY WF with error reason
            let wf_body = serde_json::json!({
                "type": "ubl/wf",
                "rho_cid": rho_cid,
                "outputs_cid": null,
                "decision": "DENY",
                "reason": e.to_string(),
                "dimension_stack": [],
            });
            let mut wf = build_receipt(
                "ubl/wf",
                vec![wa.body_cid.clone(), transition.body_cid.clone()],
                wf_body,
                sign_key,
                kid,
            )?;
            wf.observability = make_observability(ghost, &opts.logline, "wf:deny");
            let tip_cid = wf.body_cid.clone();
            return Ok(RunResult {
                wa,
                transition: Some(transition),
                wf,
                tip_cid,
                ghost,
            });
        }
    };

    // (4) WF — write-final (result)
    let wf_body = serde_json::json!({
        "type": "ubl/wf",
        "rho_cid": rho_cid,
        "outputs_cid": exec_result.cid,
        "decision": if exec_result.dimension_stack.contains(&"policy".to_string()) { "ALLOW" } else { "DENY" },
        "dimension_stack": exec_result.dimension_stack,
        "policy_trace": exec_result.policy_trace,
    });
    let mut wf = build_receipt(
        "ubl/wf",
        vec![wa.body_cid.clone(), transition.body_cid.clone()],
        wf_body,
        sign_key,
        kid,
    )?;
    wf.observability = make_observability(ghost, &opts.logline, "wf:write-final");

    let tip_cid = wf.body_cid.clone();

    Ok(RunResult {
        wa,
        transition: Some(transition),
        wf,
        tip_cid,
        ghost,
    })
}

/// Convenience wrapper using dev keys and no prev_tip (backward compat for simple calls).
pub fn run_with_receipts_simple(
    manifest: &crate::engine::Manifest,
    vars: &std::collections::BTreeMap<String, serde_json::Value>,
    cfg: &crate::engine::ExecuteConfig,
    prev_tip: Option<&str>,
) -> crate::error::Result<RunResult> {
    let keys = KeyRing::dev();
    let opts = RunOpts {
        prev_tip,
        ghost: false,
        keys: &keys,
        seen: None,
        logline: None,
    };
    run_with_receipts(manifest, vars, cfg, &opts)
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
        )
        .unwrap();
        assert_eq!(rc.t, "ubl/wa");
        assert!(rc.body_cid.starts_with("b3:"));
        assert!(verify_body_cid(&rc).unwrap());
    }

    #[test]
    fn parents_chain() {
        let key = test_key();
        let rc1 = build_receipt("ubl/wa", vec![], json!({"a":1}), &key, "did:dev#k1").unwrap();
        let rc2 = build_receipt(
            "ubl/wf",
            vec![rc1.body_cid.clone()],
            json!({"b":2}),
            &key,
            "did:dev#k1",
        )
        .unwrap();
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
        use crate::engine::{ExecuteConfig, Grammar, Manifest, Mapping, Policy};
        use std::collections::BTreeMap;

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "raw.bytes".into(),
            }],
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
        let cfg = ExecuteConfig {
            version: "0.1.0".into(),
        };

        let result = run_with_receipts_simple(&manifest, &vars, &cfg, None).unwrap();

        // All three receipts exist
        assert_eq!(result.wa.t, "ubl/wa");
        assert!(result.transition.is_some());
        assert_eq!(result.transition.as_ref().unwrap().t, "ubl/transition");
        assert_eq!(result.wf.t, "ubl/wf");

        // Chain: WA has no parents, transition parents=[wa], wf parents=[wa, transition]
        assert!(result.wa.parents.is_empty());
        assert_eq!(
            result.transition.as_ref().unwrap().parents,
            vec![result.wa.body_cid.clone()]
        );
        assert_eq!(
            result.wf.parents,
            vec![
                result.wa.body_cid.clone(),
                result.transition.as_ref().unwrap().body_cid.clone(),
            ]
        );

        // Tip is the WF body_cid
        assert_eq!(result.tip_cid, result.wf.body_cid);

        // All body_cids verify
        assert!(verify_body_cid(&result.wa).unwrap());
        assert!(verify_body_cid(result.transition.as_ref().unwrap()).unwrap());
        assert!(verify_body_cid(&result.wf).unwrap());
    }

    #[test]
    fn run_with_receipts_deterministic() {
        use crate::engine::{ExecuteConfig, Grammar, Manifest, Mapping, Policy};
        use std::collections::BTreeMap;

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "raw.bytes".into(),
            }],
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
        let cfg = ExecuteConfig {
            version: "0.1.0".into(),
        };

        let r1 = run_with_receipts_simple(&manifest, &vars, &cfg, None).unwrap();
        let r2 = run_with_receipts_simple(&manifest, &vars, &cfg, None).unwrap();
        assert_eq!(r1.tip_cid, r2.tip_cid);
        assert_eq!(r1.wa.body_cid, r2.wa.body_cid);
        assert_eq!(
            r1.transition.as_ref().unwrap().body_cid,
            r2.transition.as_ref().unwrap().body_cid,
        );
    }

    #[test]
    fn wf_body_contains_decision_allow() {
        use crate::engine::{ExecuteConfig, Grammar, Manifest, Mapping, Policy};
        use std::collections::BTreeMap;

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "raw.bytes".into(),
            }],
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
        let cfg = ExecuteConfig {
            version: "0.1.0".into(),
        };

        let result = run_with_receipts_simple(&manifest, &vars, &cfg, None).unwrap();
        assert_eq!(result.wf.body["decision"], "ALLOW");
        assert!(result.wf.body["outputs_cid"]
            .as_str()
            .unwrap()
            .starts_with("b3:"));
    }

    // ── Schema validation tests ──────────────────────────────────

    #[test]
    fn validate_rejects_bad_type() {
        let key = test_key();
        let err = build_receipt("ubl/bogus", vec![], json!({"a":1}), &key, "did:dev#k1");
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("invalid receipt type"));
    }

    #[test]
    fn validate_rejects_empty_kid() {
        let key = test_key();
        let err = build_receipt("ubl/wa", vec![], json!({"a":1}), &key, "");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("kid"));
    }

    // ── Ghost mode tests ─────────────────────────────────────────

    #[test]
    fn ghost_mode_sets_observability() {
        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let keys = KeyRing::dev();
        let opts = RunOpts {
            prev_tip: None,
            ghost: true,
            keys: &keys,
            seen: None,
            logline: None,
        };
        let result = run_with_receipts(&manifest, &vars, &cfg, &opts).unwrap();

        assert!(result.ghost);
        assert_eq!(result.wa.observability.as_ref().unwrap()["ghost"], true);
        assert_eq!(
            result
                .transition
                .as_ref()
                .unwrap()
                .observability
                .as_ref()
                .unwrap()["ghost"],
            true
        );
        assert_eq!(result.wf.observability.as_ref().unwrap()["ghost"], true);
    }

    // ── Idempotency tests ────────────────────────────────────────

    #[test]
    fn idempotency_rejects_replay() {
        use std::collections::HashSet;

        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let keys = KeyRing::dev();

        // Compute the idempotency key: pipeline:inputs_raw_cid
        let raw_bytes = serde_json::to_vec(&vars).unwrap();
        let inputs_cid = crate::cid::cid_b3(&raw_bytes);
        let idemp_key = format!("{}:{}", manifest.pipeline, inputs_cid);

        let mut seen = HashSet::new();
        seen.insert(idemp_key);

        // Run with same input should be rejected as replay
        let opts = RunOpts {
            prev_tip: None,
            ghost: false,
            keys: &keys,
            seen: Some(&seen),
            logline: None,
        };
        let err = run_with_receipts(&manifest, &vars, &cfg, &opts);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("duplicate request (replay)"));
    }

    // ── Prev-tip chaining tests ──────────────────────────────────

    #[test]
    fn prev_tip_appears_in_wa_parents() {
        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let result =
            run_with_receipts_simple(&manifest, &vars, &cfg, Some("b3:prev_tip_abc")).unwrap();
        assert_eq!(result.wa.parents[0], "b3:prev_tip_abc");
    }

    // ── DENY on engine failure ────────────────────────────────────

    #[test]
    fn engine_failure_produces_deny_wf() {
        use crate::engine::{ExecuteConfig, Grammar, Manifest, Mapping, Policy};
        use std::collections::BTreeMap;

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "raw.bytes".into(),
            }],
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
            policy: Policy { allow: false }, // will cause policy deny
        };
        let vars = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let cfg = ExecuteConfig {
            version: "0.1.0".into(),
        };

        // Should NOT return Err — should produce a DENY WF receipt
        let result = run_with_receipts_simple(&manifest, &vars, &cfg, None).unwrap();
        assert_eq!(result.wf.body["decision"], "DENY");
        assert!(result.wf.body["reason"]
            .as_str()
            .unwrap()
            .contains("policy deny"));
        assert!(result.wf.body["outputs_cid"].is_null());
    }

    // ── Key rotation test ────────────────────────────────────────

    #[test]
    fn custom_keyring_signs_with_active_kid() {
        let custom_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let keys = KeyRing {
            active: custom_key,
            active_kid: "did:custom#k2".into(),
            next: Some(ed25519_dalek::SigningKey::from_bytes(&[99u8; 32])),
            next_kid: Some("did:custom#k3".into()),
        };
        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let opts = RunOpts {
            prev_tip: None,
            ghost: false,
            keys: &keys,
            seen: None,
            logline: None,
        };
        let result = run_with_receipts(&manifest, &vars, &cfg, &opts).unwrap();
        assert_eq!(result.wa.proof.kid, "did:custom#k2");
        assert_eq!(result.wf.proof.kid, "did:custom#k2");
    }

    // ── Logline test ──────────────────────────────────────────────

    #[test]
    fn logline_attached_to_all_receipts() {
        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let keys = KeyRing::dev();
        let ctx = LoglineContext {
            who: "test-runner",
            actor_did: "did:dev#k1",
            where_: "unit-test",
            why: "verify logline attachment",
            context_id: "ctx-001",
        };
        let opts = RunOpts {
            prev_tip: None,
            ghost: false,
            keys: &keys,
            seen: None,
            logline: Some(ctx),
        };
        let result = run_with_receipts(&manifest, &vars, &cfg, &opts).unwrap();

        // All three receipts should have observability.logline
        for (label, rc) in [("wa", &result.wa), ("wf", &result.wf)] {
            let obs = rc
                .observability
                .as_ref()
                .unwrap_or_else(|| panic!("{label} missing observability"));
            let ll = &obs["logline"];
            assert_eq!(ll["who"], "test-runner", "{label} logline.who");
            assert_eq!(ll["actor_did"], "did:dev#k1", "{label} logline.actor_did");
            assert_eq!(ll["context_id"], "ctx-001", "{label} logline.context_id");
            assert!(
                !ll["when_iso"].as_str().unwrap().is_empty(),
                "{label} logline.when_iso"
            );
            assert_eq!(ll["version"], "0.1.0", "{label} logline.version");
        }
        let tr = result.transition.as_ref().unwrap();
        let tr_ll = &tr.observability.as_ref().unwrap()["logline"];
        assert_eq!(tr_ll["who"], "test-runner");
        assert!(tr_ll["what"].as_str().unwrap().contains("transition"));

        // ghost should NOT be present when ghost=false
        assert!(result
            .wa
            .observability
            .as_ref()
            .unwrap()
            .get("ghost")
            .is_none());
    }

    #[test]
    fn logline_and_ghost_coexist() {
        let (manifest, vars, cfg) = test_manifest_vars_cfg();
        let keys = KeyRing::dev();
        let ctx = LoglineContext {
            who: "ghost-test",
            actor_did: "did:dev#k1",
            where_: "unit-test",
            why: "verify ghost+logline",
            context_id: "ctx-ghost",
        };
        let opts = RunOpts {
            prev_tip: None,
            ghost: true,
            keys: &keys,
            seen: None,
            logline: Some(ctx),
        };
        let result = run_with_receipts(&manifest, &vars, &cfg, &opts).unwrap();
        let obs = result.wa.observability.as_ref().unwrap();
        assert_eq!(obs["ghost"], true);
        assert_eq!(obs["logline"]["who"], "ghost-test");
    }

    // ── Helper ────────────────────────────────────────────────────

    fn test_manifest_vars_cfg() -> (
        crate::engine::Manifest,
        std::collections::BTreeMap<String, serde_json::Value>,
        crate::engine::ExecuteConfig,
    ) {
        use crate::engine::{ExecuteConfig, Grammar, Manifest, Mapping, Policy};
        use std::collections::BTreeMap;

        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "raw.bytes".into(),
            }],
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
        let cfg = ExecuteConfig {
            version: "0.1.0".into(),
        };
        (manifest, vars, cfg)
    }
}
