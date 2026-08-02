import { useTranslation } from 'react-i18next';
import type { JobSnapshot } from '@/types/models';

export function JobOutcomeBadges({ job }: { job: JobSnapshot }) {
  const { t } = useTranslation();

  if (!job.partial && job.warningCount === 0 && job.skippedCount === 0 && job.failedCount === 0) {
    return null;
  }

  return (
    <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[10px] font-mono">
      {job.partial ? (
        <span className="border border-forensics-warning-border bg-forensics-warning-bg px-1.5 py-0.5 font-light text-forensics-warning-text-strong">
          {t('bottomDrawer.labels.partial')}
        </span>
      ) : null}
      <span className="border border-forensics-warning-border bg-forensics-surface px-1.5 py-0.5 text-forensics-warning-text">
        {t('bottomDrawer.labels.warnings')} {job.warningCount}
      </span>
      <span className="border border-forensics-350 bg-forensics-surface px-1.5 py-0.5 text-forensics-text-tertiary">
        {t('bottomDrawer.labels.skipped')} {job.skippedCount}
      </span>
      <span className="border border-forensics-error-border bg-forensics-surface px-1.5 py-0.5 text-forensics-error-text">
        {t('bottomDrawer.labels.failed')} {job.failedCount}
      </span>
    </div>
  );
}
