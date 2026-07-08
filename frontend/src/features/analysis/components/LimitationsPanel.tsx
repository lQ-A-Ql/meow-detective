import { AlertTriangle } from 'lucide-react';
import type { KnownLimitation } from '@/types/models';

function knownLimitationTone(status: KnownLimitation['status']) {
  switch (status) {
    case 'unsupported':
      return 'border-red-200 bg-red-50 text-red-800';
    case 'notGuaranteed':
      return 'border-amber-200 bg-amber-50 text-amber-900';
    case 'partial':
      return 'border-slate-200 bg-slate-50 text-slate-700';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

function knownLimitationLabel(status: KnownLimitation['status']) {
  switch (status) {
    case 'unsupported':
      return 'Unsupported';
    case 'notGuaranteed':
      return 'Not Guaranteed';
    case 'partial':
      return 'Partial';
    default:
      return status;
  }
}

export function KnownLimitationsPanel({ items }: { items: KnownLimitation[] }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2 text-[14px] font-semibold text-[#111]">
        <AlertTriangle size={16} />
        已知限制
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {items.map((item) => (
          <div key={`${item.category}-${item.item}`} className={`rounded border p-4 ${knownLimitationTone(item.status)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-semibold">{item.item}</div>
                <div className="mt-1 text-[11px] opacity-80">{item.category}</div>
              </div>
              <span className="rounded border border-current/20 bg-white/70 px-2 py-0.5 text-[10px] font-mono">
                {knownLimitationLabel(item.status)}
              </span>
            </div>
            <div className="mt-3 text-[11px]">{item.summary}</div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider opacity-70">Affected Chains</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {item.affectedChains.map((chain) => (
                  <span key={`${item.item}-${chain}`} className="rounded border border-current/20 bg-white/70 px-2 py-1 text-[10px] font-mono">
                    {chain}
                  </span>
                ))}
              </div>
            </div>
            <div className="mt-3 text-[10px] opacity-70">{item.sourceDoc}</div>
          </div>
        ))}
      </div>
    </section>
  );
}
