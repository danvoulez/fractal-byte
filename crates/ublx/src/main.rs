use clap::{Parser, Subcommand};
use colored::Colorize;
use std::process;

mod commands;

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
    /// Get a receipt by CID
    Receipt {
        /// Receipt CID
        cid: String,
    },
    /// List all receipts in the registry
    Receipts,
    /// Get a transition receipt by CID
    Transition {
        /// Transition CID or rho_cid
        cid: String,
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

fn main() {
    let cli = Cli::parse();
    let client = commands::Client::new(&cli.gate, cli.token.as_deref());

    let result = match cli.command {
        Commands::Execute { manifest, vars, ghost } => {
            commands::execute(&client, &manifest, &vars, ghost)
        }
        Commands::Receipt { cid } => commands::receipt(&client, &cid),
        Commands::Receipts => commands::receipts(&client),
        Commands::Transition { cid } => commands::transition(&client, &cid),
        Commands::Verify { file } => commands::verify(&file),
        Commands::Health => commands::health(&client),
        Commands::Cid { file } => commands::cid(&file),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        process::exit(1);
    }
}
