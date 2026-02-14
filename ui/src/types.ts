export interface Receipt {
  t: string
  parents: string[]
  body: Record<string, unknown>
  body_cid: string
  sig?: {
    alg: string
    kid: string
    sig: string
  }
  observability?: {
    ghost: boolean
    logline?: string
    phase?: string
    ts?: string
  }
}

export interface ReceiptChainEntry {
  cid: string
  receipt: Receipt
}

export interface RunResult {
  receipts: {
    wa: Receipt
    transition?: Receipt
    wf: Receipt
  }
  tip_cid: string
}

export type DecisionBadge = 'ALLOW' | 'DENY' | 'WARN'

export interface PolicyTraceEntry {
  rule_id: string
  level: string
  action: string
  condition: string
  matched: boolean
  message?: string
}
