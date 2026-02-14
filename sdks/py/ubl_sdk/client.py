"""UBL SDK — receipt-first execution client."""

from __future__ import annotations

from typing import Any

import httpx

from .errors import UBLAuthError, UBLConflictError, UBLError, UBLRateLimitError
from .types import (
    ExecuteRequest,
    ExecuteResponse,
    HealthResponse,
    IngestRequest,
    IngestResponse,
    Receipt,
    TransitionReceipt,
)


class UBLClient:
    """Synchronous + async client for UBL Gate.

    Usage::

        client = UBLClient(base_url="https://api.ubl.agency", token="Bearer ...")
        resp = client.execute(ExecuteRequest(manifest={...}, vars={...}))
        print(resp.tip_cid, resp.receipts["wf"].body["decision"])
    """

    def __init__(
        self,
        base_url: str,
        token: str,
        *,
        tenant: str | None = None,
        kid: str | None = None,
        timeout: float = 30.0,
    ):
        self.base_url = base_url.rstrip("/")
        self.token = token if token.startswith("Bearer ") else f"Bearer {token}"
        self.tenant = tenant
        self.kid = kid
        self.timeout = timeout

    # ── Public API (sync) ───────────────────────────────────────────

    def execute(self, req: ExecuteRequest) -> ExecuteResponse:
        """Receipt-first execute: manifest + vars → receipts + tip_cid + artifacts."""
        data = self._post("/v1/execute", {"manifest": req.manifest, "vars": req.vars})
        return ExecuteResponse.from_dict(data)

    def ingest(self, req: IngestRequest) -> IngestResponse:
        """Ingest raw JSON → NRF CID + optional receipt."""
        body: dict[str, Any] = {"payload": req.payload}
        if req.kind:
            body["kind"] = req.kind
        data = self._post("/v1/ingest", body)
        return IngestResponse.from_dict(data)

    def get_receipt(self, cid: str) -> Receipt:
        """Fetch receipt by CID."""
        data = self._get(f"/v1/receipt/{cid}")
        return Receipt.from_dict(data)

    def get_transition(self, cid: str) -> TransitionReceipt:
        """Fetch transition receipt by CID."""
        data = self._get(f"/v1/transition/{cid}")
        return TransitionReceipt.from_dict(data)

    def healthz(self) -> HealthResponse:
        """Health check."""
        data = self._get("/healthz")
        return HealthResponse(ok=data.get("ok", False))

    # ── Helpers ─────────────────────────────────────────────────────

    def execute_or_raise(self, req: ExecuteRequest) -> ExecuteResponse:
        """Execute and raise if decision is DENY."""
        resp = self.execute(req)
        wf = resp.receipts.get("wf")
        if wf and wf.body.get("decision") == "DENY":
            reason = wf.body.get("reason", "unknown")
            rule_id = wf.body.get("rule_id", "unknown")
            raise UBLError(f"DENY: {reason} (rule: {rule_id})", 200, resp)
        return resp

    def verify_receipt_structure(self, receipt: Receipt) -> bool:
        """Check structural integrity of a receipt (fields present, CID prefix)."""
        return (
            isinstance(receipt.body_cid, str)
            and receipt.body_cid.startswith("b3:")
            and isinstance(receipt.sig, str)
            and isinstance(receipt.kid, str)
            and isinstance(receipt.parents, list)
            and isinstance(receipt.t, str)
            and isinstance(receipt.body, dict)
        )

    def walk_chain(self, tip_cid: str) -> list[Receipt]:
        """Walk the receipt chain from tip to root."""
        chain: list[Receipt] = []
        current: str | None = tip_cid
        while current:
            receipt = self.get_receipt(current)
            chain.append(receipt)
            current = receipt.parents[0] if receipt.parents else None
        return chain

    # ── Async API ───────────────────────────────────────────────────

    async def aexecute(self, req: ExecuteRequest) -> ExecuteResponse:
        """Async receipt-first execute."""
        data = await self._apost(
            "/v1/execute", {"manifest": req.manifest, "vars": req.vars}
        )
        return ExecuteResponse.from_dict(data)

    async def aingest(self, req: IngestRequest) -> IngestResponse:
        """Async ingest."""
        body: dict[str, Any] = {"payload": req.payload}
        if req.kind:
            body["kind"] = req.kind
        data = await self._apost("/v1/ingest", body)
        return IngestResponse.from_dict(data)

    async def aget_receipt(self, cid: str) -> Receipt:
        """Async fetch receipt by CID."""
        data = await self._aget(f"/v1/receipt/{cid}")
        return Receipt.from_dict(data)

    async def ahealthz(self) -> HealthResponse:
        """Async health check."""
        data = await self._aget("/healthz")
        return HealthResponse(ok=data.get("ok", False))

    # ── HTTP internals ──────────────────────────────────────────────

    def _headers(self) -> dict[str, str]:
        h: dict[str, str] = {
            "Content-Type": "application/json",
            "Authorization": self.token,
        }
        if self.tenant:
            h["X-UBL-Tenant"] = self.tenant
        if self.kid:
            h["X-UBL-Kid"] = self.kid
        return h

    def _handle_error(self, resp: httpx.Response) -> None:
        if resp.status_code == 409:
            raise UBLConflictError(resp.json())
        if resp.status_code in (401, 403):
            raise UBLAuthError(resp.status_code, resp.json())
        if resp.status_code == 429:
            raise UBLRateLimitError(
                resp.json(), resp.headers.get("Retry-After")
            )
        if resp.status_code >= 400:
            raise UBLError(
                f"HTTP {resp.status_code}", resp.status_code, resp.text
            )

    def _get(self, path: str) -> dict[str, Any]:
        with httpx.Client(timeout=self.timeout) as c:
            resp = c.get(f"{self.base_url}{path}", headers=self._headers())
            self._handle_error(resp)
            return resp.json()  # type: ignore[no-any-return]

    def _post(self, path: str, body: Any) -> dict[str, Any]:
        with httpx.Client(timeout=self.timeout) as c:
            resp = c.post(
                f"{self.base_url}{path}", headers=self._headers(), json=body
            )
            self._handle_error(resp)
            return resp.json()  # type: ignore[no-any-return]

    async def _aget(self, path: str) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=self.timeout) as c:
            resp = await c.get(
                f"{self.base_url}{path}", headers=self._headers()
            )
            self._handle_error(resp)
            return resp.json()  # type: ignore[no-any-return]

    async def _apost(self, path: str, body: Any) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=self.timeout) as c:
            resp = await c.post(
                f"{self.base_url}{path}", headers=self._headers(), json=body
            )
            self._handle_error(resp)
            return resp.json()  # type: ignore[no-any-return]
