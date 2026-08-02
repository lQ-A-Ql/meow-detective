import type { ReactNode } from 'react';
import { ChevronDown, ChevronUp, Terminal } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { useResizableHeight } from '@/hooks/use-resizable-height';
import { BottomDrawerIssues } from '@/features/jobs/components/BottomDrawerIssues';
import { BottomDrawerJobs } from '@/features/jobs/components/BottomDrawerJobs';
import { BottomDrawerTrace } from '@/features/jobs/components/BottomDrawerTrace';
import type { BottomDrawerModel } from '@/features/jobs/model/bottom-drawer-model';

interface BottomDrawerProps {
  model: BottomDrawerModel;
  analysisProgress?: ReactNode;
}

export function BottomDrawer({ model, analysisProgress }: BottomDrawerProps) {
  const { t } = useTranslation();
  const { height, isResizing, onResizeStart } = useResizableHeight({
    defaultHeight: 224,
    minHeight: 128,
    maxHeight: 600,
    storageKey: 'bottomDrawerHeight',
  });

  return (
    <div
      className={`z-10 shrink-0 border-t border-forensics-border bg-forensics-panel transition-[height] duration-150 ${
        model.drawerOpen ? 'flex flex-col' : 'h-8 overflow-hidden'
      }`}
      style={model.drawerOpen ? { height: `${height}px` } : undefined}
    >
      <div className="grid h-8 grid-cols-[minmax(0,1fr)_auto] items-center gap-2 px-4 font-mono text-[11px] text-forensics-muted">
        <div className="flex min-w-0 items-center gap-3 overflow-hidden">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <Terminal size={12} className="shrink-0 text-forensics-muted-light" />
            <span className="block truncate" title={model.headline}>
              [{t('bottomDrawer.jobs.title')}] {model.headline}
            </span>
          </div>
          <div className="hidden shrink-0 items-center gap-3 whitespace-nowrap border-l border-forensics-border px-3 text-forensics-text-tertiary 2xl:flex">
            <span><span className="text-forensics-text">{model.runningJobs.length}</span> {t('bottomDrawer.jobs.running')}</span>
            <span><span className={model.errorIssues.length ? 'text-forensics-error-text' : 'text-forensics-text'}>{model.errorIssues.length}</span> {t('bottomDrawer.issues.errors')}</span>
            <span><span className="text-forensics-text">{model.warningIssues.length}</span> {t('bottomDrawer.jobs.warnings')}</span>
            <span><span className="text-forensics-text">{model.jobSkippedCount}</span> {t('bottomDrawer.jobs.skipped')}</span>
            <span><span className="text-forensics-text">{model.trace.length}</span> {t('bottomDrawer.jobs.trace')}</span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button type="button" variant="forensicsSurface" size="compact" onClick={model.toggleDrawer} className="gap-1">
            <span>{model.drawerOpen ? t('bottomDrawer.toggle.collapse') : t('bottomDrawer.toggle.expand')}</span>
            {model.drawerOpen ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          </Button>
          <div className="hidden min-w-0 max-w-72 border-l border-forensics-border pl-3 2xl:flex">
            <span className="shrink-0">{t('bottomDrawer.status.recent')} </span>
            <span className="truncate text-forensics-text">{model.recentScope}</span>
          </div>
        </div>
      </div>
      {model.drawerOpen ? (
        <>
          <div
            className={`h-1 shrink-0 cursor-row-resize transition-colors ${
              isResizing ? 'bg-forensics-info-bg' : 'hover:bg-forensics-info-bg'
            }`}
            onMouseDown={onResizeStart}
            title="拖拽调整抽屉高度"
          />
          <div className="grid min-h-0 flex-1 grid-cols-3 overflow-hidden border-t border-forensics-border">
            <BottomDrawerJobs model={model} analysisProgress={analysisProgress} />
            <BottomDrawerIssues errorIssues={model.errorIssues} warningIssues={model.warningIssues} />
            <BottomDrawerTrace trace={model.trace} />
          </div>
        </>
      ) : null}
    </div>
  );
}
