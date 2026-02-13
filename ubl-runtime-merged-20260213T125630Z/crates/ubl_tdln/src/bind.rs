use std::collections::BTreeMap;
use serde_json::Value;
use crate::error::{Result, TdlnError};

/// D8: deterministic input binding from vars -> grammar inputs.
pub fn bind_vars_to_inputs(
    vars: &BTreeMap<String, Value>,
    grammar_inputs: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>> {
    // 1) name match
    let mut bound = BTreeMap::new();
    let mut missing = Vec::new();
    for k in grammar_inputs.keys() {
        if let Some(v) = vars.get(k) { bound.insert(k.clone(), v.clone()); }
        else { missing.push(k.clone()); }
    }
    if missing.is_empty() { return Ok(bound); }

    // 2) fallback 1<->1
    if grammar_inputs.len()==1 && vars.len()==1 {
        let (gin, _) = grammar_inputs.iter().next().unwrap();
        let (_, v) = vars.iter().next().unwrap();
        bound.insert(gin.clone(), v.clone());
        return Ok(bound);
    }

    // 3) error
    Err(TdlnError::Binding{ missing, available: vars.keys().cloned().collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    fn map(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs.iter().map(|(k,v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn name_match() {
        let vars = map(&[("raw_b64", json!("aGVsbG8="))]);
        let ins = map(&[("raw_b64", json!(""))]);
        let b = bind_vars_to_inputs(&vars, &ins).unwrap();
        assert!(b.contains_key("raw_b64"));
    }

    #[test]
    fn fallback_1to1() {
        let vars = map(&[("input_data", json!("aGVsbG8="))]);
        let ins = map(&[("raw_b64", json!(""))]);
        let b = bind_vars_to_inputs(&vars, &ins).unwrap();
        assert!(b.contains_key("raw_b64"));
    }
}
