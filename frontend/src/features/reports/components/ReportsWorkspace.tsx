import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Progress } from '@/app/components/ui/progress';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { ActionBar } from '@/components/layout/ActionBar';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { REPORT_ACTIONS, REPORT_FORMATS, REPORT_SECTIONS } from '../report-config';
import type { ReportsWorkspaceModel } from '../use-reports-workspace-model';

export function ReportsWorkspace({ model }: { model: ReportsWorkspaceModel }) {
  const { t } = useTranslation();
  return (
    <WorkspaceFrame>
      <WorkspaceCard className="flex min-h-0 flex-1 flex-col">
        <ScrollArea className="min-h-0 flex-1" viewportClassName="workspace-content space-y-6">
          <div className="text-[13px] text-forensics-text">{t('reports.summaryTitle')}</div>
          <div className="space-y-3">
            {REPORT_SECTIONS.map((section) => (
              <label key={section} className="flex items-center gap-2 text-[12px] text-forensics-text-secondary">
                <Checkbox
                  variant="forensics"
                  checked={model.exportScope[section]}
                  onCheckedChange={(checked) => model.setScopeOption(section, checked === true)}
                />
                {t(`reports.sections.${section}`)}
              </label>
            ))}
          </div>
          <Select value={model.selectedFormat} onValueChange={model.selectFormat}>
            <SelectTrigger variant="mono" size="sm" className="w-64" aria-label={t('reports.format')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {REPORT_FORMATS.map((format) => (
                <SelectItem key={format} value={format}>
                  {t(`reports.formats.${format}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </ScrollArea>
        <ActionBar>
          <Tooltip content={t('reports.generate')}>
            <Button
              aria-label={t('reports.generate')}
              variant="forensicsPrimary"
              size="sm"
              onClick={model.runExport}
              disabled={model.exportPending}
            >
              <ActionIcon name="download" />
            </Button>
          </Tooltip>
        </ActionBar>
      </WorkspaceCard>
      <WorkspaceCard className="flex min-h-0 w-96 max-w-[40%] shrink-0 flex-col">
        <div className="flex h-10 shrink-0 items-center justify-between px-4 text-[11px] text-forensics-text-secondary">
          <span>{t('reports.recent')}</span>
          <span>{t('reports.runningSummary', { running: model.runningCount, completed: model.completedCount })}</span>
        </div>
        <ScrollArea className="min-h-0 flex-1" viewportClassName="space-y-4 p-4">
          {model.historyError ? (
            <div className="text-[12px] text-forensics-error-text">{t('reports.historyFailed')}</div>
          ) : null}
          {model.history.map((report) => (
            <div key={report.id} className="space-y-2 p-3">
              <div className="flex justify-between gap-3 text-[12px] text-forensics-text">
                <span className="min-w-0 truncate">{report.fileName}</span>
                <span className="shrink-0">{t(`reports.status.${report.status}`)}</span>
              </div>
              <div className="text-[11px] text-forensics-muted">
                {t('reports.generatedBy', { author: report.createdBy, time: report.createdAt })}
              </div>
              {report.status === 'running' ? <Progress value={report.progress ?? 0} /> : null}
              {report.errorMessage ? (
                <div className="text-[11px] text-forensics-error-text">{report.errorMessage}</div>
              ) : null}
              {REPORT_ACTIONS[report.status] === 'download' ? (
                <Tooltip content={t('media.controls.download')}>
                  <Button
                    aria-label={t('media.controls.download')}
                    variant="forensicsOutline"
                    size="compact"
                    disabled={model.savePending}
                    onClick={() => model.downloadReport(report)}
                  >
                    <ActionIcon name="download" />
                  </Button>
                </Tooltip>
              ) : null}
              {REPORT_ACTIONS[report.status] === 'retry' ? (
                <Tooltip content={t('reports.retry')}>
                  <Button
                    aria-label={t('reports.retry')}
                    variant="forensicsOutline"
                    size="compact"
                    disabled={model.exportPending}
                    onClick={() => model.retryReport(report)}
                  >
                    <ActionIcon name="refresh" />
                  </Button>
                </Tooltip>
              ) : null}
            </div>
          ))}
        </ScrollArea>
      </WorkspaceCard>
    </WorkspaceFrame>
  );
}
