use crate::error::{Result, RuntimeError};
use crate::{bind::bind_vars_to_inputs, cid::cid_b3};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grammar {
    /// Declared inputs (name -> placeholder/type metadata)
    pub inputs: BTreeMap<String, Value>,
    /// Mappings array: very minimal MVP format
    pub mappings: Vec<Mapping>,
    /// Output key name for render stage input (parse) or extraction (render)
    pub output_from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub from: String,
    pub codec: String, // e.g., "base64.decode"
    pub to: String,    // e.g., "raw.bytes"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub allow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub pipeline: String,
    pub in_grammar: Grammar,
    pub out_grammar: Grammar,
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteConfig {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub artifacts: Artifacts,
    pub dimension_stack: Vec<String>,
    pub cid: String,
    /// Policy trace from cascade evaluation (empty for legacy mode).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_trace: Vec<crate::policy::PolicyTraceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifacts {
    pub output: Value,
    pub sub_receipts: Vec<Value>,
}

fn apply_mappings(ctx: &mut BTreeMap<String, Value>, maps: &[Mapping]) -> Result<()> {
    for m in maps {
        let src = ctx.get(&m.from).ok_or_else(|| {
            RuntimeError::Validation(format!("mapping: key '{}' not found", m.from))
        })?;
        let val = match m.codec.as_str() {
            "base64.decode" => {
                use base64::Engine;
                let s = src
                    .as_str()
                    .ok_or_else(|| RuntimeError::Validation("expected string".into()))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(s)
                    .map_err(|_| RuntimeError::Validation("base64".into()))?;
                Value::String(String::from_utf8_lossy(&bytes).to_string())
            }
            _ => {
                return Err(RuntimeError::Validation(format!(
                    "unknown codec: {}",
                    m.codec
                )))
            }
        };
        ctx.insert(m.to.clone(), val);
    }
    Ok(())
}

pub fn execute(
    manifest: &Manifest,
    vars: &BTreeMap<String, Value>,
    _cfg: &ExecuteConfig,
) -> Result<ExecuteResult> {
    // parse
    let mut ctx: BTreeMap<String, Value> = BTreeMap::new();
    let bound = bind_vars_to_inputs(vars, &manifest.in_grammar.inputs)?;
    for (k, v) in bound {
        ctx.insert(k, v);
    }
    apply_mappings(&mut ctx, &manifest.in_grammar.mappings)?;
    let parse_out = ctx
        .get(&manifest.in_grammar.output_from)
        .ok_or_else(|| {
            RuntimeError::Validation(format!(
                "parse: missing '{}'",
                manifest.in_grammar.output_from
            ))
        })?
        .clone();

    // policy — evaluate via cascade resolver for backward compat
    let cascade = crate::policy::CascadePolicy {
        allow: manifest.policy.allow,
        rules: vec![],
    };
    let policy_result = crate::policy::resolve(&cascade, vars, None);
    if policy_result.decision == "DENY" {
        return Err(RuntimeError::PolicyDeny(
            policy_result.reason.unwrap_or_else(|| "policy deny".into()),
        ));
    }
    let policy_trace = policy_result.policy_trace;

    // render: feed only previous stage output via 1<->1 to grammar input
    let mut render_vars = BTreeMap::new();
    render_vars.insert("__prev_output__".into(), parse_out.clone());
    let bound = bind_vars_to_inputs(&render_vars, &manifest.out_grammar.inputs)?;
    for (k, v) in bound {
        ctx.insert(k, v);
    }
    apply_mappings(&mut ctx, &manifest.out_grammar.mappings)?;
    let final_out = ctx
        .get(&manifest.out_grammar.output_from)
        .ok_or_else(|| {
            RuntimeError::Validation(format!(
                "render: missing '{}'",
                manifest.out_grammar.output_from
            ))
        })?
        .clone();

    // canonicalize and hash for CID
    let bytes = crate::canon::canonical_bytes(&final_out)?;
    let cid = cid_b3(&bytes);

    Ok(ExecuteResult {
        artifacts: Artifacts {
            output: final_out,
            sub_receipts: vec![],
        },
        dimension_stack: vec!["parse".into(), "policy".into(), "render".into()],
        cid,
        policy_trace,
    })
}

/// Execute with a full cascade policy (rules + trace).
pub fn execute_with_cascade(
    manifest: &Manifest,
    vars: &BTreeMap<String, Value>,
    _cfg: &ExecuteConfig,
    cascade: &crate::policy::CascadePolicy,
    body_size: Option<usize>,
) -> Result<ExecuteResult> {
    // parse
    let mut ctx: BTreeMap<String, Value> = BTreeMap::new();
    let bound = bind_vars_to_inputs(vars, &manifest.in_grammar.inputs)?;
    for (k, v) in bound {
        ctx.insert(k, v);
    }
    apply_mappings(&mut ctx, &manifest.in_grammar.mappings)?;
    let parse_out = ctx
        .get(&manifest.in_grammar.output_from)
        .ok_or_else(|| {
            RuntimeError::Validation(format!(
                "parse: missing '{}'",
                manifest.in_grammar.output_from
            ))
        })?
        .clone();

    // policy — full cascade evaluation
    let policy_result = crate::policy::resolve(cascade, vars, body_size);
    if policy_result.decision == "DENY" {
        return Err(RuntimeError::PolicyDeny(
            policy_result.reason.unwrap_or_else(|| "policy deny".into()),
        ));
    }
    let policy_trace = policy_result.policy_trace;

    // render
    let mut render_vars = BTreeMap::new();
    render_vars.insert("__prev_output__".into(), parse_out.clone());
    let bound = bind_vars_to_inputs(&render_vars, &manifest.out_grammar.inputs)?;
    for (k, v) in bound {
        ctx.insert(k, v);
    }
    apply_mappings(&mut ctx, &manifest.out_grammar.mappings)?;
    let final_out = ctx
        .get(&manifest.out_grammar.output_from)
        .ok_or_else(|| {
            RuntimeError::Validation(format!(
                "render: missing '{}'",
                manifest.out_grammar.output_from
            ))
        })?
        .clone();

    let bytes = crate::canon::canonical_bytes(&final_out)?;
    let cid = cid_b3(&bytes);

    Ok(ExecuteResult {
        artifacts: Artifacts {
            output: final_out,
            sub_receipts: vec![],
        },
        dimension_stack: vec!["parse".into(), "policy".into(), "render".into()],
        cid,
        policy_trace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> ExecuteConfig {
        ExecuteConfig {
            version: "0.1.0".into(),
        }
    }

    /// Parse: base64 decode. Render: passthrough (no mappings, just forward).
    fn sample_passthrough() -> (Manifest, BTreeMap<String, Value>) {
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
        let man = Manifest {
            pipeline: "hello".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        (man, vars)
    }

    // ── Determinism ──────────────────────────────────────────────

    #[test]
    fn determinism_10x() {
        let (m, v) = sample_passthrough();
        let first = execute(&m, &v, &cfg()).unwrap();
        assert!(first.cid.starts_with("b3:"), "CID must be b3: prefixed");
        assert_eq!(first.cid.len(), 67, "b3:<64 hex chars>");
        for _ in 1..10 {
            let r = execute(&m, &v, &cfg()).unwrap();
            assert_eq!(
                r.cid, first.cid,
                "same input must produce same CID every time"
            );
            assert_eq!(r.artifacts.output, first.artifacts.output);
        }
    }

    #[test]
    fn determinism_key_order_irrelevant() {
        let (m, _) = sample_passthrough();
        let v1 = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let v2 = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]);
        let r1 = execute(&m, &v1, &cfg()).unwrap();
        let r2 = execute(&m, &v2, &cfg()).unwrap();
        assert_eq!(r1.cid, r2.cid);
    }

    #[test]
    fn different_input_different_cid() {
        let (m, _) = sample_passthrough();
        let v1 = BTreeMap::from([("input_data".into(), json!("aGVsbG8="))]); // "hello"
        let v2 = BTreeMap::from([("input_data".into(), json!("d29ybGQ="))]); // "world"
        let r1 = execute(&m, &v1, &cfg()).unwrap();
        let r2 = execute(&m, &v2, &cfg()).unwrap();
        assert_ne!(
            r1.cid, r2.cid,
            "different inputs must produce different CIDs"
        );
    }

    // ── Policy gate ─────────────────────────────────────────────

    #[test]
    fn policy_deny_blocks_execution() {
        let (mut m, v) = sample_passthrough();
        m.policy.allow = false;
        let err = execute(&m, &v, &cfg()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("policy deny"),
            "expected policy deny, got: {msg}"
        );
    }

    // ── Binding D8 ──────────────────────────────────────────────

    #[test]
    fn binding_name_match_exact() {
        let (m, _) = sample_passthrough();
        let vars = BTreeMap::from([("raw_b64".into(), json!("aGVsbG8="))]);
        let r = execute(&m, &vars, &cfg()).unwrap();
        assert!(!r.cid.is_empty());
    }

    #[test]
    fn binding_error_missing_vars() {
        let in_g = Grammar {
            inputs: BTreeMap::from([("a".into(), json!("")), ("b".into(), json!(""))]),
            mappings: vec![],
            output_from: "a".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("x".into(), json!(""))]),
            mappings: vec![],
            output_from: "x".into(),
        };
        let m = Manifest {
            pipeline: "t".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("a".into(), json!("ok"))]);
        let err = execute(&m, &vars, &cfg()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing"),
            "expected binding error, got: {msg}"
        );
        assert!(msg.contains("b"), "should mention missing key 'b'");
    }

    // ── Codec errors ────────────────────────────────────────────

    #[test]
    fn unknown_codec_rejected() {
        let in_g = Grammar {
            inputs: BTreeMap::from([("x".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "x".into(),
                codec: "rot13".into(),
                to: "y".into(),
            }],
            output_from: "y".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("z".into(), json!(""))]),
            mappings: vec![],
            output_from: "z".into(),
        };
        let m = Manifest {
            pipeline: "t".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("x".into(), json!("data"))]);
        let err = execute(&m, &vars, &cfg()).unwrap_err();
        assert!(err.to_string().contains("unknown codec"), "got: {err}");
    }

    #[test]
    fn invalid_base64_rejected() {
        let in_g = Grammar {
            inputs: BTreeMap::from([("raw_b64".into(), json!(""))]),
            mappings: vec![Mapping {
                from: "raw_b64".into(),
                codec: "base64.decode".into(),
                to: "out".into(),
            }],
            output_from: "out".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([("z".into(), json!(""))]),
            mappings: vec![],
            output_from: "z".into(),
        };
        let m = Manifest {
            pipeline: "t".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy { allow: true },
        };
        let vars = BTreeMap::from([("raw_b64".into(), json!("!!!not-base64!!!"))]);
        let err = execute(&m, &vars, &cfg()).unwrap_err();
        assert!(err.to_string().contains("base64"), "got: {err}");
    }

    // ── Dimension stack ─────────────────────────────────────────

    #[test]
    fn dimension_stack_correct() {
        let (m, v) = sample_passthrough();
        let r = execute(&m, &v, &cfg()).unwrap();
        assert_eq!(r.dimension_stack, vec!["parse", "policy", "render"]);
    }
}
