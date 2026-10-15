import type { useEventLogPanelModel } from '../../use-event-log-panel-model';
import { useMemo } from 'react';
import type { EvtxApplicationEvent, EvtxBootEvent, EvtxSecurityEvent } from '@/types/models';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { ExtractionTableSection } from './helpers';
import type { EventLogTabKey } from '../../use-event-log-panel-model-shared';


const TABS: EventLogTabKey[] = ['boot', 'logon', 'process', 'account', 'application'];


/** Boot/shutdown kinds mapped to their i18n keys; labels resolve with the active `t`. */
const BOOT_EVENT_LABEL_KEYS: Record<string, string> = {
  operatingSystemStarted: 'analysisPanels.bootLabels.operatingSystemStarted',
  operatingSystemShutdown: 'analysisPanels.bootLabels.operatingSystemShutdown',
  eventLogStarted: 'analysisPanels.bootLabels.eventLogStarted',
  eventLogStopped: 'analysisPanels.bootLabels.eventLogStopped',
  unexpectedShutdown: 'analysisPanels.bootLabels.unexpectedShutdown',
  plannedShutdown: 'analysisPanels.bootLabels.plannedShutdown',
};
export function EventLogPanelView({ model }: { model: ReturnType<typeof useEventLogPanelModel> }) {
const { hasMore, loadingMore, loadMoreFailed, onLoadMore, onRetryLoadMore, loadStateKey, t, activeTab, setActiveTab, info, tableLoadContextKey, logonEvents, processEvents, accountEvents } = model;


  // Column titles are translated, so the arrays are memoized on `t` rather
  // than rebuilt (and busting row memoization) on every render.
  const bootColumns = useMemo<DenseColumn<EvtxBootEvent>[]>(() => [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    {
      key: 'kind',
      title: t('eventLog.columns.kind'),
      className: 'w-[160px]',
      render: (row) => {
        const labelKey = BOOT_EVENT_LABEL_KEYS[row.kind];
        return labelKey ? t(labelKey) : row.kind;
      },
    },
    { key: 'provider', title: t('eventLog.columns.provider'), className: 'w-[120px]', render: (row) => row.provider ?? '-' },
    { key: 'recordId', title: t('eventLog.columns.recordId'), className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
    { key: 'sourcePath', title: t('eventLog.columns.sourcePath'), className: 'min-w-[200px]', render: (row) => row.sourcePath },
  ], [t]);


  const logonColumns = useMemo<DenseColumn<EvtxSecurityEvent>[]>(() => [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[130px]', render: (row) => row.kind },
    { key: 'targetUser', title: t('eventLog.columns.targetUser'), className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'logonType', title: t('eventLog.columns.logonType'), className: 'w-[90px]', render: (row) => row.logonType ?? '-' },
    { key: 'ipAddress', title: t('eventLog.columns.ipAddress'), className: 'w-[130px]', render: (row) => row.ipAddress ?? '-' },
    { key: 'workstation', title: t('eventLog.columns.workstation'), className: 'w-[120px]', render: (row) => row.workstation ?? '-' },
    { key: 'failureReason', title: t('eventLog.columns.failureReason'), className: 'min-w-[140px]', render: (row) => row.failureReason ?? '-' },
  ], [t]);


  const processColumns = useMemo<DenseColumn<EvtxSecurityEvent>[]>(() => [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'processName', title: t('eventLog.columns.processName'), className: 'min-w-[200px]', render: (row) => row.processName ?? '-' },
    { key: 'parentProcessName', title: t('eventLog.columns.parentProcessName'), className: 'w-[180px]', render: (row) => row.parentProcessName ?? '-' },
    { key: 'subjectUser', title: t('eventLog.columns.subjectUser'), className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'recordId', title: t('eventLog.columns.recordId'), className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
  ], [t]);


  const accountColumns = useMemo<DenseColumn<EvtxSecurityEvent>[]>(() => [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[150px]', render: (row) => row.kind },
    { key: 'targetUser', title: t('eventLog.columns.subjectUserTarget'), className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'subjectUser', title: t('eventLog.columns.subjectUser'), className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'taskName', title: t('eventLog.columns.taskName'), className: 'min-w-[180px]', render: (row) => row.taskName ?? '-' },
    { key: 'memberName', title: t('eventLog.columns.memberName'), className: 'w-[150px]', render: (row) => row.memberName ?? '-' },
  ], [t]);


  const appColumns = useMemo<DenseColumn<EvtxApplicationEvent>[]>(() => [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[130px]', render: (row) => row.kind },
    { key: 'application', title: t('eventLog.columns.application'), className: 'min-w-[200px]', render: (row) => row.application ?? '-' },
    { key: 'faultModule', title: t('eventLog.columns.faultModule'), className: 'w-[150px]', render: (row) => row.faultModule ?? '-' },
    { key: 'productName', title: t('eventLog.columns.productName'), className: 'w-[160px]', render: (row) => row.productName ?? '-' },
    { key: 'manufacturer', title: t('eventLog.columns.manufacturer'), className: 'w-[140px]', render: (row) => row.manufacturer ?? '-' },
  ], [t]);


  const tabContent: Record<EventLogTabKey, React.ReactNode> = {
    boot: (
      <DenseDataTableFrame rowCount={info.bootEvents.length}>
        <DenseDataTable
          rows={info.bootEvents}
          columns={bootColumns}
          getRowKey={(row) => `${row.sourcePath}-${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.boot.title')}
          emptyDescription={t('eventLog.empty.boot.description')}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    ),
    logon: (
      <DenseDataTableFrame rowCount={logonEvents.length}>
        <DenseDataTable
          rows={logonEvents}
          columns={logonColumns}
          getRowKey={(row) => `${row.sourcePath}-${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.logon.title')}
          emptyDescription={t('eventLog.empty.logon.description')}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    ),
    process: (
      <DenseDataTableFrame rowCount={processEvents.length}>
        <DenseDataTable
          rows={processEvents}
          columns={processColumns}
          getRowKey={(row) => `${row.sourcePath}-${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.process.title')}
          emptyDescription={t('eventLog.empty.process.description')}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    ),
    account: (
      <DenseDataTableFrame rowCount={accountEvents.length}>
        <DenseDataTable
          rows={accountEvents}
          columns={accountColumns}
          getRowKey={(row) => `${row.sourcePath}-${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.account.title')}
          emptyDescription={t('eventLog.empty.account.description')}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    ),
    application: (
      <DenseDataTableFrame rowCount={info.applicationEvents.length}>
        <DenseDataTable
          rows={info.applicationEvents}
          columns={appColumns}
          getRowKey={(row) => `${row.sourcePath}-${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.application.title')}
          emptyDescription={t('eventLog.empty.application.description')}
          loadContextKey={tableLoadContextKey}
          loadStateKey={loadStateKey}
          hasMore={hasMore}
          loadingMore={loadingMore}
          loadMoreFailed={loadMoreFailed}
          onReachEnd={onLoadMore}
          onRetryLoadMore={onRetryLoadMore}
        />
      </DenseDataTableFrame>
    ),
  };


  return (
    <ExtractionTableSection
      title={t('eventLog.title')}
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        [t('eventLog.stats.total'), info.totalCount.toString()],
        [t('eventLog.stats.boot'), info.bootShutdownCount.toString()],
        [t('eventLog.stats.logon'), info.logonLogoffCount.toString()],
        [t('eventLog.stats.process'), info.processExecutionCount.toString()],
        [t('eventLog.stats.application'), info.applicationCrashCount.toString()],
      ]}
    >
      <PanelTabs
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as EventLogTabKey)}
        tabs={TABS.map((tab) => ({ value: tab, label: t(`eventLog.tabs.${tab}`) }))}
        className="overflow-hidden"
      >
        {TABS.map((tab) => (
          <TabsContent
            key={tab}
            value={tab}
            className="flex min-h-0 flex-col overflow-hidden data-[state=inactive]:hidden"
          >
            {tabContent[tab]}
          </TabsContent>
        ))}
      </PanelTabs>
    </ExtractionTableSection>
  );
}
