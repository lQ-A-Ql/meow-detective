import { Globe, Layers, Monitor, Server } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { DashboardQueryState } from '@/features/dashboard/components/DashboardQueryState';
import { MetricCard, SectionHeader } from '@/components/data-display';
import type { PlatformCoverage } from '@/types/models';

export function PlatformCoverageSection({ data, isLoading, isError, error }: { data: PlatformCoverage | undefined; isLoading?: boolean; isError?: boolean; error?: unknown }) {
  const { t } = useTranslation();
  return (
    <section>
      <SectionHeader icon={Layers} title={t('dashboard.platforms.title')} subtitle={t('dashboard.platforms.subtitle')} />
      <DashboardQueryState isLoading={isLoading} isError={isError} error={error} hasData={data !== undefined}>
      {data ? (
        <>
          <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
            <MetricCard label={t('dashboard.platforms.windows')} value={data.windowsArtifactFamilies} subtitle={t('dashboard.platforms.familyUnit')} icon={Monitor} size="lg" />
            <MetricCard label={t('dashboard.platforms.linux')} value={data.linuxArtifactFamilies} subtitle={t('dashboard.platforms.familyUnit')} icon={Server} size="lg" />
            <MetricCard label={t('dashboard.platforms.crossPlatform')} value={data.crossPlatformArtifactFamilies} subtitle={t('dashboard.platforms.familyUnit')} icon={Globe} size="lg" />
            <MetricCard label={t('dashboard.platforms.unknown')} value={data.unknownArtifactFamilies} subtitle={t('dashboard.platforms.familyUnit')} icon={Layers} size="lg" />
          </div>
          <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface p-4">
            <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-forensics-muted-light">{t('dashboard.platforms.details')}</div>
            <div className="space-y-3">
              {data.windowsFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Monitor size={12} /> {t('dashboard.platforms.windows')}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.windowsFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.crossPlatformFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Globe size={12} /> {t('dashboard.platforms.crossPlatform')}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.crossPlatformFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-info-bg px-2 py-0.5 font-mono text-[10px] text-forensics-info-text">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.linuxFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary">
                    <Server size={12} /> {t('dashboard.platforms.linux')}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.linuxFamilies.map((f) => (
                      <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">
                        {f}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {data.unknownFamilies.length > 0 && (
                <div>
                  <div className="mb-1 flex items-center gap-1.5 text-[11px] font-light text-forensics-text-tertiary"><Layers size={12} /> {t('dashboard.platforms.unknown')}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {data.unknownFamilies.map((f) => <span key={f} className="rounded-none border border-forensics-border bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px] text-forensics-text-secondary">{f}</span>)}
                  </div>
                </div>
              )}
            </div>
          </div>
        </>
      ) : null}
      </DashboardQueryState>
    </section>
  );
}
