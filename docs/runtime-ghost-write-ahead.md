
# UBL Runtime — Ghost Records & Write-Ahead/Write-After (Addendum)

Status: PROPOSED • Last updated: 2026-02-13T125512Z

This addendum canonically defines **Ghost Records** and **Write-Ahead / Write-After** for the UBL runtime. It is LLM-first and fully auditable.

## 1) Definições
- **Ghost record**: registro **auditável** mas **não publicável** por padrão. Vai ao trilho de auditoria, aparece no recibo, mas é filtrado na publicação/telemetria externa (ou encapsulado).
- **Write-Ahead (WA)**: materialização **antes** do `execute` — fixa a **intenção** e o **preimage** do run (inputs, chip, política e ambiente).
- **Write-After (WF)**: materialização **depois** do `execute` — fixa **resultado/decisão** (ALLOW/DENY), artefatos e efeitos.

**Invariante**: todo `execute` válido produz **exatamente 2** registros NRF (WA, WF) + **1 recibo** encadeando os dois.

## 2) Modelo canônico (NRF/ρ) para WA/WF
Campos mínimos compartilhados (diferenças em `phase` e payloads):

```json
{
  "type": "ubl/run",
  "version": "1",
  "phase": "before|after",
  "ghost": true,
  "ident": { "url": "…", "cid": "b3:…", "did": "did:cid:…" },
  "chip": { "cid": "b3:…", "schema": "url" },
  "policy_ref": { "id": "url", "rev": "…" },
  "runtime": { "name": "ubl-runtime", "version": "…" },
  "bind": { "strategy": "name|1to1|explicit", "map": { "raw_b64": "input_data" } },
  "inputs": { "vars": { "…" }, "capsules": ["cid…"] },
  "env": { "caller": "did:…", "ts": "2026-02-13T12:40:00Z" },
  "links": {
    "wa_cid": "b3:…",
    "prev": "b3:…",
    "receipt": null
  },
  "artifacts": {
    "input_cid": "b3:…",
    "output_cid": null,
    "sub_receipts": []
  },
  "note": "texto livre curto"
}
```

### Diferenças específicas
- **WA (`phase=before`)**: `output_cid=null`, `decision=null`, `links.receipt=null`.
- **WF (`phase=after`)**: adiciona `decision: "ALLOW|DENY"`, `reason`, `rule_id`, `output_cid` (se ALLOW), e `links.receipt="b3:…"` após assinar o recibo.

## 3) Recibo encadeado (conta a história)
O **recibo** referencia **ambos**:

```json
{
  "type":"ubl/receipt","version":"1",
  "wa_cid":"b3:<WA>",
  "wf_cid":"b3:<WF>",
  "policy":{"decision":"ALLOW|DENY","rule_id":"…","reason":"…"},
  "timeline":[
    {"t":"…","verb":"INGEST"},
    {"t":"…","verb":"PARSE"},
    {"t":"…","verb":"POLICY","decision":"…"},
    {"t":"…","verb":"RENDER"}
  ],
  "artifacts":{"input_cid":"b3:…","output_cid":"b3:…","sub_receipts":[]},
  "ident":{"url":"…","cid":"b3:…","did":"did:cid:…"},
  "ghost": false,
  "jws":"<EdDSA>"
}
```

## 4) Regras de sistema
1) **Sempre WA→exec→WF**; se `exec` falha, ainda assim gera **WF** (`DENY` com `reason="RUNTIME_ERROR:…"`, `output_cid=null`).
2) **Ghost por padrão**: WA e WF `ghost:true`. O **recibo** é público (salvo política/cápsula).
3) **Imutabilidade**: WA/WF/recibo são bytes-endereçados (**CID**). Nada de update; só encadeamento.
4) **Determinismo**: mesma entrada → mesmos bytes WA e WF/recibo (campos de tempo são observabilidade).
5) **Privacidade**: trechos sensíveis via **ubl-capsule**. Recibo guarda prova/ponteiro, não conteúdo.

## 5) Contratos por componente
- **ubl-gate**: gera WA (ghost), chama runtime, recebe `wf_cid`+`receipt_cid`. View JSON entende WA/WF (fallback base64).
- **ubl-runtime**: `bind → parse → policy → (render|deny)`; materializa WF (ghost) + recibo assinado.
- **ubl-ledger**: `put/get/exists` NRF bytes.
- **ubl-receipt**: assina/verifica JWS; expõe claims canônicos.
- **ubl-policy**: pura, sem efeitos colaterais.
- **ubl-capsule**: wrap/unwrap com prova/ponteiro em WA/WF/recibo.

## 6) Testes de conformidade
1) Encadeamento WA/WF/REC correto.
2) Determinismo (mesma entrada → mesmos CIDs, exceto tempo).
3) DENY por política.
4) Falha de runtime ainda produz WF (`DENY`).
5) Ghost por padrão (WA/WF).
6) Capsule com prova/ponteiro.
7) Narrativa LLM-first a partir dos campos.

## 7) Exemplos

**WA (ghost)**
```json
{
  "type":"ubl/run","version":"1","phase":"before","ghost":true,
  "chip":{"cid":"b3:…"},
  "bind":{"strategy":"1to1","map":{"raw_b64":"input_data"}},
  "inputs":{"vars":{"input_data":"aGVsbG8="}},
  "env":{"caller":"did:key:…","ts":"2026-02-13T12:41:05Z"},
  "artifacts":{"input_cid":"b3:…","output_cid":null},
  "links":{"wa_cid":"b3:…","prev":null,"receipt":null}
}
```

**WF (ghost)**
```json
{
  "type":"ubl/run","version":"1","phase":"after","ghost":true,
  "decision":"DENY","reason":"RULE_X_BLOCKED","rule_id":"RULE_X",
  "artifacts":{"input_cid":"b3:…","output_cid":null,"sub_receipts":[]},
  "links":{"wa_cid":"b3:<WA>","receipt":"b3:<REC>"}
}
```

---

Implementation notes:
- Schemas: `schemas/run.schema.json` deve validar WA/WF.
- Runtime: `write_ahead(ctx)->wa_cid`; `write_after(ctx, decision, output?)->wf_cid`; `make_receipt(wa_cid,wf_cid,decision,…)`.
- Gate: views WA/WF (`.json`) com fallback base64.
- Ledger: testes de stress de `put/get`.
