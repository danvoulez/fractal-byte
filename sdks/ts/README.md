# @ubl/sdk (TypeScript)

Receipt-first execution client for UBL Gate.

## Install

```bash
npm install @ubl/sdk
```

## Usage

```typescript
import { UBLClient } from "@ubl/sdk";

const ubl = new UBLClient({
  baseUrl: "https://api.ubl.agency",
  token: "Bearer your-token-here",
});

// Receipt-first execute
const result = await ubl.execute({
  manifest: {
    pipeline: "my-pipeline",
    in_grammar: { inputs: { message: "" }, mappings: [], output_from: "message" },
    out_grammar: { inputs: { result: "" }, mappings: [], output_from: "result" },
    policy: { allow: true },
  },
  vars: { message: "Hello" },
});

console.log(result.tip_cid);
console.log(result.receipts.wf.body.decision); // "ALLOW" or "DENY"

// Execute and throw on DENY
const safe = await ubl.executeOrThrow({ manifest, vars });

// Walk the receipt chain
const chain = await ubl.walkChain(result.tip_cid);

// Health check
const health = await ubl.healthz();
```

## Error Handling

```typescript
import { UBLConflictError, UBLAuthError, UBLRateLimitError } from "@ubl/sdk";

try {
  await ubl.execute(req);
} catch (e) {
  if (e instanceof UBLConflictError) {
    // 409 — duplicate execution
  } else if (e instanceof UBLAuthError) {
    // 401 or 403
  } else if (e instanceof UBLRateLimitError) {
    // 429 — retry after e.retryAfter seconds
  }
}
```

## Test

```bash
npm run build && npm test
```
