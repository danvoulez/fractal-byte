# API: `POST /v1/execute` com `engine:"rb"` no ubl_gate

> Data: 2026-02-13

# API: `POST /v1/execute` (engine=rb)

**Resumo**
Exposição pública para executar chips RB determinísticos via Gate.

**Request**
```json
{"engine":"rb","chip_b64":"...","inputs":["b3:..."],"ghost":false,"fuel":50000}
```
**Response**
```json
{"rc_cid":"b3:...","fuel_used":123,"steps":37}
```

**Checklist**
- [ ] Validação de base64 e limites (fuel)
- [ ] Ghost propaga para RC
- [ ] Métricas HTTP + por opcode

**Critérios de aceite**
- Smoke test com chip deny <18
