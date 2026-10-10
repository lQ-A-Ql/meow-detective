import { Puzzle } from 'lucide-react';
import { TreeRow } from '@/components/tree/TreeRow';
import { useTranslation } from 'react-i18next';


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
      <TreeRow
        depth={0}
        expanded={row.expanded}
        hasChildren
        selected={selected}
        active={selected}
        ariaLabel={source.name}
        disabled={model.disabled}
        onActivate={() => onSelectOrToggle(source.id, selected, row.expanded)}
        onToggle={() => onSelectOrToggle(source.id, selected, row.expanded)}
        toggleLabel={t(row.expanded ? 'analysis.sidebar.collapseSource' : 'analysis.sidebar.expandSource')}
        icon={<SourceIcon size={14} className="shrink-0 text-forensics-ink-3" />}
        label={source.name}
        meta={source.fileCount !== undefined ? <span className="font-mono text-[10px] text-forensics-ink-3">{source.fileCount}</span> : null}
        className="h-8"
      />
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
        <TreeRow
          depth={1}
          active={active}
          selected={active}
          disabled={model.disabled}
          ariaLabel={`${source.name} / ${module.displayName}`}
          connector={<TreeConnector depth={1} isLast={row.isLast} />}
          icon={<Puzzle size={12} className="shrink-0" />}
          label={module.displayName}
          meta={<span className="font-mono text-[10px] text-forensics-ink-3">({module.totalCount})</span>}
          onActivate={() => {
            if (!selected) model.onSelectDataSource(source.id);
            model.onSelectPluginModule?.(module.pluginId);
          }}
          className="h-7 text-[11px]"
        />
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
      <TreeRow
        depth={1}
        active={active}
        selected={active}
        disabled={model.disabled}
        ariaLabel={`${source.name} / ${label}`}
        connector={<TreeConnector depth={1} isLast={row.isLast} />}
        icon={<Icon size={12} className="shrink-0" />}
        label={count === undefined ? label : `${label}(${count})`}
        onActivate={() => {
          if (!selected) model.onSelectDataSource(source.id);
          if (source.platform === 'windows' && node.windowsTab) model.onWindowsTabChange(node.windowsTab);
          if (source.platform === 'linux' && node.linuxTab) model.onLinuxTabChange(node.linuxTab);
          if (source.platform === 'android' && node.androidTab) model.onAndroidTabChange(node.androidTab);
        }}
        className="h-7 text-[11px]"
      />
    </div>
  );
}
