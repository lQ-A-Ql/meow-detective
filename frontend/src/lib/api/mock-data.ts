import {
  ArtifactRow,
  CaseMetrics,
  CaseSummary,
  DataSourceSummary,
  FileEntryRow,
  JobSnapshot,
  RecentCase,
  RecentObject,
  ReportHistoryItem,
  ReportTemplate,
  SearchHit,
  TimelineEventDto,
  TraceItem,
  WarningItem,
} from '@/types/models';

export const currentCase: CaseSummary = {
  id: 'case-2026-fx-091',
  name: 'WannaCry 爆发溯源',
  number: '2026-FX-091',
  examiner: 'Qin Ao',
  createdAt: '2026-05-14T08:30:00Z',
  updatedAt: '2026-05-16T11:20:00Z',
};

export const caseMetrics: CaseMetrics = {
  dataSourceCount: 4,
  indexedFileCount: 1492033,
  timelineEventCount: 8401922,
  artifactCount: 45102,
};

export const recentObjects: RecentObject[] = [
  {
    id: 'obj-1',
    title: 'tasksche.exe',
    detail: '匹配 Yara 规则: WannaCry_Payload',
    time: '2 小时前',
    kind: 'file',
  },
  {
    id: 'obj-2',
    title: 'HKLM\\SOFTWARE\\WanaCrypt0r',
    detail: '注册表项已创建',
    time: '3 小时前',
    kind: 'registry',
  },
];

export const recentCases: RecentCase[] = [
  {
    caseRoot: 'D:/Cases/WannaCry',
    name: 'WannaCry 爆发溯源',
    openedAt: '2026-05-16T11:20:00Z',
  },
  {
    caseRoot: 'D:/Cases/RDP-Lateral',
    name: 'RDP 横向移动排查',
    openedAt: '2026-05-15T17:40:00Z',
  },
];

export const dataSources: DataSourceSummary[] = [
  {
    id: 'ds-001',
    name: 'FINCH-1.E01',
    kind: 'e01',
    sourcePath: 'E:/evidence/FINCH-1.E01',
    importedAt: '2026-05-16T10:30:00Z',
    fileCount: 12844,
    partitions: [
      {
        index: 1,
        name: 'EFI system partition',
        kindLabel: 'FAT',
        status: 'supported',
        offset: 1048576,
        length: 104857600,
        filesystem: 'FAT',
        typeGuid: 'C12A7328-F81F-11D2-BA4B-00A0C93EC93B',
      },
      {
        index: 2,
        name: 'Microsoft reserved partition',
        kindLabel: 'Microsoft reserved',
        status: 'unsupported',
        offset: 105906176,
        length: 16777216,
        typeGuid: 'E3C9E316-0B5C-4DB8-817D-F92DF00215AE',
      },
      {
        index: 3,
        name: 'Basic data partition',
        kindLabel: 'NTFS',
        status: 'supported',
        offset: 122683392,
        length: 136575975424,
        filesystem: 'NTFS',
        typeGuid: 'EBD0A0A2-B9E5-4433-87C0-68B6B72699C7',
      },
      {
        index: 5,
        name: 'Basic data partition',
        kindLabel: 'BitLocker',
        status: 'locked',
        offset: 137436856320,
        length: 137438953472,
        filesystem: 'BitLocker',
        typeGuid: 'EBD0A0A2-B9E5-4433-87C0-68B6B72699C7',
        unlockHint: 'BitLocker 分区需要先解锁后才能浏览文件内容。',
      },
    ],
  },
  {
    id: 'ds-002',
    name: 'System32 Snapshot',
    kind: 'logical_directory',
    sourcePath: 'E:/mounted/System32',
    importedAt: '2026-05-16T12:05:00Z',
    fileCount: 2801,
    partitions: [],
  },
];

export const filesTree = [
  { id: 'tree-system32', name: 'System32', depth: 0, expanded: true },
  { id: 'tree-config', name: 'config', depth: 1 },
  { id: 'tree-drivers', name: 'drivers', depth: 1 },
  { id: 'tree-winevt', name: 'winevt', depth: 1, expanded: true, active: true },
  { id: 'tree-logs', name: 'Logs', depth: 2 },
];

export const fileRows: FileEntryRow[] = [
  {
    id: 'file-ntoskrnl',
    path: 'C:/Windows/System32/ntoskrnl.exe',
    name: 'ntoskrnl.exe',
    entryType: 'file',
    size: 10200000,
    deleted: false,
    modifiedAt: '2026-05-10 14:23:01',
    hashSha256: '7e1d6a0d9e8d7a2fd2814b2ef06cfd8d3b0b789f7c44a11c03f47005d82fa4e0',
  },
  {
    id: 'file-cmd-exe',
    path: 'C:/Windows/System32/cmd.exe',
    name: 'cmd.exe',
    entryType: 'file',
    size: 289000,
    deleted: false,
    modifiedAt: '2025-11-20 09:11:00',
    accessedAt: '2026-05-15 10:02:11',
    changedAt: '2025-11-20 09:11:00',
    createdAt: '2024-03-01 12:00:00',
    hashSha256: '2d7c4fd1ab783f91688fc2cdbf951a94544d45ef09c9cf63365b4de6b9f10d52',
  },
  {
    id: 'file-hal-dll',
    path: 'C:/Windows/System32/hal.dll',
    name: 'hal.dll',
    entryType: 'file',
    size: 1100000,
    deleted: false,
    modifiedAt: '2026-02-15 11:44:22',
    hashSha256: '9c62cd5f0a36e9385331d562d3e0e0518ed27af0d22e09b0586d2d9b4c48de8a',
  },
];

