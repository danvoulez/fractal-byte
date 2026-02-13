# Providers: Canon plugável + CAS FS (BLAKE3) + Signer Ed25519 (ENV/arquivo)

> Data: 2026-02-13

# Providers: Canon + CAS FS + Signer

**Resumo**
Pluga `CanonProvider` (provisório `NaiveCanon`), CAS filesystem com BLAKE3 e signer Ed25519 via ENV/arquivo.

**Mudanças**
- `canon::{CanonProvider, NaiveCanon}`
- `providers::cas_fs::FsCas`
- `providers::sign_env::EnvSigner`

**Checklist**
- [ ] JsonNormalize usa o provider de canon
- [ ] FsCas grava em `./.cas/<shard>/<resto>`
- [ ] EnvSigner expõe `kid` e assina bytes (MVP)

**Critérios de aceite**
- Runner end‑to‑end gera `rc_cid` com `FsCas`
- Sem panics; falhas retornam `ExecError`

**Plano de rollout**
- Substituir `NaiveCanon` pela canon NRF oficial no PR seguinte

**Riscos**
- Canon provisória divergir do NRF final → cobrir com testes de conformance
