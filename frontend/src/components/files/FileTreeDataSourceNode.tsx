import { Monitor, Server, Folder, ChevronDown, ChevronRight, HardDrive } from 'lucide-react';
import type { FileTreeNode } from '@/types/models';
import type { DataSourceSummary } from '@/types/models';

function sourceIcon(kind: string) {
  switch (kind) {
    case 'e01':
      return HardDrive;
    case 'raw':
      return Server;
    case 'logical_directory':
      return Folder;
    default:
      return Monitor;
  }
}

function kindLabel(kind: string): string {
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

export interface FileTreeDataSourceNodeProps {
  node: FileTreeNode & { active: boolean; expanded: boolean };
  dataSource: DataSourceSummary | undefined;
  onClick: () => void;
}

export function FileTreeDataSourceNode({
  node,
  dataSource,
  onClick,
}: FileTreeDataSourceNodeProps) {
  const Icon = dataSource ? sourceIcon(dataSource.kind) : Monitor;
  const kind = dataSource?.kind ?? '';

  return (
    <div
      className="flex items-center cursor-pointer hover:bg-forensics-hover py-0.5"
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
        node.expanded ? (
          <ChevronDown size={10} className="text-forensics-500 mr-1 shrink-0" />
        ) : (
          <ChevronRight size={10} className="text-forensics-500 mr-1 shrink-0" />
        )
      ) : (
        <span className="w-[10px] mr-1 shrink-0" />
      )}
      <Icon size={13} className="text-forensics-primary-blue mr-1.5 shrink-0" />
      <span className="truncate font-medium text-forensics-text text-[11px]">
        {node.name}
      </span>
      <span className="ml-1.5 shrink-0 text-[9px] text-forensics-muted-light bg-forensics-surface-muted px-1 py-px rounded">
        {kindLabel(kind)}
      </span>
    </div>
  );
}
