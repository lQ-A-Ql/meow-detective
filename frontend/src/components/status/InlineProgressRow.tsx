interface InlineProgressRowProps {
  title: string;
  subtitle: string;
  detail: string;
  progress: number;
}

export function InlineProgressRow({ title, subtitle, detail, progress }: InlineProgressRowProps) {
  return (
    <div className="flex items-start gap-3">
      <div className="mt-0.5 h-2 w-2 rounded-full bg-[#111]" />
      <div className="flex-1">
        <div className="flex items-center justify-between gap-4">
          <div className="text-[13px] font-medium text-[#111]">{title}</div>
          <div className="font-mono text-[10px] text-[#888]">{detail}</div>
        </div>
        <div className="mt-0.5 text-[11px] text-[#555]">{subtitle}</div>
        <div className="mt-2 h-1 w-full overflow-hidden border border-[#e0e0e0] bg-[#eee]">
          <div className="h-full bg-[#111]" style={{ width: `${progress}%` }} />
        </div>
      </div>
    </div>
  );
}
