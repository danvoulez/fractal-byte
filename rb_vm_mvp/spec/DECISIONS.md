# DECISIONS.md (RB-VM / Fractal)

- D1: Canon JSON único compartilhado com NRF-1.1 (ordenar chaves por codepoint, NFC, números i64 quando exatos; decimais canonizados como string; proibir NaN/Infinity).
- D2: Hash/CAS: BLAKE3 (raw), prefixo textual `b3:` para exibição. (Se migrarmos para CIDv1/sha2-256, atualizar este arquivo e a VM).
- D3: No-IO: a VM só pode interagir via CAS e assinatura (providers determinísticos).
- D4: Fuel: limite padrão 50_000; cada opcode debita custo fixo + por KB quando aplicável.
- D5: Assinatura: JWS Ed25519 sobre NRF(payload), com `kid` apontando para DID publicado em `/.well-known/did.json`.
- D6: Ghost Mode: quando habilitado, o RC sai com `ghost: true`; pode omitir assinatura ou marcar header `simulated`.
