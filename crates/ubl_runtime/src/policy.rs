use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A single policy rule with a condition expression and action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Unique rule identifier (e.g., "ACME_REQUIRE_BRAND")
    pub id: String,
    /// Cascade level: "global", "tenant", or "app"
    pub level: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// JSONPath-like condition key that must be non-null in vars.
    /// Format: "inputs.<key>" checks vars[key] != null.
    /// Format: "body_size <= N" checks body size constraint.
    /// Empty string or "true" means always pass.
    #[serde(default = "default_condition")]
    pub condition: String,
    /// Action on condition failure: "DENY" or "WARN"
    #[serde(default = "default_action")]
    pub action: String,
    /// Human-readable reason shown on DENY
    #[serde(default)]
    pub reason: String,
}

fn default_condition() -> String {
    "true".into()
}
fn default_action() -> String {
    "DENY".into()
}

/// Result of evaluating a single rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTraceEntry {
    pub level: String,
    pub rule: String,
    pub result: String, // "PASS" or "DENY"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Outcome of the full policy cascade evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyResult {
    /// "ALLOW" or "DENY"
    pub decision: String,
    /// The rule that decided (if DENY)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    /// Human-readable reason (if DENY)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Full trace of all rules evaluated
    pub policy_trace: Vec<PolicyTraceEntry>,
}

/// Extended policy supporting cascading rules.
///
/// Backward compatible: if `rules` is empty and `allow` is set,
/// behaves like the legacy `Policy { allow: bool }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadePolicy {
    /// Legacy field: simple allow/deny gate
    #[serde(default = "default_true")]
    pub allow: bool,
    /// Ordered rules evaluated in cascade: global → tenant → app.
    /// Rules MUST be ordered by level priority.
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

fn default_true() -> bool {
    true
}

impl Default for CascadePolicy {
    fn default() -> Self {
        Self {
            allow: true,
            rules: vec![],
        }
    }
}

impl CascadePolicy {
    /// Create a simple allow policy (backward compat).
    pub fn allow() -> Self {
        Self {
            allow: true,
            rules: vec![],
        }
    }

    /// Create a simple deny policy (backward compat).
    pub fn deny() -> Self {
        Self {
            allow: false,
            rules: vec![],
        }
    }
}

/// Evaluate the cascade policy against the given variables.
///
/// Rules are evaluated in order (global first, then tenant, then app).
/// A lower-level rule can never override a higher-level DENY.
/// If no rules are defined, falls back to the legacy `allow` boolean.
pub fn resolve(
    policy: &CascadePolicy,
    vars: &BTreeMap<String, Value>,
    body_size: Option<usize>,
) -> PolicyResult {
    // Legacy mode: no rules, just allow/deny
    if policy.rules.is_empty() {
        if policy.allow {
            return PolicyResult {
                decision: "ALLOW".into(),
                decided_by: None,
                reason: None,
                policy_trace: vec![PolicyTraceEntry {
                    level: "global".into(),
                    rule: "UBL_LEGACY_ALLOW".into(),
                    result: "PASS".into(),
                    reason: None,
                }],
            };
        } else {
            return PolicyResult {
                decision: "DENY".into(),
                decided_by: Some("UBL_LEGACY_DENY".into()),
                reason: Some("policy deny".into()),
                policy_trace: vec![PolicyTraceEntry {
                    level: "global".into(),
                    rule: "UBL_LEGACY_DENY".into(),
                    result: "DENY".into(),
                    reason: Some("policy deny".into()),
                }],
            };
        }
    }

    // Cascade evaluation
    let mut trace = Vec::with_capacity(policy.rules.len());

    for rule in &policy.rules {
        let pass = evaluate_condition(&rule.condition, vars, body_size);

        if pass {
            trace.push(PolicyTraceEntry {
                level: rule.level.clone(),
                rule: rule.id.clone(),
                result: "PASS".into(),
                reason: None,
            });
        } else {
            let reason = if rule.reason.is_empty() {
                format!("Rule {} failed: {}", rule.id, rule.condition)
            } else {
                rule.reason.clone()
            };

            trace.push(PolicyTraceEntry {
                level: rule.level.clone(),
                rule: rule.id.clone(),
                result: "DENY".into(),
                reason: Some(reason.clone()),
            });

            if rule.action == "DENY" {
                return PolicyResult {
                    decision: "DENY".into(),
                    decided_by: Some(rule.id.clone()),
                    reason: Some(reason),
                    policy_trace: trace,
                };
            }
            // WARN: continue evaluation but record the failure
        }
    }

    PolicyResult {
        decision: "ALLOW".into(),
        decided_by: None,
        reason: None,
        policy_trace: trace,
    }
}

