use colored::Colorize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};

pub struct Client {
    base: String,
    http: reqwest::blocking::Client,
    token: Option<String>,
}

impl Client {
    pub fn new(base: &str, token: Option<&str>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::new(),
            token: token.map(|t| t.to_string()),
        }
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response, String> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.get(&url);
        if let Some(ref tok) = self.token {
            req = req.bearer_auth(tok);
        }
        req.send().map_err(|e| format!("request failed: {e}"))
    }

    fn post(&self, path: &str, body: &Value) -> Result<reqwest::blocking::Response, String> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.post(&url).json(body);
        if let Some(ref tok) = self.token {
            req = req.bearer_auth(tok);
        }
        req.send().map_err(|e| format!("request failed: {e}"))
    }
}

// ── ingest ──────────────────────────────────────────────────────

pub fn ingest(client: &Client, file: &str, certify: bool) -> Result<(), String> {
    let content = if file == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)
            .map_err(|e| format!("read stdin: {e}"))?;
        buf
    } else {
        fs::read_to_string(file)
            .map_err(|e| format!("read file: {e}"))?
    };
    let payload: Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse JSON: {e}"))?;

    let body = serde_json::json!({
        "payload": payload,
        "certify": certify,
    });

    let resp = client.post("/v1/ingest", &body)?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse response: {e}"))?;

    if status.is_success() {
        let cid = json.get("cid").and_then(|c| c.as_str()).unwrap_or("?");
        let tenant = json.get("tenant_id").and_then(|t| t.as_str()).unwrap_or("?");
        println!("{} {}", "CID:   ".dimmed(), cid.cyan());
        println!("{} {}", "Tenant:".dimmed(), tenant.dimmed());
        if certify {
            println!("{}", "  (certified)".green());
        }
    } else {
        let code = status.as_u16();
        let detail = json.get("error").or_else(|| json.get("detail"))
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(format!("HTTP {code}: {detail}"));
    }

    Ok(())
}

// ── execute ─────────────────────────────────────────────────────

pub fn execute(client: &Client, manifest_path: &str, vars_path: &str, ghost: bool) -> Result<(), String> {
    let manifest_str = fs::read_to_string(manifest_path)
        .map_err(|e| format!("read manifest: {e}"))?;
    let manifest: Value = serde_json::from_str(&manifest_str)
        .map_err(|e| format!("parse manifest: {e}"))?;

    let vars_str = if vars_path == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)
            .map_err(|e| format!("read stdin: {e}"))?;
        buf
    } else {
        fs::read_to_string(vars_path)
            .map_err(|e| format!("read vars: {e}"))?
    };
    let vars: Value = serde_json::from_str(&vars_str)
        .map_err(|e| format!("parse vars: {e}"))?;

    let body = serde_json::json!({
        "manifest": manifest,
        "vars": vars,
        "ghost": ghost,
    });

    let resp = client.post("/v1/execute", &body)?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse response: {e}"))?;

    if status.is_success() {
        let decision = json.get("decision").and_then(|d| d.as_str()).unwrap_or("?");
        let tip = json.get("tip_cid").and_then(|t| t.as_str()).unwrap_or("?");
        let ghost_flag = json.get("ghost").and_then(|g| g.as_bool()).unwrap_or(false);

        let badge = match decision {
            "ALLOW" => "ALLOW".green().bold(),
            "DENY" => "DENY".red().bold(),
            _ => decision.yellow().bold(),
        };

        println!("{} {}", "Decision:".dimmed(), badge);
        println!("{} {}", "Tip CID: ".dimmed(), tip.cyan());
        if ghost_flag {
            println!("{}", "  (ghost mode)".dimmed());
        }

        // Show dimension stack if present
        if let Some(dims) = json.get("dimension_stack").and_then(|d| d.as_array()) {
            let stack: Vec<&str> = dims.iter().filter_map(|d| d.as_str()).collect();
            println!("{} {}", "Stack:   ".dimmed(), stack.join(" → ").dimmed());
        }

        // Show receipts summary
        if let Some(receipts) = json.get("receipts") {
            println!();
            println!("{}", "Receipts:".bold());
            for (label, key) in [("  WA", "wa"), ("  TR", "transition"), ("  WF", "wf")] {
                if let Some(r) = receipts.get(key) {
                    let cid = r.get("body_cid").and_then(|c| c.as_str()).unwrap_or("—");
                    let t = r.get("t").and_then(|t| t.as_str()).unwrap_or("?");
                    println!("  {} {} {}", label.blue(), t.dimmed(), cid.dimmed());
                }
            }
        }
    } else {
        let code = status.as_u16();
        let detail = json.get("detail").or_else(|| json.get("error"))
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(format!("HTTP {code}: {detail}"));
    }

    Ok(())
}

