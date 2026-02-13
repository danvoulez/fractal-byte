# UBL • OpenAPI (Esqueleto 3.1)

> Escopo: **todos** os endpoints públicos do **ubl-gate** (`:3000`).
> Caminhos com `:cid` usam sintaxe do roteador (matchit/axum).
> Este documento reflete **exatamente** o que está implementado no código.
>
> **Receipt-first**: every mutation returns receipts (WA → Transition → WF).
> Failures produce DENY WF receipts (HTTP 200), never 422/500.
> Replay of identical inputs returns HTTP 409 CONFLICT.

```yaml
openapi: 3.1.0
info:
  title: UBL Gate API
  version: "0.1.0"
  contact:
    email: dan@ubl.agency
  description: >
    Edge/API do Universal Business Ledger. Tudo é chip (exceto o gate).
    Recibos são a imagem/persistência/linha do tempo (CID/DID/JWS).

servers:
  - url: http://localhost:3000
    description: Local dev

paths:

  # ── Health ───────────────────────────────────────────────────────
  /healthz:
    get:
      summary: Liveness check
      operationId: getHealthz
      responses:
        "200":
          description: Serviço ativo
          content:
            application/json:
              schema:
                type: object
                properties:
                  ok: { type: boolean, example: true }

  # ── Ingest ──────────────────────────────────────────────────────
  /v1/ingest:
    post:
      summary: Encapsula JSON em NRF, grava no ledger, opcionalmente emite recibo
      operationId: postIngest
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/IngestRequest" }
      responses:
        "200":
          description: Conteúdo aceito e gravado
          content:
            application/json:
              schema: { $ref: "#/components/schemas/IngestResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }

  # ── Execute (runtime) ──────────────────────────────────────────
  /v1/execute:
    post:
      summary: Receipt-first execute — manifest (parse → policy → render) via ubl_runtime
      operationId: postExecute
      security:
        - bearerAuth: []
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/ExecuteRequest" }
      responses:
        "200":
          description: >
            Execution completed. On success: decision=ALLOW.
            On failure (policy deny, codec error): decision=DENY with WF receipt.
            Both cases return 200 with full receipt chain.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ExecuteResponse" }
        "409":
          description: Duplicate request (idempotency replay). Same pipeline+inputs already executed.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Error" }
        "415":
          description: Missing or wrong Content-Type (must be application/json)
        "401":
          description: Missing or invalid Bearer token (when auth enabled)

  # ── Execute RB-VM (chip) ──────────────────────────────────────
  /v1/execute/rb:
    post:
      summary: Executa chip RB-VM determinístico (TLV bytecode + fuel metering)
      operationId: postExecuteRb
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/ExecuteRbRequest" }
      responses:
        "200":
          description: Chip executado com sucesso
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ExecuteRbResponse" }
        "400":
          description: Base64 inválido ou chip malformado
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Error" }
        "422":
          description: Falha de execução (policy deny, fuel exhausted, type mismatch)
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ExecuteError" }

  # ── Transition Receipt ────────────────────────────────────────
  /v1/transition/{cid}:
    get:
      summary: Retorna o Transition Receipt (RB→rho) pelo CID do recibo ou rho_cid
      operationId: getTransition
      parameters:
        - name: cid
          in: path
          required: true
          schema: { type: string }
          description: CID do transition receipt ou rho_cid
      responses:
        "200":
          description: Transition receipt envelope (body + proof JWS detached)
          content:
            application/json:
              schema: { $ref: "#/components/schemas/TransitionEnvelope" }
        "404":
          description: Transition receipt não encontrado
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Error" }

  # ── Certify ────────────────────────────────────────────────────
  /v1/certify:
    post:
      summary: Emite recibo JWS para um CID já ingerido
      operationId: postCertify
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [cid]
              properties:
                cid: { type: string, description: "CID do conteúdo a certificar" }
      responses:
        "200":
          description: Recibo emitido
          content:
            application/json:
              schema:
                type: object
                properties:
                  receipt: { type: string, description: "JWS compact (header.payload.signature)" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "404": { description: "Conteúdo não encontrado no ledger" }

  # ── Receipt ────────────────────────────────────────────────────
  /v1/receipt/:cid:
    get:
      summary: Retorna recibo JWS para um CID
      operationId: getReceipt
      parameters:
        - { name: cid, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: Recibo JWS
          content:
            application/jose+json:
              schema: { type: string, description: "JWS compact" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "404": { $ref: "#/components/responses/NotFound" }

  # ── Resolve ────────────────────────────────────────────────────
  /v1/resolve:
    post:
      summary: Resolve um DID ou CID para seus links e metadados
      operationId: postResolve
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [id]
              properties:
                id: { type: string, description: "did:cid:... ou CID string" }
      responses:
        "200":
          description: Documento resolvido
          content:
            application/json:
              schema:
                type: object
                properties:
                  id: { type: string }
                  links:
                    type: array
                    items: { type: string }

  # ── CID raw bytes ──────────────────────────────────────────────
  /cid/:cid:
    get:
      summary: Retorna bytes NRF (ρ) do objeto; sufixo .json retorna view JSON
      operationId: getCid
      parameters:
        - { name: cid, in: path, required: true, schema: { type: string }, description: "CID puro ou CID.json" }
      responses:
        "200":
          description: Bytes NRF (sem .json) ou JSON view (com .json)
          content:
            application/x-nrf:
              schema: { type: string, format: binary }
            application/json:
              schema: { $ref: "#/components/schemas/JsonView" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "404": { $ref: "#/components/responses/NotFound" }

  # ── DID Document ───────────────────────────────────────────────
  /.well-known/did.json:
    get:
      summary: DID Document do emissor (Ed25519 verifying key)
      operationId: getWellKnownDid
      responses:
        "200":
          description: DID Document
          content:
            application/json:
              schema: { $ref: "#/components/schemas/DidDocument" }

# ══════════════════════════════════════════════════════════════════
components:
  responses:
    BadRequest:
      description: Erro de validação (ρ violation, CID inválido, JSON malformado)
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    NotFound:
      description: Recurso não encontrado no ledger
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }

  schemas:

    # ── Ingest ──────────────────────────────────────────────────
    IngestRequest:
      type: object
      required: [payload]
      properties:
        payload: { type: object, description: "JSON a encapsular em NRF (ρ-validated)" }
        certify: { type: boolean, default: false, description: "Se true, emite recibo JWS imediatamente" }
    IngestResponse:
      type: object
      required: [cid, did, bytes_len, content_type, url, receipt_url]
      properties:
        cid: { type: string, example: "bafkrei..." }
        did: { type: string, example: "did:cid:bafkrei..." }
        bytes_len: { type: integer }
        content_type: { type: string, example: "application/x-nrf" }
        url: { type: string, example: "http://localhost:3000/cid/bafkrei..." }
        receipt_url: { type: string, example: "http://localhost:3000/v1/receipt/bafkrei..." }

    # ── Execute ─────────────────────────────────────────────────
    ExecuteRequest:
      type: object
      required: [manifest, vars]
      properties:
        manifest:
          $ref: "#/components/schemas/Manifest"
        vars:
          type: object
          additionalProperties: true
          description: "Variables for binding (BTreeMap<String, Value>)"
        ghost:
          type: boolean
          default: false
          description: "Ghost mode: receipts emitted with observability.ghost=true, ledger writes skipped"
    ExecuteResponse:
      type: object
      required: [tip_cid, decision, ghost, receipts]
      properties:
        tip_cid: { type: string, description: "b3:<hex64> — CID of the WF receipt body (chain tip)" }
        decision: { type: string, enum: [ALLOW, DENY], description: "Pipeline decision" }
        ghost: { type: boolean, description: "Whether this was a ghost-mode execution" }
        receipts:
          type: object
          required: [wa, wf]
          properties:
            wa: { $ref: "#/components/schemas/Receipt" }
            transition: { $ref: "#/components/schemas/Receipt" }
            wf: { $ref: "#/components/schemas/Receipt" }

    # ── Execute RB-VM ───────────────────────────────────────────
    ExecuteRbRequest:
      type: object
      required: [chip_b64, inputs]
      properties:
        chip_b64: { type: string, description: "TLV bytecode do chip, codificado em base64" }
        inputs:
          type: array
          items: { type: object }
          description: "JSON values que serão gravados no CAS como inputs do chip"
        ghost: { type: boolean, default: false, description: "Se true, RC sai com ghost:true" }
        fuel: { type: integer, default: 50000, description: "Limite de fuel (cada opcode debita 1+)" }
    ExecuteRbResponse:
      type: object
      required: [steps, fuel_used]
      properties:
        rc_cid: { type: string, nullable: true, description: "b3:<hex64> — CID do Receipt emitido (null se chip não emitiu EmitRc)" }
        steps: { type: integer, description: "Número de instruções executadas" }
        fuel_used: { type: integer, description: "Fuel consumido" }
        transition_receipt:
          nullable: true
          $ref: "#/components/schemas/TransitionEnvelope"

    # ── Transition Receipt ─────────────────────────────────────
    TransitionEnvelope:
      type: object
      required: [cid, body, proof]
      properties:
        cid: { type: string, description: "b3:<hex64> — CID canônico do corpo do transition receipt" }
        body:
          type: object
          required: [t, v, from_layer, to_layer, op, preimage_raw_cid, rho_cid, witness]
          properties:
            t: { type: string, const: "ubl/transition" }
            v: { type: string, const: "1" }
            from_layer: { type: string, example: "-1:rb" }
            to_layer: { type: string, example: "0:rho" }
            op: { type: string, example: "rho.normalize@ai-nrf1/v1" }
            preimage_raw_cid: { type: string, pattern: "^b3:[0-9a-f]{64}$" }
            rho_cid: { type: string, pattern: "^b3:[0-9a-f]{64}$" }
            witness:
              type: object
              properties:
                vm: { type: string, example: "rb-vm@0.1.0" }
                bytecode_cid: { type: string, nullable: true }
                fuel_spent: { type: integer, nullable: true }
            ghost: { type: boolean, nullable: true }
            parents: { type: array, items: { type: string }, default: [] }
        proof:
          $ref: "#/components/schemas/JwsDetached"
    JwsDetached:
      type: object
      required: [protected, signature, kid]
      description: "JWS detached (RFC 7797, b64=false). Payload = canonical body bytes (não embarcado)."
      properties:
        protected: { type: string, description: "Base64url do header protegido (alg:EdDSA, b64:false, typ:ubl/rc+json)" }
        signature: { type: string, description: "Base64url da assinatura Ed25519" }
        kid: { type: string, description: "DID key identifier", example: "did:dev#k1" }

    # ── Manifest (runtime) ──────────────────────────────────────
    Manifest:
      type: object
      required: [pipeline, in_grammar, out_grammar, policy]
      properties:
        pipeline: { type: string }
        in_grammar:
          $ref: "#/components/schemas/Grammar"
        out_grammar:
          $ref: "#/components/schemas/Grammar"
        policy:
          type: object
          properties:
            allow: { type: boolean }
    Grammar:
      type: object
      required: [inputs, mappings, output_from]
      properties:
        inputs:
          type: object
          additionalProperties: { type: string }
        mappings:
          type: array
          items:
            type: object
            properties:
              from: { type: string }
              codec: { type: string, description: "base64.decode | (extensível)" }
              to: { type: string }
        output_from: { type: string }

    # ── JSON View ───────────────────────────────────────────────
    JsonView:
      type: object
      description: "Decode NRF → JSON; fallback base64 se decode falhar"
      properties:
        decoded: { type: object, description: "JSON decodificado do NRF (quando sucesso)" }
        cid: { type: string }
        content_type: { type: string }
        nrf_base64: { type: string, description: "Base64 do payload bruto (fallback)" }
        note: { type: string }

    # ── DID Document ────────────────────────────────────────────
    DidDocument:
      type: object
      properties:
        id: { type: string, example: "did:key:z6Mk..." }
        verificationMethod:
          type: array
          items:
            type: object
            properties:
              id: { type: string }
              type: { type: string, example: "Ed25519VerificationKey2020" }
              controller: { type: string }
              publicKeyMultibase: { type: string }
        assertionMethod:
          type: array
          items: { type: string }

    # ── Receipt (unified envelope) ─────────────────────────────
    Receipt:
      type: object
      required: [t, parents, body, body_cid, proof]
      properties:
        t: { type: string, enum: ["ubl/wa", "ubl/transition", "ubl/wf", "ubl/attestation"] }
        parents: { type: array, items: { type: string }, description: "CIDs of parent receipts (chaining)" }
        body: { type: object, description: "Stage-specific content" }
        body_cid: { type: string, pattern: "^b3:[0-9a-f]{64}$" }
        proof: { $ref: "#/components/schemas/JwsDetached" }
        observability:
          nullable: true
          type: object
          properties:
            ghost: { type: boolean, nullable: true }
            logline: { $ref: "#/components/schemas/Logline" }

    # ── Logline (LLM-first observability) ─────────────────────
    Logline:
      type: object
      required: [who, actor_did, what, where, when_iso, why, context_id, version]
      properties:
        who: { type: string, description: "Human-readable actor name" }
        actor_did: { type: string, description: "DID of the acting entity" }
        what: { type: string, description: "Stage-specific action label" }
        where: { type: string, description: "Execution context location" }
        when_iso: { type: string, format: date-time, description: "ISO 8601 timestamp" }
        why: { type: string, description: "Reason for the action" }
        context_id: { type: string, description: "Correlation ID for tracing" }
        version: { type: string, example: "0.1.0" }

    # ── Error ───────────────────────────────────────────────────
    Error:
      type: object
      properties:
        error: { type: string }
        detail: { type: string }

  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      description: >
        Bearer token authentication. Disabled by default in dev (UBL_AUTH_DISABLED=1).
        Public paths (/healthz, /.well-known/did.json) do not require auth.
        Dev token: ubl-dev-token-001
```

