import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { ApplicationLogLevel, ApplicationLogPage, ApplicationLogRecord } from '@/types/applicationLog';
import type { useLogPanelModel } from '../use-log-panel-model';

export interface LogPanelViewProps {
  level: ApplicationLogLevel;
  rows: ApplicationLogRecord[];
  status?: ApplicationLogPage;
  selected?: ApplicationLogRecord;
  onSelect: (record: ApplicationLogRecord) => void;
  scroll: ReturnType<typeof useLogPanelModel>;
}

export function LogPanelView({ level, rows, status, selected, onSelect, scroll }: LogPanelViewProps) {
  const { t } = useTranslation();
  return (
    <section className={`log-panel log-panel-${level}`} aria-label={t(`logs.levels.${level}`)}>
      <header className="log-panel-header">
        <span className="log-panel-title">
          <ActionIcon name={level === 'info' ? 'info' : 'warning'} />
          {t(`logs.levels.${level}`)}
        </span>
        <span className="log-panel-count">{t('logs.count', { count: rows.length, total: status?.emitted ?? 0 })}</span>
        {!scroll.following ? (
          <Button shape="pill" variant="forensicsGhost" size="sm" onClick={scroll.returnToLatest}>
            {t('logs.latest')}
          </Button>
        ) : null}
      </header>
      <DenseDataTableFrame layout="fill" variant="plain">
        <DenseDataTable
          rows={rows}
          getRowKey={(record) => `${record.sessionId}:${record.sequence}`}
          selectedRowKey={selected ? `${selected.sessionId}:${selected.sequence}` : undefined}
          onRowClick={onSelect}
          followLatest={scroll.following}
          onFollowLatestChange={scroll.setFollowing}
          emptyTitle={t('logs.empty')}
          columns={[
            {
              key: 'timestamp',
              title: t('logs.time'),
              className: 'w-24',
              render: (record) => (
                <span className="tabular-nums">{new Date(record.timestamp).toLocaleTimeString()}</span>
              ),
            },
            {
              key: 'message',
              title: t('logs.message'),
              render: (record) => (
                <div className="min-w-0" title={record.message}>
                  <div className="truncate">{record.message}</div>
                  <div className="truncate text-forensics-muted-light">{record.target}</div>
                </div>
              ),
            },
          ]}
        />
      </DenseDataTableFrame>
      {status && (status.evicted || status.fileDropped || status.truncated || status.gap) ? (
        <footer className="log-panel-footer">
          {t('logs.retention', { evicted: status.evicted, dropped: status.fileDropped, truncated: status.truncated })}
        </footer>
      ) : null}
    </section>
  );
}
