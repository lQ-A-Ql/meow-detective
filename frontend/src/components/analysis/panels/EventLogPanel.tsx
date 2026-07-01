import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  EvtxApplicationEvent,
  EvtxBootEvent,
  EvtxEventSummary,
  EvtxSecurityEvent,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisExtractionProgress,
  type AnalysisExtractionProgressInfo,
  DenseTableFrame,
  ExtractionTableSection,
} from './helpers';

type EventLogTabKey = 'boot' | 'logon' | 'process' | 'account' | 'application';

const TABS: EventLogTabKey[] = ['boot', 'logon', 'process', 'account', 'application'];

export function EventLogPanel({
  summary,
  progress,
}: {
  summary?: EvtxEventSummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<EventLogTabKey>('boot');

  const info = summary ?? {
    status: 'unavailable' as const,
    bootShutdownCount: 0,
    logonLogoffCount: 0,
    privilegeEscalationCount: 0,
    processExecutionCount: 0,
    accountManagementCount: 0,
    scheduledTaskCount: 0,
    applicationCrashCount: 0,
    softwareInstallationCount: 0,
    otherCount: 0,
    totalCount: 0,
    bootEvents: [],
    securityEvents: [],
    applicationEvents: [],
    warnings: ['Event log summary unavailable.'],
    generatedAt: '',
  };

  const logonEvents = info.securityEvents.filter((e: EvtxSecurityEvent) =>
    ['logonSuccess', 'logonFailure', 'explicitCredentials'].includes(e.kind),
  );
  const processEvents = info.securityEvents.filter((e: EvtxSecurityEvent) => e.kind === 'processCreated');
  const accountEvents = info.securityEvents.filter((e: EvtxSecurityEvent) =>
    ['scheduledTaskCreated', 'scheduledTaskModified', 'accountCreated', 'groupMemberAdded'].includes(e.kind),
  );

  const bootColumns: DenseColumn<EvtxBootEvent>[] = [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[140px]', render: (row) => row.kind },
    { key: 'provider', title: t('eventLog.columns.provider'), className: 'w-[120px]', render: (row) => row.provider ?? '-' },
    { key: 'recordId', title: t('eventLog.columns.recordId'), className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
    { key: 'sourcePath', title: t('eventLog.columns.sourcePath'), className: 'min-w-[200px]', render: (row) => row.sourcePath },
  ];

  const logonColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[130px]', render: (row) => row.kind },
    { key: 'targetUser', title: t('eventLog.columns.targetUser'), className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'logonType', title: t('eventLog.columns.logonType'), className: 'w-[90px]', render: (row) => row.logonType ?? '-' },
    { key: 'ipAddress', title: t('eventLog.columns.ipAddress'), className: 'w-[130px]', render: (row) => row.ipAddress ?? '-' },
    { key: 'workstation', title: t('eventLog.columns.workstation'), className: 'w-[120px]', render: (row) => row.workstation ?? '-' },
    { key: 'failureReason', title: t('eventLog.columns.failureReason'), className: 'min-w-[140px]', render: (row) => row.failureReason ?? '-' },
  ];

  const processColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'processName', title: t('eventLog.columns.processName'), className: 'min-w-[200px]', render: (row) => row.processName ?? '-' },
    { key: 'parentProcessName', title: t('eventLog.columns.parentProcessName'), className: 'w-[180px]', render: (row) => row.parentProcessName ?? '-' },
    { key: 'subjectUser', title: t('eventLog.columns.subjectUser'), className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'recordId', title: t('eventLog.columns.recordId'), className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
  ];

  const accountColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[150px]', render: (row) => row.kind },
    { key: 'targetUser', title: t('eventLog.columns.subjectUserTarget'), className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'subjectUser', title: t('eventLog.columns.subjectUser'), className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'taskName', title: t('eventLog.columns.taskName'), className: 'min-w-[180px]', render: (row) => row.taskName ?? '-' },
    { key: 'memberName', title: t('eventLog.columns.memberName'), className: 'w-[150px]', render: (row) => row.memberName ?? '-' },
  ];

  const appColumns: DenseColumn<EvtxApplicationEvent>[] = [
    { key: 'timestamp', title: t('eventLog.columns.timestamp'), className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: t('eventLog.columns.eventId'), className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: t('eventLog.columns.kind'), className: 'w-[130px]', render: (row) => row.kind },
    { key: 'application', title: t('eventLog.columns.application'), className: 'min-w-[200px]', render: (row) => row.application ?? '-' },
    { key: 'faultModule', title: t('eventLog.columns.faultModule'), className: 'w-[150px]', render: (row) => row.faultModule ?? '-' },
    { key: 'productName', title: t('eventLog.columns.productName'), className: 'w-[160px]', render: (row) => row.productName ?? '-' },
    { key: 'manufacturer', title: t('eventLog.columns.manufacturer'), className: 'w-[140px]', render: (row) => row.manufacturer ?? '-' },
  ];

  const tabContent: Record<EventLogTabKey, React.ReactNode> = {
    boot: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.bootEvents}
          columns={bootColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.boot.title')}
          emptyDescription={t('eventLog.empty.boot.description')}
        />
      </DenseTableFrame>
    ),
    logon: (
      <DenseTableFrame>
        <DenseDataTable
          rows={logonEvents}
          columns={logonColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.logon.title')}
          emptyDescription={t('eventLog.empty.logon.description')}
        />
      </DenseTableFrame>
    ),
    process: (
      <DenseTableFrame>
        <DenseDataTable
          rows={processEvents}
          columns={processColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.process.title')}
          emptyDescription={t('eventLog.empty.process.description')}
        />
      </DenseTableFrame>
    ),
    account: (
      <DenseTableFrame>
        <DenseDataTable
          rows={accountEvents}
          columns={accountColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.account.title')}
          emptyDescription={t('eventLog.empty.account.description')}
        />
      </DenseTableFrame>
    ),
    application: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.applicationEvents}
          columns={appColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle={t('eventLog.empty.application.title')}
          emptyDescription={t('eventLog.empty.application.description')}
        />
      </DenseTableFrame>
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
      <AnalysisExtractionProgress progress={progress} />

      <div className="mb-3 flex flex-wrap gap-1">
        {TABS.map((tab) => (
          <button
            key={tab}
            type="button"
            onClick={() => setActiveTab(tab)}
            className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
              activeTab === tab
                ? 'bg-forensics-primary-blue text-white'
                : 'bg-forensics-surface-muted text-forensics-text-soft hover:bg-forensics-hover-muted'
            }`}
          >
            {t(`eventLog.tabs.${tab}`)}
          </button>
        ))}
      </div>

      {tabContent[activeTab]}
    </ExtractionTableSection>
  );
}
