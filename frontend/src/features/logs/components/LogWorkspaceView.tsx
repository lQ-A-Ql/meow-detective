import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ResizableGroup, ResizablePanel, ResizableHandle } from '@/app/components/ui/resizable';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { LogPanelContainer } from '../containers/LogPanelContainer';
import { LogDetailsView } from './LogDetailsView';
import type { LogWorkspaceModel } from '../use-log-workspace-model';
import type { ApplicationLogLevel, ApplicationLogFilter } from '@/types/applicationLog';

export function LogWorkspaceView({ model }: { model: LogWorkspaceModel }) {
  const { t } = useTranslation();
  const panel = (level: ApplicationLogLevel) => (
    <LogPanelContainer
      level={level}
      rows={model.buffer.rows[level]}
      selected={model.selected}
      onSelect={model.select}
      status={model.buffer.latest?.pages.find((page) => page.level === level)}
    />
  );
  const fields: Array<keyof ApplicationLogFilter> = ['text', 'target', 'caseId', 'dataSourceId', 'jobId'];
  return (
    <main className="log-workspace" aria-label={t('logs.title')}>
      <header className="log-toolbar">
        <h1>{t('logs.title')}</h1>
        <div className="log-filters">
          {fields.map((field) => (
            <Input
              key={field}
              value={model.filter[field]}
              aria-label={t(`logs.filters.${field}`)}
              placeholder={t(`logs.filters.${field}`)}
              maxLength={256}
              onChange={(event) => model.setFilter((current) => ({ ...current, [field]: event.target.value }))}
            />
          ))}
        </div>
        <Button shape="pill" variant="forensicsSurface" onClick={() => model.setPaused(!model.paused)}>
          <ActionIcon name={model.paused ? 'play' : 'pause'} />
          {t(model.paused ? 'logs.resume' : 'logs.pause')}
        </Button>
        <Button shape="pill" variant="forensicsGhost" onClick={model.clearDisplay}>
          <ActionIcon name="erase" />
          {t('logs.clear')}
        </Button>
        <Button shape="pill" variant="forensicsGhost" disabled={model.busy} onClick={model.loadHistory}>
          <ActionIcon name="history" />
          {t('logs.history')}
        </Button>
      </header>
      {model.error ? (
        <div role="alert" className="log-failure">
          {model.error}
        </div>
      ) : null}
      {model.buffer.latest?.writer.failure ? (
        <div role="alert" className="log-failure">
          {t('logs.writerFailed')} {model.buffer.latest.writer.failure}
          <Button shape="pill" variant="forensicsSurface" disabled={model.busy} onClick={model.reopenWriter}>
            {t('logs.retry')}
          </Button>
        </div>
      ) : null}
      {model.buffer.latest?.writer.failedRecords ? (
        <div role="status" className="log-failure">
          {t('logs.failedRecords', { count: model.buffer.latest.writer.failedRecords })}
        </div>
      ) : null}
      <div className="log-panels">
        <ResizableGroup direction="horizontal" autoSaveId="application-log-columns">
          <ResizablePanel defaultSize={64} minSize={30}>
            {panel('info')}
          </ResizablePanel>
          <ResizableHandle aria-label={t('logs.resizeColumns')} />
          <ResizablePanel defaultSize={36} minSize={25}>
            <ResizableGroup direction="vertical" autoSaveId="application-log-rows">
              <ResizablePanel defaultSize={50} minSize={20}>
                {panel('warning')}
              </ResizablePanel>
              <ResizableHandle aria-label={t('logs.resizeRows')} />
              <ResizablePanel defaultSize={50} minSize={20}>
                {panel('error')}
              </ResizablePanel>
            </ResizableGroup>
          </ResizablePanel>
        </ResizableGroup>
      </div>
      <LogDetailsView model={model} />
    </main>
  );
}
