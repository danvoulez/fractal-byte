"""Tests for UBL SDK (unit — no live server required)."""

import pytest

from ubl_sdk import (
    UBLClient,
    UBLError,
    UBLConflictError,
    UBLAuthError,
    UBLRateLimitError,
    Receipt,
    ExecuteResponse,
    IngestResponse,
    HealthResponse,
)


class TestUBLClient:
    def _client(self, **kw):
        defaults = {"base_url": "http://localhost:3000", "token": "test"}
        defaults.update(kw)
        return UBLClient(**defaults)

    def test_construct_minimal(self):
        c = self._client()
        assert c.base_url == "http://localhost:3000"
        assert c.token == "Bearer test"

    def test_construct_full(self):
        c = self._client(
            base_url="https://api.ubl.agency/",
            token="Bearer xyz",
            tenant="acme",
            kid="acme-001",
            timeout=5.0,
        )
        assert c.base_url == "https://api.ubl.agency"
        assert c.token == "Bearer xyz"
        assert c.tenant == "acme"
        assert c.kid == "acme-001"
        assert c.timeout == 5.0

    def test_verify_receipt_structure_valid(self):
        c = self._client()
        r = Receipt(
            t="2026-02-14T08:00:00Z",
            parents=[],
            body={"type": "ubl/run", "phase": "before"},
            body_cid="b3:abc123def456",
            sig="eyJ...",
            kid="runtime-001",
        )
        assert c.verify_receipt_structure(r) is True

    def test_verify_receipt_structure_bad_prefix(self):
        c = self._client()
        r = Receipt(
            t="2026-02-14T08:00:00Z",
            parents=[],
            body={"type": "ubl/run"},
            body_cid="sha256:abc",
            sig="eyJ...",
            kid="runtime-001",
        )
        assert c.verify_receipt_structure(r) is False


class TestReceiptFromDict:
    def test_minimal(self):
        d = {
            "t": "2026-02-14T08:00:00Z",
            "parents": [],
            "body": {"type": "ubl/run"},
            "body_cid": "b3:abc",
            "sig": "eyJ...",
            "kid": "k1",
        }
        r = Receipt.from_dict(d)
        assert r.body_cid == "b3:abc"
        assert r.observability is None

    def test_with_observability(self):
        d = {
            "t": "2026-02-14T08:00:00Z",
            "parents": ["b3:parent"],
            "body": {"type": "ubl/run"},
            "body_cid": "b3:abc",
            "sig": "eyJ...",
            "kid": "k1",
            "observability": {"latency_ms": 5, "stage": "wf"},
        }
        r = Receipt.from_dict(d)
        assert r.observability is not None
        assert r.observability.latency_ms == 5
        assert r.observability.stage == "wf"


class TestExecuteResponseFromDict:
    def test_full(self):
        d = {
            "receipts": {
                "wa": {
                    "t": "t1", "parents": [], "body": {}, "body_cid": "b3:wa",
                    "sig": "s", "kid": "k",
                },
                "transition": {
                    "t": "t2", "parents": ["b3:wa"], "body": {}, "body_cid": "b3:tr",
                    "sig": "s", "kid": "k",
                },
                "wf": {
                    "t": "t3", "parents": ["b3:tr"], "body": {"decision": "ALLOW"},
                    "body_cid": "b3:wf", "sig": "s", "kid": "k",
                },
            },
            "tip_cid": "b3:wf",
            "artifacts": {"result": "hello"},
        }
        resp = ExecuteResponse.from_dict(d)
        assert resp.tip_cid == "b3:wf"
        assert "wa" in resp.receipts
        assert "transition" in resp.receipts
        assert "wf" in resp.receipts
        assert resp.artifacts == {"result": "hello"}


class TestErrors:
    def test_ubl_error(self):
        e = UBLError("test", 500, {"detail": "oops"})
        assert e.status == 500
        assert e.body == {"detail": "oops"}

    def test_conflict(self):
        e = UBLConflictError({"tip": "b3:..."})
        assert e.status == 409

    def test_auth_401(self):
        e = UBLAuthError(401)
        assert str(e) == "Unauthorized"

    def test_auth_403(self):
        e = UBLAuthError(403)
        assert str(e) == "Forbidden"

    def test_rate_limit_with_retry(self):
        e = UBLRateLimitError(None, "30")
        assert e.retry_after == 30

    def test_rate_limit_no_retry(self):
        e = UBLRateLimitError(None, None)
        assert e.retry_after is None
