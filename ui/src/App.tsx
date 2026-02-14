import { useEffect, useState, useCallback } from 'react'
import { Search, RefreshCw, Activity, Database, Shield } from 'lucide-react'
import { fetchReceiptChain, healthz } from './api'
import type { Receipt } from './types'
import ReceiptCard from './components/ReceiptCard'

type FilterType = 'all' | 'ubl/wa' | 'ubl/transition' | 'ubl/wf'
type DecisionFilter = 'all' | 'ALLOW' | 'DENY'

export default function App() {
  const [receipts, setReceipts] = useState<Record<string, Receipt>>({})
  const [search, setSearch] = useState('')
  const [typeFilter, setTypeFilter] = useState<FilterType>('all')
  const [decisionFilter, setDecisionFilter] = useState<DecisionFilter>('all')
  const [loading, setLoading] = useState(false)
  const [health, setHealth] = useState<{ status: string; version: string } | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [chain, h] = await Promise.all([fetchReceiptChain(), healthz()])
      setReceipts(chain)
      setHealth(h)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch receipts')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { refresh() }, [refresh])

  const entries = Object.entries(receipts)
    .filter(([cid, r]) => {
      if (typeFilter !== 'all' && r.t !== typeFilter) return false
      const decision = (r.body as Record<string, unknown>).decision as string | undefined
      if (decisionFilter !== 'all' && decision !== decisionFilter) return false
      if (search) {
        const q = search.toLowerCase()
        return cid.toLowerCase().includes(q) ||
          r.t.toLowerCase().includes(q) ||
          r.body_cid.toLowerCase().includes(q) ||
          JSON.stringify(r.body).toLowerCase().includes(q)
      }
      return true
    })
    .sort(([, a], [, b]) => {
      const order: Record<string, number> = { 'ubl/wf': 0, 'ubl/transition': 1, 'ubl/wa': 2 }
      return (order[a.t] ?? 3) - (order[b.t] ?? 3)
    })

  const totalCount = Object.keys(receipts).length
  const wfCount = Object.values(receipts).filter(r => r.t === 'ubl/wf').length
  const denyCount = Object.values(receipts).filter(r => (r.body as Record<string, unknown>).decision === 'DENY').length

  return (
    <div className="min-h-screen bg-gray-950">
      {/* Header */}
      <header className="border-b border-gray-800 bg-gray-950/80 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-5xl mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center">
                <Database className="w-4 h-4 text-white" />
              </div>
              <div>
                <h1 className="text-lg font-semibold text-white">UBL Receipt Registry</h1>
                <p className="text-xs text-gray-500">Canonical receipt explorer</p>
              </div>
            </div>
            <div className="flex items-center gap-3">
              {health && (
                <div className="flex items-center gap-1.5 text-xs text-gray-500">
                  <Activity className="w-3 h-3 text-emerald-500" />
                  <span>{health.status}</span>
                  <span className="text-gray-700">|</span>
                  <span>v{health.version}</span>
                </div>
              )}
              <button
                onClick={refresh}
                disabled={loading}
                className="p-2 rounded-lg bg-gray-900 border border-gray-800 text-gray-400 hover:text-white hover:border-gray-700 transition-all disabled:opacity-50"
              >
                <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
              </button>
            </div>
          </div>
        </div>
      </header>

      <main className="max-w-5xl mx-auto px-4 py-6 space-y-6">
        {/* Stats */}
        <div className="grid grid-cols-3 gap-3">
          <div className="bg-gray-900 border border-gray-800 rounded-xl px-4 py-3">
            <div className="text-2xl font-bold text-white">{totalCount}</div>
            <div className="text-xs text-gray-500 mt-0.5">Total Receipts</div>
          </div>
          <div className="bg-gray-900 border border-gray-800 rounded-xl px-4 py-3">
            <div className="text-2xl font-bold text-emerald-400">{wfCount}</div>
            <div className="text-xs text-gray-500 mt-0.5">Write-Final</div>
          </div>
          <div className="bg-gray-900 border border-gray-800 rounded-xl px-4 py-3">
            <div className="text-2xl font-bold text-red-400">{denyCount}</div>
            <div className="text-xs text-gray-500 mt-0.5">Denied</div>
          </div>
        </div>

        {/* Filters */}
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500" />
            <input
              type="text"
              placeholder="Search by CID, type, or body content..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="w-full pl-10 pr-4 py-2.5 bg-gray-900 border border-gray-800 rounded-xl text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-gray-600 transition-colors"
            />
          </div>
          <div className="flex gap-2">
            <select
              value={typeFilter}
              onChange={e => setTypeFilter(e.target.value as FilterType)}
              className="px-3 py-2.5 bg-gray-900 border border-gray-800 rounded-xl text-sm text-gray-300 focus:outline-none focus:border-gray-600"
            >
              <option value="all">All Types</option>
              <option value="ubl/wa">Write-Ahead</option>
              <option value="ubl/transition">Transition</option>
              <option value="ubl/wf">Write-Final</option>
            </select>
            <select
              value={decisionFilter}
              onChange={e => setDecisionFilter(e.target.value as DecisionFilter)}
              className="px-3 py-2.5 bg-gray-900 border border-gray-800 rounded-xl text-sm text-gray-300 focus:outline-none focus:border-gray-600"
            >
              <option value="all">All Decisions</option>
              <option value="ALLOW">ALLOW</option>
              <option value="DENY">DENY</option>
            </select>
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="bg-red-500/10 border border-red-500/30 rounded-xl px-4 py-3 text-sm text-red-400">
            {error}
          </div>
        )}

        {/* Receipt list */}
        {entries.length === 0 && !loading && !error && (
          <div className="text-center py-16">
            <Shield className="w-12 h-12 text-gray-800 mx-auto mb-3" />
            <p className="text-gray-500 text-sm">No receipts found</p>
            <p className="text-gray-600 text-xs mt-1">Execute a pipeline to generate receipts</p>
          </div>
        )}

        <div className="space-y-3">
          {entries.map(([cid, receipt]) => (
            <ReceiptCard
              key={cid}
              cid={cid}
              receipt={receipt}
              onSelectCid={(targetCid) => setSearch(targetCid.slice(0, 16))}
            />
          ))}
        </div>

        {/* Footer */}
        <footer className="text-center py-8 text-xs text-gray-700">
          UBL Receipt Registry &middot; Receipt-first pipeline
        </footer>
      </main>
    </div>
  )
}
