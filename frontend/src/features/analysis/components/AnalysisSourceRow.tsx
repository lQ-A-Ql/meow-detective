import { ChevronDown, ChevronRight, Puzzle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { cn } from '@/app/components/ui/utils';
import { TreeConnector } from '@/components/tree/TreeConnector';
import { dataSourcePlatformLabel, sourceKindIconLarge } from '@/lib/data-source-utils';
import type { AnalysisSourceRowData } from './analysis-source-rows';
import type { AnalysisSourceSidebarProps } from './analysis-source-sidebar-types';

interface Props {
  row: AnalysisSourceRowData;
  model: AnalysisSourceSidebarProps;
  onSelectOrToggle: (id: string, selected: boolean, expanded: boolean) => void;
}

export function AnalysisSourceRow({ row, model, onSelectOrToggle }: Props) {
  const { t } = useTranslation();
  const { source, selected } = row;
  if (row.kind === 'source') {
    const SourceIcon = sourceKindIconLarge(source.kind);
    return (
      <Button type="button" variant="forensicsGhost" size="inline" disabled={model.disabled}
        aria-label={source.name} aria-current={selected ? 'true' : undefined} aria-expanded={row.expanded}
        title={t(row.expanded ? 'analysis.sidebar.collapseSource' : 'analysis.sidebar.expandSource')}
        onClick={() => onSelectOrToggle(source.id, selected, row.expanded)}
        className={cn('h-8 w-full min-w-0 justify-start gap-2 border border-transparent px-2 text-left text-[12px] hover:border-forensics-border',
          selected && 'border-forensics-border bg-forensics-surface text-forensics-text')}>
        {row.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        <SourceIcon size={14} className="shrink-0 text-forensics-muted-light" />
        <span className="min-w-0 flex-1 truncate" title={source.name}>{source.name}</span>
        {source.fileCount !== undefined ? <span className="font-mono text-[10px] text-forensics-muted-light">{source.fileCount}</span> : null}
      </Button>
    );
  }
  if (row.kind === 'platform' || row.kind === 'pluginGroup') {
    return <div className="ml-4 border-l border-forensics-border-light py-1 pl-3 text-[10px] text-forensics-muted-light">
      {row.kind === 'platform' ? dataSourcePlatformLabel(source) : t('pluginModule.groupTitle')}
    </div>;
  }
  if (row.kind === 'plugin') {
    const { module } = row;
    const active = model.activePluginId === module.pluginId &&
      (source.platform === 'windows' ? model.activeWindowsTab === 'plugin' : model.activeLinuxTab === 'plugin');
    return (
      <div className="ml-4 border-l border-forensics-border-light pl-1">
        <Button type="button" variant="forensicsGhost" size="inline" disabled={model.disabled}
          aria-label={`${source.name} / ${module.displayName}`} aria-current={active ? 'true' : undefined}
          onClick={() => { if (!selected) model.onSelectDataSource(source.id); model.onSelectPluginModule?.(module.pluginId); }}
          className={cn('h-7 w-full min-w-0 justify-start gap-1 px-1 text-left text-[11px] text-forensics-muted hover:text-forensics-text',
            active && 'bg-forensics-surface text-forensics-text')}>
          <TreeConnector depth={1} isLast={row.isLast} /><Puzzle size={12} />
          <span className="min-w-0 flex-1 truncate" title={module.displayName}>{`${module.displayName}(${module.totalCount})`}</span>
        </Button>
      </div>
    );
  }
  if (row.kind !== 'node') {
    return null;
  }
  const { node } = row;
  const Icon = node.icon;
  const active = selected && (source.platform === 'windows' ? node.windowsTab === model.activeWindowsTab
    : source.platform === 'linux' ? node.linuxTab === model.activeLinuxTab : node.androidTab === model.activeAndroidTab);
  const progress = selected && node.category ? model.progress[node.category] : undefined;
  const summaryCount = selected && source.platform === 'linux' && node.linuxTab ? model.linuxNodeCounts?.[node.linuxTab] : undefined;
  const count = summaryCount ?? (progress && progress.status !== 'idle' ? progress.artifactCount : undefined);
  const label = t(node.labelKey);
  return (
    <div className="ml-4 border-l border-forensics-border-light pl-1">
      <Button type="button" variant="forensicsGhost" size="inline" disabled={model.disabled}
        aria-label={`${source.name} / ${label}`} aria-current={active ? 'true' : undefined}
        onClick={() => {
          if (!selected) model.onSelectDataSource(source.id);
          if (source.platform === 'windows' && node.windowsTab) model.onWindowsTabChange(node.windowsTab);
          if (source.platform === 'linux' && node.linuxTab) model.onLinuxTabChange(node.linuxTab);
          if (source.platform === 'android' && node.androidTab) model.onAndroidTabChange(node.androidTab);
        }}
        className={cn('h-7 w-full min-w-0 justify-start gap-1 px-1 text-left text-[11px] text-forensics-muted hover:text-forensics-text',
          active && 'bg-forensics-surface text-forensics-text')}>
        <TreeConnector depth={1} isLast={row.isLast} /><Icon size={12} className="shrink-0 text-forensics-muted-light" />
        <span className="min-w-0 flex-1 truncate">{count === undefined ? label : `${label}(${count})`}</span>
      </Button>
    </div>
  );
}
