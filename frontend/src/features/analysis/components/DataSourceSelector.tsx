import { ToggleGroup, ToggleGroupItem } from '@/app/components/ui/toggle-group';
import { cn } from '@/app/components/ui/utils';
import type { DataSourceSummary } from '@/types/models';
import { dataSourcePlatformLabel, sourceKindIconLarge } from '@/lib/data-source-utils';

export interface DataSourceSelectorProps {
  dataSources: DataSourceSummary[];
  selectedId?: string;
  onSelect: (id: string) => void;
  disabled?: boolean;
  className?: string;
}

export function DataSourceSelector({
  dataSources,
  selectedId,
  onSelect,
  disabled = false,
  className,
}: DataSourceSelectorProps) {
  if (dataSources.length === 0) {
    return null;
  }

  return (
    <ToggleGroup
      type="single"
      value={selectedId ?? ''}
      disabled={disabled}
      onValueChange={(value) => {
        if (!disabled && value) {
          onSelect(value);
        }
      }}
      className={cn('flex-wrap', className)}
    >
      {dataSources.map((ds) => {
        const Icon = sourceKindIconLarge(ds.kind);
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
              ({dataSourcePlatformLabel(ds)})
            </span>
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}
