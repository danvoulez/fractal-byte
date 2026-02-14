import { Shield, ShieldAlert, ShieldCheck, ChevronDown, ChevronUp, Link, Clock } from 'lucide-react'
import { useState } from 'react'
import type { Receipt, DecisionBadge } from '../types'

function badgeColor(decision: DecisionBadge): string {
  switch (decision) {
    case 'ALLOW': return 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30'
    case 'DENY': return 'bg-red-500/20 text-red-400 border-red-500/30'
    case 'WARN': return 'bg-amber-500/20 text-amber-400 border-amber-500/30'
    default: return 'bg-gray-500/20 text-gray-400 border-gray-500/30'
  }
}

function BadgeIcon({ decision }: { decision: DecisionBadge }) {
  switch (decision) {
    case 'ALLOW': return <ShieldCheck className="w-4 h-4" />
    case 'DENY': return <ShieldAlert className="w-4 h-4" />
    default: return <Shield className="w-4 h-4" />
  }
}

function typeLabel(t: string): string {
  if (t === 'ubl/wa') return 'Write-Ahead'
  if (t === 'ubl/transition') return 'Transition'
  if (t === 'ubl/wf') return 'Write-Final'
  return t
}

function typeColor(t: string): string {
  if (t === 'ubl/wa') return 'text-blue-400'
  if (t === 'ubl/transition') return 'text-purple-400'
  if (t === 'ubl/wf') return 'text-emerald-400'
  return 'text-gray-400'
}

interface Props {
  cid: string
  receipt: Receipt
  onSelectCid?: (cid: string) => void
}

export default function ReceiptCard({ cid, receipt, onSelectCid }: Props) {
  const [expanded, setExpanded] = useState(false)
  const decision = (receipt.body as Record<string, unknown>).decision as DecisionBadge | undefined
  const ghost = receipt.observability?.ghost ?? false
  const phase = receipt.observability?.phase ?? ''

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden hover:border-gray-700 transition-colors">
      {/* Header */}
      <div
        className="flex items-center justify-between px-4 py-3 cursor-pointer select-none"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-3 min-w-0">
          <span className={`text-xs font-semibold uppercase tracking-wider ${typeColor(receipt.t)}`}>
            {typeLabel(receipt.t)}
          </span>
          {decision && (
            <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium border ${badgeColor(decision)}`}>
              <BadgeIcon decision={decision} />
              {decision}
            </span>
          )}
          {ghost && (
            <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-gray-800 text-gray-500 border border-gray-700">
              GHOST
            </span>
          )}
          {phase && (
            <span className="flex items-center gap-1 text-xs text-gray-500">
              <Clock className="w-3 h-3" />
              {phase}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <code className="text-xs text-gray-500 font-mono truncate max-w-[200px]">
            {cid.slice(0, 16)}...
          </code>
          {expanded ? <ChevronUp className="w-4 h-4 text-gray-500" /> : <ChevronDown className="w-4 h-4 text-gray-500" />}
        </div>
      </div>

      {/* Expanded body */}
      {expanded && (
        <div className="border-t border-gray-800 px-4 py-3 space-y-3">
          {/* CID */}
          <div>
            <span className="text-xs text-gray-500 uppercase tracking-wider">Body CID</span>
            <code className="block mt-1 text-xs font-mono text-gray-300 break-all">{receipt.body_cid}</code>
          </div>

          {/* Parents */}
          {receipt.parents.length > 0 && (
            <div>
              <span className="text-xs text-gray-500 uppercase tracking-wider">Parents</span>
              <div className="mt-1 space-y-1">
                {receipt.parents.map((p, i) => (
                  <button
                    key={i}
                    onClick={() => onSelectCid?.(p)}
                    className="flex items-center gap-1 text-xs font-mono text-blue-400 hover:text-blue-300 transition-colors"
                  >
                    <Link className="w-3 h-3" />
                    {p.slice(0, 24)}...
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Signature */}
          {receipt.sig && (
            <div>
              <span className="text-xs text-gray-500 uppercase tracking-wider">Signature</span>
              <div className="mt-1 text-xs font-mono text-gray-400">
                <span className="text-gray-500">alg:</span> {receipt.sig.alg}
                <span className="mx-2 text-gray-700">|</span>
                <span className="text-gray-500">kid:</span> {receipt.sig.kid}
              </div>
            </div>
          )}

          {/* Body JSON */}
          <div>
            <span className="text-xs text-gray-500 uppercase tracking-wider">Body</span>
            <pre className="mt-1 p-3 bg-gray-950 rounded-lg text-xs font-mono text-gray-300 overflow-x-auto max-h-64 overflow-y-auto">
              {JSON.stringify(receipt.body, null, 2)}
            </pre>
          </div>

          {/* Policy Trace */}
          {receipt.body && Array.isArray((receipt.body as Record<string, unknown>).policy_trace) && (
            <div>
              <span className="text-xs text-gray-500 uppercase tracking-wider">Policy Trace</span>
              <div className="mt-1 space-y-1">
                {((receipt.body as Record<string, unknown>).policy_trace as Array<Record<string, unknown>>).map((entry, i) => (
                  <div
                    key={i}
                    className={`flex items-center gap-2 px-2 py-1 rounded text-xs font-mono ${
                      entry.matched ? 'bg-emerald-500/10 text-emerald-400' : 'bg-gray-800 text-gray-500'
                    }`}
                  >
                    <span className="font-semibold">[{entry.level as string}]</span>
                    <span>{entry.rule_id as string}</span>
                    <span className="text-gray-600">→</span>
                    <span className={entry.action === 'DENY' ? 'text-red-400' : entry.action === 'WARN' ? 'text-amber-400' : 'text-emerald-400'}>
                      {entry.action as string}
                    </span>
                    {Boolean(entry.matched) && <span className="text-emerald-500">✓</span>}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
