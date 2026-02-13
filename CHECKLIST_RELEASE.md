# UBL • Checklist de Release

> Objetivo: garantir **determinismo, auditoria completa** (WA + WF + Recibo) e **estabilidade de API** em cada release.

## 0) Gatekeepers (go/no-go)
- [ ] p95 latência do **ubl-gate** ≤ alvo da release
- [ ] 100% dos **runs** com **write-ahead (WA)**, **write-after (WF)** e **recibo JWS**
- [ ] **Binding D8** ativo no parse e render (sem mapeamentos implícitos inesperados)
- [ ] Sem **churn de CID** em golden tests
- [ ] Contratos **REST/gRPC** congelados (OpenAPI/Protos)
- [ ] Sem secrets em claro; cápsulas/escopos revisados
- [ ] Plano de rollback validado (policies e schema)

## 1) Determinismo
- [ ] Golden set (N≥100) — **mesmo input ⇒ mesmo CID/bytes**
- [ ] Replay idempotente de **/v1/ingest** e **/v1/execute**
- [ ] Execuções paralelas não introduzem variância
- [ ] Clocks normalizados (NTP), timestamps só no **recibo** (não no preimage)

## 2) Auditoria (LLM-first)
- [ ] **Recibo** contém narrativa completa (quem/quando/o quê/por quê)
- [ ] Campos: `url`, `cid`, `did`, `kind`, `version`, `time`, `actor`, `space`, `rho`
- [ ] Encadeamento WA → WF por `preimage_cid`
- [ ] **Ghost records** funcionam e são auditáveis
- [ ] Política (ALLOW/DENY) assinada e reprodutível

## 3) Segurança & Privacidade
- [ ] **DID** publicado e resolvível (`/.well-known/did.json`)
- [ ] Rotação de chaves documentada e testada
- [ ] **Cápsulas**: AES-256-GCM, escopos (private/shared/org/public), unseal auditável
- [ ] Redução de PII/PHI por padrão em **ubl-json (logline)**
- [ ] Retenção e purge com **recibos de purge**

## 4) Políticas
- [ ] **UBL Policy DSL v1**: compile/validate/lint sem warnings
- [ ] 10+ templates testados (PII, exfil, horário, custo, space-boundary)
- [ ] Simulação (dry-run) gera **recibos de simulação**
- [ ] Rollback versionado com recibo de mudança

## 5) Transporte & Ledger
- [ ] **SIRP**: retries com backoff, delivery receipts, idempotência por CID
- [ ] **S3**: buckets regionais, replicação cruzada, retenção aplicada
- [ ] Durabilidade (restore drills) e throughput

## 6) Observabilidade
- [ ] Prometheus (métricas), Grafana (dashboards), Jaeger (tracing), ELK (logs)
- [ ] Alertas: p95/erro de política/erro de criptografia/fila de transporte
- [ ] Amostragem de logs sem dados sensíveis

## 7) Compatibilidade & Docs
- [ ] OpenAPI e Protos publicados (tag da release)
- [ ] Guia de migração (se houver breaking changes)
- [ ] **ROADMAP.md** e **SECURITY.md** atualizados
- [ ] Exemplos no CLI/SDK (race card) funcionando end-to-end

---

## Testes de Aceite (mínimo)
- [ ] **Race Card 6 passos**: ingest → raw → json → receipt → did → determinismo
- [ ] **CID drift = 0** em 3 execuções do mesmo caso
- [ ] **DENY** previsto por policy com recibo narrativo correto
- [ ] **Capsule**: seal/unseal com trilha de auditoria
- [ ] **Vector/Lintval** (se aplicável): busca com recibo de leitura

---

## Critérios de Saída
- [ ] Todos os itens P0/P1 concluídos
- [ ] Sem CVEs abertas críticas/altas
- [ ] Performance dentro das metas
- [ ] Assinatura da release (tag) e artefatos reproduzíveis
