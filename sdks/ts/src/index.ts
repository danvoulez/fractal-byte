/**
 * @ubl/sdk — Receipt-first execution client for UBL Gate.
 *
 * Usage:
 *   const ubl = new UBLClient({ baseUrl: "https://api.ubl.agency", token: "Bearer ..." });
 *   const result = await ubl.execute({ manifest, vars });
 *   console.log(result.tip_cid, result.receipts.wf.body.decision);
 */

// ── Types ────────────────────────────────────────────────────────────

export interface UBLClientOptions {
  baseUrl: string;
  token: string;
  /** Optional tenant header override (admin only) */
  tenant?: string;
  /** Optional kid header */
  kid?: string;
  /** Request timeout in ms (default 30000) */
  timeoutMs?: number;
}

export interface Grammar {
  inputs: Record<string, string>;
  mappings: unknown[];
  output_from: string;
}

export interface Manifest {
  pipeline: string;
  in_grammar: Grammar;
  out_grammar: Grammar;
  policy: Record<string, unknown>;
  adapters?: Record<string, unknown>;
}

export interface ExecuteRequest {
  manifest: Manifest;
  vars: Record<string, unknown>;
}

export interface IngestRequest {
  payload: unknown;
  kind?: string;
}

export interface Observability {
  latency_ms?: number;
  stage?: string;
  timeline?: Array<{ t: string; verb: string; decision?: string }>;
  policy_trace?: Array<{ level: string; rule: string; result: string; reason?: string }>;
}

export interface Receipt {
  t: string;
  parents: string[];
  body: Record<string, unknown>;
  body_cid: string;
  sig: string;
  kid: string;
  observability?: Observability;
}

export interface ExecuteResponse {
  receipts: {
    wa: Receipt;
    transition: Receipt;
    wf: Receipt;
  };
  tip_cid: string;
  artifacts: Record<string, unknown> | null;
}

export interface IngestResponse {
  cid: string;
  nrf_cid: string;
  receipt?: Receipt;
}

export interface TransitionReceipt {
  body: {
    type: string;
    from_layer: number;
    to_layer: number;
    preimage_raw_cid: string;
    rho_cid: string;
    witness: { vm_tag: string; bytecode_cid: string; fuel_spent: number };
  };
  body_cid: string;
  sig: string;
  kid: string;
}

export interface HealthResponse {
  ok: boolean;
}

// ── Errors ───────────────────────────────────────────────────────────

export class UBLError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly body: unknown
  ) {
    super(message);
    this.name = "UBLError";
  }
}

export class UBLConflictError extends UBLError {
  constructor(body: unknown) {
    super("Duplicate execution (idempotency)", 409, body);
    this.name = "UBLConflictError";
  }
}

export class UBLAuthError extends UBLError {
  constructor(status: number, body: unknown) {
    super(status === 401 ? "Unauthorized" : "Forbidden", status, body);
    this.name = "UBLAuthError";
  }
}

export class UBLRateLimitError extends UBLError {
  public readonly retryAfter: number | null;
  constructor(body: unknown, retryAfter: string | null) {
    super("Rate limited", 429, body);
    this.name = "UBLRateLimitError";
    this.retryAfter = retryAfter ? parseInt(retryAfter, 10) : null;
  }
}

// ── Client ───────────────────────────────────────────────────────────

export class UBLClient {
  private readonly baseUrl: string;
  private readonly token: string;
  private readonly tenant?: string;
  private readonly kid?: string;
  private readonly timeoutMs: number;

  constructor(opts: UBLClientOptions) {
    this.baseUrl = opts.baseUrl.replace(/\/+$/, "");
    this.token = opts.token;
    this.tenant = opts.tenant;
    this.kid = opts.kid;
    this.timeoutMs = opts.timeoutMs ?? 30_000;
  }

  // ── Public API ───────────────────────────────────────────────────

  /** Receipt-first execute: manifest + vars → receipts + tip_cid + artifacts */
  async execute(req: ExecuteRequest): Promise<ExecuteResponse> {
    return this.post<ExecuteResponse>("/v1/execute", req);
  }

