# RB-VM Rust Skeleton

- `crates/rb_vm`: biblioteca da VM (executor, TLV, opcodes, tipos, providers)
- `crates/rb_vm_disasm`: binário para dissecar chips TLV
- `crates/rb_vm_tests`: suíte inicial de testes (expandir com as Leis)
- `bridge_snippets/`: trechos de código para integrar `ubl_runtime` e `ubl_gate`

## Tarefas imediatas
- Conectar `JsonNormalize` à canon NRF real
- Implementar `SignProvider` com Ed25519 + JWS
- Fornecer `CasProvider` backed por FS/S3 do ledger
- Preencher goldens nas Leis e no exemplo `deny_age`
