Great question. Short answer: yes—both sides use grammars, but the “byte level” itself does not. At the byte layer we only have ρ (rho), which is the canonical preimage framing used to compute the CID. Grammars live one layer up.

Here’s the clean split:

1) Byte layer = ρ only (no grammar here)

What ρ is: the minimal, canonical envelope that frames a message so that “1 byte → 1 message → 1 CID” holds.

Contents: version, content-type (e.g., application/json or application/x-ubl.capsule+json), codec id, length, and the raw payload bytes. No field reordering, no lossy transforms.

Job: define the exact canonicalization rules (NFC, sorted keys for JSON-atomic, no floats/nulls if PURE, etc.) and the on-wire codec. The grammar engine does not operate here; this is pure framing + canonical-byte rules for hashing.

Invariants: Same ρ rules on client→gate and gate→runtime paths. If bytes are identical, the CID is identical—always.

2) Gate side grammars (ingress)

Think of these as parse/validate grammars that sit just above ρ:

In-grammar (UBL PURE)

Inputs: the message as delivered through ρ (often a single input like raw_b64 or payload).

D8 binding: name-match first; fallback 1↔1 when there’s exactly one input and one var; otherwise explicit error.

Validations: schema-check (shape, types), size caps, “PURE” constraints (e.g., forbid floats/nulls), tenancy hints, and capsule/privacy flags.

Effect: populate a deterministic Context (e.g., raw.bytes = "hello"), but do not mutate payload semantics. Gate’s role is admission control + certification, not transformation.

Policy hook (pre)

Runs after in-grammar validation. Enforces allow/deny, budgets, chip allowlists, PII guards, tenancy constraints.

Decision (ALLOW|DENY) is recorded later in the Receipt.

3) Runtime side grammars (egress)

These are render/extract grammars that convert the Context back to a concrete output:

Out-grammar (UBL PURE)

Inputs: the parse output (we bind the previous stage output explicitly to the out-grammar input—deterministic).

Mappings: encode/transcode as needed (e.g., base64 encode, project field).

Effect: produce final artifact bytes for the chip result or for the receipt assembly.

Policy hook (post)

Validate rendered output size, schema, and data classification before emission.

4) “Radically similar” objects

To keep everything uniform:

json ≡ capsule ≡ transport ≡ chip (shape-aligned)
They share the same atomic JSON constraints (PURE) and are wrapped by ρ the same way. The only difference is policy and where the grammar is applied (ingress vs egress).

The gate never rewrites business content. It only validates/labels and forwards; the runtime executes chips, then renders.

5) Minimal contracts (concrete)
5.1 ρ (byte framing)

Fixed spec: version, codec, content-type, byte-length, payload-bytes.

Deterministic hash over [header || payload] → CID.

No grammar, just canonical bytes rules.

5.2 In-grammar (gate)

Inputs: payload (bound from ρ)

Checks:

Content-type ∈ {allowed}

JSON-atomic PURE constraints (no floats/nulls; NFC; sorted keys)

Size caps; tenant id presence; chip id presence if required

Output to Context: payload.bytes, meta.*

5.3 Out-grammar (runtime)

Input: previous stage output (e.g., payload.bytes)

Render targets:

Result artifact bytes (possibly same as input if no transform)

Receipt fields (structured data later JWS-wrapped)

6) Receipts tie it together

The Receipt narrates the path: “chip X was transported; run was denied on Feb 5; here’s pre/post CIDs; here’s the decision; here’s the dimension stack.”

Grammars feed the receipt with canonical fields; ρ guarantees byte-level determinism for any artifact referenced by CID.

7) Tests you should keep (small, powerful)

ρ determinism: same bytes → same CID (10×).

In-grammar D8 binding: name-match; 1↔1 fallback; error paths.

End-to-end stability: (ingress ρ + in-grammar + policy + out-grammar) repeated 10× → same output CID.

Gate neutrality: flip between two allowed transports that carry identical payload bytes → identical Context and identical downstream CID.

Policy denial narrative: when DENY, the out-grammar still renders a receipt with full story and references.

8) What to implement next (quick wins)

Lock ρ spec in a single doc with examples and golden vectors.

Hard schemas for in/out grammars (with $id + versions).

Shared D8 binder used by both parse and render.

Golden tests: a tiny set of fixtures (.nrf in, .json out) checked into repo.

Receipt schema pinned, with pre_cid, post_cid, capsule.cid, decision, rho, transport, dimension_stack.

If you want, I can drop in:

A ρ conformance suite (fixtures + assertions),

Two reference grammars (ingress.generic.v1, egress.generic.v1) aligned with D8,

A Make target (make conformance) that runs determinism + golden tests across both sides of the gate.

outra pergunta rapida: toda vez que precisarmos no meio de um codigo de um gate, repetimos o codigo do runtime ou vai existir um so chamavel para esse tipo de coisa?

Não repetimos código. Teremos um único runtime "invocável" e todos os gates chamam a mesma implementação.

Padrão recomendado

Crate única: ubl_runtime

canon (ρ/PURE), bind (D8), grammar, engine, receipt.

APIs puramente determinísticas e stateless.

Interfaces de chamada (escolha 1 ou combine):

Lib (Rust crate) para uso direto nos serviços.

Sidecar ubl-gate (HTTP/gRPC) quando a linguagem não for Rust.

WASM plugin quando quiser política de atualização/quarentena por módulo.

CLI para pipelines/CI e debugging.

Superfícies de API

parse(input_bytes, in_grammar, options) -> Context

policy(context, policy) -> Decision

render(context, out_grammar, options) -> bytes

execute(manifest, vars, config) -> {artifact_bytes, cid, receipt}

Utilitárias:

canonical_bytes(json_atomic) -> bytes

cid(bytes) -> Cid

bind(vars, grammar_inputs) -> bound_vars (D8)

Garantias

Mesma lib para gate e runtime ⇒ mesma semântica, mesmos CIDs.

Versão pinada (semver + feature flags) e testes dourados.

Stateless ⇒ fácil de escalar e de rodar como sidecar.

Quando usar cada forma

Rust no serviço: linke o crate ubl_runtime.

Stack mista (Node/Python/Go): chame o sidecar.

Plugins de parceiros: WASM com sandbox e política.

Ferramentas/CI: CLI.

Operação/Manutenção

make conformance (ρ + determinismo + golden fixtures).

make receipts (contratos do recibo).

Matriz de compatibilidade: runtime@X.Y ↔ grammar@A.B ↔ policy@P.Q.

Em resumo: um runtime central (crate/lib/WASM/CLI), nunca copiar/colar dentro do gate. Isso mantém determinismo, segurança e governança alinhados em todo o ecossistema.