  /** Ingest raw JSON → NRF CID + optional receipt */
  async ingest(req: IngestRequest): Promise<IngestResponse> {
    return this.post<IngestResponse>("/v1/ingest", req);
  }

  /** Fetch receipt by CID */
  async getReceipt(cid: string): Promise<Receipt> {
    return this.get<Receipt>(`/v1/receipt/${encodeURIComponent(cid)}`);
  }

  /** Fetch transition receipt by CID */
  async getTransition(cid: string): Promise<TransitionReceipt> {
    return this.get<TransitionReceipt>(`/v1/transition/${encodeURIComponent(cid)}`);
  }

  /** Health check */
  async healthz(): Promise<HealthResponse> {
    return this.get<HealthResponse>("/healthz");
  }

  // ── Helpers ──────────────────────────────────────────────────────

  /**
   * Execute and assert ALLOW. Throws if decision is DENY.
   * Convenience wrapper for the common case.
   */
  async executeOrThrow(req: ExecuteRequest): Promise<ExecuteResponse> {
    const res = await this.execute(req);
    const decision = res.receipts.wf.body.decision as string | undefined;
    if (decision === "DENY") {
      const reason = res.receipts.wf.body.reason as string | undefined;
      const ruleId = res.receipts.wf.body.rule_id as string | undefined;
      throw new UBLError(
        `DENY: ${reason ?? "unknown"} (rule: ${ruleId ?? "unknown"})`,
        200,
        res
      );
    }
    return res;
  }

  /**
   * Verify a receipt's body_cid matches the body content.
   * NOTE: full verification requires BLAKE3 hashing + NRF canonicalization.
   * This method only checks structural integrity (fields present).
   */
  verifyReceiptStructure(receipt: Receipt): boolean {
    return (
      typeof receipt.body_cid === "string" &&
      receipt.body_cid.startsWith("b3:") &&
      typeof receipt.sig === "string" &&
      typeof receipt.kid === "string" &&
      Array.isArray(receipt.parents) &&
      typeof receipt.t === "string" &&
      typeof receipt.body === "object" &&
      receipt.body !== null
    );
  }

  /**
   * Walk the receipt chain from tip to root.
   * Returns receipts in order: [WF, Transition, WA].
   */
  async walkChain(tipCid: string): Promise<Receipt[]> {
    const chain: Receipt[] = [];
    let currentCid: string | null = tipCid;

    while (currentCid) {
      const receipt = await this.getReceipt(currentCid);
      chain.push(receipt);
      currentCid = receipt.parents.length > 0 ? receipt.parents[0] : null;
    }

    return chain;
  }

  // ── HTTP ─────────────────────────────────────────────────────────

  private headers(): Record<string, string> {
    const h: Record<string, string> = {
      "Content-Type": "application/json",
      Authorization: this.token.startsWith("Bearer ")
        ? this.token
        : `Bearer ${this.token}`,
    };
    if (this.tenant) h["X-UBL-Tenant"] = this.tenant;
    if (this.kid) h["X-UBL-Kid"] = this.kid;
    return h;
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const res = await fetch(url, {
        method,
        headers: this.headers(),
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      if (res.status === 409) {
        throw new UBLConflictError(await res.json().catch(() => null));
      }
      if (res.status === 401 || res.status === 403) {
        throw new UBLAuthError(res.status, await res.json().catch(() => null));
      }
      if (res.status === 429) {
        throw new UBLRateLimitError(
          await res.json().catch(() => null),
          res.headers.get("Retry-After")
        );
      }
      if (!res.ok) {
        throw new UBLError(
          `HTTP ${res.status} ${res.statusText}`,
          res.status,
          await res.text().catch(() => null)
        );
      }

      return (await res.json()) as T;
    } finally {
      clearTimeout(timer);
    }
  }

  private get<T>(path: string): Promise<T> {
    return this.request<T>("GET", path);
  }

  private post<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>("POST", path, body);
  }
}
