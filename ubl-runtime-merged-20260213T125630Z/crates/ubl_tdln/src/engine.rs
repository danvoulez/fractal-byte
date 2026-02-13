use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::{canon::canonical_bytes, cid::cid_b3, bind::bind_vars_to_inputs};
use crate::error::{Result, TdlnError};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifacts {
    pub output: Value,
    pub sub_receipts: Vec<Value>,
}

fn apply_mappings(ctx: &mut BTreeMap<String, Value>, maps: &[Mapping]) -> Result<()> {
    for m in maps {
        let src = ctx.get(&m.from)
            .ok_or_else(|| TdlnError::Validation(format!("mapping: key '{}' not found", m.from)))?;
        let val = match m.codec.as_str() {
            "base64.decode" => {
                let s = src.as_str().ok_or_else(|| TdlnError::Validation("expected string".into()))?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(s)
                    .map_err(|_| TdlnError::Validation("base64".into()))?;
                Value::String(String::from_utf8_lossy(&bytes).to_string())
            }
            _ => return Err(TdlnError::Validation(format!("unknown codec: {}", m.codec))),
        };
        ctx.insert(m.to.clone(), val);
    }
    Ok(())
}

pub fn execute(manifest: &Manifest, vars: &BTreeMap<String, Value>, _cfg: &ExecuteConfig) -> Result<ExecuteResult> {
    // parse
    let mut ctx: BTreeMap<String, Value> = BTreeMap::new();
    let bound = bind_vars_to_inputs(vars, &manifest.in_grammar.inputs)?;
    for (k,v) in bound { ctx.insert(k, v); }
    apply_mappings(&mut ctx, &manifest.in_grammar.mappings)?;
    let parse_out = ctx.get(&manifest.in_grammar.output_from)
        .ok_or_else(|| TdlnError::Validation(format!("parse: missing '{}'", manifest.in_grammar.output_from)))?
        .clone();

    // policy
    if !manifest.policy.allow {
        return Err(TdlnError::Validation("policy deny".into()));
    }

    // render: feed only previous stage output via 1<->1 to grammar input
    let mut render_vars = BTreeMap::new();
    render_vars.insert("__prev_output__".into(), parse_out.clone());
    let bound = bind_vars_to_inputs(&render_vars, &manifest.out_grammar.inputs)?;
    for (k,v) in bound { ctx.insert(k, v); }
    apply_mappings(&mut ctx, &manifest.out_grammar.mappings)?;
    let final_out = ctx.get(&manifest.out_grammar.output_from)
        .ok_or_else(|| TdlnError::Validation(format!("render: missing '{}'", manifest.out_grammar.output_from)))?
        .clone();

    // canonicalize and hash for CID
    let bytes = crate::canon::canonical_bytes(&final_out)?;
    let cid = cid_b3(&bytes);

    Ok(ExecuteResult {
        artifacts: Artifacts { output: final_out, sub_receipts: vec![] },
        dimension_stack: vec!["parse".into(), "policy".into(), "render".into()],
        cid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> (Manifest, BTreeMap<String, Value>) {
        let in_g = Grammar {
            inputs: BTreeMap::from([( "raw_b64".into(), json!(""))]),
            mappings: vec![ Mapping{ from:"raw_b64".into(), codec:"base64.decode".into(), to:"raw.bytes".into() } ],
            output_from: "raw.bytes".into(),
        };
        let out_g = Grammar {
            inputs: BTreeMap::from([( "payload".into(), json!(""))]),
            mappings: vec![ Mapping{ from:"payload".into(), codec:"base64.decode".into(), to:"render.text".into() } ],
            output_from: "render.text".into(),
        };
        let man = Manifest {
            pipeline: "hello".into(),
            in_grammar: in_g,
            out_grammar: out_g,
            policy: Policy{ allow: true },
        };
        let vars = BTreeMap::from([( "input_data".into(), json!("aGVsbG8="))]);
        (man, vars)
    }

    #[test]
    fn determinism_10x() {
        let (m, v) = sample();
        let mut first = None;
        for _ in 0..10 {
            let r = execute(&m, &v, &ExecuteConfig{ version: "0.1.0"}).unwrap();
            if let Some(f) = &first { assert_eq!(f, &r.cid); } else { first = Some(r.cid); }
        }
    }
}
