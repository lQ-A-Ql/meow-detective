import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { DrawerIssueCard } from '@/features/jobs/components/DrawerIssueCard';
import type { DrawerIssue } from '@/features/jobs/model/bottom-drawer-model';

interface BottomDrawerIssuesProps {
  errorIssues: DrawerIssue[];
  warningIssues: DrawerIssue[];
}

export function BottomDrawerIssues({ errorIssues, warningIssues }: BottomDrawerIssuesProps) {
  const { t } = useTranslation();
  return (
    <ScrollArea
      className="min-h-0 min-w-0 overflow-hidden border-r border-forensics-border"
      viewportClassName="min-w-0 overflow-x-hidden p-3"
    >
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2 text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary">
        <span className="shrink-0">{t('bottomDrawer.issues.title')}</span>
        <span className="min-w-0 truncate text-right font-mono text-forensics-muted-light">
          {errorIssues.length} {t('bottomDrawer.issues.errors')} / {warningIssues.length}{' '}
          {t('bottomDrawer.jobs.warnings')}
        </span>
      </div>
      <div className="min-w-0 space-y-2 overflow-hidden">
        {errorIssues.map((issue) => <DrawerIssueCard key={issue.id} issue={issue} />)}
        {warningIssues.map((issue) => <DrawerIssueCard key={issue.id} issue={issue} />)}
        {errorIssues.length === 0 && warningIssues.length === 0 ? (
          <div className="border border-forensics-border-light bg-forensics-surface p-3 text-[11px] text-forensics-muted">
            {t('bottomDrawer.issues.empty')}
          </div>
        ) : null}
      </div>
    </ScrollArea>
  );
}
