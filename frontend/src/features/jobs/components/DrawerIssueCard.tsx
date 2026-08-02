import { AlertCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type { DrawerIssue } from '@/features/jobs/model/bottom-drawer-model';

export function DrawerIssueCard({ issue }: { issue: DrawerIssue }) {
  const { t } = useTranslation();
  const isError = issue.severity === 'error';

  return (
    <div
      className={`border p-3 text-[11px] ${
        isError
          ? 'border-forensics-error-border bg-forensics-error-bg text-forensics-error-text'
          : 'border-forensics-warning-border bg-forensics-surface text-forensics-text'
      }`}
    >
      <div className="flex items-start gap-2">
        <AlertCircle
          size={12}
          className={`mt-0.5 shrink-0 ${isError ? 'text-forensics-error-text' : 'text-forensics-warning'}`}
        />
        <div className="min-w-0 flex-1">
          <div className="break-words font-light">{issue.title}</div>
          <div
            className={`mt-1 whitespace-pre-wrap break-words ${
              isError ? 'text-forensics-error-text/90' : 'text-forensics-muted'
            }`}
          >
            {issue.detail}
          </div>
        </div>
      </div>
      {issue.meta.length > 0 ? (
        <div className="mt-2 grid grid-cols-2 gap-1.5">
          {issue.meta.map((item) => (
            <div
              key={`${item.label}-${item.value}`}
              className={`border px-1.5 py-1 ${
                isError
                  ? 'border-forensics-error-border bg-forensics-surface/60'
                  : 'border-forensics-border-light bg-forensics-panel'
              }`}
            >
              <span className={isError ? 'text-forensics-error-text/80' : 'text-forensics-muted-light'}>
                {item.label}:{' '}
              </span>
              <span className="break-all font-mono">{item.value}</span>
            </div>
          ))}
        </div>
      ) : null}
      {issue.suggestion ? (
        <div
          className={`mt-2 border px-2 py-1.5 ${
            isError
              ? 'border-forensics-error-border bg-forensics-surface/60'
              : 'border-forensics-border-light bg-forensics-panel'
          }`}
        >
          <span className="font-light">{t('bottomDrawer.issues.suggestion')}: </span>
          <span className="break-words">{issue.suggestion}</span>
        </div>
      ) : null}
      {issue.details ? (
        <ScrollArea
          className={`mt-2 max-h-28 border ${
            isError
              ? 'border-forensics-error-border bg-forensics-surface/70 text-forensics-error-text'
              : 'border-forensics-border-light bg-forensics-panel text-forensics-text-tertiary'
          }`}
          showHorizontalScrollbar
        >
          <pre className="whitespace-pre-wrap break-words px-2 py-1.5 font-mono text-[10px]">{issue.details}</pre>
        </ScrollArea>
      ) : null}
    </div>
  );
}
