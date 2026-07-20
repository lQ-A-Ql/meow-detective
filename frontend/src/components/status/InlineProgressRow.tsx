interface InlineProgressRowProps {
  title: string;
  subtitle: string;
  detail: string;
  progress: number;
}

export function InlineProgressRow({ title, subtitle, detail, progress }: InlineProgressRowProps) {
  return (
    <div className="flex items-start gap-3">
      <div className="mt-0.5 h-2 w-2 rounded-none bg-forensics-primary-blue" />
      <div className="flex-1">
        <div className="flex items-center justify-between gap-4">
          <div className="text-[13px] font-light text-forensics-text">{title}</div>
          <div className="font-mono text-[10px] text-forensics-muted-light">{detail}</div>
        </div>
        <div className="mt-0.5 text-[11px] text-forensics-text-tertiary">{subtitle}</div>
        <div className="mt-2 h-1 w-full overflow-hidden border border-forensics-border bg-forensics-panel-strong">
          <div className="h-full bg-forensics-primary-blue" style={{ width: `${progress}%` }} />
        </div>
      </div>
    </div>
  );
}
