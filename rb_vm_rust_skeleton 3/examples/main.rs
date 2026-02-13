use rb_vm::{
    exec::{Vm, VmConfig},
    tlv::decode_stream,
    providers::cas_fs::FsCas,
    providers::sign_env::EnvSigner,
    canon::NaiveCanon,
    types::Cid
};

fn main() {
    let chip = std::fs::read("examples/chip_deny_age.tlv").expect("chip");
    let code = decode_stream(&chip).expect("decode");
    let mut cas = FsCas::new("./.cas");
    // Seed de teste (32 bytes fixos) - dev only
    let signer = EnvSigner::from_seed_bytes("did:dev#k1", [7u8;32]);
    let canon = NaiveCanon{};
    // Grava input JSON no CAS para gerar um CID de teste
    let input_json = r#"{"age":17,"name":"Alice"}"#;
    let input_cid = cas.put(input_json.as_bytes());
    let cfg = VmConfig{ fuel_limit: 50_000, ghost: false };
    let mut vm = Vm::new(cfg, cas, &signer, canon, vec![input_cid.clone()]);
    let outcome = vm.run(&code).expect("run");
    println!("RC CID: {:?}", outcome.rc_cid.map(|c| c.0));
    println!("Fuel used: {}", outcome.fuel_used);
    println!("Steps: {}", outcome.steps);
}
