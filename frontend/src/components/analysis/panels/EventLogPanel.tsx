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
  boot: '开关机',
  logon: '登录事件',
  process: '进程创建',
  account: '账户管理',
  application: '应用程序事件',
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

  const logonEvents = info.securityEvents.filter((e: EvtxSecurityEvent) =>
    ['logonSuccess', 'logonFailure', 'explicitCredentials'].includes(e.kind),
  );
  const processEvents = info.securityEvents.filter((e: EvtxSecurityEvent) => e.kind === 'processCreated');
  const accountEvents = info.securityEvents.filter((e: EvtxSecurityEvent) =>
    ['scheduledTaskCreated', 'scheduledTaskModified', 'accountCreated', 'groupMemberAdded'].includes(e.kind),
  );

  const bootColumns: DenseColumn<EvtxBootEvent>[] = [
    { key: 'timestamp', title: '时间', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: '事件 ID', className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: '类型', className: 'w-[140px]', render: (row) => row.kind },
    { key: 'provider', title: '提供程序', className: 'w-[120px]', render: (row) => row.provider ?? '-' },
    { key: 'recordId', title: '记录 ID', className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
    { key: 'sourcePath', title: '来源日志', className: 'min-w-[200px]', render: (row) => row.sourcePath },
  ];

  const logonColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: '时间', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: '事件 ID', className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: '类型', className: 'w-[130px]', render: (row) => row.kind },
    { key: 'targetUser', title: '用户', className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'logonType', title: '登录类型', className: 'w-[90px]', render: (row) => row.logonType ?? '-' },
    { key: 'ipAddress', title: 'IP 地址', className: 'w-[130px]', render: (row) => row.ipAddress ?? '-' },
    { key: 'workstation', title: '工作站', className: 'w-[120px]', render: (row) => row.workstation ?? '-' },
    { key: 'failureReason', title: '失败原因', className: 'min-w-[140px]', render: (row) => row.failureReason ?? '-' },
  ];

  const processColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: '时间', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'processName', title: '进程', className: 'min-w-[200px]', render: (row) => row.processName ?? '-' },
    { key: 'parentProcessName', title: '父进程', className: 'w-[180px]', render: (row) => row.parentProcessName ?? '-' },
    { key: 'subjectUser', title: '用户', className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'recordId', title: '记录 ID', className: 'w-[70px]', render: (row) => row.recordId?.toString() ?? '-' },
  ];

  const accountColumns: DenseColumn<EvtxSecurityEvent>[] = [
    { key: 'timestamp', title: '时间', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: '事件 ID', className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: '类型', className: 'w-[150px]', render: (row) => row.kind },
    { key: 'targetUser', title: '目标用户', className: 'w-[120px]', render: (row) => row.targetUser ?? '-' },
    { key: 'subjectUser', title: '主体用户', className: 'w-[120px]', render: (row) => row.subjectUser ?? '-' },
    { key: 'taskName', title: '任务名', className: 'min-w-[180px]', render: (row) => row.taskName ?? '-' },
    { key: 'memberName', title: '成员名', className: 'w-[150px]', render: (row) => row.memberName ?? '-' },
  ];

  const appColumns: DenseColumn<EvtxApplicationEvent>[] = [
    { key: 'timestamp', title: '时间', className: 'w-[180px]', render: (row) => row.timestamp },
    { key: 'eventId', title: '事件 ID', className: 'w-[70px]', render: (row) => row.eventId },
    { key: 'kind', title: '类型', className: 'w-[130px]', render: (row) => row.kind },
    { key: 'application', title: '应用程序', className: 'min-w-[200px]', render: (row) => row.application ?? '-' },
    { key: 'faultModule', title: '故障模块', className: 'w-[150px]', render: (row) => row.faultModule ?? '-' },
    { key: 'productName', title: '产品', className: 'w-[160px]', render: (row) => row.productName ?? '-' },
    { key: 'manufacturer', title: '制造商', className: 'w-[140px]', render: (row) => row.manufacturer ?? '-' },
  ];

  const tabContent: Record<EventLogTabKey, React.ReactNode> = {
    boot: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.bootEvents}
          columns={bootColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="暂无开关机事件"
          emptyDescription="System.evtx 开关机候选事件（6005、6006、6008、1074）。"
        />
      </DenseTableFrame>
    ),
    logon: (
      <DenseTableFrame>
        <DenseDataTable
          rows={logonEvents}
          columns={logonColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="暂无登录事件"
          emptyDescription="Security.evtx 登录事件（4624、4625、4648）。"
        />
      </DenseTableFrame>
    ),
    process: (
      <DenseTableFrame>
        <DenseDataTable
          rows={processEvents}
          columns={processColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="暂无进程创建事件"
          emptyDescription="Security.evtx 进程创建事件（4688）。"
        />
      </DenseTableFrame>
    ),
    account: (
      <DenseTableFrame>
        <DenseDataTable
          rows={accountEvents}
          columns={accountColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="暂无账户管理事件"
          emptyDescription="Security.evtx 账户/任务事件（4698、4702、4720、4732）。"
        />
      </DenseTableFrame>
    ),
    application: (
      <DenseTableFrame>
        <DenseDataTable
          rows={info.applicationEvents}
          columns={appColumns}
          getRowKey={(row) => `${row.eventId}-${row.recordId ?? row.timestamp}`}
          emptyTitle="暂无应用程序事件"
          emptyDescription="Application.evtx 崩溃/安装事件（1000、1001、1002、1033、11707、11708）。"
        />
      </DenseTableFrame>
    ),
  };

  return (
    <ExtractionTableSection
      title="事件日志分析"
      status={info.totalCount > 0 ? 'parsed' : 'notFound'}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['事件总数', info.totalCount.toString()],
        ['开关机', info.bootShutdownCount.toString()],
        ['登录', info.logonLogoffCount.toString()],
        ['进程', info.processExecutionCount.toString()],
        ['应用程序', info.applicationCrashCount.toString()],
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
