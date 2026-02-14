# ubl-sdk (Python)

Receipt-first execution client for UBL Gate.

## Install

```bash
pip install ubl-sdk
```

## Usage

```python
from ubl_sdk import UBLClient, ExecuteRequest

client = UBLClient(
    base_url="https://api.ubl.agency",
    token="your-token-here",
)

# Receipt-first execute
resp = client.execute(ExecuteRequest(
    manifest={
        "pipeline": "my-pipeline",
        "in_grammar": {"inputs": {"message": ""}, "mappings": [], "output_from": "message"},
        "out_grammar": {"inputs": {"result": ""}, "mappings": [], "output_from": "result"},
        "policy": {"allow": True},
    },
    vars={"message": "Hello"},
))

print(resp.tip_cid)
print(resp.receipts["wf"].body["decision"])  # "ALLOW" or "DENY"

# Execute and raise on DENY
safe = client.execute_or_raise(ExecuteRequest(manifest=..., vars=...))

# Walk the receipt chain
chain = client.walk_chain(resp.tip_cid)

# Async
resp = await client.aexecute(ExecuteRequest(manifest=..., vars=...))
```

## Error Handling

```python
from ubl_sdk import UBLConflictError, UBLAuthError, UBLRateLimitError

try:
    client.execute(req)
except UBLConflictError:
    pass  # 409 — duplicate execution
except UBLAuthError as e:
    pass  # 401 or 403
except UBLRateLimitError as e:
    print(f"Retry after {e.retry_after}s")  # 429
```

## Test

```bash
pip install -e ".[dev]"
pytest tests/
```
