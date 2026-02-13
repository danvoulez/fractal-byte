# Bridge: `--engine=rb` no ubl-runtime + Exec API interna

> Data: 2026-02-13

# Bridge: `--engine=rb` no ubl-runtime

**Resumo**
Habilita executar chips RB via runtime com `Engine::execute_rb` e flag `--engine=rb`.

**Mudanças**
- `ubl_runtime::execute_rb(ExecuteRbReq) -> ExecuteRbRes`
- Conexão de providers: CAS, Sign, Canon
- Mapeamento de erros da VM para `ExecutionError` do runtime

**Checklist**
- [ ] CLI/flag `--engine=rb` aceita `chip_b64`, `inputs`, `ghost`, `fuel`
- [ ] Telemetria básica: steps, fuel_used
- [ ] Logs controlados (sem dados sensíveis)

**Critérios de aceite**
- Execução reproduz `rc_cid` do runner
