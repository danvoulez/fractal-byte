//! UBL Adapters — Wasm-safe IO boundary for external data.
//!
//! Adapters freeze external data (HTTP responses, LLM completions) by CID,
//! ensuring the deterministic runtime never sees non-reproducible IO directly.
//!
//! # Architecture
//!
//! ```text
//! Runtime (deterministic, No-IO)
//!   │
//!   ▼
//! AdapterRequest { kind, params_cid, ... }
//!   │
//!   ▼  (IO boundary — runs OUTSIDE Wasm)
//! Adapter::execute()
//!   │
//!   ▼
//! AdapterResponse { response_cid, pinned_body, ... }
//! ```
//!
//! The runtime only ever sees CIDs. The actual IO happens outside the
//! deterministic boundary, and the response is pinned by its content hash.

pub mod cid;
pub mod error;
pub mod http;
pub mod types;

pub use error::AdapterError;
pub use types::{AdapterRequest, AdapterResponse, HttpParams, PinnedBlob};
