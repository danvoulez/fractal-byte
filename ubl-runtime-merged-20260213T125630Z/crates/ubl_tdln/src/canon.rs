use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

fn normalize_value(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(*b),
        Value::Number(n) => {
            if n.is_f64() { panic!("floating point not allowed"); }
            Value::Number(n.clone())
        }
        Value::String(s) => Value::String(s.nfc().collect::<String>()),
        Value::Array(arr) => Value::Array(arr.iter().map(normalize_value).collect()),
        Value::Object(obj) => {
            let mut out = Map::new();
            let mut keys: Vec<_> = obj.keys().cloned().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = obj.get(&k) {
                    if *v != Value::Null {
                        out.insert(k, normalize_value(v));
                    }
                }
            }
            Value::Object(out)
        }
    }
}

pub fn canonical_bytes(v: &Value) -> crate::error::Result<Vec<u8>> {
    let norm = normalize_value(v);
    let s = serde_json::to_string(&norm)?;
    Ok(s.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_bytes() {
        let a = json!({"b":"x","a":1,"z":null});
        let b = json!({"a":1,"b":"x"});
        assert_eq!(canonical_bytes(&a).unwrap(), canonical_bytes(&b).unwrap());
    }
}
