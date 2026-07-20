import { AlertTriangle } from 'lucide-react';
import type { KnownLimitation } from '@/types/models';

function knownLimitationTone(status: KnownLimitation['status']) {
  switch (status) {
    case 'unsupported':
      return 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text';
    case 'notGuaranteed':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'partial':
      return 'border-forensics-border bg-forensics-panel text-forensics-muted';
    default:
      return 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
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
      <div className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
        <AlertTriangle size={16} />
        已知限制
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {items.map((item) => (
          <div key={`${item.category}-${item.item}`} className={`rounded-none border p-4 ${knownLimitationTone(item.status)}`}>
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="text-[13px] font-light">{item.item}</div>
                <div className="mt-1 text-[11px] opacity-80">{item.category}</div>
              </div>
              <span className="rounded-none border border-current/20 bg-forensics-surface/70 px-2 py-0.5 text-[10px] font-mono">
                {knownLimitationLabel(item.status)}
              </span>
            </div>
            <div className="mt-3 text-[11px]">{item.summary}</div>
            <div className="mt-3">
              <div className="text-[10px] uppercase tracking-wider opacity-70">Affected Chains</div>
              <div className="mt-2 flex flex-wrap gap-2">
                {item.affectedChains.map((chain) => (
                  <span key={`${item.item}-${chain}`} className="rounded-none border border-current/20 bg-forensics-surface/70 px-2 py-1 text-[10px] font-mono">
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
