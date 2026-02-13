# Core: RB-VM (executor, TLV, opcodes MVP, fuel)

> Data: 2026-02-13

# Core: RB-VM (executor, TLV, opcodes MVP, fuel)

**Resumo**
Entrega o executor determinístico (stack + fuel), decoder TLV e subset de opcodes MVP.

**Mudanças**
- `crates/rb_vm/`: `opcode.rs`, `tlv.rs`, `types.rs`, `exec.rs`.
- Tipos: `Value`, `Cid`, `RcPayload`.
- Execução com `Fuel`, `VmConfig`, `VmOutcome`.

**Opcodes MVP**
`ConstI64`, `ConstBytes`, `PushInput`, `Drop`,
`JsonNormalize`, `JsonValidate`, `JsonGetKey`,
`AddI64`, `SubI64`, `MulI64`, `CmpI64`, `AssertTrue`,
`HashBlake3`, `CasPut`, `CasGet`,
`SetRcBody`, `AttachProof`, `SignDefault`, `EmitRc`.

**Checklist**
- [ ] Underflow/TypeMismatch cobertos
- [ ] Fuel debitado por instrução
- [ ] Deny gera erro determinístico (`ExecError::Deny`)
- [ ] `EmitRc` retorna `rc_cid`

**Critérios de aceite**
- Rodar chip de exemplo (deny <18) produz RC CID estável em ambiente dev

**Plano de rollout**
- Merge → habilitar runner binário para smoke test

**Riscos**
- JsonNormalize provisório; substituir por canon real no PR03
