use crate::{opcode::Opcode, tlv::Instr, types::{Value, Cid, RcPayload}};
use serde_json::json;

pub type Fuel = u64;

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("fuel exhausted")]
    FuelExhausted,
    #[error("stack underflow for {0:?}")]
    StackUnderflow(Opcode),
    #[error("type mismatch for {0:?}")]
    TypeMismatch(Opcode),
    #[error("invalid payload for {0:?}")]
    InvalidPayload(Opcode),
    #[error("deny: {0}")]
    Deny(String),
}

pub trait CasProvider {
    fn put(&mut self, bytes: &[u8]) -> Cid;
    fn get(&self, cid: &Cid) -> Option<Vec<u8>>;
}

pub trait SignProvider {
    fn sign_jws(&self, payload_nrf_bytes: &[u8]) -> Vec<u8>;
    fn kid(&self) -> String;
}

#[derive(Clone)]
pub struct VmConfig {
    pub fuel_limit: Fuel,
    pub ghost: bool,
}

pub struct Vm<'a, C: CasProvider, S: SignProvider> {
    cfg: VmConfig,
    stack: Vec<Value>,
    steps: u64,
    fuel_used: Fuel,
    cas: C,
    signer: &'a S,
    inputs: Vec<Cid>,
    rc_body: serde_json::Value,
    proofs: Vec<Cid>,
}

pub struct VmOutcome {
    pub rc_cid: Option<Cid>,
    pub steps: u64,
    pub fuel_used: Fuel,
}

impl<'a, C: CasProvider, S: SignProvider> Vm<'a, C, S> {
    pub fn new(cfg: VmConfig, cas: C, signer: &'a S, inputs: Vec<Cid>) -> Self {
        Self{
            cfg, stack: Vec::new(), steps:0, fuel_used:0, cas, signer, inputs,
            rc_body: json!({}), proofs: Vec::new()
        }
    }

