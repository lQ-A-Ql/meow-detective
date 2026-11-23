import type { TFunction } from 'i18next';
import { formatBytes } from '@/lib/format-bytes';
import type { ImportPhaseMetrics } from '@/types/models';

export function importProgressMetrics(metrics: ImportPhaseMetrics, t: TFunction, language: string) {
  const number = new Intl.NumberFormat(language, { maximumFractionDigits: 1 });
  const values: Array<{ label: string; value: string }> = [];
  if (metrics.rowsProcessed > 0 || metrics.rowsTotal !== undefined) {
    values.push({
      label: t('taskProgress.items'),
      value:
        metrics.rowsTotal === undefined
          ? number.format(metrics.rowsProcessed)
          : t('taskProgress.count', {
              processed: number.format(metrics.rowsProcessed),
              total: number.format(metrics.rowsTotal),
            }),
    });
  }
  if (metrics.bytesProcessed > 0 || metrics.bytesTotal !== undefined) {
    values.push({
      label: t('taskProgress.data'),
      value:
        metrics.bytesTotal === undefined
          ? formatBytes(metrics.bytesProcessed)
          : t('taskProgress.count', {
              processed: formatBytes(metrics.bytesProcessed),
              total: formatBytes(metrics.bytesTotal),
            }),
    });
  }
  if (metrics.rowsPerSec !== undefined && Number.isFinite(metrics.rowsPerSec) && metrics.rowsPerSec > 0) {
    values.push({
      label: t('taskProgress.rate'),
      value: t('taskProgress.itemsPerSecond', { amount: number.format(metrics.rowsPerSec) }),
    });
  } else if (metrics.mbPerSec !== undefined && Number.isFinite(metrics.mbPerSec) && metrics.mbPerSec > 0) {
    values.push({
      label: t('taskProgress.rate'),
      value: t('taskProgress.bytesPerSecond', { amount: formatBytes(metrics.mbPerSec * 1024 * 1024) }),
    });
  }
  if (metrics.elapsedMs !== undefined) {
    values.push({
      label: t('taskProgress.stageElapsed'),
      value: t('taskProgress.seconds', { amount: number.format(metrics.elapsedMs / 1000) }),
    });
  }
  return values;
}
