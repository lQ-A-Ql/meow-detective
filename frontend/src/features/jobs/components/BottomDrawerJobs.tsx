import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ImportStatusPanel } from '@/features/jobs/components/ImportStatusPanel';
import { JobStatusCard } from '@/features/jobs/components/JobStatusCard';
import type { BottomDrawerModel } from '@/features/jobs/model/bottom-drawer-model';

interface BottomDrawerJobsProps {
  model: BottomDrawerModel;
  analysisProgress?: ReactNode;
}

export function BottomDrawerJobs({ model, analysisProgress }: BottomDrawerJobsProps) {
  const { t } = useTranslation();
  return (
    <ScrollArea
      className="min-h-0 min-w-0 overflow-hidden border-r border-forensics-border"
      viewportClassName="min-w-0 overflow-x-hidden p-3"
    >
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2 text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary">
        <span className="shrink-0">{t('bottomDrawer.jobs.title')}</span>
        <span className="min-w-0 truncate text-right font-mono text-forensics-muted-light">
          {t('bottomDrawer.jobs.stats', {
            running: model.runningJobs.length,
            completed: model.completedJobs.length,
            partial: model.partialJobCount,
            failed: model.failedJobs.length,
          })}
        </span>
      </div>
      <div className="min-w-0 space-y-3 overflow-hidden">
        {analysisProgress}
        <ImportStatusPanel
          importSignals={model.importSignals}
          evidenceHashStatus={model.evidenceHashStatus}
        />
        {model.runningJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="running" />)}
        {model.completedJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="completed" />)}
        {model.failedJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="failed" />)}
        {model.warningJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="warning" />)}
        {model.cancellingJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="cancelling" />)}
        {model.cancelledJobs.map((job) => <JobStatusCard key={job.id} job={job} tone="cancelled" />)}
      </div>
    </ScrollArea>
  );
}
