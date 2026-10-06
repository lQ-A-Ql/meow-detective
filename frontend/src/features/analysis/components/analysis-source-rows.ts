import type { DataSourceSummary, PluginModule } from '@/types/models';
import { ANDROID_NODES, LINUX_NODES, WINDOWS_NODES, WINDOWS_DELETED_RECOVERY_NODE, type SourceTreeNode } from './analysis-source-nodes';

export type AnalysisSourceRowData = {
  key: string;
  source: DataSourceSummary;
  selected: boolean;
} & (
  | { kind: 'source'; expanded: boolean }
  | { kind: 'platform' | 'pluginGroup' }
  | { kind: 'node'; node: SourceTreeNode; isLast: boolean }
  | { kind: 'plugin'; module: PluginModule; isLast: boolean }
);

/** Flatten source groups before windowing, including plugin rows, so a single
 * selected source with many plugins cannot create an unbounded DOM subtree. */
export function buildAnalysisSourceRows(
  sources: DataSourceSummary[], selectedId: string | undefined,
  modules: PluginModule[] | undefined, collapsed: ReadonlySet<string>,
) {
  const rows: AnalysisSourceRowData[] = [];
  for (const source of sources) {
    const selected = source.id === selectedId;
    const expanded = !collapsed.has(source.id);
    const key = (kind: string, id = '') => JSON.stringify([source.id, kind, id]);
    rows.push({ key: key('source'), kind: 'source', source, selected, expanded });
    if (!expanded) continue;
    rows.push({ key: key('platform'), kind: 'platform', source, selected });
    const nodes = source.platform === 'windows' ? [...WINDOWS_NODES, WINDOWS_DELETED_RECOVERY_NODE]
      : source.platform === 'linux' ? LINUX_NODES : ANDROID_NODES;
    const plugins = selected ? (modules ?? []).filter((module) => module.evidencePlatform === source.platform) : [];
    nodes.forEach((node, index) => rows.push({ key: key('node', node.labelKey), kind: 'node', source, selected,
      node, isLast: index === nodes.length - 1 && plugins.length === 0 }));
    if (plugins.length) rows.push({ key: key('pluginGroup'), kind: 'pluginGroup', source, selected });
    plugins.forEach((module, index) => rows.push({ key: key('plugin', module.pluginId), kind: 'plugin', source,
      selected, module, isLast: index === plugins.length - 1 }));
  }
  return rows;
}