    fn charge(&mut self, units: Fuel) -> Result<(), ExecError> {
        let next = self.fuel_used.saturating_add(units);
        if next > self.cfg.fuel_limit { return Err(ExecError::FuelExhausted); }
        self.fuel_used = next;
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, ExecError> {
        self.stack.pop().ok_or(ExecError::StackUnderflow(Opcode::Drop))
    }

    fn push(&mut self, v: Value) { self.stack.push(v); }

    pub fn run(&mut self, code: &[Instr<'_>]) -> Result<VmOutcome, ExecError> {
        use Value::*;
        for ins in code {
            self.charge(1)?;
            self.steps += 1;
            match ins.op {
                Opcode::ConstI64 => {
                    if ins.payload.len() != 8 { return Err(ExecError::InvalidPayload(Opcode::ConstI64)); }
                    let v = i64::from_be_bytes(ins.payload.try_into().unwrap());
                    self.push(I64(v));
                }
                Opcode::ConstBytes => {
                    self.push(Bytes(ins.payload.to_vec()));
                }
                Opcode::Drop => { self.pop()?; }
                Opcode::PushInput => {
                    if ins.payload.len()!=2 { return Err(ExecError::InvalidPayload(Opcode::PushInput)); }
                    let idx = u16::from_be_bytes([ins.payload[0], ins.payload[1]]) as usize;
                    let cid = self.inputs.get(idx).cloned()
                        .ok_or(ExecError::InvalidPayload(Opcode::PushInput))?;
                    self.push(Value::Cid(cid));
                }
                Opcode::AddI64 | Opcode::SubI64 | Opcode::MulI64 => {
                    let b = match self.pop()? { I64(v)=>v, _=>return Err(ExecError::TypeMismatch(ins.op)) };
                    let a = match self.pop()? { I64(v)=>v, _=>return Err(ExecError::TypeMismatch(ins.op)) };
                    let r = match ins.op {
                        Opcode::AddI64 => a.saturating_add(b),
                        Opcode::SubI64 => a.saturating_sub(b),
                        _ => a.saturating_mul(b),
                    };
                    self.push(I64(r));
                }
                Opcode::CmpI64 => {
                    if ins.payload.len()!=1 { return Err(ExecError::InvalidPayload(Opcode::CmpI64)); }
                    let b = match self.pop()? { I64(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::CmpI64)) };
                    let a = match self.pop()? { I64(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::CmpI64)) };
                    let op = ins.payload[0];
                    let ok = match op {
                        0 => a==b,
                        1 => a!=b,
                        2 => a< b,
                        3 => a<=b,
                        4 => a> b,
                        5 => a>=b,
                        _ => return Err(ExecError::InvalidPayload(Opcode::CmpI64)),
                    };
                    self.push(Bool(ok));
                }
                Opcode::AssertTrue => {
                    let v = match self.pop()? { Bool(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::AssertTrue)) };
                    if !v { return Err(ExecError::Deny("assert_false".into())); }
                }
                Opcode::CasGet => {
                    let cid = match self.pop()? { Value::Cid(c)=>c, _=>return Err(ExecError::TypeMismatch(Opcode::CasGet)) };
                    let bytes = self.cas.get(&cid).ok_or(ExecError::Deny("cas_get_not_found".into()))?;
                    self.push(Bytes(bytes));
                }
                Opcode::CasPut => {
                    let bytes = match self.pop()? { Bytes(b)=>b, _=>return Err(ExecError::TypeMismatch(Opcode::CasPut)) };
                    let cid = self.cas.put(&bytes);
                    self.push(Value::Cid(cid));
                }
                // Placeholders for JSON and sign/emit (to be wired to lower layer canon and JWS)
                Opcode::JsonNormalize => {
                    let bytes = match self.pop()? { Bytes(b)=>b, _=>return Err(ExecError::TypeMismatch(Opcode::JsonNormalize)) };
                    let v: serde_json::Value = serde_json::from_slice(&bytes)
                        .map_err(|_| ExecError::Deny("json_parse_error".into()))?;
                    // MVP: no NFC/order here; real impl must call the canon
                    self.push(Value::Json(v));
                }
                Opcode::JsonValidate => {
                    let v = match self.pop()? { Value::Json(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::JsonValidate)) };
                    // MVP: passthrough; replace with limits/scheme
                    self.push(Value::Json(v));
                }
                Opcode::JsonGetKey => {
                    let key = std::str::from_utf8(ins.payload).map_err(|_| ExecError::InvalidPayload(Opcode::JsonGetKey))?;
                    let v = match self.pop()? { Value::Json(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::JsonGetKey)) };
                    let n = v.get(key).and_then(|x| x.as_i64()).ok_or(ExecError::Deny("json_key_missing_or_not_i64".into()))?;
                    self.push(Value::I64(n));
                }
                Opcode::HashBlake3 => {
                    let bytes = match self.pop()? { Value::Bytes(b)=>b, _=>return Err(ExecError::TypeMismatch(Opcode::HashBlake3)) };
                    let hash = blake3::hash(&bytes);
                    self.push(Value::Bytes(hash.as_bytes().to_vec()));
                }
                Opcode::SetRcBody => {
                    let v = match self.pop()? { Value::Json(v)=>v, _=>return Err(ExecError::TypeMismatch(Opcode::SetRcBody)) };
                    self.rc_body = v;
                }
                Opcode::AttachProof => {
                    let cid = match self.pop()? { Value::Cid(c)=>c, _=>return Err(ExecError::TypeMismatch(Opcode::AttachProof)) };
                    self.proofs.push(cid);
                }
                Opcode::SignDefault => {
                    // no-op here; signing is done in EmitRc using provider
                }
                Opcode::EmitRc => {
                    // Build minimal RC payload
                    let payload = RcPayload{
                        subject_cid: None,
                        engine: "rb-vm/0.1.0".into(),
                        ghost: false,
                        inputs: self.inputs.clone(),
                        proofs: self.proofs.clone(),
                        steps: self.steps,
                        fuel_used: self.fuel_used,
                        policy_id: "default:v1".into(),
                        decision: json!({"status":"ok"}),
                        body: self.rc_body.clone(),
                    };
                    let bytes = serde_json::to_vec(&payload).unwrap(); // TODO: canon NRF
                    let cid = self.cas.put(&bytes);
                    return Ok(VmOutcome{ rc_cid: Some(cid), steps: self.steps, fuel_used: self.fuel_used });
                }
            }
        }
        Ok(VmOutcome{ rc_cid: None, steps: self.steps, fuel_used: self.fuel_used })
    }
}
