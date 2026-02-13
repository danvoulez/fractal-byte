// ubl_runtime bridge (snippet)
pub enum EngineKind { Current, Rb }

pub struct ExecuteRbReq {
    pub chip: Vec<u8>,
    pub inputs: Vec<String>, // textual CIDs
    pub ghost: bool,
    pub fuel: Option<u64>,
}

pub struct ExecuteRbRes {
    pub rc_cid: String,
    pub steps: u64,
    pub fuel_used: u64,
}

pub fn execute_rb(req: ExecuteRbReq) -> ExecuteRbRes {
    // TODO: wire rb_vm::Vm with CAS and Sign providers from ubl_runtime
    unimplemented!()
}
