import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Puzzle } from 'lucide-react';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { usePluginFamilyEntries } from '@/features/analysis/hooks';
import type { PluginArtifactEntry, PluginModule } from '@/types/models';
import { EmptyLine, WarningList } from './helpers';

const DYNAMIC_ATTR_COLUMN_CAP = 6;

function attrText(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value);
}

function confidenceText(confidence?: number): string {
  return confidence === undefined ? '' : `${Math.round(confidence * 100)}%`;
}

/**
 * Dynamic columns come from the attrs keys of the currently loaded rows:
 * keys ranked by row coverage (desc), first-seen order breaks ties, capped at
 * DYNAMIC_ATTR_COLUMN_CAP so the table stays readable for chatty plugins.
 */
export function deriveDynamicAttrKeys(rows: readonly PluginArtifactEntry[]): string[] {
  const counts = new Map<string, number>();
  for (const row of rows) {
    for (const key of Object.keys(row.attrs)) {
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, DYNAMIC_ATTR_COLUMN_CAP)
    .map(([key]) => key);
}

function PluginFamilyAttrsDetail({ entry }: { entry: PluginArtifactEntry }) {
  const { t } = useTranslation();
  const attrs = Object.entries(entry.attrs);
  return (
    <div className="border border-forensics-border bg-forensics-panel px-3 py-2">
      <div className="mb-1 flex min-w-0 items-center gap-2 text-[11px] text-forensics-text">
        <span className="shrink-0 text-forensics-muted">{t('pluginModule.attrsTitle')}</span>
        <span className="min-w-0 truncate" title={entry.title}>{entry.title}</span>
      </div>
      {attrs.length === 0 ? (
        <div className="text-[11px] text-forensics-muted">{t('pluginModule.attrsEmpty')}</div>
      ) : (
        <dl className="space-y-0.5">
          {attrs.map(([key, value]) => (
            <div key={key} className="flex min-w-0 gap-2 text-[11px]">
              <dt className="w-44 shrink-0 truncate font-mono text-forensics-muted" title={key}>
                {key}
              </dt>
              <dd className="min-w-0 break-all font-mono text-forensics-text-secondary">
                {attrText(value) || '-'}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}

function PluginFamilyTable({
  dataSourceId,
  pluginId,
  family,
  expectedCount,
  loadContextKey,
}: {
  dataSourceId: string;
  pluginId: string;
  family: string;
  expectedCount: number;
  loadContextKey: string;
}) {
  const { t } = useTranslation();
  const query = usePluginFamilyEntries({ dataSourceId, pluginId, family });
  const rows = useMemo(() => query.data?.entries ?? [], [query.data]);
  const [selectedRowKey, setSelectedRowKey] = useState<string>();
  const dynamicKeys = useMemo(() => deriveDynamicAttrKeys(rows), [rows]);

  const columns = useMemo<DenseColumn<PluginArtifactEntry>[]>(() => {
    const fixed: DenseColumn<PluginArtifactEntry>[] = [
      {
        key: 'title',
        title: t('pluginModule.columns.title'),
        className: 'min-w-[180px]',
        text: (row) => row.title,
        filterable: true,
        render: (row) => row.title || '-',
      },
      {
        key: 'summary',
        title: t('pluginModule.columns.summary'),
        className: 'min-w-[240px]',
        text: (row) => row.summary,
        render: (row) => row.summary || '-',
      },
      {
        key: 'confidence',
        title: t('pluginModule.columns.confidence'),
        className: 'w-[90px]',
        text: (row) => confidenceText(row.confidence),
        render: (row) => confidenceText(row.confidence) || '-',
      },
      {
        key: 'sourcePath',
        title: t('pluginModule.columns.sourcePath'),
        className: 'min-w-[220px]',
        text: (row) => row.sourcePath,
        filterable: true,
        render: (row) => row.sourcePath || '-',
      },
    ];
    const dynamic: DenseColumn<PluginArtifactEntry>[] = dynamicKeys.map((key) => ({
      key: `attr:${key}`,
      title: key,
      className: 'min-w-[140px]',
      text: (row) => attrText(row.attrs[key]),
      render: (row) => attrText(row.attrs[key]) || '-',
    }));
    return [...fixed, ...dynamic];
  }, [dynamicKeys, t]);

  const totalCount = query.data?.totalCount ?? expectedCount;
  const truncated = query.data?.truncated ?? false;
  const selectedEntry = selectedRowKey
    ? rows.find((row) => row.artifactId === selectedRowKey)
    : undefined;

  return (
    <div className="space-y-1">
      {query.isLoading ? (
        <div className="px-1 text-[11px] text-forensics-muted">{t('pluginModule.loading')}</div>
      ) : null}
      {!query.isLoading && (rows.length < totalCount || truncated) ? (
        <div className="flex flex-wrap items-center gap-2 px-1 text-[11px]">
          <span className="font-mono text-forensics-muted">
            {t('pluginModule.loadProgress', { loaded: rows.length, total: totalCount })}
          </span>
          {truncated && totalCount > 0 ? (
            <span className="text-forensics-warning-text">
              {t('pluginModule.truncatedWarning')}
            </span>
          ) : null}
        </div>
      ) : null}
      <DenseDataTableFrame rowCount={rows.length}>
        <DenseDataTable
          rows={rows}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          selectedRowKey={selectedRowKey}
          onRowClick={(row) => {
            setSelectedRowKey((current) => (current === row.artifactId ? undefined : row.artifactId));
          }}
          emptyTitle={t('pluginModule.empty.title')}
          emptyDescription={t('pluginModule.empty.description')}
          filterable
          horizontalScroll
          loadContextKey={loadContextKey}
          loadStateKey={query.dataUpdatedAt}
          hasMore={Boolean(query.hasNextPage)}
          loadingMore={query.isFetchingNextPage}
          loadMoreFailed={query.isFetchNextPageError}
          loadMoreErrorText={t('pluginModule.loadError')}
          onReachEnd={() => {
            void query.fetchNextPage();
          }}
          onRetryLoadMore={() => query.refetch()}
          initialLoadFailed={query.isError}
          initialLoadErrorText={t('pluginModule.loadError')}
          retryInitialLoadLabel={t('pluginModule.retry')}
          onRetryInitialLoad={() => {
            void query.refetch();
          }}
        />
      </DenseDataTableFrame>
      {selectedEntry ? <PluginFamilyAttrsDetail entry={selectedEntry} /> : null}
    </div>
  );
}

export interface PluginModulePanelProps {
  dataSourceId: string;
  module: PluginModule;
  /** Source-context key; combined with pluginId/family to reset table state. */
  loadContextKey?: string;
}

/** Generic plugin module panel: one dense table per declared family. */
export function PluginModulePanel({
  dataSourceId,
  module,
  loadContextKey,
}: PluginModulePanelProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="flex items-center gap-2 text-[14px] font-light text-forensics-text">
            <Puzzle size={16} />
            {module.displayName}
          </h3>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-forensics-muted">
            <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-tertiary">
              v{module.pluginVersion}
            </span>
            <span>
              {t('pluginModule.totalCount')}: {module.totalCount}
            </span>
          </div>
        </div>
      </div>
      {module.warnings.length > 0 ? <WarningList warnings={module.warnings} /> : null}
      {module.families.length === 0 ? (
        <EmptyLine text={t('pluginModule.empty.description')} />
      ) : (
        module.families.map((family) => (
          <section key={family.family}>
            <div className="mb-2 text-[12px] font-light text-forensics-text">
              {family.family} ({family.count})
            </div>
            <PluginFamilyTable
              dataSourceId={dataSourceId}
              pluginId={module.pluginId}
              family={family.family}
              expectedCount={family.count}
              loadContextKey={`${loadContextKey ?? ''}:${module.pluginId}:${family.family}`}
            />
          </section>
        ))
      )}
    </div>
  );
}
