interface StatusChipProps {
  label: string;
  detail?: string;
  tone: string;
}

export function StatusChip({ label, detail, tone }: StatusChipProps) {
  return (
    <span className={`border px-1.5 py-0.5 text-[10px] font-light ${toneClass(tone)}`}>
      {label}
      {detail ? <span className="ml-1 font-mono opacity-80">{detail}</span> : null}
    </span>
  );
}

function toneClass(tone: string) {
  switch (tone) {
    case 'ready':
    case 'reused':
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'pending':
    case 'partial':
    case 'warming':
    case 'warning':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'failed':
    case 'stale':
    case 'invalidated':
    case 'cancelled':
      return 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text';
    default:
      return 'border-forensics-350 bg-forensics-surface text-forensics-text-tertiary';
  }
}
