use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use data_encoding::BASE32_NOPAD;
use serde_json::json;
use sha2::{Digest, Sha256};

const LEDGER_DIR: &str = "ledger";
const ATTEST_DIR: &str = "attestations";
const EVENTS_DIR: &str = "events";
const SCHEMAS_DIR: &str = "schemas";

fn repo_root() -> PathBuf {
    env::current_dir().expect("cwd")
}

fn cidv1_raw_sha256_base32(bytes: &[u8]) -> String {
    // CIDv1 (raw, sha2-256) prefix (multicodec + multihash) simplificado: usamos um marcador textual no MVP.
    // Em produção, troque por uma implementação CID/multihash real.
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let b32 = BASE32_NOPAD.encode(&digest);
    format!("cidv1-raw-sha2-256:{}", b32.to_lowercase())
}

fn ensure_dir(p: &Path) -> io::Result<()> {
    if !p.exists() { fs::create_dir_all(p)?; }
    Ok(())
}

fn cmd_put(path: &Path) -> io::Result<()> {
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let cid = cidv1_raw_sha256_base32(&buf);

    // store under ledger/<prefix>/<cid>
    let root = repo_root();
    let ledger = root.join(LEDGER_DIR);
    ensure_dir(&ledger)?;
    let prefix = &cid[cid.len().saturating_sub(2)..];
    let shard = ledger.join(prefix);
    ensure_dir(&shard)?;
    let dst = shard.join(&cid);
    fs::write(&dst, &buf)?;

    println!("{}", cid);
    Ok(())
}

fn cmd_get(cid: &str, out: Option<&Path>) -> io::Result<()> {
    let root = repo_root();
    let ledger = root.join(LEDGER_DIR);
    let prefix = &cid[cid.len().saturating_sub(2)..];
    let path = ledger.join(prefix).join(cid);
    let bytes = fs::read(&path)?;
    if let Some(outp) = out {
        fs::write(outp, &bytes)?;
        println!("written: {}", outp.display());
    } else {
        io::stdout().write_all(&bytes)?;
    }
    Ok(())
}

fn load_schema(name: &str) -> io::Result<serde_json::Value> {
    let root = repo_root();
    let path = root.join(SCHEMAS_DIR).join(name);
    let s = fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(v)
}

// Minimal validator: check required fields only (no full JSON Schema engine to keep deps tiny)
fn validate_required(obj: &serde_json::Value, required: &[&str]) -> io::Result<()> {
    for k in required {
        if obj.get(*k).is_none() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("missing field: {}", k)));
        }
    }
    Ok(())
}

fn cmd_attest(target_cid: &str, claim: &str, signer: &str) -> io::Result<()> {
    let now = Utc::now().to_rfc3339();
    let att = json!({
        "type": "attestation",
        "target_cid": target_cid,
        "claim": claim,
        "evidence": [],
        "signer": signer,
        "created_at": now,
        "signature": "base64:TODO"
    });

    // validate minimal
    validate_required(&att, &["type","target_cid","claim","signer","created_at","signature"])?;

    let root = repo_root();
    let dir = root.join(ATTEST_DIR);
    ensure_dir(&dir)?;
    let fname = format!("attest-{}-{}.json", claim, now.replace(':'  ,"_"));
    fs::write(dir.join(fname), serde_json::to_string_pretty(&att).unwrap())?;
    println!("ok");
    Ok(())
}

fn cmd_event(kind: &str, subject: &str, title: Option<&str>) -> io::Result<()> {
    let now = Utc::now().to_rfc3339();
    let mut meta = serde_json::Map::new();
    if let Some(t) = title { meta.insert("title".into(), serde_json::Value::String(t.into())); }
    let ev = json!({
        "type": "event",
        "kind": kind,
        "subjects": [subject],
        "metadata": meta,
        "author": "@local/user",
        "created_at": now,
        "signature": "base64:TODO"
    });
    validate_required(&ev, &["type","kind","subjects","author","created_at","signature"])?;
    let root = repo_root();
    let dir = root.join(EVENTS_DIR);
    ensure_dir(&dir)?;
    let fname = format!("event-{}-{}.json", kind, now.replace(':'  ,"_"));
    fs::write(dir.join(fname), serde_json::to_string_pretty(&ev).unwrap())?;
    println!("ok");
    Ok(())
}

