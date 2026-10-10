import { Download, FileText, RefreshCw, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import type { EvidenceClassificationSummary } from '@/types/models';
import {
  CATEGORY_COLORS,
  CATEGORY_ICONS,
  EmptyLine,
  formatSize,
  Metric,
  StatusPill,
  statusLabel,
  SummaryStrip,
  WarningList,
} from './helpers';
import { TokenIcon } from '@/components/data-display';

export function EvidenceClassificationPanel({
  summary,
  pending,
  onRun,
}: {
  summary?: EvidenceClassificationSummary;
  pending: boolean;
  onRun: () => void;
}) {
  const { t } = useTranslation();
  const info = summary ?? {
    status: 'unavailable' as const,
    categories: [],
    generatedAt: '',
    warnings: [t('analysisPanels.classification.unavailable')],
    totals: {
      categoryCount: 0,
      candidateFileCount: 0,
      totalSize: 0,
      artifactCount: 0,
    },
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-[14px] font-light text-forensics-text">{t('analysisPanels.classification.title')}</h3>
          <div className="mt-1 text-[11px] leading-5 text-forensics-muted">
            {t('analysisPanels.classification.description')}
          </div>
        </div>
        <Button
          type="button"
          onClick={onRun}
          disabled={pending}
          className="h-8 rounded-none border border-forensics-text bg-forensics-text px-4 text-[12px] text-white hover:bg-forensics-text-secondary"
        >
          {pending ? <RefreshCw size={14} className="opacity-70" /> : <Shield size={14} />}
          {pending ? t('analysisPanels.classification.running') : t('analysisPanels.classification.run')}
        </Button>
      </div>

      <SummaryStrip
        items={[
          [t('analysisPanels.classification.statFamilies'), info.totals.categoryCount.toString()],
          [t('analysisPanels.classification.statCandidateFiles'), info.totals.candidateFileCount.toString()],
          [t('analysisPanels.classification.statCandidateSize'), formatSize(info.totals.totalSize)],
          [t('analysisPanels.classification.statParsedArtifacts'), info.totals.artifactCount.toString()],
        ]}
      />

      {info.warnings.length > 0 ? <WarningList warnings={info.warnings} /> : null}

      {info.categories.length === 0 ? (
        <EmptyLine text={t('analysisPanels.classification.noData')} />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {info.categories.map((category) => {
            const Icon = CATEGORY_ICONS[category.category] || Shield;
            const color = CATEGORY_COLORS[category.category] || 'var(--forensics-text-secondary)';
            return (
              <section key={category.category} className="rounded-none border border-forensics-border bg-forensics-surface p-4">
                <div className="mb-3 flex items-start justify-between gap-3">
                  <div className="flex items-center gap-2">
                    <TokenIcon icon={Icon} size={17} color={color} />
                    <div>
                      <h4 className="text-[13px] font-light text-forensics-text">{category.displayName}</h4>
                      <div className="font-mono text-[10px] text-forensics-muted-light">{category.category}</div>
                    </div>
                  </div>
                  <StatusPill status={category.status} />
                </div>

                <div className="mb-3 grid grid-cols-2 gap-2 text-[11px]">
                  <Metric label={t('infrastructure.states.candidate')} value={t('analysisPanels.classification.metricCandidateCount', { count: category.fileCount })} />
                  <Metric label={t('common.sort.size')} value={formatSize(category.totalSize)} />
                  <Metric label="Artifact" value={t('analysisPanels.classification.metricArtifactCount', { count: category.artifactCount })} />
                  <Metric label={t('graph.details.confidence')} value={`${Math.round(category.confidence * 100)}%`} />
                </div>

                {category.warnings.length > 0 ? <WarningList warnings={category.warnings.slice(0, 2)} /> : null}

                {category.sources.length > 0 ? (
                  <div className="space-y-2">
                    {category.sources.slice(0, 4).map((source) => (
                      <div key={source.fileId} className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="truncate font-mono text-[11px] text-forensics-text-secondary">{source.path}</div>
                          <span className="shrink-0 text-[10px] text-forensics-muted">{statusLabel(source.status)}</span>
                        </div>
                        <div className="mt-1 flex flex-wrap gap-2 font-mono text-[10px] text-forensics-muted-light">
                          <span>{source.evidenceKind}</span>
                          <span>{source.parser}</span>
                          <span>{formatSize(source.size)}</span>
                          <span>artifacts={source.artifactCount}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <EmptyLine text={t('analysisPanels.classification.noSources')} />
                )}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function AnalysisReportPanel({
  pending,
  onDownload,
}: {
  pending: boolean;
  onDownload: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex h-64 flex-col items-center justify-center gap-4">
      <FileText size={48} className="text-forensics-muted-lighter" />
      <div className="text-[14px] text-forensics-muted">{t('analysisPanels.report.title')}</div>
      <Button
        type="button"
        onClick={onDownload}
        disabled={pending}
        className="h-9 rounded-none bg-forensics-text px-6 text-[12px] text-white hover:bg-forensics-text-secondary"
      >
        {pending ? <RefreshCw size={14} className="opacity-70" /> : <Download size={14} />}
        {t('analysisPanels.report.download')}
      </Button>
    </div>
  );
}
