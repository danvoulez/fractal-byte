# UBL Wasm Adapters

> IO only via Wasm. Every external byte is frozen by CID before the runtime touches it.

## The No-IO Boundary

The UBL runtime is deterministic — no network, no filesystem, no clock. But real-world executions need external data (APIs, LLMs, files). Wasm adapters bridge this gap:

```text
┌──────────────────────────────────────────────────────┐
│                    UBL Runtime (No-IO)                │
│                                                      │
│  Receives only CID-addressed, frozen parameters      │
│  Same input → same output → same CID (always)        │
└──────────────────────────┬───────────────────────────┘
                           │ frozen bytes + CID
                           │
┌──────────────────────────┴───────────────────────────┐
│                  Wasm Adapter Layer                   │
│                                                      │
│  Performs IO (HTTP fetch, LLM call, file read)        │
│  Canonicalizes result → NRF bytes → CID              │
│  Produces wasm_acquire receipt                        │
│  Passes frozen CID to runtime as parameter            │
└──────────────────────────────────────────────────────┘
```

**Rule**: if the result depends on the world or time, Wasm acquires it and the runtime seals it. The runtime never sees raw external data — only CID-addressed frozen bytes.

## Adapter Contract

Every Wasm adapter must implement:

```rust
trait WasmAdapter {
    /// Acquire external data, canonicalize, return CID + frozen bytes.
    fn acquire(&self, params: AcquireParams) -> Result<AcquireResult, AdapterError>;
}

struct AcquireParams {
    /// Adapter-specific configuration (e.g., URL, headers, timeout)
    config: serde_json::Value,
    /// Policy constraints (allowlist, max_bytes, timeout)
    policy: AdapterPolicy,
}

struct AcquireResult {
    /// CID of the frozen, NRF-canonical bytes
    cid: String,
    /// The frozen bytes themselves (stored in CAS)
    bytes: Vec<u8>,
    /// Metadata about the acquisition
    meta: AcquireMeta,
}

struct AcquireMeta {
    /// Adapter type (e.g., "http", "llm", "file")
    adapter_type: String,
    /// Source identifier (e.g., URL, model name)
    source: String,
    /// Acquisition timestamp
    acquired_at: String,
    /// Size in bytes
    size: usize,
}
```

## HTTP Adapter

The most common adapter. Fetches data from an HTTP endpoint, canonicalizes the response, and pins it by CID.

### Manifest Configuration

```json
{
  "manifest": {
    "pipeline": "brand-theme",
    "adapters": {
      "http": {
        "brand_theme": {
          "url": "https://api.brand.com/v1/theme/${brand_id}",
          "method": "GET",
          "headers": { "Accept": "application/json" },
          "allowlist": ["api.brand.com", "cdn.brand.com"],
          "timeout_ms": 5000,
          "max_bytes": 524288,
          "extract": {
            "type": "jsonpath",
            "path": "$.theme.primary_color"
          }
        }
      }
    }
  }
}
```

### Policy Constraints

| Constraint | Default | Description |
| --- | --- | --- |
| `allowlist` | `[]` (deny all) | Domains the adapter may contact |
| `timeout_ms` | `5000` | Maximum time for the HTTP request |
| `max_bytes` | `1048576` (1 MiB) | Maximum response body size |
| `methods` | `["GET"]` | Allowed HTTP methods |
| `tls_required` | `true` | Require HTTPS |

If any constraint is violated, the adapter returns an error and the execution produces a DENY receipt with the reason.

### Execution Flow

```text
1. Gate receives manifest with adapter config
2. Wasm HTTP adapter:
   a. Validates URL against allowlist → reject if not allowed
   b. Performs HTTP request (within timeout + max_bytes)
   c. Canonicalizes response body via NRF-1.1
   d. Computes CID: b3:<blake3(nrf_bytes)>
   e. Stores frozen bytes in CAS
   f. Produces wasm_acquire receipt
3. Frozen CID passed to runtime as input parameter
4. Runtime executes with frozen data (deterministic)
5. Receipt chain: WA → wasm_acquire → transition → WF
```

### wasm_acquire Receipt

```json
{
  "type": "ubl/wasm_acquire",
  "version": "1",
  "adapter": "http",
  "source": "https://api.brand.com/v1/theme/acme-001",
  "acquired_at": "2026-02-14T08:00:00Z",
  "frozen_cid": "b3:abc123...",
  "size_bytes": 2048,
  "extract": {
    "type": "jsonpath",
    "path": "$.theme.primary_color",
    "result_cid": "b3:def456..."
  },
  "policy": {
    "allowlist_matched": "api.brand.com",
    "timeout_ms": 5000,
    "actual_latency_ms": 120
  }
}
```

