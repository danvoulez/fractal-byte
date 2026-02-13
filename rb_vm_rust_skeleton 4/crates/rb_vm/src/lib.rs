//! RB-VM (MVP) - deterministic stack VM for Fractal
//!
//! Goals (MVP):
//! - No-IO by construction (except CAS + Sign providers)
//! - Deterministic execution with fuel metering
//! - TLV bytecode format
//! - Minimal opcode set aligned with Fractal lower layer canon

pub mod opcode;
pub mod tlv;
pub mod types;
pub mod exec;
pub mod providers;

pub use opcode::Opcode;
pub use exec::{Vm, VmConfig, VmOutcome, Fuel, ExecError, CasProvider, SignProvider};
pub use types::{Value, Cid, RcPayload};
