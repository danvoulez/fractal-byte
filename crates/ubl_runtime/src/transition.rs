//! Transition Receipt: proves the deterministic jump from layer -1 (RB/bytecode)
//! to layer 0 (rho/canonical). Links preimage_raw_cid → rho_cid with witness metadata.

use serde::{Serialize, Deserialize};
use crate::cid::cid_b3;
use crate::canon::canonical_bytes;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionReceiptBody {
    pub t: String,
    pub v: String,
    pub from_layer: String,
    pub to_layer: String,
    pub op: String,
    pub preimage_raw_cid: String,
    pub rho_cid: String,
    pub witness: TransitionWitness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ghost: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionWitness {
    pub vm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_spent: Option<u64>,
}

impl TransitionReceiptBody {
    pub fn new(
        preimage_raw_cid: String,
        rho_cid: String,
        vm: &str,
        bytecode_cid: Option<String>,
        fuel_spent: Option<u64>,
        ghost: bool,
    ) -> Self {
        Self {
            t: "ubl/transition".into(),
            v: "1".into(),
            from_layer: "-1:rb".into(),
            to_layer: "0:rho".into(),
            op: "rho.normalize@ai-nrf1/v1".into(),
            preimage_raw_cid,
            rho_cid,
            witness: TransitionWitness {
                vm: vm.into(),
                bytecode_cid,
                fuel_spent,
            },
            ghost: if ghost { Some(true) } else { None },
            parents: Vec::new(),
        }
    }

    /// Canonical CID of this receipt body (deterministic, transport-independent).
    pub fn cid(&self) -> crate::error::Result<String> {
        let bytes = self.canonical_bytes()?;
        Ok(cid_b3(&bytes))
    }

    /// Canonical JSON bytes of this receipt body (for CID and signing).
    pub fn canonical_bytes(&self) -> crate::error::Result<Vec<u8>> {
        let val = serde_json::to_value(self)?;
        canonical_bytes(&val)
    }
}

/// Build a transition receipt for the RB→rho jump.
///
/// - `raw_bytes`: the pre-normalization bytes (layer -1)
/// - `rho_bytes`: the post-normalization canonical bytes (layer 0)
/// - `vm_tag`: e.g. "rb-vm@0.1.0"
/// - `bytecode_cid`: optional CID of the chip bytecode
/// - `fuel_spent`: optional fuel consumed
/// - `ghost`: if true, raw_bytes should NOT be persisted (privacy)
pub fn build_transition(
    raw_bytes: &[u8],
    rho_bytes: &[u8],
    vm_tag: &str,
    bytecode_cid: Option<String>,
    fuel_spent: Option<u64>,
    ghost: bool,
) -> TransitionReceiptBody {
    let preimage_raw_cid = cid_b3(raw_bytes);
    let rho_cid = cid_b3(rho_bytes);
    TransitionReceiptBody::new(preimage_raw_cid, rho_cid, vm_tag, bytecode_cid, fuel_spent, ghost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_receipt_deterministic() {
        let raw = b"hello raw";
        let rho = b"hello canonical";
        let tr1 = build_transition(raw, rho, "rb-vm@0.1.0", None, Some(42), false);
        let tr2 = build_transition(raw, rho, "rb-vm@0.1.0", None, Some(42), false);
        assert_eq!(tr1.cid().unwrap(), tr2.cid().unwrap());
    }

    #[test]
    fn transition_receipt_fields() {
        let tr = build_transition(b"raw", b"rho", "rb-vm@0.1.0", None, None, false);
        assert_eq!(tr.t, "ubl/transition");
        assert_eq!(tr.v, "1");
        assert_eq!(tr.from_layer, "-1:rb");
        assert_eq!(tr.to_layer, "0:rho");
        assert_eq!(tr.op, "rho.normalize@ai-nrf1/v1");
        assert!(tr.preimage_raw_cid.starts_with("b3:"));
        assert!(tr.rho_cid.starts_with("b3:"));
        assert!(tr.ghost.is_none());
    }

    #[test]
    fn transition_receipt_ghost() {
        let tr = build_transition(b"raw", b"rho", "rb-vm@0.1.0", None, None, true);
        assert_eq!(tr.ghost, Some(true));
    }

    #[test]
    fn transition_receipt_different_input_different_cid() {
        let tr1 = build_transition(b"input_a", b"rho_a", "rb-vm@0.1.0", None, None, false);
        let tr2 = build_transition(b"input_b", b"rho_b", "rb-vm@0.1.0", None, None, false);
        assert_ne!(tr1.cid().unwrap(), tr2.cid().unwrap());
    }

    #[test]
    fn transition_receipt_cid_stable_10x() {
        let raw = b"stable test input";
        let rho = b"stable canonical output";
        let first = build_transition(raw, rho, "rb-vm@0.1.0", Some("b3:abc".into()), Some(99), false);
        let first_cid = first.cid().unwrap();
        for _ in 0..10 {
            let tr = build_transition(raw, rho, "rb-vm@0.1.0", Some("b3:abc".into()), Some(99), false);
            assert_eq!(tr.cid().unwrap(), first_cid);
        }
    }

    #[test]
    fn replay_forense() {
        let raw = br#"{"age":17,"name":"Alice"}"#;
        // Simulate rho normalization (sorted keys, NFC — already sorted here)
        let rho_val = serde_json::from_slice::<serde_json::Value>(raw).unwrap();
        let rho_bytes = canonical_bytes(&rho_val).unwrap();
        let rho_cid_expected = cid_b3(&rho_bytes);

        let tr = build_transition(raw, &rho_bytes, "rb-vm@0.1.0", None, None, false);
        assert_eq!(tr.rho_cid, rho_cid_expected, "replay must match rho_cid");

        // Replay: given raw bytes, re-normalize and verify
        let replay_rho = canonical_bytes(&serde_json::from_slice::<serde_json::Value>(raw).unwrap()).unwrap();
        let replay_cid = cid_b3(&replay_rho);
        assert_eq!(replay_cid, tr.rho_cid, "forensic replay must produce same rho_cid");
    }

    #[test]
    fn replay_negative_mutated_byte() {
        let raw = br#"{"age":17,"name":"Alice"}"#;
        let rho_val = serde_json::from_slice::<serde_json::Value>(raw).unwrap();
        let rho_bytes = canonical_bytes(&rho_val).unwrap();

        let tr = build_transition(raw, &rho_bytes, "rb-vm@0.1.0", None, None, false);

        // Mutate 1 byte
        let mut mutated = raw.to_vec();
        mutated[7] = b'8'; // age:18 instead of 17
        let mutated_rho = canonical_bytes(&serde_json::from_slice::<serde_json::Value>(&mutated).unwrap()).unwrap();
        let mutated_cid = cid_b3(&mutated_rho);
        assert_ne!(mutated_cid, tr.rho_cid, "mutated input must NOT match original rho_cid");
    }
}
