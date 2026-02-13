# UBL — Universal Business Ledger • Roadmap

**Versão:** 1.0
**Atualizado:** Fevereiro/2026
**Horizonte:** 12 meses

## Visão

Permitir que qualquer organização adote workflows com IA de forma **segura e transparente**.
UBL é a camada de **governança verificável** para operações de IA — "compliance by default" para dizer **sim** à produtividade sem dizer **sim** ao risco descontrolado.

## Princípios

1. Segurança antes de tudo
2. Valor incremental em cada sprint
3. Guiado por clientes
4. Aberto por padrão (padrões e protocolos)
5. Qualidade de produção em toda release

---

## Arquitetura (resumo executivo)

> **Tudo é um chip, exceto o gate.**
> O **recibo** é a imagem/persistência/linha do tempo (CID/DID/JWS) do que aconteceu.

* **ubl-gate**: API/edge (entrada única)
* **ubl-runtime**: orquestração determinística (parse → policy → render)
* **ubl-chip**: gramáticas + transformação (byte/semântica); "json, capsule, transport **e chip** são radicalmente semelhantes"
* **ubl-receipt**: imagem/persistência/auditoria (CID/DID/JWS + narrativa)
* **ubl-ledger**: armazenamento (S3) dos **bytes NRF** (ρ/rho)
* **ubl-policy**: políticas/decisões (allow/deny); **UBL Policy DSL**
* **ubl-byte (ai-nrf1)**: formato de baixo nível (ρ)
* **ubl-transport (SIRP)**: transporte/entrega determinística
* **ubl-capsule (json-atomic)**: privacidade/escopos (private/shared/org/public)
* **ubl-vector (LLLV)**: semântica vetorial (embeddings por CID)
* **ubl-json (logline)**: protocolo de logs/telemetria

### Propriedades canônicas (comuns)

* `url`, `cid`, `did`
* `kind`, `version`
* `time` (NTP-sourced, padronizado)
* `actor` (humano/sistema), `space` (tenant/projeto)
* `rho` (byte-level), narrativa para LLM
* Canonicalização determinística e **binding D8** (abaixo)

### Binding D8 (determinismo de I/O das gramáticas)

1. Nome idêntico → usa direto
2. Fallback **1↔1** (existe 1 input e 1 var) → bind determinístico
3. Caso contrário → erro claro (lista de faltantes e disponíveis)
   Aplicado no **parse** e no **render**.

---

## Linha do tempo (12 meses)

```
Fev/26        Mai/26 (M6)       Ago/26 (M9)        Nov/26 (M12)
 │               │                 │                   │
 ├─ Day-1         ├─ GA 1.0         ├─ Enterprise        ├─ Platform 2.0
 │  • Skeleton    │  • 99.9% SLA    │  • Federação       │  • Marketplace + Co-pilot
 │  • Core 4 docs │  • 50+ clientes │  • LLLV avançado   │  • Multi-chain/bridge
```

> Datas referenciais (Lisboa): **GA 1.0 ~ Maio/2026**, **Enterprise ~ Agosto/2026**, **Platform 2.0 ~ Novembro/2026**.

---

## Fase 1 — Fundação (Meses 1–3)

**Meta:** Core pronto para produção

### M1.1 • Walking Skeleton ✅

* [x] Gate aceita chip e avalia políticas simples
* [x] **Ghost + Write-Ahead (pré)** gravados no ledger
* [x] **Write-After (pós)** linkado ao WA por `preimage_cid`
* [x] Recibo JWS com **narrativa LLM-first** (história legível por IA)
* [x] **Determinismo D8** (parse e render)
* [x] Persistência local; verificação de CID

### M1.2 • Core API estável

* [ ] **Gate API**:

  * `POST /v1/ingest` (WA), `POST /v1/execute` (run)
  * `GET /v1/receipt/:cid` (JWS + narrativa)
  * `GET /cid/:cid` (NRF raw), `GET /cid/:cid.json` (view JSON)
  * `GET /.well-known/did.json`
* [ ] **Ledger API**: query por CID/timestamp; retenção
* [ ] **Chip API**: validar/assinar/checar gramática
* [ ] **REST + gRPC** (OpenAPI/Protos publicados)
* [ ] **Erros determinísticos** (sem campos voláteis no preimage)

### M1.3 • Policy System v1

* [ ] **UBL Policy DSL v1** (compiler + validator)
* [ ] 10+ templates (PII, exfil, escopos, horário, custo)
* [ ] **Dry-run/simulação** com recibos de simulação
* [ ] **Rollback** versionado com recibos de mudança
* [ ] Integração com Git (deploy por tag assinada)

### M1.4 • Transporte (SIRP)

* [ ] Retries com backoff, delivery receipts
* [ ] **Idempotência por CID** (entrega determinística)
* [ ] Roteamento multi-região

### M1.5 • Ledger S3

* [ ] **WA/WF/Recibos** em buckets regionais
* [ ] Retenção e replicação cruzada
* [ ] Testes de durabilidade/restauração

### M1.6 • CLI/SDK v0.9

* [ ] `ubl` CLI: ingest/run/receipt/verify/search
* [ ] SDKs: Rust, Python, TypeScript
* [ ] Exemplos completos (race card)

#### Métricas (fim da Fase 1)

* ✅ 5 pilotos em produção
* ✅ 100% dos runs com **WA + WF + Recibo**
* ✅ 1.000+ chips/dia
* ✅ 95%+ acurácia de policy
* ✅ p95 < 50ms no gate
* ✅ 0 divergência de CID em replays idênticos

