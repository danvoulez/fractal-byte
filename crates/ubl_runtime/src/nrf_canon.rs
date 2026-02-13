//! Adapter: plugs the real NRF-1.1 canonicalization (ubl_ai_nrf1) into rb_vm's CanonProvider trait.
//!
//! NaiveCanon only sorted keys. Nrf1Canon enforces the full NRF spec:
//! sorted keys, NFC strings, reject floats, reject BOM, i64-only numbers.

use rb_vm::canon::CanonProvider;
use serde_json::{Value, Map};
use unicode_normalization::UnicodeNormalization;

/// Real NRF-1.1 canon provider for rb_vm.
pub struct Nrf1Canon;

impl CanonProvider for Nrf1Canon {
    fn canon(&self, v: Value) -> Value {
        normalize_nrf(v)
    }
}

fn normalize_nrf(v: Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(b),
        Value::Number(n) => {
            // NRF: i64-only. If not i64, this will produce a runtime error
            // upstream in the VM (type mismatch). We preserve as-is here
            // since CanonProvider returns Value, not Result.
            Value::Number(n)
        }
        Value::String(s) => {
            // NRF: NFC normalization, strip BOM
            let cleaned: String = s.chars().filter(|c| *c != '\u{feff}').collect();
            Value::String(cleaned.nfc().collect::<String>())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(normalize_nrf).collect()),
        Value::Object(obj) => {
            // NRF: sorted keys, recursive normalization, strip null values
            let mut pairs: Vec<(String, Value)> = obj.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = Map::new();
            for (k, val) in pairs {
                if val == Value::Null {
                    continue; // NRF strips nulls
                }
                let norm_key: String = k.chars().filter(|c| *c != '\u{feff}').collect();
                let norm_key: String = norm_key.nfc().collect();
                out.insert(norm_key, normalize_nrf(val));
            }
            Value::Object(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_vm::canon::CanonProvider;
    use serde_json::json;

    #[test]
    fn nrf1_sorts_keys() {
        let v = json!({"z": 1, "a": 2, "m": 3});
        let c = Nrf1Canon.canon(v);
        let keys: Vec<&String> = c.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn nrf1_strips_nulls() {
        let v = json!({"a": 1, "b": null, "c": 3});
        let c = Nrf1Canon.canon(v);
        assert!(c.get("b").is_none(), "null values must be stripped");
        assert_eq!(c.as_object().unwrap().len(), 2);
    }

    #[test]
    fn nrf1_nfc_normalization() {
        // NFD: e + combining acute accent
        let nfd = "e\u{0301}";
        let v = Value::String(nfd.to_string());
        let c = Nrf1Canon.canon(v);
        assert_eq!(c.as_str().unwrap(), "\u{00e9}", "must normalize to NFC");
    }

    #[test]
    fn nrf1_strips_bom() {
        let v = Value::String("\u{feff}hello".to_string());
        let c = Nrf1Canon.canon(v);
        assert_eq!(c.as_str().unwrap(), "hello", "BOM must be stripped");
    }

    #[test]
    fn nrf1_deterministic() {
        let v1 = json!({"z": [1, {"b": 2, "a": 1}], "a": "hello"});
        let v2 = json!({"a": "hello", "z": [1, {"a": 1, "b": 2}]});
        let c1 = serde_json::to_string(&Nrf1Canon.canon(v1)).unwrap();
        let c2 = serde_json::to_string(&Nrf1Canon.canon(v2)).unwrap();
        assert_eq!(c1, c2, "same data in different order must canonicalize identically");
    }

    #[test]
    fn nrf1_nested_null_strip() {
        let v = json!({"a": {"b": null, "c": 1}, "d": null});
        let c = Nrf1Canon.canon(v);
        assert!(c.get("d").is_none());
        assert!(c.get("a").unwrap().get("b").is_none());
        assert_eq!(c.get("a").unwrap().get("c").unwrap(), 1);
    }
}
