import { useState } from 'react';
import {
  Clock,
  Database,
  HardDrive,
  Shield,
  Usb,
  Wifi,
} from 'lucide-react';
import type {
  InstalledSoftware,
  NetworkProfileEntry,
  RegistryExtractionSummary,
  RegistryHiveOverview,
  RegistryStructuredSummary,
  RegistryValue,
  SamUserAccount,
  UsbDeviceHistory,
  UserAssistEntry,
} from '@/types/models';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  DenseTableFrame,
  ExtractionTableSection,
} from './helpers';

export function RegistryExtractionPanel({
  summary,
  structured,
}: {
  summary?: RegistryExtractionSummary;
  structured?: RegistryStructuredSummary;
}) {
  const [activeTab, setActiveTab] = useState<'users' | 'activity' | 'network' | 'software' | 'usb' | 'raw'>('users');

  const info = summary ?? { status: 'unavailable' as const, total: 0, values: [], generatedAt: '', warnings: ['注册表提取结果暂不可用。'] };
  const s = structured;

  // Hive overview status badges
  const hiveOverviews: RegistryHiveOverview[] = s?.hiveOverviews ?? [];

  // SAM users columns
  const samColumns: DenseColumn<SamUserAccount>[] = [
    {
      key: 'username', title: '用户名', className: 'w-[130px] font-light',
      render: (row) => (
        <span className={row.accountStatus === 'disabled' ? 'text-forensics-muted-lighter' : ''}>
          {row.username}
        </span>
      ),
    },
    { key: 'rid', title: 'RID', className: 'w-[60px] font-mono text-[10px]', render: (row) => row.ridHex },
    { key: 'sid', title: 'SID', className: 'min-w-[220px] font-mono text-[10px]', render: (row) => row.sid || '-' },
    {
      key: 'accountStatus', title: '状态', className: 'w-[60px]',
      render: (row) => (
        <span className={row.accountStatus === 'enabled' ? 'text-forensics-success-text' : 'text-forensics-muted'}>
          {row.accountStatus === 'enabled' ? '启用' : row.accountStatus === 'locked' ? '锁定' : '禁用'}
        </span>
      ),
    },
    { key: 'groups', title: '组', className: 'w-[180px]', render: (row) => row.groups.join(', ') },
    { key: 'loginCount', title: '登录次数', className: 'w-[80px] text-right', render: (row) => row.loginCount.toLocaleString() },
    { key: 'lastLogin', title: '最后登录', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastLogin ? row.lastLogin.replace('T', ' ').replace('Z', '') : '从未登录' },
    { key: 'profilePath', title: 'Profile 路径', className: 'min-w-[200px] font-mono text-[10px]', render: (row) => row.profilePath ?? '-' },
    {
      key: 'passwordHash', title: '密码哈希 (LM:NT)', className: 'min-w-[180px] font-mono text-[10px]',
      render: (row) => row.passwordHash
        ? <span className="select-all text-forensics-error-text truncate block">{row.passwordHash}</span>
        : <span className="text-forensics-muted-lighter">—</span>,
    },
    { key: 'passwordHint', title: '密码提示', className: 'w-[120px]', render: (row) => row.passwordHint ?? '-' },
  ];

  // UserAssist columns
  const userAssistColumns: DenseColumn<UserAssistEntry>[] = [
    {
      key: 'programPath', title: '程序路径', className: 'min-w-[320px] font-mono text-[10px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-forensics-error-text font-light' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.programPath}
        </span>
      ),
    },
    { key: 'execCount', title: '执行次数', className: 'w-[80px] text-right', render: (row) => row.execCount.toLocaleString() },
    { key: 'lastExecTime', title: '最后执行', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastExecTime ? row.lastExecTime.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'suspiciousReason', title: '备注', className: 'min-w-[200px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  // Network profile columns (SOFTWARE\NetworkList)
  const networkColumns: DenseColumn<NetworkProfileEntry>[] = [
    { key: 'profileName', title: '配置文件名称', className: 'min-w-[180px]', render: (row) => row.profileName },
    { key: 'profileGuid', title: 'GUID', className: 'w-[220px] font-mono text-[10px]', render: (row) => row.profileGuid },
    { key: 'managed', title: '托管', className: 'w-[60px] text-center', render: (row) => (row.managed ? '是' : '否') },
    { key: 'firstNetwork', title: '首次网络', className: 'min-w-[160px]', render: (row) => row.firstNetwork ?? '-' },
    { key: 'defaultGatewayMacHex', title: '网关 MAC', className: 'w-[140px] font-mono text-[10px]', render: (row) => row.defaultGatewayMacHex ?? '-' },
    { key: 'dnsSuffix', title: 'DNS 后缀', className: 'w-[120px]', render: (row) => row.dnsSuffix ?? '-' },
    { key: 'dateCreated', title: '创建时间', className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateCreated?.slice(0, 10) ?? '-' },
    { key: 'dateLastConnected', title: '最后连接', className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateLastConnected?.slice(0, 10) ?? '-' },
    { key: 'description', title: '备注', className: 'w-[120px]', render: (row) => row.description ?? '-' },
  ];

  // Installed software columns
  const softwareColumns: DenseColumn<InstalledSoftware>[] = [
    {
      key: 'displayName', title: '软件名称', className: 'min-w-[220px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-forensics-error-text font-light' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.displayName}
        </span>
      ),
    },
    { key: 'version', title: '版本', className: 'w-[130px] font-mono text-[10px]', render: (row) => row.version },
    { key: 'publisher', title: '发布商', className: 'min-w-[200px]', render: (row) => row.publisher ?? <span className="text-forensics-error-text">未知</span> },
    { key: 'installDate', title: '安装日期', className: 'w-[100px]', render: (row) => row.installDate ?? '-' },
    { key: 'estimatedSize', title: '大小', className: 'w-[80px] text-right', render: (row) => row.estimatedSize ?? '-' },
  ];

  // USB device columns
  const usbColumns: DenseColumn<UsbDeviceHistory>[] = [
    {
      key: 'deviceName', title: '设备名称', className: 'min-w-[200px]',
      render: (row) => (
        <span className={row.isSuspicious ? 'text-forensics-error-text font-light' : ''}>
          {row.isSuspicious ? '🚩 ' : ''}{row.deviceName}
        </span>
      ),
    },
    { key: 'serialNumber', title: '序列号', className: 'w-[180px] font-mono text-[10px]', render: (row) => row.serialNumber },
    { key: 'driveLetter', title: '盘符', className: 'w-[60px]', render: (row) => row.driveLetter ?? '-' },
    { key: 'fileSystem', title: '文件系统', className: 'w-[80px]', render: (row) => row.fileSystem ?? '-' },
    { key: 'capacity', title: '容量', className: 'w-[70px] text-right', render: (row) => row.capacity ?? '-' },
    { key: 'firstConnect', title: '首次连接', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.firstConnect ? row.firstConnect.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'lastConnect', title: '最后连接', className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastConnect ? row.lastConnect.replace('T', ' ').replace('Z', '') : '-' },
    { key: 'suspiciousReason', title: '备注', className: 'min-w-[220px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  // Raw registry columns
  const rawColumns: DenseColumn<RegistryValue>[] = [
    { key: 'hivePath', title: 'Hive', className: 'w-[110px]', render: (row) => row.hivePath || '-' },
    { key: 'keyPath', title: 'Key', className: 'min-w-[260px]', render: (row) => row.keyPath || '-' },
    { key: 'valueName', title: 'Value', className: 'w-[180px]', render: (row) => row.valueName || '-' },
    { key: 'valueType', title: 'Type', className: 'w-[90px]', render: (row) => row.valueType || '-' },
    { key: 'data', title: 'Data', className: 'min-w-[220px]', render: (row) => row.data || '-' },
    { key: 'parser', title: 'Parser', className: 'w-[150px]', render: (row) => row.parser || '-' },
  ];

  const TABS: Array<{ key: typeof activeTab; label: string; icon: typeof Database }> = [
    { key: 'users', label: '用户账户', icon: Shield },
    { key: 'activity', label: '用户活动', icon: Clock },
    { key: 'network', label: '网络配置', icon: Wifi },
    { key: 'software', label: '软件列表', icon: Database },
    { key: 'usb', label: 'USB 设备', icon: Usb },
    { key: 'raw', label: '原始键值', icon: HardDrive },
  ];

  const hiveSummaryStats: Array<[string, string]> = hiveOverviews.map(
    (h) => [h.hiveName, `${h.keyValueCount} 条${h.txlogMerged ? ' · txlog✓' : ''}${h.deletedKeysFound > 0 ? ` · ⚠${h.deletedKeysFound}已删` : ''}`],
  );

  return (
    <ExtractionTableSection
      title="注册表提取"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={hiveSummaryStats.length > 0 ? hiveSummaryStats : [
        ['键值数', info.total.toString()],
        ['来源 Hive', new Set(info.values.map((v) => v.hivePath)).size.toString()],
      ]}
    >
      <PanelTabs
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as typeof activeTab)}
        tabs={TABS.map(({ key, label, icon }) => ({ value: key, label, icon }))}
        variant="underline"
      >
        <DenseTableFrame>
          <TabsContent value="users" className="min-h-0">
            <DenseDataTable
              rows={s?.samUsers ?? []}
              columns={samColumns}
              getRowKey={(row) => row.username}
              emptyTitle="暂无用户账户数据"
              emptyDescription="运行提取后将显示 SAM 账户信息。"
            />
          </TabsContent>
          <TabsContent value="activity" className="min-h-0">
            <DenseDataTable
              rows={s?.userAssistEntries ?? []}
              columns={userAssistColumns}
              getRowKey={(row) => row.programPath}
              emptyTitle="暂无 UserAssist 数据"
              emptyDescription="从 NTUSER.DAT 提取程序执行记录。"
            />
          </TabsContent>
          <TabsContent value="network" className="min-h-0">
            <DenseDataTable
              rows={s?.networkProfiles ?? []}
              columns={networkColumns}
              getRowKey={(row) => row.profileGuid}
              emptyTitle="暂无网络配置数据"
              emptyDescription="从 SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\NetworkList 提取网络配置文件及 Wi-Fi 连接历史。"
            />
          </TabsContent>
          <TabsContent value="software" className="min-h-0">
            <DenseDataTable
              rows={s?.installedSoftware ?? []}
              columns={softwareColumns}
              getRowKey={(row) => `${row.displayName}-${row.version}`}
              emptyTitle="暂无已安装软件数据"
              emptyDescription="从 SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall 提取。"
            />
          </TabsContent>
          <TabsContent value="usb" className="min-h-0">
            <DenseDataTable
              rows={s?.usbDevices ?? []}
              columns={usbColumns}
              getRowKey={(row) => row.serialNumber}
              emptyTitle="暂无 USB 设备历史"
              emptyDescription="从 SYSTEM hive USBSTOR 提取连接记录。"
            />
          </TabsContent>
          <TabsContent value="raw" className="min-h-0">
            <DenseDataTable
              rows={info.values}
              columns={rawColumns}
              getRowKey={(row) => row.artifactId}
              emptyTitle="暂无原始键值数据"
              emptyDescription="运行提取后会显示关键 hive 的 key/value 摘要。"
            />
          </TabsContent>
        </DenseTableFrame>
      </PanelTabs>
    </ExtractionTableSection>
  );
}
