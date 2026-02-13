# Canon: integrar codec/canon NRF‑1.1 real na RB‑VM

> Data: 2026-02-13

# Canon: integrar codec/canon NRF‑1.1 real

**Resumo**
Substitui `NaiveCanon` pelo codec/canon NRF oficial (ordenar por codepoint, NFC, i64/decimal, proibição de NaN/Infinity etc.).

**Mudanças**
- `CanonProvider` agora delega para `ubl_ai_nrf1` (ou crate de canon)
- `JsonNormalize`/`JsonValidate` com validações reais e erros canônicos

**Checklist**
- [ ] Paridade byte‑a‑byte com conformance do NRF
- [ ] 30+ casos de conformance passando
- [ ] Documentação com exemplos

**Critérios de aceite**
- `decode(encode(canon(x))) == canon(x)`
- `cid_of_json` estável (quando aplicado)

**Riscos**
- Retrocompat com fixtures antigos
