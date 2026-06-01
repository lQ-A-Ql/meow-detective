import {
  ArtifactRow,
  AnalysisFileClassification,
  AnalysisSystemInfo,
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
    time: '2026-05-16T09:20:00Z',
    kind: 'file',
  },
  {
    id: 'obj-2',
    title: 'HKLM\\SOFTWARE\\WanaCrypt0r',
    detail: '注册表项已创建',
    time: '2026-05-16T08:20:00Z',
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

export const analysisSystemInfo: AnalysisSystemInfo = {
  networkAdapters: [],
  bootHistory: [],
  status: 'notParsed',
  warnings: [
    '系统信息解析器尚未接入 Registry/EVTX；当前不会输出未验证主机事实。',
  ],
  provenance: [
    {
      dataSourceId: 'ds-001',
      artifactPath: 'Windows/System32/config/SYSTEM',
      parser: 'registry.system',
      parsedAt: '2026-06-01T10:00:00Z',
      status: 'notParsed',
      warnings: ['已发现 Registry hive，但 key/value 遍历尚未实现。'],
    },
    {
      dataSourceId: 'ds-001',
      artifactPath: 'Windows/System32/winevt/Logs/System.evtx',
      parser: 'evtx.boot_shutdown',
      parsedAt: '2026-06-01T10:00:00Z',
      status: 'unavailable',
      warnings: ['EVTX parser not implemented'],
    },
  ],
};

export const analysisClassifications: AnalysisFileClassification[] = [
  {
    category: 'Executables',
    totalSize: 10200000,
    status: 'parsed',
    warnings: [],
    files: [
      {
        fileId: 'file-ntoskrnl',
        path: 'C:/Windows/System32/ntoskrnl.exe',
        name: 'ntoskrnl.exe',
        size: 10200000,
        fileType: 'PE',
        magicDescription: 'Windows Executable',
        provenance: {
          dataSourceId: 'ds-001',
          artifactPath: 'C:/Windows/System32/ntoskrnl.exe',
          parser: 'analysis.magic',
          parsedAt: '2026-06-01T10:00:00Z',
          status: 'parsed',
          warnings: [],
        },
      },
    ],
    provenance: [
      {
        dataSourceId: 'ds-001',
        artifactPath: 'C:/Windows/System32/ntoskrnl.exe',
        parser: 'analysis.magic',
        parsedAt: '2026-06-01T10:00:00Z',
        status: 'parsed',
        warnings: [],
      },
    ],
  },
  {
    category: 'Documents',
    totalSize: 4096,
    status: 'parsed',
    warnings: [],
    files: [
      {
        fileId: 'file-doc-1',
        path: 'C:/Users/Admin/Documents/Project_Alpha.doc',
        name: 'Project_Alpha.doc',
        size: 4096,
        fileType: 'Office',
        magicDescription: 'Office Document',
        provenance: {
          dataSourceId: 'ds-001',
          artifactPath: 'C:/Users/Admin/Documents/Project_Alpha.doc',
          parser: 'analysis.magic',
          parsedAt: '2026-06-01T10:00:00Z',
          status: 'parsed',
          warnings: [],
        },
      },
    ],
    provenance: [
      {
        dataSourceId: 'ds-001',
        artifactPath: 'C:/Users/Admin/Documents/Project_Alpha.doc',
        parser: 'analysis.magic',
        parsedAt: '2026-06-01T10:00:00Z',
        status: 'parsed',
        warnings: [],
      },
    ],
  },
  {
    category: 'Other',
    totalSize: 1100000,
    status: 'parsed',
    warnings: ['仅分析前 1000 个文件；数据源包含 12844 个文件。'],
    files: [
      {
        fileId: 'file-hal-dll',
        path: 'C:/Windows/System32/hal.dll',
        name: 'hal.dll',
        size: 1100000,
        fileType: 'Unknown',
        magicDescription: 'Unknown file type',
        provenance: {
          dataSourceId: 'ds-001',
          artifactPath: 'C:/Windows/System32/hal.dll',
          parser: 'analysis.magic',
          parsedAt: '2026-06-01T10:00:00Z',
          status: 'parsed',
          warnings: [],
        },
      },
    ],
    provenance: [
      {
        dataSourceId: 'ds-001',
        artifactPath: 'C:/Windows/System32/hal.dll',
        parser: 'analysis.magic',
        parsedAt: '2026-06-01T10:00:00Z',
        status: 'parsed',
        warnings: [],
      },
    ],
  },
];

export const analysisSummary = `# 数据源分析报告

## 系统信息

- **状态**: 未解析

### 系统信息告警

- 系统信息解析器尚未接入 Registry/EVTX；当前不会输出未验证主机事实。

## 文件分类

| 类别 | 文件数 | 总大小 | 状态 |
|------|--------|--------|------|
| Executables | 1 | 9.7 MB | 已解析 |
| Documents | 1 | 0.0 MB | 已解析 |
| Other | 1 | 1.0 MB | 已解析 |
`;

export const filesTree = [
  { id: 'tree-system32', name: 'System32', depth: 0, hasChildren: true, expanded: true },
  { id: 'tree-config', name: 'config', depth: 1, hasChildren: false },
  { id: 'tree-drivers', name: 'drivers', depth: 1, hasChildren: false },
  { id: 'tree-winevt', name: 'winevt', depth: 1, hasChildren: true, expanded: true, active: true },
  { id: 'tree-logs', name: 'Logs', depth: 2, hasChildren: false },
];

export const fileRows: FileEntryRow[] = [
  {
    id: 'file-ntoskrnl',
    path: 'C:/Windows/System32/ntoskrnl.exe',
    name: 'ntoskrnl.exe',
    entryType: 'file',
    size: 10200000,
    deleted: false,
    modifiedAt: '2026-05-10T14:23:01Z',
    hashSha256: '7e1d6a0d9e8d7a2fd2814b2ef06cfd8d3b0b789f7c44a11c03f47005d82fa4e0',
  },
  {
    id: 'file-cmd-exe',
    path: 'C:/Windows/System32/cmd.exe',
    name: 'cmd.exe',
    entryType: 'file',
    size: 289000,
    deleted: false,
    modifiedAt: '2025-11-20T09:11:00Z',
    accessedAt: '2026-05-15T10:02:11Z',
    changedAt: '2025-11-20T09:11:00Z',
    createdAt: '2024-03-01T12:00:00Z',
    hashSha256: '2d7c4fd1ab783f91688fc2cdbf951a94544d45ef09c9cf63365b4de6b9f10d52',
  },
  {
    id: 'file-hal-dll',
    path: 'C:/Windows/System32/hal.dll',
    name: 'hal.dll',
    entryType: 'file',
    size: 1100000,
    deleted: false,
    modifiedAt: '2026-02-15T11:44:22Z',
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
    ts: '2026-05-11T14:01:12Z',
    title: 'Successful logon for user SYSTEM',
    description: '检测到成功登录事件。',
    attrs: { source: 'Security.evtx' },
  },
  {
    id: 'timeline-2',
    sourceObjectId: 'file-cmd-exe',
    eventType: 'File Created',
    ts: '2026-05-11T14:02:45Z',
    title: 'C:/Windows/tasksche.exe',
    description: '在卷根目录检测到文件创建。疑似有效载荷投放。',
    attrs: { source: 'File System' },
  },
  {
    id: 'timeline-3',
    sourceObjectId: 'evtx-system-7045',
    eventType: 'Service 7045',
    ts: '2026-05-11T14:03:10Z',
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
    createdAt: '2026-01-10T09:12:00Z',
    attrs: { targetPath: 'C:/Windows/System32/cmd.exe', arguments: '-' },
  },
  {
    id: 'artifact-2',
    artifactType: 'LNK',
    title: 'C:/Users/Admin/Recent/payload.lnk',
    summary: '目标路径: C:/Temp/payload.exe',
    createdAt: '2026-05-11T14:05:22Z',
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
    createdAt: '2026-05-14T10:00:00Z',
    status: 'completed',
  },
  {
    id: 'report-2',
    fileName: 'IOC_List_Final.csv',
    createdBy: 'Qin Ao',
    createdAt: '2026-05-15T11:30:00Z',
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
    warningCount: 0,
    skippedCount: 0,
    failedCount: 0,
    partial: false,
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
    warningCount: 2,
    skippedCount: 1,
    failedCount: 0,
    partial: true,
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
    warningCount: 0,
    skippedCount: 0,
    failedCount: 0,
    partial: false,
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
  { id: 'trace-1', ts: '2026-05-16T14:08:11Z', message: 'job.started artifacts-lnk-scan' },
  { id: 'trace-2', ts: '2026-05-16T14:09:02Z', message: 'artifact.added payload.lnk' },
  { id: 'trace-3', ts: '2026-05-16T14:09:11Z', message: 'timeline.updated file-created' },
];