## LLM Adapter

Calls a local or remote LLM. The response is frozen by CID before the runtime uses it.

### Manifest Configuration

```json
{
  "adapters": {
    "llm": {
      "metadata_enrichment": {
        "model": "ollama/llama3",
        "endpoint": "http://localhost:11434/api/generate",
        "prompt_template": "Extract metadata from: ${input_text}",
        "max_tokens": 512,
        "temperature": 0
      }
    }
  }
}
```

**Important**: `temperature: 0` is recommended for reproducibility, but the result is still non-deterministic (model weights, quantization). This is why the result MUST be frozen by CID — the runtime treats it as an opaque parameter, not a computation.

### Flow

```text
1. Wasm LLM adapter calls model endpoint
2. Response canonicalized → CID
3. wasm_acquire receipt records: model, prompt hash, response CID
4. Runtime receives frozen CID as parameter
5. If re-executed with same CID → same result (deterministic)
6. If re-executed without CID → Wasm re-acquires (potentially different result, new CID)
```

## Schema: `schemas/adapter_http.schema.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "UBL HTTP Adapter Config",
  "type": "object",
  "required": ["url", "allowlist"],
  "properties": {
    "url": {
      "type": "string",
      "description": "URL template. Variables: ${var_name}"
    },
    "method": {
      "type": "string",
      "enum": ["GET", "POST", "PUT"],
      "default": "GET"
    },
    "headers": {
      "type": "object",
      "additionalProperties": { "type": "string" }
    },
    "allowlist": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Allowed domains"
    },
    "timeout_ms": {
      "type": "integer",
      "default": 5000,
      "maximum": 30000
    },
    "max_bytes": {
      "type": "integer",
      "default": 1048576,
      "maximum": 10485760
    },
    "tls_required": {
      "type": "boolean",
      "default": true
    },
    "extract": {
      "type": "object",
      "properties": {
        "type": { "enum": ["jsonpath", "regex", "full"] },
        "path": { "type": "string" }
      }
    }
  }
}
```

## Error Handling

| Error | Receipt Decision | Reason |
| --- | --- | --- |
| Domain not in allowlist | DENY | `ADAPTER_DOMAIN_BLOCKED: {domain}` |
| Timeout exceeded | RETRY | `ADAPTER_TIMEOUT: {url} after {ms}ms` |
| Response too large | DENY | `ADAPTER_MAX_BYTES: {size} > {limit}` |
| TLS required but HTTP used | DENY | `ADAPTER_TLS_REQUIRED: {url}` |
| HTTP error (4xx/5xx) | RETRY or DENY | `ADAPTER_HTTP_{status}: {url}` |
| Extract path not found | DENY | `ADAPTER_EXTRACT_FAILED: {path} in {cid}` |

## Testing

```bash
# Adapter success: fetch + freeze + CID
cargo test -p ubl_wasm_adapters -- http_success

# Adapter timeout
cargo test -p ubl_wasm_adapters -- http_timeout

# Adapter domain blocked
cargo test -p ubl_wasm_adapters -- http_domain_blocked

# Adapter body too large
cargo test -p ubl_wasm_adapters -- http_max_bytes

# Determinism: same CID from same frozen input
cargo test -p ubl_runtime -- determinism
```

## Manifest Example: Wasm + Runtime

```json
{
  "manifest": {
    "pipeline": "brand-recolor",
    "adapters": {
      "http": {
        "brand_theme": {
          "url": "https://api.brand.com/v1/theme/${brand_id}",
          "allowlist": ["api.brand.com"],
          "timeout_ms": 5000,
          "max_bytes": 524288,
          "extract": { "type": "jsonpath", "path": "$.primary_color" }
        }
      }
    },
    "in_grammar": {
      "inputs": { "image_cid": "", "brand_id": "" },
      "mappings": [],
      "output_from": "image_cid"
    },
    "out_grammar": {
      "inputs": { "recolored_image": "" },
      "mappings": [],
      "output_from": "recolored_image"
    },
    "policy": { "allow": true }
  },
  "vars": {
    "image_cid": "b3:original_image...",
    "brand_id": "acme-001"
  }
}
```

**Execution**:

1. Wasm HTTP adapter fetches brand theme → `brand_theme_cid` + `resolved_color`
2. Runtime recolors image using `resolved_color` (deterministic, no IO)
3. Receipt chain: WA → wasm_acquire → transition → WF
