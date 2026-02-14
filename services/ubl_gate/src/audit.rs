//! Audit report generator — produces structured reports from the receipt chain.
//!
//! Reports include:
//! - Summary statistics (total, by type, by decision)
//! - Timeline of receipts
//! - Policy trace aggregation
//! - Integrity verification (body_cid checks)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub generated_at: String,
    pub summary: AuditSummary,
    pub by_type: BTreeMap<String, usize>,
    pub by_decision: BTreeMap<String, usize>,
    pub timeline: Vec<TimelineEntry>,
    pub integrity: IntegrityReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub total_receipts: usize,
    pub ghost_count: usize,
    pub signed_count: usize,
    pub unsigned_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub cid: String,
    pub receipt_type: String,
    pub decision: Option<String>,
    pub ghost: bool,
    pub phase: Option<String>,
    pub parents: Vec<String>,
    pub has_signature: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub total_checked: usize,
    pub valid: usize,
    pub invalid: usize,
    pub failures: Vec<IntegrityFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityFailure {
    pub cid: String,
    pub claimed_body_cid: String,
    pub computed_body_cid: String,
}

/// Generate an audit report from the receipt chain.
pub fn generate_report(chain: &BTreeMap<String, Value>) -> AuditReport {
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_decision: BTreeMap<String, usize> = BTreeMap::new();
    let mut timeline = Vec::new();
    let mut ghost_count = 0usize;
    let mut signed_count = 0usize;
    let mut unsigned_count = 0usize;
    let mut integrity_valid = 0usize;
    let mut integrity_invalid = 0usize;
    let mut failures = Vec::new();

    for (cid, receipt) in chain {
        // Type
        let t = receipt
            .get("t")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown")
            .to_string();
        *by_type.entry(t.clone()).or_insert(0) += 1;

        // Decision
        let decision = receipt
            .get("body")
            .and_then(|b| b.get("decision"))
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
        if let Some(ref d) = decision {
            *by_decision.entry(d.clone()).or_insert(0) += 1;
        }

        // Ghost
        let ghost = receipt
            .get("observability")
            .and_then(|o| o.get("ghost"))
            .and_then(|g| g.as_bool())
            .unwrap_or(false);
        if ghost {
            ghost_count += 1;
        }

        // Phase
        let phase = receipt
            .get("observability")
            .and_then(|o| o.get("phase"))
            .and_then(|p| p.as_str())
            .map(|s| s.to_string());

        // Signature
        let has_signature = receipt.get("sig").is_some();
        if has_signature {
            signed_count += 1;
        } else {
            unsigned_count += 1;
        }

        // Parents
        let parents = receipt
            .get("parents")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Integrity check: verify body_cid matches body
        let claimed_body_cid = receipt
            .get("body_cid")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        if let Some(body) = receipt.get("body") {
            if let Ok(body_bytes) = serde_json::to_vec(body) {
                let hash = blake3::hash(&body_bytes);
                let computed = format!("b3:{}", hex::encode(hash.as_bytes()));
                if computed == claimed_body_cid {
                    integrity_valid += 1;
                } else {
                    integrity_invalid += 1;
                    failures.push(IntegrityFailure {
                        cid: cid.clone(),
                        claimed_body_cid: claimed_body_cid.to_string(),
                        computed_body_cid: computed,
                    });
                }
            }
        }

        timeline.push(TimelineEntry {
            cid: cid.clone(),
            receipt_type: t,
            decision,
            ghost,
            phase,
            parents,
            has_signature,
        });
    }

    let total = chain.len();

    AuditReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        summary: AuditSummary {
            total_receipts: total,
            ghost_count,
            signed_count,
            unsigned_count,
        },
        by_type,
        by_decision,
        timeline,
        integrity: IntegrityReport {
            total_checked: integrity_valid + integrity_invalid,
            valid: integrity_valid,
            invalid: integrity_invalid,
            failures,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_chain() -> BTreeMap<String, Value> {
        let body = json!({"type": "ubl/wa", "rho_cid": "b3:abc"});
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let hash = blake3::hash(&body_bytes);
        let body_cid = format!("b3:{}", hex::encode(hash.as_bytes()));

        let wa = json!({
            "t": "ubl/wa",
            "parents": [],
            "body": body,
            "body_cid": body_cid,
            "sig": {"alg": "EdDSA", "kid": "did:dev#k1", "sig": "abc"},
            "observability": {"ghost": false, "phase": "wa:write-ahead"}
        });

        let wf_body = json!({"type": "ubl/wf", "decision": "ALLOW", "dimension_stack": ["parse", "policy", "render"]});
        let wf_body_bytes = serde_json::to_vec(&wf_body).unwrap();
        let wf_hash = blake3::hash(&wf_body_bytes);
        let wf_body_cid = format!("b3:{}", hex::encode(wf_hash.as_bytes()));

        let wf = json!({
            "t": "ubl/wf",
            "parents": [body_cid],
            "body": wf_body,
            "body_cid": wf_body_cid,
            "sig": {"alg": "EdDSA", "kid": "did:dev#k1", "sig": "def"},
            "observability": {"ghost": false, "phase": "wf:write-final"}
        });

        let mut chain = BTreeMap::new();
        chain.insert(body_cid, wa);
        chain.insert(wf_body_cid, wf);
        chain
    }

    #[test]
    fn report_summary() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        assert_eq!(report.summary.total_receipts, 2);
        assert_eq!(report.summary.signed_count, 2);
        assert_eq!(report.summary.unsigned_count, 0);
        assert_eq!(report.summary.ghost_count, 0);
    }

    #[test]
    fn report_by_type() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        assert_eq!(report.by_type.get("ubl/wa"), Some(&1));
        assert_eq!(report.by_type.get("ubl/wf"), Some(&1));
    }

    #[test]
    fn report_by_decision() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        assert_eq!(report.by_decision.get("ALLOW"), Some(&1));
    }

    #[test]
    fn report_integrity_all_valid() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        assert_eq!(report.integrity.valid, 2);
        assert_eq!(report.integrity.invalid, 0);
        assert!(report.integrity.failures.is_empty());
    }

    #[test]
    fn report_integrity_detects_tamper() {
        let mut chain = sample_chain();
        // Tamper with a receipt body
        let first_key = chain.keys().next().unwrap().clone();
        if let Some(receipt) = chain.get_mut(&first_key) {
            receipt["body"]["tampered"] = json!(true);
        }
        let report = generate_report(&chain);
        assert_eq!(report.integrity.invalid, 1);
        assert_eq!(report.integrity.failures.len(), 1);
    }

    #[test]
    fn report_timeline_has_entries() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        assert_eq!(report.timeline.len(), 2);
        assert!(report.timeline.iter().all(|e| e.has_signature));
    }

    #[test]
    fn report_serializes() {
        let chain = sample_chain();
        let report = generate_report(&chain);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("generated_at"));
        assert!(json.contains("integrity"));
    }

    #[test]
    fn empty_chain_report() {
        let chain = BTreeMap::new();
        let report = generate_report(&chain);
        assert_eq!(report.summary.total_receipts, 0);
        assert!(report.timeline.is_empty());
        assert_eq!(report.integrity.total_checked, 0);
    }
}
