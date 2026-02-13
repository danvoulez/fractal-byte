# Tests: 10 Leis com goldens por CID + fixtures de exemplo

> Data: 2026-02-13

# Tests: Leis + Goldens

**Resumo**
Implementa as 10 Leis como testes de integração com goldens por CID e fixtures.

**Mudanças**
- `crates/rb_vm_tests/` com casos: determinismo, No‑IO, fuel, tipos, ghost, CID-chain, canon, assinatura, narrativa
- `tests/examples/deny_age` com `expected.rc.cid` definido

**Checklist**
- [ ] 10/10 leis verdes no CI
- [ ] Reexecução em máquinas diferentes mantém CIDs
- [ ] Narrativa mínima para denies

**Critérios de aceite**
- CI verde; reports legíveis (falhas mostram diff binário onde aplicável)
