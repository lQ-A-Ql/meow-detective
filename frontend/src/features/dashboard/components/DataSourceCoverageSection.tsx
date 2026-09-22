import { GitBranch, Layers } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { DataSourceSummary } from '@/types/models';

export function DataSourceCoverageSection({
  dataSources,
  isLoading,
  isError,
  error,
}: {
  dataSources: DataSourceSummary[] | undefined;
  isLoading?: boolean;
  isError?: boolean;
  error?: unknown;
}) {
  const { t } = useTranslation();
  return (
    <section>
      <SectionHeader icon={Layers} title={t('dashboard.sources.title')} subtitle={t('dashboard.sources.subtitle')} />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={dataSources !== undefined}>
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricCard label={t('dashboard.sources.count')} value={dataSources?.length ?? 0} icon={Layers} size="lg" />
        <MetricCard label={t('dashboard.sources.partitions')} value={dataSources?.reduce((sum, ds) => sum + (ds.partitions?.length ?? 0), 0) ?? 0} icon={GitBranch} size="lg" />
        <MetricCard label={t('dashboard.sources.e01')} value={dataSources?.filter((ds) => ds.kind === 'e01').length ?? 0} subtitle={t('dashboard.sources.ewf')} size="lg" />
        <MetricCard label={t('dashboard.sources.raw')} value={dataSources?.filter((ds) => ds.kind === 'raw').length ?? 0} subtitle={t('dashboard.sources.rawSubtitle')} size="lg" />
      </div>
      {dataSources && dataSources.length > 0 ? (
        <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
          <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">{t('dashboard.sources.details')}</div>
          <div className="space-y-2">
            {dataSources.map((ds) => (
              <div
                key={ds.id}
                className="flex items-center justify-between border-b border-forensics-hover pb-2 text-xs last:border-none last:pb-0"
              >
                <div className="min-w-0 flex-1 truncate">
                  <span className="font-mono text-forensics-text">{ds.name}</span>
                  <span className="ml-2 font-mono text-forensics-500">{ds.kind}</span>
                </div>
                <div className="flex items-center gap-4 font-mono text-forensics-muted">
                  {ds.fileCount !== undefined ? <span>{t('dashboard.sources.files', { count: ds.fileCount })}</span> : null}
                  <span>{t('dashboard.sources.partitionCount', { count: ds.partitions?.length ?? 0 })}</span>
                  {ds.readerKind ? <span className="text-forensics-muted-light">{ds.readerKind}</span> : null}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : <div className="mt-3 rounded-none border border-dashed border-forensics-border-strong bg-forensics-panel p-6 text-center text-[12px] text-forensics-muted-lighter">{t('dashboard.sources.empty')}</div>}
      </DashboardQueryState>
    </section>
  );
}