export const searchHits: SearchHit[] = [
  {
    fileId: 'file-doc-1',
    path: 'C:/Users/Admin/Documents/Project_Alpha.doc',
    score: 0.98,
    snippets: [
      {
        text: 'CONFIDENTIAL: The upcoming merger with GlobalCorp will be announced on May 20th.',
        highlights: [{ start: 40, end: 50 }],
      },
    ],
  },
  {
    fileId: 'file-doc-2',
    path: 'C:/Data/Financials_Q1.xls',
    score: 0.72,
    snippets: [{ text: 'Q1 Revenue sheet 4...', highlights: [] }],
  },
];

export const timelineEvents: TimelineEventDto[] = [
  {
    id: 'timeline-1',
    sourceObjectId: 'evtx-security-4624',
    eventType: 'Logon 4624',
    ts: '2026-05-11 14:01:12',
    title: 'Successful logon for user SYSTEM',
    description: '检测到成功登录事件。',
    attrs: { source: 'Security.evtx' },
  },
  {
    id: 'timeline-2',
    sourceObjectId: 'file-cmd-exe',
    eventType: 'File Created',
    ts: '2026-05-11 14:02:45',
    title: 'C:/Windows/tasksche.exe',
    description: '在卷根目录检测到文件创建。疑似有效载荷投放。',
    attrs: { source: 'File System' },
  },
  {
    id: 'timeline-3',
    sourceObjectId: 'evtx-system-7045',
    eventType: 'Service 7045',
    ts: '2026-05-11 14:03:10',
    title: 'New service installed: mssecsvc2.0',
    description: '检测到新服务安装。',
    attrs: { source: 'System.evtx' },
  },
];

export const artifactFamilies = ['Prefetch', 'Amcache', 'Shimcache', 'LNK', 'JumpLists', 'UserAssist'];

export const artifactRows: ArtifactRow[] = [
  {
    id: 'artifact-1',
    artifactType: 'LNK',
    title: 'C:/Users/Admin/Desktop/cmd.lnk',
    summary: '目标路径: C:/Windows/System32/cmd.exe',
    createdAt: '2026-01-10 09:12:00',
    attrs: { targetPath: 'C:/Windows/System32/cmd.exe', arguments: '-' },
  },
  {
    id: 'artifact-2',
    artifactType: 'LNK',
    title: 'C:/Users/Admin/Recent/payload.lnk',
    summary: '目标路径: C:/Temp/payload.exe',
    createdAt: '2026-05-11 14:05:22',
    attrs: {
      targetPath: 'C:/Temp/payload.exe',
      arguments: '-hidden -execute',
      driveType: 'Fixed (3)',
      volumeSerial: 'A1B2-C3D4',
      machineId: 'DESKTOP-X921',
    },
  },
];

export const reportTemplates: ReportTemplate[] = [
  { id: 'summary', name: '执行摘要', description: '关键发现、时间线和受感染资产的高层次概述。PDF 格式。' },
  { id: 'detailed', name: '综合详情', description: '包含所有技术痕迹、完整时间线和提取字符串。HTML/JSON 格式。' },
  { id: 'ioc', name: 'IOC 导出', description: '妥协指标（哈希、IP、域名）的 STIX/CSV 导出。' },
];

export const reportHistory: ReportHistoryItem[] = [
  {
    id: 'report-1',
    fileName: 'Executive_Summ_v1.pdf',
    createdBy: 'Qin Ao',
    createdAt: '2026-05-14 10:00Z',
    status: 'completed',
  },
  {
    id: 'report-2',
    fileName: 'IOC_List_Final.csv',
    createdBy: 'Qin Ao',
    createdAt: '2026-05-15 11:30Z',
    status: 'running',
    progress: 60,
  },
];

export const jobs: JobSnapshot[] = [
  {
    id: 'job-1',
    name: '导入数据源',
    scope: '分区 2/5',
    progress: 45,
    status: 'running',
    detail: 'Enumerating Partition 3 (NTFS) - Basic data partition',
    currentPartition: 'Partition 3 (NTFS) - Basic data partition',
    completedPartitions: 1,
    totalPartitions: 5,
    partitionProgress: 42,
  },
  {
    id: 'job-2',
    name: '索引 MFT',
    scope: 'Vol_1 / $MFT',
    progress: 100,
    status: 'completed',
    detail: '完成 (2m 14s)',
    currentPartition: undefined,
    completedPartitions: undefined,
    totalPartitions: undefined,
    partitionProgress: undefined,
  },
  {
    id: 'job-3',
    name: '哈希计算',
    scope: '已处理 1,492,033 个文件',
    progress: 100,
    status: 'completed',
    detail: '完成 (45m 10s)',
    currentPartition: undefined,
    completedPartitions: undefined,
    totalPartitions: undefined,
    partitionProgress: undefined,
  },
];

export const warnings: WarningItem[] = [
  {
    id: 'warning-1',
    title: '事件流滞后',
    detail: 'timeline bucket cache 命中后等待回源校验。',
  },
];

export const traces: TraceItem[] = [
  { id: 'trace-1', ts: '14:08:11', message: 'job.started artifacts-lnk-scan' },
  { id: 'trace-2', ts: '14:09:02', message: 'artifact.added payload.lnk' },
  { id: 'trace-3', ts: '14:09:11', message: 'timeline.updated file-created' },
];

export const hexPreview = [
  '4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00',
  'B8 00 00 00 00 00 00 00 40 00 00 00 00 00 00 00',
  '00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00',
  '00 00 00 00 00 00 00 00 00 00 00 00 80 00 00 00',
  '0E 1F BA 0E 00 B4 09 CD 21 B8 01 4C CD 21 54 68',
];
