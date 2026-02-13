# ubl-runtime — Blueprint

## Papel no ecossistema

* **Orquestrador de execução de chips** (determinístico, idempotente, auditável).
* **Aplica política (ubl-policy)** antes/durante/depois do run.
* **Isola execução** (sandbox) e **produz recibos** (ubl-receipt), persiste bytes no **ledger** com CID (ai-nrf1 como formato de bytes).
* **Conecta-se ao transport (SIRP)** para receber/entregar mensagens e ao **gate** para entrada/saída HTTP.
* **Fala “rho”**: resolve nomes/rotas/versionamento de chips/pipes (descoberta e binding).

## O que é / o que não é

* É: executor/scheduler, sandbox, aplicador de política, gerador de recibos, ponte capsule↔bytes.
* Não é: o **gate** (fronteira HTTP), nem **policy store** (regras são insumo), nem **vector store**, nem “agente autônomo”.

## Fractal: byte → semântica → política

1. **Byte**: tudo começa/termina em **ai-nrf1** (1 byte-stream → 1 mensagem → 1 CID).
2. **Semântica**: cápsula JSON-Atomic (ubl-capsule) + manifests/gramáticas (UBL PURE).
3. **Política**: decisões (ALLOW/DENY/ESCALATE) por **ubl-policy**, com **state verbs** e **ghost records**.

## Invariantes LLM-first

* **Write-ahead + write-after**: recibo antes (intenção) e depois (resultado).
* **Audit trail por recibos**: cada transição de estado tem **CID** e **DID**.
* **Privacy por cápsulas**: payload selado; só metadados mínimos no recibo.
* **Determinismo**: mesma entrada ⇒ mesmos bytes ⇒ mesmo CID.

---

## MVP — responsabilidades

1. **Scheduler mínimo (FIFO)** com filas nomeadas por chip/rho.
2. **Sandbox** (processo/WSL/wasmtime) com orçamento (CPU/mem/tempo/IO).
3. **Roteamento rho**: `rho://org/prod/chip@v` → resolve para **manifest** e binário/handler.
4. **Execução runtime**: parse → policy (pre) → run → policy (post) → render.
5. **Recibos**:

   * **pre**: intenção, entradas, cápsulas referenciadas (CID/URL/DID), política avaliada.
   * **post**: saída, sub-recibos, medições (tempo, bytes), decisão final.
   * ambos JWS + DID, guardados no ledger (S3/FS).
6. **Transport**: consumir/emitir via SIRP (opcional no MVP; stub com fila local).
7. **Observabilidade**: logs estruturados + metrics (exec_scheduled, exec_started, exec_done, policy_denied, sandbox_kill).

---

## Interfaces (MVP)

### 1. Runtime API (interna; o gate chama)

* `POST /runtime/submit`

  ```json
  {
    "chip": "rho://org/prod/echo@1",
    "capsule": {"type":"json-atomic","payload":{"hello":"world"}},
    "policy": {"name":"allow-all@1"},
    "options": {"ttl_ms": 30000, "priority": 0}
  }
  ```

  **200** `{ "job_id":"...", "pre_receipt_cid":"b3:..." }`

* `GET /runtime/job/:id`
  **200** `{ "status":"queued|running|done|denied|error", "post_receipt_cid": "b3:..." }`

* `POST /runtime/cancel/:id` → tentativa best-effort (sandbox kill).

### 2. Worker ABI (sandbox)

* **stdin**: ai-nrf1 bytes da cápsula (ou `capsule.json` em arquivo).
* **stdout**: ai-nrf1 bytes de saída ou `result.json`.
* **env**: `RHO=...`, `JOB_ID=...`, `POLICY_HINT=...`.

### 3. Rho Resolver

* `resolve(chip_ref) -> {manifest_cid, entry, resources, grammars}`
  Cache local + carimbo de tempo; determinístico por CID.

---

## Modelo de estados e **state verbs**

Estados do job: `QUEUED → RUNNING → (DONE | DENIED | ERROR | KILLED)`
Verbos canon (persistidos em recibos):

* `propose` (pre-recibo)
* `approve|deny` (política)
* `execute` (run)
* `emit` (render/transport)
* `archive` (finalização)

