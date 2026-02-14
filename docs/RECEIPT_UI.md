# UBL Receipt UI Specification

> Canonical UI for displaying receipts. PASS / RETRY / DENY at a glance.
> Sober base state, didactic on hover. Mobile-first.

## Design Principles

1. **Status dominates**: PASS / RETRY / DENY is the largest element, visible instantly.
2. **Sober base**: neutral tones, no visual noise. Information reveals on interaction.
3. **Didactic hovers**: pink for pipeline stages, blue for proofs. Short explainers appear on hover.
4. **Mobile stack**: vertical layout, fixed receipt width, tooltips become expandable sections on touch.
5. **Accessible**: WCAG 2.1 AA contrast, screen-reader labels on all interactive elements.

## Layout Structure

```text
┌─────────────────────────────────────────┐
│  ██ PASS ██          "Execution OK"     │  ← Status banner (green/amber/red)
│                      short hint          │
├─────────────────────────────────────────┤
│  IDENTIFIERS                            │
│  CID      b3:a1b2c3...                  │
│  URL      api.ubl.agency/v1/receipt/... │
│  DID      did:cid:b3:a1b2...           │
│  TIP CID  b3:f4e5d6...                  │
├─────────────────────────────────────────┤
│  PIPELINE STAGES                        │
│  ┌──────┐  ┌──────┐  ┌──────┐  ┌────┐  │
│  │  rb  │→│  ρ   │→│  WA  │→│ WF │  │
│  │ 2ms  │  │ 1ms  │  │ 0ms  │  │3ms │  │
│  └──────┘  └──────┘  └──────┘  └────┘  │
│  micro-pills with latency               │
├─────────────────────────────────────────┤
│  PROOFS                                 │
│  JWS       EdDSA ✓  kid: runtime-001    │
│  Chain     3 receipts, tip verified     │
│  Parents   [b3:..., b3:...]             │
├─────────────────────────────────────────┤
│  DETAILS                                │
│  "This execution processed input        │
│   brand_id=acme-001 through the full    │
│   pipeline. All policy gates passed."   │
│                                         │
│  ┌─────────┐  ┌──────────┐             │
│  │ Raw JSON │  │ Export PDF│             │
│  └─────────┘  └──────────┘             │
└─────────────────────────────────────────┘
```

## Status Banner

The top section is the most prominent element.

| Status | Color | Icon | Hint |
| --- | --- | --- | --- |
| PASS | Green (`#16a34a`) | ✓ checkmark | "Execution completed successfully" |
| RETRY | Amber (`#d97706`) | ↻ retry arrow | "Transient failure — retry recommended" |
| DENY | Red (`#dc2626`) | ✕ cross | "Blocked by policy: {rule_id}" |

The hint is a single sentence extracted from the receipt's `reason` field (DENY/RETRY) or a default message (PASS).

## Sections

### 1. Identifiers

| Field | Source | Display |
| --- | --- | --- |
| CID | `tip_cid` | Monospace, truncated with copy button |
| URL | Constructed from gate base + CID | Clickable link |
| DID | `did:cid:{tip_cid}` | Monospace |
| TIP CID | `receipts.wf.body_cid` | Monospace, truncated with copy button |

### 2. Pipeline Stages (pink tones)

Each stage is a **micro-pill** showing:
- Stage name (`rb`, `ρ`, `WA`, `WF`)
- Latency in ms (from `observability.timeline`)
- Status indicator (✓ / ✕)

**Hover behavior** (pink `#f472b6` accent):
- Shows stage description ("RB-VM: deterministic bytecode execution, no IO")
- Shows input/output CIDs for that stage
- Shows fuel spent (for `rb` stage)

**Color coding**:
- Completed: pink pill with white text
- Current/active: pulsing pink border
- Failed: red pill

### 3. Proofs (blue tones)

| Field | Source | Display |
| --- | --- | --- |
| JWS | `receipts.wf.sig` | "EdDSA ✓" or "⚠ unverified" |
| KID | `receipts.wf.kid` | Key identifier |
| Chain | Count of receipts in parents→tip | "3 receipts, tip verified" |
| Parents | `receipts.wf.parents` | List of CIDs, each clickable |

**Hover behavior** (blue `#60a5fa` accent):
- JWS: shows full signature, verification status, DID resolution
- Chain: shows parent→child graph
- Parents: shows each parent receipt's status

### 4. Details

- **Narrative**: brief, LLM-generated explanation of what happened and why to trust it.
- **Policy trace**: if present, shows which rules were evaluated (from `observability.policy_trace`).
- **Export buttons**: Raw JSON (downloads receipt), Export PDF (renders printable version).

## Hover & Interaction

| Element | Hover color | Content |
| --- | --- | --- |
| Stage pill | Pink `#f472b6` | Stage description + CIDs + latency |
| Proof field | Blue `#60a5fa` | Verification details + DID |
| CID value | Gray `#9ca3af` | Full CID (untruncated) + copy action |
| Parent CID | Blue `#60a5fa` | Links to parent receipt page |

On mobile (touch), hovers become expandable accordion sections.

## States

### PASS State
- Green banner, all pills green/pink
- Narrative: "Execution completed. All {n} policy gates passed."
- Proofs section shows verified chain

### RETRY State
- Amber banner, last stage pill shows amber
- Narrative: "Transient failure at {stage}. Recommended: {action}."
- Shows `Retry-After` value if rate-limited

### DENY State
- Red banner, failed stage pill is red
- Narrative: "Blocked by {rule_id}: {reason}"
- Policy trace highlights the denying rule
- Recommendation: what to change to pass

## Component Structure

```text
<ReceiptView>
  <StatusBanner status={PASS|RETRY|DENY} hint={string} />
  <IdentifierSection cid={} url={} did={} tip={} />
  <PipelineStages stages={[{name, latency_ms, status, cids}]} />
  <ProofSection jws={} kid={} chain={} parents={[]} />
  <DetailSection narrative={} policy_trace={[]} />
  <ExportToolbar onRawJSON={} onPDF={} />
</ReceiptView>
```

## Responsive Breakpoints

| Breakpoint | Layout |
| --- | --- |
| `≥768px` | Fixed-width card (480px), centered |
| `<768px` | Full-width, vertical stack, hovers → accordions |
| `<400px` | Compact mode: CIDs truncated to 12 chars, pills stacked vertically |

## Accessibility

- All interactive elements have `aria-label`
- Status banner has `role="status"` and `aria-live="polite"`
- Color is never the sole indicator (icons + text accompany every color)
- Keyboard navigation: Tab through sections, Enter to expand
- Minimum contrast ratio: 4.5:1 (AA)
