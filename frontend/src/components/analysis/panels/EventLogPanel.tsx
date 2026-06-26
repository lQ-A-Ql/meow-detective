import { useState } from 'react';
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

const TAB_LABELS: Record<EventLogTabKey, string> = {
  boot: 'Boot/Shutdown',
  logon: 'Logon Events',
  process: 'Process Creation',
  account: 'Account Management',
  application: 'Application Events',
};

export function EventLogPanel({
  summary,
  progress,
}: {
  summary?: EvtxEventSummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const [activeTab, setActiveTab] = useState<EventLogTabKey>('boot');

  const info = summary ?? {
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

  const logonEvents = info.securityEvents.filter((e) =>
    ['logonSuccess', 'logonFailure', 'explicitCredentials'].includes(e.kind),
  );
  const processEvents = info.securityEvents.filter((e) => e.kind === 'processCreated');
  const accountEvents = info.securityEvents.filter((e) =>
    ['scheduledTaskCreated', 'scheduledTaskModified', 'accountCreated', 'groupMemberAdded'].includes(e.kind),
  );

  const bootColumns: DenseColumn<EvtxBootEvent>[] = [
    { key: 'timestamp', title: 'Time', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: 'ID', className: 'w-[60px]', render: (row) => row.eventId },
    { key: 'kind', title: 'Type', className: 'w-[140px]', render: (row) => row.kind },
    { key: 'provider', title: 'Provider', className: 'w-[120px]', render: (row) => row.provider ?? '-' },
    { key: 'recordId', title: 'RecID', className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
    { key: 'sourcePath', title: 'Source', className: 'min-w-[200px]', render: (row) => row.sourcePath },
  ];

  const logonColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: 'Time', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: 'ID', className: 'w-[60px]', render: (row) => row.eventId },
    { key: 'kind', title: 'Type', className: 'w-[130px]', render: (row) => row.kind },
    { key: 'targetUser', title: 'User', className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'logonType', title: 'Logon', className: 'w-[70px]', render: (row) => row.logonType ?? '-' },
    { key: 'ipAddress', title: 'IP', className: 'w-[130px]', render: (row) => row.ipAddress ?? '-' },
    { key: 'workstation', title: 'Workstation', className: 'w-[120px]', render: (row) => row.workstation ?? '-' },
    { key: 'failureReason', title: 'Reason', className: 'min-w-[140px]', render: (row) => row.failureReason ?? '-' },
  ];

  const processColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: 'Time', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'processName', title: 'Process', className: 'min-w-[200px]', render: (row) => row.processName ?? '-' },
    { key: 'parentProcessName', title: 'Parent', className: 'w-[180px]', render: (row) => row.parentProcessName ?? '-' },
    { key: 'subjectUser', title: 'User', className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'recordId', title: 'RecID', className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
  ];

  const accountColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: 'Time', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: 'ID', className: 'w-[60px]', render: (row) => row.eventId },
    { key: 'kind', title: 'Type', className: 'w-[150px]', render: (row) => row.kind },
    { key: 'targetUser', title: 'Target', className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'subjectUser', title: 'Subject', className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'taskName', title: 'Task', className: 'min-w-[180px]', render: (row) => row.taskName ?? '-' },
    { key: 'memberName', title: 'Member', className: 'w-[150px]', render: (row) => row.memberName ?? '-' },
  ];

  const appColumns: DenseColumn<EvtxApplicationEvent>[] = [
    { key: 'timestamp', title: 'Time', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: 'ID', className: 'w-[60px]', render: (row) => row.eventId },
    { key: 'kind', title: 'Type', className: 'w-[130px]', render: (row) => row.kind },
    { key: 'application', title: 'Application', className: 'min-w-[200px]', render: (row) => row.application ?? '-' },
    { key: 'faultModule', title: 'Module', className: 'w-[150px]', render: (row) => row.faultModule ?? '-' },
    { key: 'productName', title: 'Product', className: 'w-[160px]', render: (row) => row.productName ?? '-' },
    { key: 'manufacturer', title: 'Manufacturer', className: 'w-[140px]', render: (row) => row.manufacturer ?? '-' },
  ];

  const tabContent: Record<EventLogTabKey, React.ReactNode> = {
    boot: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.bootEvents}
          columns={bootColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="No boot/shutdown events"
          emptyDescription="System.evtx boot/shutdown candidates (6005, 6006, 6008, 1074)."
        />
      </DenseTableFrame>
    ),
    logon: (
      <DenseTableFrame>
        <DenseDataTable
          rows={logonEvents}
          columns={logonColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="No logon events"
          emptyDescription="Security.evtx logon events (4624, 4625, 4648)."
        />
      </DenseTableFrame>
    ),
    process: (
      <DenseTableFrame>
        <DenseDataTable
          rows={processEvents}
          columns={processColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="No process creation events"
          emptyDescription="Security.evtx process creation events (4688)."
        />
      </DenseTableFrame>
    ),
    account: (
      <DenseTableFrame>
        <DenseDataTable
          rows={accountEvents}
          columns={accountColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="No account management events"
          emptyDescription="Security.evtx account/task events (4698, 4702, 4720, 4732)."
        />
      </DenseTableFrame>
    ),
    application: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.applicationEvents}
          columns={appColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="No application events"
          emptyDescription="Application.evtx crash/install events (1000, 1001, 1002, 1033, 11707, 11708)."
        />
      </DenseTableFrame>
    ),
  };

  return (
    <ExtractionTableSection
      title="Event Log Analysis"
      status={info.totalCount > 0 ? 'parsed' : 'notFound'}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['Total', info.totalCount.toString()],
        ['Boot/Shutdown', info.bootShutdownCount.toString()],
        ['Logon', info.logonLogoffCount.toString()],
        ['Process', info.processExecutionCount.toString()],
        ['Application', info.applicationCrashCount.toString()],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />

      <div className="mb-3 flex flex-wrap gap-1">
        {(Object.keys(TAB_LABELS) as EventLogTabKey[]).map((tab) => (
          <button
            key={tab}
            type="button"
            onClick={() => setActiveTab(tab)}
            className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
              activeTab === tab
                ? 'bg-[#175cd3] text-white'
                : 'bg-[#f2f4f7] text-[#475467] hover:bg-[#e4e7ec]'
            }`}
          >
            {TAB_LABELS[tab]}
          </button>
        ))}
      </div>

      {tabContent[activeTab]}
    </ExtractionTableSection>
  );
}
