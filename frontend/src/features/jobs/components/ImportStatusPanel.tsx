import { useTranslation } from 'react-i18next';
import {
  getCacheStateLabel,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  getFreshnessLabel,
  getImportPhaseLabel,
  getImportPhaseStateLabel,
  getPartialKindLabel,
} from '@/features/jobs/import-event-state';
import type { BottomDrawerModel } from '@/features/jobs/model/bottom-drawer-model';
import { StatusChip } from '@/features/jobs/components/StatusChip';

type ImportStatusPanelProps = Pick<BottomDrawerModel, 'importSignals' | 'evidenceHashStatus'>;

export function ImportStatusPanel({ importSignals, evidenceHashStatus }: ImportStatusPanelProps) {
  const { t } = useTranslation();
  const visible = Boolean(
    importSignals.latestPhase ||
      importSignals.latestCancellation ||
      importSignals.partialResults.length ||
      importSignals.cacheStatuses.length ||
      importSignals.latestReport,
  );
  if (!visible) return null;

  return (
    <div className="border border-forensics-350 bg-forensics-surface p-3 text-[11px]">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[10px] font-light uppercase tracking-wider text-forensics-muted">
            {t('bottomDrawer.importSignals.title')}
          </div>
          <div className="mt-1 font-light text-forensics-text">
            {importSignals.latestPhase
              ? `${getImportPhaseLabel(importSignals.latestPhase.phase)} · ${getImportPhaseStateLabel(importSignals.latestPhase.state)}`
              : importSignals.latestCancellation
                ? `${t('bottomDrawer.labels.cancellation')} · ${getCacheStateLabel(importSignals.latestCancellation.state)}`
                : t('bottomDrawer.importSignals.waiting')}
          </div>
          <div className="mt-1 text-forensics-muted">
            {importSignals.latestCancellation?.detail ||
              importSignals.latestPhase?.detail ||
              t('bottomDrawer.importSignals.noStatus')}
          </div>
        </div>
        {importSignals.latestPhase ? (
          <div className="text-right">
            <div className="font-mono text-forensics-text">
              {importSignals.latestPhase.percent}{t('bottomDrawer.importSignals.percent')}
            </div>
            <div className="text-[10px] text-forensics-muted-light">{importSignals.lastUpdatedAt ?? '-'}</div>
          </div>
        ) : null}
      </div>
      {importSignals.latestPhase ? (
        <div className="mt-2 flex items-center gap-2">
          <div className="h-1 flex-1 overflow-hidden border border-forensics-border bg-forensics-200">
            <div className="h-full bg-forensics-text" style={{ width: `${importSignals.latestPhase.percent}%` }} />
          </div>
          <span className="font-mono text-[10px] text-forensics-text-tertiary">
            {importSignals.latestPhase.metrics.rowsProcessed}
            {importSignals.latestPhase.metrics.rowsTotal ? `/${importSignals.latestPhase.metrics.rowsTotal}` : ''}
          </span>
        </div>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-1.5">
        {importSignals.latestCancellation ? (
          <StatusChip
            tone={importSignals.latestCancellation.safeToClose ? 'ready' : 'warning'}
            label={
              importSignals.latestCancellation.safeToClose
                ? t('bottomDrawer.labels.safeToClose')
                : getCacheStateLabel(importSignals.latestCancellation.state)
            }
          />
        ) : null}
        {importSignals.partialResults.slice(0, 4).map((result) => (
          <StatusChip
            key={`${result.kind}-${result.scopeId}-${result.queryKey}`}
            tone={result.freshness}
            label={`${getPartialKindLabel(result.kind)} ${getFreshnessLabel(result.freshness)}`}
            detail={`${result.readyCount}${result.totalEstimate ? `/${result.totalEstimate}` : ''}`}
          />
        ))}
        {importSignals.cacheStatuses.slice(0, 3).map((status) => (
          <StatusChip
            key={status.cacheKey}
            tone={status.state}
            label={cacheKeyLabel(status.cacheKey, t)}
            detail={getCacheStateLabel(status.state)}
          />
        ))}
        {evidenceHashStatus ? (
          <StatusChip
            tone={evidenceHashStatus}
            label={`${t('bottomDrawer.labels.evidenceHash')} ${getEvidenceHashStatusLabel(evidenceHashStatus)}`}
          />
        ) : null}
        {importSignals.latestPhase && importSignals.latestPhase.metrics.warnings > 0 ? (
          <StatusChip
            tone="warning"
            label={t('bottomDrawer.labels.warnings')}
            detail={importSignals.latestPhase.metrics.warnings.toString()}
          />
        ) : null}
        {importSignals.latestPhase && importSignals.latestPhase.metrics.failed > 0 ? (
          <StatusChip
            tone="failed"
            label={t('bottomDrawer.labels.failed')}
            detail={importSignals.latestPhase.metrics.failed.toString()}
          />
        ) : null}
      </div>
      {evidenceHashStatus && evidenceHashStatus !== 'ready' ? (
        <div className="mt-2 border border-forensics-warning-border bg-forensics-warning-bg px-2 py-1.5 text-forensics-warning-text">
          {getEvidenceHashCaveatText(evidenceHashStatus)}
        </div>
      ) : null}
      {importSignals.latestReport ? (
        <div className="mt-3 border-t border-forensics-border-light pt-2 text-forensics-text-tertiary">
          <div className="flex items-center justify-between gap-3">
            <span className="text-[10px] font-light uppercase tracking-wider text-forensics-muted">
              {t('bottomDrawer.performance.title')}
            </span>
            <span className="font-mono text-forensics-text">{importSignals.latestReport.summary.elapsedMs}ms</span>
          </div>
          <div className="mt-1 text-forensics-muted">{importSignals.latestReport.summary.summary}</div>
        </div>
      ) : null}
    </div>
  );
}

function cacheKeyLabel(cacheKey: string, t: (key: string) => string) {
  if (cacheKey.startsWith('timeline:')) return t('bottomDrawer.labels.cache.timeline');
  if (cacheKey.startsWith('artifacts:')) return t('bottomDrawer.labels.cache.artifacts');
  if (cacheKey.startsWith('search:')) return t('bottomDrawer.labels.cache.search');
  return cacheKey;
}
