use clap::{Parser, Subcommand};
use colored::Colorize;
use std::process;

mod commands;

/// Standardized exit codes for CLI.
/// 0 = OK, 2 = input error, 3 = conflict (409), 4 = auth (401/403), 5 = rate limit (429), 1 = other.
#[allow(dead_code)]
const EXIT_OK: i32 = 0;
const EXIT_OTHER: i32 = 1;
const EXIT_INPUT: i32 = 2;
const EXIT_CONFLICT: i32 = 3;
const EXIT_AUTH: i32 = 4;
const EXIT_RATE: i32 = 5;

#[derive(Parser)]
#[command(name = "ublx", version, about = "UBL public CLI — execute, inspect, verify")]
struct Cli {
    /// Gate server URL (default: http://localhost:3000)
    #[arg(long, env = "UBL_GATE_URL", default_value = "http://localhost:3000")]
    gate: String,

    /// Bearer token for authentication
    #[arg(long, env = "UBL_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a pipeline from a manifest JSON file
    Execute {
        /// Path to manifest JSON file
        manifest: String,
        /// Path to vars JSON file (or - for stdin)
        #[arg(default_value = "-")]
        vars: String,
        /// Run in ghost mode (no persistence)
        #[arg(long)]
        ghost: bool,
    },
    /// Ingest raw JSON payload into the ledger
    Ingest {
        /// Path to payload JSON file (or - for stdin)
        #[arg(default_value = "-")]
        file: String,
        /// Also certify the ingested content
        #[arg(long)]
        certify: bool,
    },
    /// Get a receipt by CID
    Receipt {
        /// Receipt CID
        cid: String,
    },
    /// List all receipts in the registry
    Receipts,
    /// Get the audit report
    Audit,
    /// Get a transition receipt by CID
    Transition {
        /// Transition CID or rho_cid
        cid: String,
    },
    /// Resolve a DID or CID
    Resolve {
        /// DID or CID to resolve
        id: String,
    },
    /// Verify a receipt JSON file (check body_cid integrity)
    Verify {
        /// Path to receipt JSON file
        file: String,
    },
    /// Check gate server health
    Health,
    /// Compute BLAKE3 CID of a file
    Cid {
        /// Path to file
        file: String,
    },
}

/// Map error strings to exit codes based on HTTP status patterns.
fn exit_code_for(err: &str) -> i32 {
    if err.contains("HTTP 401") || err.contains("HTTP 403") {
        EXIT_AUTH
    } else if err.contains("HTTP 409") {
        EXIT_CONFLICT
    } else if err.contains("HTTP 429") {
        EXIT_RATE
    } else if err.contains("read ") || err.contains("parse ") || err.contains("missing ") {
        EXIT_INPUT
    } else {
        EXIT_OTHER
    }
}

fn main() {
    let cli = Cli::parse();
    let client = commands::Client::new(&cli.gate, cli.token.as_deref());

    let result = match cli.command {
        Commands::Execute { manifest, vars, ghost } => {
            commands::execute(&client, &manifest, &vars, ghost)
        }
        Commands::Ingest { file, certify } => commands::ingest(&client, &file, certify),
        Commands::Receipt { cid } => commands::receipt(&client, &cid),
        Commands::Receipts => commands::receipts(&client),
        Commands::Audit => commands::audit(&client),
        Commands::Transition { cid } => commands::transition(&client, &cid),
        Commands::Resolve { id } => commands::resolve(&client, &id),
        Commands::Verify { file } => commands::verify(&file),
        Commands::Health => commands::health(&client),
        Commands::Cid { file } => commands::cid(&file),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        process::exit(exit_code_for(&e));
    }
}
