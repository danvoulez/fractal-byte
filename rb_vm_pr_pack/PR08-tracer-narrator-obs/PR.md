# Observabilidade: tracer por passo + narrador + métricas

> Data: 2026-02-13

# Observabilidade: tracer + narrador + métricas

**Resumo**
Entrega tracer passo-a-passo, narrador de RCs (deny/allow) e métricas.

**Mudanças**
- `rb_vm::trace` e hooks no executor
- `rb_vm::narrate` gera `narrative.md` (opcional) anexável via `AttachProof`
- Métricas: `rb_vm_opcodes_total`, `rb_vm_fuel_used_total`, `rb_vm_steps_total`, `rb_vm_bytes_cas_total{op}`, `rb_vm_denies_total{reason}`

**Checklist**
- [ ] Tracer não vaza dados sensíveis por padrão
- [ ] Narrativa cobre 100% de denies críticos
- [ ] Métricas exportadas no Gate (prom‑style)

**Critérios de aceite**
- p95 ≤ 50 ms nos chips de referência com tracer desativado
