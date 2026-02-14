import type { Receipt } from './types'

const BASE = ''

export async function fetchReceipt(cid: string): Promise<Receipt | null> {
  const res = await fetch(`${BASE}/v1/receipt/${cid}`)
  if (!res.ok) return null
  return res.json()
}

export async function fetchReceiptChain(): Promise<Record<string, Receipt>> {
  const res = await fetch(`${BASE}/v1/receipts`)
  if (!res.ok) return {}
  return res.json()
}

export async function fetchTransition(cid: string): Promise<Record<string, unknown> | null> {
  const res = await fetch(`${BASE}/v1/transition/${cid}`)
  if (!res.ok) return null
  return res.json()
}

export async function healthz(): Promise<{ status: string; version: string }> {
  const res = await fetch(`${BASE}/v1/healthz`)
  return res.json()
}
