# Spec: RB-VM Decisions + LAWS (canon única, No‑IO, fuel, assinatura, ghost)

> Data: 2026-02-13

# Spec: RB-VM Decisions + LAWS

**Resumo**
Formaliza as decisões iniciais da RB-VM e introduz `LAWS.md` com 10 leis e fixtures base.

**Mudanças**
- `spec/DECISIONS.md`: D1..D6 (canon NRF única; CAS=BLAKE3; No‑IO; fuel; JWS/DID; ghost).
- `spec/LAWS.md`: 10 leis (determinismo, No‑IO, fuel, tipos, ghost, CID-chain, canon, assinatura, narrativa).
- Skeleton de fixtures em `tests/laws/*`.

**Motivação**
Travar contratos e permitir TDD da VM com goldens por CID.

**Checklist**
- [ ] DECISIONS.md revisado (hash/CID unificado)
- [ ] LAWS.md revisado (itens, linguagem, exemplos)
- [ ] Fixtures iniciais aprovadas

**Critérios de aceite**
- CI roda e publica artefatos de fixtures
- Time assina D1..D6 como baseline de arquitetura

**Plano de rollout**
- Merge → abrir issues derivados por lei (testes e goldens)

**Riscos**
- Divergência hash/CID no futuro → mitigar com D2 claramente versionado