/// Evaluate a condition expression against vars and body_size.
///
/// Supported conditions:
/// - "true" or "" → always pass
/// - "inputs.<key>" or "inputs.<key> != null" → vars[key] exists and is not null
/// - "body_size <= N" → body_size <= N
/// - "inputs.<key> == <value>" → vars[key] == value (string comparison)
fn evaluate_condition(
    condition: &str,
    vars: &BTreeMap<String, Value>,
    body_size: Option<usize>,
) -> bool {
    let cond = condition.trim();

    if cond.is_empty() || cond == "true" {
        return true;
    }

    // body_size <= N
    if let Some(rest) = cond.strip_prefix("body_size") {
        let rest = rest.trim();
        if let Some(n_str) = rest.strip_prefix("<=") {
            if let Ok(limit) = n_str.trim().parse::<usize>() {
                return body_size.is_none_or(|s| s <= limit);
            }
        }
        return true; // unparseable → pass (fail-open for unknown conditions)
    }

    // inputs.<key> ...
    if let Some(key_expr) = cond.strip_prefix("inputs.") {
        // inputs.<key> != null
        if let Some(key) = key_expr.strip_suffix("!= null") {
            let key = key.trim();
            return vars.get(key).is_some_and(|v| !v.is_null());
        }
        // inputs.<key> == "<value>"
        if let Some((key, expected)) = key_expr.split_once("==") {
            let key = key.trim();
            let expected = expected.trim().trim_matches('"');
            return vars
                .get(key)
                .and_then(|v| v.as_str())
                .is_some_and(|v| v == expected);
        }
        // inputs.<key> (shorthand for != null)
        let key = key_expr.trim();
        return vars.get(key).is_some_and(|v| !v.is_null());
    }

    // Unknown condition → pass (fail-open)
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vars_with(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // ── Legacy mode ──────────────────────────────────────────────

    #[test]
    fn legacy_allow() {
        let p = CascadePolicy::allow();
        let r = resolve(&p, &BTreeMap::new(), None);
        assert_eq!(r.decision, "ALLOW");
        assert_eq!(r.policy_trace.len(), 1);
        assert_eq!(r.policy_trace[0].result, "PASS");
    }

    #[test]
    fn legacy_deny() {
        let p = CascadePolicy::deny();
        let r = resolve(&p, &BTreeMap::new(), None);
        assert_eq!(r.decision, "DENY");
        assert_eq!(r.decided_by.as_deref(), Some("UBL_LEGACY_DENY"));
    }

    // ── Cascade rules ────────────────────────────────────────────

    #[test]
    fn single_rule_pass() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![PolicyRule {
                id: "REQUIRE_BRAND".into(),
                level: "tenant".into(),
                description: "".into(),
                condition: "inputs.brand_id".into(),
                action: "DENY".into(),
                reason: "brand_id required".into(),
            }],
        };
        let vars = vars_with(&[("brand_id", json!("acme"))]);
        let r = resolve(&p, &vars, None);
        assert_eq!(r.decision, "ALLOW");
        assert_eq!(r.policy_trace.len(), 1);
        assert_eq!(r.policy_trace[0].result, "PASS");
    }

    #[test]
    fn single_rule_deny() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![PolicyRule {
                id: "REQUIRE_BRAND".into(),
                level: "tenant".into(),
                description: "".into(),
                condition: "inputs.brand_id".into(),
                action: "DENY".into(),
                reason: "brand_id required".into(),
            }],
        };
        let vars = vars_with(&[("message", json!("hello"))]);
        let r = resolve(&p, &vars, None);
        assert_eq!(r.decision, "DENY");
        assert_eq!(r.decided_by.as_deref(), Some("REQUIRE_BRAND"));
        assert_eq!(r.reason.as_deref(), Some("brand_id required"));
    }

    #[test]
    fn cascade_global_then_tenant() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![
                PolicyRule {
                    id: "UBL_AUTH".into(),
                    level: "global".into(),
                    description: "".into(),
                    condition: "true".into(),
                    action: "DENY".into(),
                    reason: "".into(),
                },
                PolicyRule {
                    id: "ACME_BRAND".into(),
                    level: "tenant".into(),
                    description: "".into(),
                    condition: "inputs.brand_id".into(),
                    action: "DENY".into(),
                    reason: "brand_id required".into(),
                },
            ],
        };
        let vars = vars_with(&[("brand_id", json!("acme"))]);
        let r = resolve(&p, &vars, None);
        assert_eq!(r.decision, "ALLOW");
        assert_eq!(r.policy_trace.len(), 2);
        assert_eq!(r.policy_trace[0].rule, "UBL_AUTH");
        assert_eq!(r.policy_trace[0].result, "PASS");
        assert_eq!(r.policy_trace[1].rule, "ACME_BRAND");
        assert_eq!(r.policy_trace[1].result, "PASS");
    }

    #[test]
    fn cascade_global_deny_stops_early() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![
                PolicyRule {
                    id: "UBL_REQUIRE_TOKEN".into(),
                    level: "global".into(),
                    description: "".into(),
                    condition: "inputs.token".into(),
                    action: "DENY".into(),
                    reason: "token required".into(),
                },
                PolicyRule {
                    id: "ACME_BRAND".into(),
                    level: "tenant".into(),
                    description: "".into(),
                    condition: "inputs.brand_id".into(),
                    action: "DENY".into(),
                    reason: "brand_id required".into(),
                },
            ],
        };
        let vars = vars_with(&[("brand_id", json!("acme"))]);
        let r = resolve(&p, &vars, None);
        assert_eq!(r.decision, "DENY");
        assert_eq!(r.decided_by.as_deref(), Some("UBL_REQUIRE_TOKEN"));
        // Only 1 entry in trace — stopped at global deny
        assert_eq!(r.policy_trace.len(), 1);
    }

    #[test]
    fn body_size_rule() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![PolicyRule {
                id: "MAX_BODY".into(),
                level: "global".into(),
                description: "".into(),
                condition: "body_size <= 1024".into(),
                action: "DENY".into(),
                reason: "body too large".into(),
            }],
        };
        // Within limit
        let r = resolve(&p, &BTreeMap::new(), Some(512));
        assert_eq!(r.decision, "ALLOW");

        // Over limit
        let r = resolve(&p, &BTreeMap::new(), Some(2048));
        assert_eq!(r.decision, "DENY");
        assert_eq!(r.decided_by.as_deref(), Some("MAX_BODY"));
    }

    #[test]
    fn warn_action_continues() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![
                PolicyRule {
                    id: "SOFT_CHECK".into(),
                    level: "tenant".into(),
                    description: "".into(),
                    condition: "inputs.optional_field".into(),
                    action: "WARN".into(),
                    reason: "optional_field missing".into(),
                },
                PolicyRule {
                    id: "HARD_CHECK".into(),
                    level: "tenant".into(),
                    description: "".into(),
                    condition: "true".into(),
                    action: "DENY".into(),
                    reason: "".into(),
                },
            ],
        };
        let vars = vars_with(&[("message", json!("hi"))]);
        let r = resolve(&p, &vars, None);
        // WARN doesn't block — should still ALLOW
        assert_eq!(r.decision, "ALLOW");
        assert_eq!(r.policy_trace.len(), 2);
        assert_eq!(r.policy_trace[0].result, "DENY"); // recorded as DENY in trace
        assert_eq!(r.policy_trace[1].result, "PASS");
    }

    #[test]
    fn inputs_equals_condition() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![PolicyRule {
                id: "CHECK_ENV".into(),
                level: "app".into(),
                description: "".into(),
                condition: "inputs.env == \"production\"".into(),
                action: "DENY".into(),
                reason: "must be production".into(),
            }],
        };
        let vars = vars_with(&[("env", json!("production"))]);
        assert_eq!(resolve(&p, &vars, None).decision, "ALLOW");

        let vars = vars_with(&[("env", json!("staging"))]);
        assert_eq!(resolve(&p, &vars, None).decision, "DENY");
    }

    // ── Serialization roundtrip ──────────────────────────────────

    #[test]
    fn serde_roundtrip() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![PolicyRule {
                id: "R1".into(),
                level: "global".into(),
                description: "test".into(),
                condition: "inputs.x".into(),
                action: "DENY".into(),
                reason: "x required".into(),
            }],
        };
        let json = serde_json::to_string(&p).unwrap();
        let p2: CascadePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.rules.len(), 1);
        assert_eq!(p2.rules[0].id, "R1");
    }

    #[test]
    fn deserialize_legacy_policy() {
        let json = r#"{"allow": true}"#;
        let p: CascadePolicy = serde_json::from_str(json).unwrap();
        assert!(p.allow);
        assert!(p.rules.is_empty());
    }

    #[test]
    fn policy_trace_serializes() {
        let p = CascadePolicy {
            allow: true,
            rules: vec![
                PolicyRule {
                    id: "G1".into(),
                    level: "global".into(),
                    description: "".into(),
                    condition: "true".into(),
                    action: "DENY".into(),
                    reason: "".into(),
                },
                PolicyRule {
                    id: "T1".into(),
                    level: "tenant".into(),
                    description: "".into(),
                    condition: "inputs.brand_id".into(),
                    action: "DENY".into(),
                    reason: "need brand".into(),
                },
            ],
        };
        let vars = vars_with(&[("brand_id", json!("acme"))]);
        let r = resolve(&p, &vars, None);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["decision"], "ALLOW");
        assert!(json["policy_trace"].is_array());
        assert_eq!(json["policy_trace"].as_array().unwrap().len(), 2);
    }
}