// ── receipt ─────────────────────────────────────────────────────

pub fn receipt(client: &Client, cid: &str) -> Result<(), String> {
    let resp = client.get(&format!("/v1/receipt/{cid}"))?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if !status.is_success() {
        let code = status.as_u16();
        return Err(format!("HTTP {code}: receipt not found: {cid}"));
    }

    print_receipt(&json);
    Ok(())
}

// ── receipts (list) ─────────────────────────────────────────────

pub fn receipts(client: &Client) -> Result<(), String> {
    let resp = client.get("/v1/receipts")?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if !status.is_success() {
        let code = status.as_u16();
        return Err(format!("HTTP {code}: failed to list receipts"));
    }

    let map = json.as_object().ok_or("expected object")?;
    if map.is_empty() {
        println!("{}", "No receipts in registry.".dimmed());
        return Ok(());
    }

    println!("{} {} receipts\n", "Registry:".bold(), map.len());

    // Group by type
    let mut by_type: BTreeMap<String, Vec<(&str, &Value)>> = BTreeMap::new();
    for (cid, receipt) in map {
        let t = receipt.get("t").and_then(|t| t.as_str()).unwrap_or("unknown").to_string();
        by_type.entry(t).or_default().push((cid.as_str(), receipt));
    }

    for (t, entries) in &by_type {
        let label = match t.as_str() {
            "ubl/wa" => "Write-Ahead".blue(),
            "ubl/transition" => "Transition".purple(),
            "ubl/wf" => "Write-Final".green(),
            _ => t.normal(),
        };
        println!("{} ({})", label.bold(), entries.len());
        for (cid, receipt) in entries {
            let decision = receipt.get("body")
                .and_then(|b| b.get("decision"))
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let badge = match decision {
                "ALLOW" => " ALLOW".green(),
                "DENY" => " DENY".red(),
                "" => "".normal(),
                d => format!(" {d}").yellow(),
            };
            println!("  {} {}{}", "•".dimmed(), &cid[..cid.len().min(32)].dimmed(), badge);
        }
        println!();
    }

    Ok(())
}

// ── transition ──────────────────────────────────────────────────

pub fn transition(client: &Client, cid: &str) -> Result<(), String> {
    let resp = client.get(&format!("/v1/transition/{cid}"))?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if !status.is_success() {
        let code = status.as_u16();
        return Err(format!("HTTP {code}: transition not found: {cid}"));
    }

    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
    Ok(())
}

// ── verify ──────────────────────────────────────────────────────

pub fn verify(file: &str) -> Result<(), String> {
    let content = fs::read_to_string(file)
        .map_err(|e| format!("read file: {e}"))?;
    let receipt: Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse JSON: {e}"))?;

    let claimed_cid = receipt.get("body_cid")
        .and_then(|c| c.as_str())
        .ok_or("missing body_cid field")?;

    let body = receipt.get("body")
        .ok_or("missing body field")?;

    // Canonical serialize body and compute BLAKE3
    let body_bytes = serde_json::to_vec(body)
        .map_err(|e| format!("serialize body: {e}"))?;
    let hash = blake3::hash(&body_bytes);
    let computed_cid = format!("b3:{}", hex::encode(hash.as_bytes()));

    if computed_cid == claimed_cid {
        println!("{} body_cid verified", "✓".green().bold());
        println!("  {}", claimed_cid.dimmed());
    } else {
        println!("{} body_cid mismatch!", "✗".red().bold());
        println!("  claimed:  {}", claimed_cid.red());
        println!("  computed: {}", computed_cid.green());
        return Err("CID verification failed".into());
    }

    // Show receipt type and parents
    if let Some(t) = receipt.get("t").and_then(|t| t.as_str()) {
        println!("  type: {}", t.cyan());
    }
    if let Some(parents) = receipt.get("parents").and_then(|p| p.as_array()) {
        if !parents.is_empty() {
            println!("  parents:");
            for p in parents {
                if let Some(s) = p.as_str() {
                    println!("    → {}", s.dimmed());
                }
            }
        }
    }

    // Check signature presence
    if receipt.get("sig").is_some() {
        println!("  {} signature present", "✓".green());
    } else {
        println!("  {} no signature", "⚠".yellow());
    }

    Ok(())
}

// ── audit ───────────────────────────────────────────────────────

