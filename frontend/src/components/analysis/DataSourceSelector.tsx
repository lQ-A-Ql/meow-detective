import { Monitor, Server, Folder, Database } from 'lucide-react';
import { ToggleGroup, ToggleGroupItem } from '@/app/components/ui/toggle-group';
import { cn } from '@/app/components/ui/utils';
import type { DataSourceSummary } from '@/types/models';

function sourceKindLabel(kind: string): string {
  switch (kind) {
    case 'e01':
      return 'E01';
    case 'raw':
      return 'RAW';
    case 'logical_directory':
      return '目录';
    default:
      return kind;
  }
}

function sourceKindIcon(kind: string) {
  switch (kind) {
    case 'e01':
      return Database;
    case 'raw':
      return Server;
    case 'logical_directory':
      return Folder;
    default:
      return Monitor;
  }
}

export interface DataSourceSelectorProps {
  dataSources: DataSourceSummary[];
  selectedId?: string;
  onSelect: (id?: string) => void;
  className?: string;
}

export function DataSourceSelector({
  dataSources,
  selectedId,
  onSelect,
  className,
}: DataSourceSelectorProps) {
  if (dataSources.length === 0) {
    return null;
  }

  return (
    <ToggleGroup
      type="single"
      value={selectedId ?? ''}
      onValueChange={(value) => onSelect(value || undefined)}
      className={cn('flex-wrap', className)}
    >
      <ToggleGroupItem
        value=""
        aria-label="全部数据源"
        className="h-7 px-2.5 text-[11px]"
      >
        全部数据源
      </ToggleGroupItem>
      {dataSources.map((ds) => {
        const Icon = sourceKindIcon(ds.kind);
        return (
          <ToggleGroupItem
            key={ds.id}
            value={ds.id}
            aria-label={ds.name}
            className="h-7 gap-1.5 px-2.5 text-[11px]"
          >
            <Icon size={12} className="text-forensics-muted-light shrink-0" />
            <span className="truncate max-w-[160px]">{ds.name}</span>
            <span className="text-[10px] text-forensics-muted-light shrink-0">
              ({sourceKindLabel(ds.kind)})
            </span>
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}