**Ghost records**: entradas “sombreadas” apenas com CIDs, sem payload em claro; usadas para reconstrução/verificação.

---

## Segurança & Política

* **Pre-policy** decide se pode executar (contexto: quem chamou, alvo rho, tamanho, tags).
* **Post-policy** decide se pode **emitir**/persistir saída (ex.: redigir campos).
* **Capsule privacy**: payload cifrado opcional; runtime só vê quando permitido.
* **Sandbox budgets**: tempo (wall/cpu), memória, open-files, rede (on/off).

---

## Integrações

* **ai-nrf1**: encode/decode de payloads + hashing para CID.
* **ubl-receipt**: JWS Ed25519 + DID (did:key, chave publicada).
* **ubl-ledger**: put/get por CID (FS/S3).
* **ubl-gate**: frontdoor HTTP que delega para o runtime.
* **ubl-policy**: DSL/engine chamável (lib ou sidecar).

---

## Layout de repositório (sugestão)

```
crates/
  ubl_runtime/
    src/
      lib.rs            // new_app(), wiring
      runtime.rs        // scheduler, state, job store
      sandbox.rs        // executor isolado
      rho.rs            // resolver
      engine.rs         // glue parse/policy/run/render
      policy.rs         // client para ubl-policy
      receipts.rs       // pre/post JWS
      transport.rs      // SIRP stub
      metrics.rs        // counters, histos
    tests/
      race_card.rs      // E2E com gate opcionalmente embutido
      determinism.rs
    Cargo.toml
services/
  ubl_gate/             // já existe (continua chamando o runtime)
```

---

## Make & scripts

**Makefile**

```
run:        cargo run -p ubl_gate
test:       cargo test --workspace
race:       cargo test -p ubl_runtime --test race_card -- --nocapture
lint:       cargo clippy --workspace -- -D warnings
fmt:        cargo fmt --all
```

**.env**

```
UBL_STORE_DIR=./store
UBL_POLICY_URL=http://localhost:3100
UBL_TRANSPORT_URL=stub://local
RHO_REGISTRY=./registry
```

---

## Testes essenciais

1. **Determinismo**: 20 execuções idênticas → 1 CID.
2. **Pre-deny**: política nega antes do run → estado `DENIED`, recibo pós ausente.
3. **Post-deny**: run ok, política nega emissão → recibo pós com `decision=DENY`, sem output em claro.
4. **Sandbox budget**: script que excede CPU/mem → `KILLED` + recibo com motivo.
5. **Capsule privacy**: cápsula cifrada; runtime sem chave não lê, mas encadeia CIDs; com chave, decode ok.
6. **Ghost record**: somente CIDs referenciados; reconstrução posterior bate.
7. **Rho binding**: `rho://x/y/chip@1` resolve fixamente para manifest CID; upgrade só via novo ref.
8. **Idempotência**: reenvio de job com mesmo `capsule_cid` + `chip_ref` retorna mesmo resultado (sem duplicar efeitos).
9. **Race card completo** via gate.

---

## Roadmap curto (3 sprints)

* **S1 (MVP exec)**: scheduler FIFO, sandbox processo, pre/post policy, recibos, ledger, testes 1–4.
* **S2 (Privacy & Rho)**: cápsulas cifradas, ghost records, resolver rho com cache e verificação, testes 5–7.
* **S3 (Resiliência & Ops)**: idempotência forte, replays controlados, metrics + dashboards, healthz/readyz, testes 8–9.

---

## “Why we trust” (resumo runtime)

* **Determinismo + CIDs**: bytes canônicos (ai-nrf1) → hash imutável.
* **Recibos JWS+DID**: cada decisão/resultado é assinado e verificável.
* **Política aplicável em dois tempos**: evita “efeitos colaterais” indevidos.
* **Sandbox com orçamentos**: limites objetivos de risco operacional.
* **Trilha completa**: write-before/after e ghost records reconstroem a história.

Se quiser, eu já escrevo o esqueleto `crates/ubl_runtime` com `runtime.rs`, `sandbox.rs`, `rho.rs`, `engine.rs`, `receipts.rs`, testes de determinismo e o `race_card.rs` rodando contra o gate atual.
