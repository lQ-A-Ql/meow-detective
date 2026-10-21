import type { DataSourceSummary } from '@/types/models';
import { useLinuxRootMountModel } from '../use-linux-root-mount-model';

export function LinuxRootMounts({ source }: { source: DataSourceSummary }) {
  const model = useLinuxRootMountModel(source);
  if (!model.visible) return null;
  return (
    <div className="mt-2 space-y-1 text-[10px] text-forensics-muted-light">
      {model.entries.map((entry) => (
        <div key={entry.id} className="break-all font-mono">{entry.text}</div>
      ))}
      <div>{model.hint}</div>
    </div>
  );
}
