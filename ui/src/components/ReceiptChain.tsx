import { ArrowDown } from 'lucide-react'
import type { Receipt } from '../types'
import ReceiptCard from './ReceiptCard'

interface Props {
  wa: Receipt
  transition?: Receipt
  wf: Receipt
  tipCid: string
  onSelectCid?: (cid: string) => void
}

export default function ReceiptChain({ wa, transition, wf, tipCid, onSelectCid }: Props) {
  return (
    <div className="space-y-2">
      <ReceiptCard cid={wa.body_cid} receipt={wa} onSelectCid={onSelectCid} />
      <div className="flex justify-center">
        <ArrowDown className="w-4 h-4 text-gray-600" />
      </div>
      {transition && (
        <>
          <ReceiptCard cid={transition.body_cid} receipt={transition} onSelectCid={onSelectCid} />
          <div className="flex justify-center">
            <ArrowDown className="w-4 h-4 text-gray-600" />
          </div>
        </>
      )}
      <ReceiptCard cid={wf.body_cid} receipt={wf} onSelectCid={onSelectCid} />
      <div className="mt-2 text-center">
        <span className="text-xs text-gray-500">Tip: </span>
        <code className="text-xs font-mono text-gray-400">{tipCid.slice(0, 24)}...</code>
      </div>
    </div>
  )
}