pub fn audit(client: &Client) -> Result<(), String> {
    let resp = client.get("/v1/audit")?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if !status.is_success() {
        let code = status.as_u16();
        let detail = json.get("error").and_then(|d| d.as_str()).unwrap_or("unknown");
        return Err(format!("HTTP {code}: {detail}"));
    }

    // Summary
    if let Some(summary) = json.get("summary") {
        println!("{}", "Audit Summary".bold());
        let total = summary.get("total_receipts").and_then(|t| t.as_u64()).unwrap_or(0);
        println!("  {} {}", "Total receipts:".dimmed(), total);
    }

    // By decision
    if let Some(by_dec) = json.get("by_decision").and_then(|d| d.as_object()) {
        println!("  {}:", "By decision".dimmed());
        for (dec, count) in by_dec {
            let badge = match dec.as_str() {
                "ALLOW" => dec.green().bold(),
                "DENY" => dec.red().bold(),
                _ => dec.yellow().bold(),
            };
            println!("    {badge} {count}");
        }
    }

    // Integrity
    if let Some(integrity) = json.get("integrity") {
        let valid = integrity.get("valid").and_then(|v| v.as_u64()).unwrap_or(0);
        let invalid = integrity.get("invalid").and_then(|v| v.as_u64()).unwrap_or(0);
        if invalid == 0 {
            println!("  {} {} valid, {} invalid", "\u{2713}".green(), valid, invalid);
        } else {
            println!("  {} {} valid, {} invalid", "\u{2717}".red(), valid, invalid);
        }
    }

    Ok(())
}

// ── resolve ─────────────────────────────────────────────────────

pub fn resolve(client: &Client, id: &str) -> Result<(), String> {
    let body = serde_json::json!({ "id": id });
    let resp = client.post("/v1/resolve", &body)?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if !status.is_success() {
        let code = status.as_u16();
        let detail = json.get("error").and_then(|d| d.as_str()).unwrap_or("unknown");
        return Err(format!("HTTP {code}: {detail}"));
    }

    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
    Ok(())
}

// ── health ──────────────────────────────────────────────────────

pub fn health(client: &Client) -> Result<(), String> {
    let resp = client.get("/healthz")?;
    let status = resp.status();
    let json: Value = resp.json().map_err(|e| format!("parse: {e}"))?;

    if status.is_success() {
        let s = json.get("status").and_then(|s| s.as_str()).unwrap_or("?");
        let v = json.get("version").and_then(|v| v.as_str()).unwrap_or("?");
        println!("{} {} (v{})", "●".green(), s.green().bold(), v.dimmed());
    } else {
        println!("{} gate unreachable (HTTP {})", "●".red(), status.as_u16());
    }

    Ok(())
}

// ── cid ─────────────────────────────────────────────────────────

pub fn cid(file: &str) -> Result<(), String> {
    let bytes = fs::read(file)
        .map_err(|e| format!("read file: {e}"))?;
    let hash = blake3::hash(&bytes);
    let cid = format!("b3:{}", hex::encode(hash.as_bytes()));
    println!("{cid}");
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────

fn print_receipt(receipt: &Value) {
    let t = receipt.get("t").and_then(|t| t.as_str()).unwrap_or("?");
    let cid = receipt.get("body_cid").and_then(|c| c.as_str()).unwrap_or("?");

    let type_label = match t {
        "ubl/wa" => "Write-Ahead".blue().bold(),
        "ubl/transition" => "Transition".purple().bold(),
        "ubl/wf" => "Write-Final".green().bold(),
        _ => t.normal().bold(),
    };

    println!("{} {}", type_label, cid.dimmed());

    if let Some(parents) = receipt.get("parents").and_then(|p| p.as_array()) {
        if !parents.is_empty() {
            println!("  {}:", "parents".dimmed());
            for p in parents {
                if let Some(s) = p.as_str() {
                    println!("    → {}", s.dimmed());
                }
            }
        }
    }

    if let Some(body) = receipt.get("body") {
        let decision = body.get("decision").and_then(|d| d.as_str());
        if let Some(d) = decision {
            let badge = match d {
                "ALLOW" => d.green().bold(),
                "DENY" => d.red().bold(),
                _ => d.yellow().bold(),
            };
            println!("  decision: {badge}");
        }
        println!("  {}:", "body".dimmed());
        let pretty = serde_json::to_string_pretty(body).unwrap_or_default();
        for line in pretty.lines() {
            println!("    {}", line.dimmed());
        }
    }

    if let Some(sig) = receipt.get("sig") {
        let kid = sig.get("kid").and_then(|k| k.as_str()).unwrap_or("?");
        let alg = sig.get("alg").and_then(|a| a.as_str()).unwrap_or("?");
        println!("  sig: {} {}", alg.dimmed(), kid.dimmed());
    }
}
