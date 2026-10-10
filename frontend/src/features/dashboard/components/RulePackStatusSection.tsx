import { Activity, BarChart3, Shield } from 'lucide-react';
import { ImportDataAction } from '@/components/layout/EmptyStateActions';
import { useTranslation } from 'react-i18next';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { StatGrid, EmptyState, MetricCard } from '@/components/data-display';
import type { RulePackStatus } from '@/types/models';

export function RulePackStatusSection({ data, isLoading, isError, error }: { data: RulePackStatus | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  const { t } = useTranslation();
  return (
    <section>
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <>
          <StatGrid collapseZeros hiddenNote={(hidden) => t('dashboard.metricsHidden', { count: hidden })} className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
            <MetricCard label={t('dashboard.rulePacks.loaded')} value={data.loadedPacks.length} icon={Shield} size="lg" />
            <MetricCard label={t('dashboard.rulePacks.totalRules')} value={data.totalRuleCount} icon={BarChart3} size="lg" />
            <MetricCard label={t('dashboard.rulePacks.definitionStatus')} value={data.loadStatus} icon={Activity} size="lg" />
            <MetricCard label={t('dashboard.rulePacks.executionStatus')} value={data.executionStatus} icon={Activity} size="lg" />
          </StatGrid>
          {data.loadedPacks.length > 0 && (
            <div className="mt-3 rounded-none bg-transparent p-4">
              <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">{t('dashboard.rulePacks.loaded')}</div>
              <div className="space-y-1">
                {data.loadedPacks.map((pack) => (
                  <div key={pack.name} className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-forensics-text">{pack.name}</span>
                      <span className="font-mono text-forensics-500">v{pack.version}</span>
                    </div>
                    <div className="flex items-center gap-3 font-mono text-forensics-muted">
                      <span>{t('dashboard.rulePacks.ruleCount', { count: pack.ruleCount })}</span>
                      <span className="text-forensics-muted-light">{pack.author}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      ) : <EmptyState className="mt-3" title={t('dashboard.rulePacks.empty')} action={<ImportDataAction />} />}
      </DashboardQueryState>
    </section>
  );
}