---

## Compatibility

All receipt-first endpoints follow these invariants:

- **Every mutation returns receipts** — WA, Transition, WF chain
- **Failures produce DENY WF receipts** (HTTP 200), never 422/500
- **Replay returns 409 CONFLICT** — idempotency via `pipeline:inputs_raw_cid`
- **Ghost mode** — `ghost: true` in request → `observability.ghost: true` on all receipts, ledger skipped
- **Strong chaining** — `parents[0] == prev_tip` when chain exists
- **Schema validated** — every receipt passes `validate_receipt()` before return
- **Auth** — Bearer token required when `UBL_AUTH_DISABLED != 1`
- **Edge protections** — 1 MiB body limit, 30s timeout, `application/json` required on POST

## Cobertura

| Endpoint | Método | Implementado | Testado |
|----------|--------|:---:|:---:|
| `/healthz` | GET | ✅ | ✅ |
| `/v1/ingest` | POST | ✅ | ✅ |
| `/v1/execute` | POST | ✅ | ✅ |
| `/v1/execute/rb` | POST | ✅ | ✅ |
| `/v1/transition/:cid` | GET | ✅ | ✅ |
| `/v1/certify` | POST | ✅ | ✅ |
| `/v1/receipt/:cid` | GET | ✅ | ✅ |
| `/v1/resolve` | POST | ✅ | ✅ |
| `/cid/:cid` | GET | ✅ | ✅ |
| `/cid/:cid.json` | GET | ✅ | ✅ |
| `/.well-known/did.json` | GET | ✅ | ✅ |