fn parse_created_at(v: &serde_json::Value) -> String {
    v.get("created_at").and_then(|x| x.as_str()).unwrap_or("1970-01-01T00:00:00Z").to_string()
}

fn load_jsons(dir: &Path) -> io::Result<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    if !dir.exists() { return Ok(out); }
    for ent in fs::read_dir(dir)? {
        let ent = ent?;
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(s) = fs::read_to_string(&path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    out.push(v);
                }
            }
        }
    }
    Ok(out)
}

fn cmd_story(target: &str) -> io::Result<()> {
    let root = repo_root();
    let atts = load_jsons(&root.join(ATTEST_DIR))?
        .into_iter()
        .filter(|v| v.get("target_cid").and_then(|x| x.as_str()) == Some(target))
        .collect::<Vec<_>>();
    let events = load_jsons(&root.join(EVENTS_DIR))?
        .into_iter()
        .filter(|v| {
            v.get("subjects").and_then(|x| x.as_array())
             .map(|arr| arr.iter().any(|s| s.as_str() == Some(target))).unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let mut events_sorted = events.clone();
    let mut atts_sorted = atts.clone();
    events_sorted.sort_by_key(|v| parse_created_at(v));
    atts_sorted.sort_by_key(|v| parse_created_at(v));

    println!("# Story for {}\n", target);
    if !events_sorted.is_empty() {
        println!("## Events");
        for ev in &events_sorted {
            let created = parse_created_at(ev);
            let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let title = ev.get("metadata").and_then(|m| m.get("title")).and_then(|v| v.as_str()).unwrap_or("");
            println!("- [{}] {} {}", created, kind, title);
        }
        println!();
    }
    if !atts_sorted.is_empty() {
        println!("## Attestations");
        for at in &atts_sorted {
            let created = parse_created_at(at);
            let claim = at.get("claim").and_then(|v| v.as_str()).unwrap_or("?");
            let signer = at.get("signer").and_then(|v| v.as_str()).unwrap_or("?");
            println!("- [{}] {} (by {})", created, claim, signer);
        }
        println!();
    }
    if events_sorted.is_empty() && atts_sorted.is_empty() {
        println!("_No events or attestations found for this CID._");
    }
    Ok(())
}

fn cmd_verify(cid: &str) -> io::Result<()> {
    if !cid.starts_with("cidv1-raw-sha2-256:") || cid.len() < 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid CID form"));
    }
    // check ledger existence
    let root = repo_root();
    let prefix = &cid[cid.len().saturating_sub(2)..];
    let path = root.join(LEDGER_DIR).join(prefix).join(cid);
    if !path.exists() {
        println!("warning: blob not found in local ledger");
    }
    println!("ok");
    Ok(())
}

fn help() {
    println!("ubl — Universal Business Ledger CLI (MVP)\n");
    println!("USAGE:");
    println!("  ubl put <file>               # store blob and print CID");
    println!("  ubl get <cid> [out]          # fetch blob by CID");
    println!("  ubl attest <cid> <claim> <signer>");
    println!("  ubl event <kind> <cid> [title]   # kind=release|supersede|deprecate|yank");
    println!("  ubl story <cid>              # timeline");
    println!("  ubl verify <cid>             # basic checks");
}

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("put") => {
            let file = args.next().expect("provide file path");
            cmd_put(Path::new(&file))?
        }
        Some("get") => {
            let cid = args.next().expect("provide cid");
            let out = args.next().map(PathBuf::from);
            cmd_get(&cid, out.as_deref())?
        }
        Some("attest") => {
            let cid = args.next().expect("cid");
            let claim = args.next().expect("claim");
            let signer = args.next().expect("signer");
            cmd_attest(&cid, &claim, &signer)?
        }
        Some("event") => {
            let kind = args.next().expect("kind");
            let cid = args.next().expect("cid");
            let title = args.next();
            cmd_event(&kind, &cid, title.as_deref())?
        }
        Some("story") => {
            let cid = args.next().expect("cid");
            cmd_story(&cid)?
        }
        Some("verify") => {
            let cid = args.next().expect("cid");
            cmd_verify(&cid)?
        }
        _ => help()
    }
    Ok(())
}
