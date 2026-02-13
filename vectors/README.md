# Vectors
Casos de compatibilidade reprodutíveis.

- `hello.bin` → bytes brutos
- `hello.cid` → CIDv1 (raw/sha2-256) em base32 (pref. `cidv1-raw-sha2-256:<b32>` neste MVP)
- `hello.jws.json` → recibo JWS (placeholder)

Como gerar localmente:
```
ubl put vectors/hello.bin
```
