"""UBL SDK type definitions."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Observability:
    latency_ms: int | None = None
    stage: str | None = None
    timeline: list[dict[str, Any]] | None = None
    policy_trace: list[dict[str, Any]] | None = None


@dataclass
class Receipt:
    t: str
    parents: list[str]
    body: dict[str, Any]
    body_cid: str
    sig: str
    kid: str
    observability: Observability | None = None

    @staticmethod
    def from_dict(d: dict[str, Any]) -> Receipt:
        obs = None
        if "observability" in d and d["observability"]:
            obs = Observability(**d["observability"])
        return Receipt(
            t=d["t"],
            parents=d.get("parents", []),
            body=d["body"],
            body_cid=d["body_cid"],
            sig=d["sig"],
            kid=d["kid"],
            observability=obs,
        )


@dataclass
class TransitionReceipt:
    body: dict[str, Any]
    body_cid: str
    sig: str
    kid: str

    @staticmethod
    def from_dict(d: dict[str, Any]) -> TransitionReceipt:
        return TransitionReceipt(
            body=d["body"],
            body_cid=d["body_cid"],
            sig=d["sig"],
            kid=d["kid"],
        )


@dataclass
class ExecuteRequest:
    manifest: dict[str, Any]
    vars: dict[str, Any]


@dataclass
class ExecuteResponse:
    receipts: dict[str, Receipt]
    tip_cid: str
    artifacts: dict[str, Any] | None

    @staticmethod
    def from_dict(d: dict[str, Any]) -> ExecuteResponse:
        receipts = {}
        for key in ("wa", "transition", "wf"):
            if key in d.get("receipts", {}):
                receipts[key] = Receipt.from_dict(d["receipts"][key])
        return ExecuteResponse(
            receipts=receipts,
            tip_cid=d["tip_cid"],
            artifacts=d.get("artifacts"),
        )


@dataclass
class IngestRequest:
    payload: Any
    kind: str | None = None


@dataclass
class IngestResponse:
    cid: str
    nrf_cid: str
    receipt: Receipt | None = None

    @staticmethod
    def from_dict(d: dict[str, Any]) -> IngestResponse:
        receipt = None
        if "receipt" in d and d["receipt"]:
            receipt = Receipt.from_dict(d["receipt"])
        return IngestResponse(
            cid=d["cid"],
            nrf_cid=d["nrf_cid"],
            receipt=receipt,
        )


@dataclass
class HealthResponse:
    ok: bool
