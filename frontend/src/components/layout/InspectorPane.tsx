import { PropsWithChildren, ReactNode } from 'react';

interface InspectorPaneProps extends PropsWithChildren {
  title: ReactNode;
  widthClassName?: string;
  subtitle?: ReactNode;
  className?: string;
}

export function InspectorPane({ title, widthClassName = 'w-72', subtitle, className, children }: InspectorPaneProps) {
  return (
    <aside className={`${widthClassName} shrink-0 border-l border-forensics-border bg-forensics-panel flex flex-col ${className ?? ''}`}>
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel-strong px-4 py-2">
        <div className="text-[10px] font-semibold tracking-wider text-forensics-text-tertiary uppercase">{title}</div>
        {subtitle ? <div className="mt-1 text-[10px] font-mono text-forensics-muted-light truncate">{subtitle}</div> : null}
      </div>
      <div className="flex-1 overflow-auto p-4">{children}</div>
    </aside>
  );
}

export function InspectorSection({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="space-y-2 border-t border-forensics-border pt-4 first:border-t-0 first:pt-0">
      <div className="text-[10px] uppercase tracking-wide text-forensics-muted-light">{title}</div>
      <div>{children}</div>
    </section>
  );
}

export function InspectorValue({ value, mono, strong }: { value: ReactNode; mono?: boolean; strong?: boolean }) {
  return (
    <div
      className={[
        'break-all border border-forensics-border-strong bg-forensics-surface p-2 text-[11px]',
        mono ? 'font-mono' : 'font-sans',
        strong ? 'font-medium text-forensics-text' : 'text-forensics-text-secondary',
      ].join(' ')}
    >
      {value}
    </div>
  );
}
