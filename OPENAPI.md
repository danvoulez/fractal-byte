# UBL • OpenAPI (Esqueleto 3.1)

> Escopo: endpoints públicos do **ubl-gate** e utilitários de leitura (CID/DID/recibo).
> Nota: caminhos com `:cid` usam sintaxe do roteador (matchit/axum).

```yaml
openapi: 3.1.0
info:
  title: UBL Gate API
  version: "1.0.0"
  description: >
    Edge/API do Universal Business Ledger. Tudo é chip (exceto o gate).
    Recibos são a imagem/persistência/linha do tempo (CID/DID/JWS).

servers:
  - url: https://api.ubl.foundation
  - url: http://localhost:3000

paths:
  /v1/ingest:
    post:
      summary: Create Write-Ahead (WA) e opcionalmente certificar (recibo antecipado)
      operationId: postIngest
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/IngestRequest" }
      responses:
        "200":
          description: WA aceito
          content:
            application/json:
              schema: { $ref: "#/components/schemas/IngestResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "409": { description: CID já existe (idempotente) }

  /v1/execute:
    post:
      summary: Executa (parse → policy → render), grava WF e recibo JWS
      operationId: postExecute
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/ExecuteRequest" }
      responses:
        "200":
          description: Execução concluída
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ExecuteResponse" }
        "403": { description: Policy DENY com recibo }
        "400": { $ref: "#/components/responses/BadRequest" }

  /v1/receipt/:cid:
    get:
      summary: Retorna recibo JWS (ALLOW/DENY) com narrativa LLM-first
      operationId: getReceipt
      parameters:
        - { name: cid, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: Recibo JWS (application/jose)
          content:
            application/jose:
              schema: { type: string, description: "Compact/JSON JWS" }
        "404": { $ref: "#/components/responses/NotFound" }

  /cid/:cid:
    get:
      summary: Retorna bytes NRF (ρ) do objeto
      operationId: getCidRaw
      parameters:
        - { name: cid, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: Bytes NRF
          content:
            application/x-nrf:
              schema: { type: string, format: binary }
        "404": { $ref: "#/components/responses/NotFound" }

  /cid/:cid.json:
    get:
      summary: Visualização JSON do NRF (decode); fallback base64 se falhar
      operationId: getCidJson
      parameters:
        - { name: cid, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: View JSON
          content:
            application/json:
              schema: { $ref: "#/components/schemas/JsonView" }
        "404": { $ref: "#/components/responses/NotFound" }

  /.well-known/did.json:
    get:
      summary: DID Document do emissor (verifying key)
      operationId: getWellKnownDid
      responses:
        "200":
          description: DID Document
          content:
            application/json:
              schema: { $ref: "#/components/schemas/DidDocument" }

  /v1/search/receipts:
    get:
      summary: Busca recibos por filtros (cid, actor, time, policy, space)
      operationId: searchReceipts
      parameters:
        - { name: cid, in: query, required: false, schema: { type: string } }
        - { name: actor, in: query, required: false, schema: { type: string } }
        - { name: space, in: query, required: false, schema: { type: string } }
        - { name: policy, in: query, required: false, schema: { type: string, enum: ["ALLOW","DENY"] } }
        - { name: time_from, in: query, required: false, schema: { type: string, format: date-time } }
        - { name: time_to, in: query, required: false, schema: { type: string, format: date-time } }
      responses:
        "200":
          description: Lista paginada de recibos
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ReceiptSearchResponse" }

components:
  responses:
    BadRequest:
      description: Erro de validação
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    NotFound:
      description: Recurso não encontrado
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }

  schemas:
    IngestRequest:
      type: object
      required: [payload]
      properties:
        payload: { description: "JSON a encapsular em NRF", type: object }
        certify: { type: boolean, default: false, description: "Se true, emite recibo antecipado" }
        space: { type: string }
        actor: { type: string }
    IngestResponse:
      type: object
      required: [cid, url, content_type, bytes_len]
      properties:
        cid: { type: string }
        did: { type: string }
        url: { type: string }
        receipt_url: { type: string }
        content_type: { type: string, example: "application/x-nrf" }
        bytes_len: { type: integer }

    ExecuteRequest:
      type: object
      required: [cid]
      properties:
        cid: { type: string, description: "CID previamente ingerido (WA)" }
        policy_set: { type: string, description: "Conjunto de políticas a aplicar" }
        vars: { type: object, additionalProperties: true, description: "Variáveis para binding D8" }
    ExecuteResponse:
      type: object
      required: [cid, decision, receipt_url]
      properties:
        cid: { type: string }
        decision: { type: string, enum: ["ALLOW", "DENY"] }
        receipt_url: { type: string }

    JsonView:
      type: object
      properties:
        decoded: { type: object, description: "JSON decodificado do NRF" }
        nrf_base64: { type: string, description: "Base64 do payload bruto (fallback)" }
        note: { type: string }

    Receipt:
      type: object
      required: [jws, narrative, links]
      properties:
        jws: { type: string, description: "Assinatura JWS compact ou JSON" }
        narrative:
          type: object
          properties:
            kind: { type: string, example: "ubl-receipt" }
            version: { type: string, example: "1.0.0" }
            time: { type: string, format: date-time }
            actor: { type: string }
            space: { type: string }
            rho: { type: object, additionalProperties: true }
            decision: { type: string, enum: ["ALLOW","DENY"] }
            story: { type: string, description: "Resumo LLM-first do que ocorreu" }
        links:
          type: object
          properties:
            preimage_cid: { type: string }
            write_ahead_cid: { type: string }
            write_after_cid: { type: string }

    DidDocument:
      type: object
      properties:
        id: { type: string }
        verificationMethod:
          type: array
          items:
            type: object
            properties:
              id: { type: string }
              type: { type: string, example: "Ed25519VerificationKey2020" }
              controller: { type: string }
              publicKeyMultibase: { type: string }

    ReceiptSearchResponse:
      type: object
      properties:
        items:
          type: array
          items: { $ref: "#/components/schemas/Receipt" }
        next_page_token: { type: string, nullable: true }

    Error:
      type: object
      properties:
        code: { type: string }
        message: { type: string }
        details: { type: object }
```
