import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import {
  UBLClient,
  UBLError,
  UBLConflictError,
  UBLAuthError,
  UBLRateLimitError,
} from "./index";

describe("UBLClient", () => {
  it("constructs with required options", () => {
    const client = new UBLClient({
      baseUrl: "https://api.ubl.agency",
      token: "test-token",
    });
    assert.ok(client);
  });

  it("constructs with all options", () => {
    const client = new UBLClient({
      baseUrl: "https://api.ubl.agency/",
      token: "Bearer test-token",
      tenant: "acme-corp",
      kid: "acme-web-001",
      timeoutMs: 5000,
    });
    assert.ok(client);
  });

  it("verifyReceiptStructure accepts valid receipt", () => {
    const client = new UBLClient({
      baseUrl: "http://localhost:3000",
      token: "t",
    });
    const valid = client.verifyReceiptStructure({
      t: "2026-02-14T08:00:00Z",
      parents: [],
      body: { type: "ubl/run", phase: "before" },
      body_cid: "b3:abc123def456",
      sig: "eyJ...",
      kid: "runtime-001",
    });
    assert.equal(valid, true);
  });

  it("verifyReceiptStructure rejects missing body_cid prefix", () => {
    const client = new UBLClient({
      baseUrl: "http://localhost:3000",
      token: "t",
    });
    const invalid = client.verifyReceiptStructure({
      t: "2026-02-14T08:00:00Z",
      parents: [],
      body: { type: "ubl/run" },
      body_cid: "sha256:abc",
      sig: "eyJ...",
      kid: "runtime-001",
    });
    assert.equal(invalid, false);
  });

  it("verifyReceiptStructure rejects null body", () => {
    const client = new UBLClient({
      baseUrl: "http://localhost:3000",
      token: "t",
    });
    const invalid = client.verifyReceiptStructure({
      t: "2026-02-14T08:00:00Z",
      parents: [],
      body: null as unknown as Record<string, unknown>,
      body_cid: "b3:abc",
      sig: "eyJ...",
      kid: "runtime-001",
    });
    assert.equal(invalid, false);
  });
});

describe("UBL Errors", () => {
  it("UBLError has status and body", () => {
    const err = new UBLError("test", 500, { detail: "oops" });
    assert.equal(err.status, 500);
    assert.equal(err.name, "UBLError");
    assert.deepEqual(err.body, { detail: "oops" });
  });

  it("UBLConflictError is 409", () => {
    const err = new UBLConflictError({ tip_cid: "b3:..." });
    assert.equal(err.status, 409);
    assert.equal(err.name, "UBLConflictError");
  });

  it("UBLAuthError distinguishes 401 vs 403", () => {
    const e401 = new UBLAuthError(401, null);
    assert.equal(e401.message, "Unauthorized");
    const e403 = new UBLAuthError(403, null);
    assert.equal(e403.message, "Forbidden");
  });

  it("UBLRateLimitError parses Retry-After", () => {
    const err = new UBLRateLimitError(null, "30");
    assert.equal(err.retryAfter, 30);
    const err2 = new UBLRateLimitError(null, null);
    assert.equal(err2.retryAfter, null);
  });
});