---

## Fase 2 — Escala (Meses 4–6)

**Meta:** Plataforma pronta para enterprise

### M2.1 • Cápsulas (privacidade)

* [ ] **AES-256-GCM** com escopos (private/shared/org/public)
* [ ] **Ghost records** (GDPR) sem quebrar verificação do recibo
* [ ] Unseal audit trail (quem/quando/por quê)
* [ ] 10k ops/s com perf estável

### M2.2 • Policy avançada

* [ ] Regras com **rho** (risk score byte→semântica)
* [ ] Recomendações/Anomalias/Score de risco
* [ ] Simulação A/B com recibos

### M2.3 • LLLV Vector Memory

* [ ] Embeddings indexados por CID (temporal)
* [ ] **Recibo para leituras** (auditoria de retrieval)
* [ ] Enforço de policy no search

### M2.4 • Observabilidade

* [ ] Prometheus/Grafana/Jaeger + ELK
* [ ] Alertas em tempo real

### M2.5 • Alta disponibilidade

* [ ] Multi-região, failover, circuit breakers
* [ ] 99.99% uptime

### M2.6 • Compliance

* [ ] SOC 2 Type II (em andamento)
* [ ] ISO 27001 (em progresso)
* [ ] GDPR/HIPAA docs e automações

#### Métricas (fim da Fase 2)

* ✅ 50+ clientes em produção
* ✅ 100k chips/dia
* ✅ 99.9% uptime
* ✅ p50 < 20ms no gate

---

## Fase 3 — Inteligência (Meses 7–9)

**Meta:** Recursos AI-nativos

### M3.1 • Encadeamento de recibos

* [ ] DAG (parent/child), multi-step
* [ ] Explorer visual

### M3.2 • AI-NRF1

* [ ] **ρ/rho** como camada semântica de missão
* [ ] Autor de policies em linguagem natural
* [ ] Geração de políticas por exemplos
* [ ] Narrativa LLM-first enriquecida

### M3.3 • Predictive Deny

* [ ] Modelos de risco proativos
* [ ] Training contínuo a partir de recibos
* [ ] Explicabilidade

### M3.4 • Receipt Analytics

* [ ] Séries temporais, padrões, BI e export

### M3.5 • Recomendações

* [ ] Otimização de políticas/chips/custos/latência

### M3.6 • Self-healing policies

* [ ] Adaptação automática + A/B
* [ ] Rollout gradual + rollback seguro

#### Métricas (fim da Fase 3)

* ✅ 100+ clientes
* ✅ 1M chips/dia
* ✅ −25% false denies
* ✅ 80% políticas AI-geradas/assistidas
* ✅ p50 < 10ms (com ML)

---

## Fase 4 — Ecossistema (Meses 10–12)

**Meta:** Plataforma e comunidade

* **Federated Gates** (policies federadas, DID cross-org)
* **Policy Marketplace** (templates por indústria)
* **Connectors** (Slack, GitHub, Jira…), Webhooks
* **Developer Platform** (plugins, SDKs de comunidade)
* **AI Co-Pilot** (NL chip creation, incident response)
* **Blockchain Bridge** (anchoring, IPFS/Arweave, DID)

#### Métricas (fim da Fase 4)

* ✅ 500+ clientes
* ✅ 10M chips/dia
* ✅ 1.000+ policies da comunidade
* ✅ 50+ parceiros
* ✅ 100k devs nos SDKs

---

## Segurança & Compliance (sempre-ligados)

* **Chaves/DID**: rotação, HSM/KMS, backup offline; roll-over com `.well-known/did.json`
* **Segredos**: nunca em claro; cápsulas ou secret manager
* **PII/PHI**: classificação, encapsulamento, unseal auditável
* **Logs**: `ubl-json (logline)` com redução de sensíveis por padrão
* **Retenção**: políticas por escopo; recibos de purge
* **Threat model**: MITM, replay, supply-chain; mitigação documentada
* **Determinismo observável**: qualquer não-determinismo → observável e negado por padrão

---

## Não-metas

* ❌ "Agentes autônomos" executando fora do gate
* ❌ Execução não-determinística sem observabilidade
* ❌ Persistir dados sensíveis sem cápsula/escopo

---

## Critérios de aceite (por release)

* **Determinismo**: `N` replays idênticos ⇒ **mesmo CID** e **mesmos bytes**
* **Auditoria total**: 100% dos runs têm **WA + WF + Recibo**
* **Políticas**: templates cobrem casos comuns; simulações com recibos
* **Contrato**: OpenAPI/Protos congelados e migração documentada

---

## Riscos & Mitigação

* **Churn de CID** por mapeamentos → **Binding D8** + testes dourados de CID
* **Latência com S3/cripto** → conexão pool, cache de verificação, I/O assíncrono
* **Crescimento de recibos** → partição temporal, TTL por escopo, export jobs

---

## Comunidade & Canais

* **Contato:** [dan@ubl.agency](mailto:dan@ubl.agency)
* **GitHub:** [https://github.com/ubl-foundation/ubl](https://github.com/ubl-foundation/ubl)
* **Discord:** [https://discord.gg/ubl](https://discord.gg/ubl)

---

## Atualização do Roadmap

Documento vivo, revisado **trimestralmente** conforme:

* Feedback de clientes
* Condições de mercado
* Descobertas técnicas
* Prioridades estratégicas

**Última atualização:** Fev/2026 • **Próxima:** Mai/2026
