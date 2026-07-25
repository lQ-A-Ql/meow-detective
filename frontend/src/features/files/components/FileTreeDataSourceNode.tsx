import { memo } from 'react';
import { Monitor, ChevronDown, ChevronRight } from 'lucide-react';
import type { FileTreeNode, DataSourceSummary } from '@/types/models';
import { sourceKindLabel, sourceKindIcon } from '@/lib/data-source-utils';

export interface FileTreeDataSourceNodeProps {
  node: FileTreeNode;
  expanded: boolean;
  dataSource: DataSourceSummary | undefined;
  onClick: () => void;
}

export const FileTreeDataSourceNode = memo(function FileTreeDataSourceNode({
  node,
  expanded,
  dataSource,
  onClick,
}: FileTreeDataSourceNodeProps) {
  const Icon = dataSource ? sourceKindIcon(dataSource.kind) : Monitor;
  const kind = dataSource?.kind ?? '';

  return (
    <div
      className="flex min-w-0 items-center overflow-hidden cursor-pointer hover:bg-forensics-hover py-0.5"
      style={{ paddingLeft: 8 }}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
    >
      {node.hasChildren ? (
        expanded ? (
          <ChevronDown size={10} className="text-forensics-500 mr-1 shrink-0" />
        ) : (
          <ChevronRight size={10} className="text-forensics-500 mr-1 shrink-0" />
        )
      ) : (
        <span className="w-[10px] mr-1 shrink-0" />
      )}
      <Icon size={13} className="text-forensics-primary-blue mr-1.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate font-light text-forensics-text text-[11px]">
        {node.name}
      </span>
      {kind ? (
        <span className="ml-1.5 shrink-0 text-[9px] text-forensics-muted-light bg-forensics-surface-muted px-1 py-px rounded-none">
          {sourceKindLabel(kind)}
        </span>
      ) : null}
    </div>
  );
});
