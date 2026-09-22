import { AlertTriangle } from 'lucide-react';
import type {
  InstalledSoftware,
  NetworkProfileEntry,
  RegistryNetworkAdapter,
  RegistryValue,
  SamUserAccount,
  UsbDeviceHistory,
  UserAssistEntry,
} from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';

type Translate = (key: string, options?: Record<string, unknown>) => string;

const emptyValue = '-';

function suspiciousLabel(value: string, suspicious: boolean | undefined) {
  return suspicious ? (
    <span className="inline-flex items-center gap-1 text-forensics-error-text font-light">
      <AlertTriangle size={12} aria-hidden="true" />
      {value}
    </span>
  ) : value;
}

/** Builds translated registry table columns without coupling them to panel state. */
export function createRegistryColumns(t: Translate) {
  const samColumns: DenseColumn<SamUserAccount>[] = [
    { key: 'username', title: t('analysis.registry.columns.username'), className: 'w-[130px] font-light', render: (row) => <span className={row.accountStatus === 'disabled' ? 'text-forensics-muted-lighter' : ''}>{row.username}</span> },
    { key: 'rid', title: 'RID', className: 'w-[60px] font-mono text-[10px]', render: (row) => row.ridHex },
    { key: 'sid', title: 'SID', className: 'min-w-[220px] font-mono text-[10px]', render: (row) => row.sid || emptyValue },
    { key: 'accountStatus', title: t('analysis.registry.columns.status'), className: 'w-[60px]', render: (row) => <span className={row.accountStatus === 'enabled' ? 'text-forensics-success-text' : 'text-forensics-muted'}>{t(`analysis.registry.accountStates.${row.accountStatus}`)}</span> },
    { key: 'groups', title: t('analysis.registry.columns.groups'), className: 'w-[180px]', render: (row) => row.groups.join(', ') },
    { key: 'loginCount', title: t('analysis.registry.columns.loginCount'), className: 'w-[80px] text-right', render: (row) => row.loginCount.toLocaleString() },
    { key: 'lastLogin', title: t('analysis.registry.columns.lastLogin'), className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastLogin ? row.lastLogin.replace('T', ' ').replace('Z', '') : t('analysis.registry.values.never') },
    { key: 'profilePath', title: t('analysis.registry.columns.profilePath'), className: 'min-w-[200px] font-mono text-[10px]', render: (row) => row.profilePath ?? emptyValue },
    { key: 'passwordHash', title: t('analysis.registry.columns.passwordHash'), className: 'min-w-[180px] font-mono text-[10px]', render: (row) => row.passwordHash ? <span className="block truncate select-all text-forensics-error-text">{row.passwordHash}</span> : <span className="text-forensics-muted-lighter">{emptyValue}</span> },
    { key: 'passwordHint', title: t('analysis.registry.columns.passwordHint'), className: 'w-[120px]', render: (row) => row.passwordHint ?? emptyValue },
  ];

  const userAssistColumns: DenseColumn<UserAssistEntry>[] = [
    { key: 'programPath', title: t('analysis.registry.columns.programPath'), className: 'min-w-[320px] font-mono text-[10px]', render: (row) => suspiciousLabel(row.programPath, row.isSuspicious) },
    { key: 'execCount', title: t('analysis.registry.columns.executionCount'), className: 'w-[80px] text-right', render: (row) => row.execCount.toLocaleString() },
    { key: 'lastExecTime', title: t('analysis.registry.columns.lastExecution'), className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastExecTime ? row.lastExecTime.replace('T', ' ').replace('Z', '') : emptyValue },
    { key: 'suspiciousReason', title: t('analysis.registry.columns.notes'), className: 'min-w-[200px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  const networkColumns: DenseColumn<NetworkProfileEntry>[] = [
    { key: 'profileName', title: t('analysis.registry.columns.profileName'), className: 'min-w-[180px]', render: (row) => row.profileName },
    { key: 'profileGuid', title: 'GUID', className: 'w-[220px] font-mono text-[10px]', render: (row) => row.profileGuid },
    { key: 'managed', title: t('analysis.registry.columns.managed'), className: 'w-[60px] text-center', render: (row) => row.managed ? t('common.yes') : t('common.no') },
    { key: 'firstNetwork', title: t('analysis.registry.columns.firstNetwork'), className: 'min-w-[160px]', render: (row) => row.firstNetwork ?? emptyValue },
    { key: 'defaultGatewayMacHex', title: t('analysis.registry.columns.gatewayMac'), className: 'w-[140px] font-mono text-[10px]', render: (row) => row.defaultGatewayMacHex ?? emptyValue },
    { key: 'dnsSuffix', title: t('analysis.registry.columns.dnsSuffix'), className: 'w-[120px]', render: (row) => row.dnsSuffix ?? emptyValue },
    { key: 'dateCreated', title: t('analysis.registry.columns.createdAt'), className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateCreated?.slice(0, 10) ?? emptyValue },
    { key: 'dateLastConnected', title: t('analysis.registry.columns.lastConnected'), className: 'w-[110px] font-mono text-[10px]', render: (row) => row.dateLastConnected?.slice(0, 10) ?? emptyValue },
    { key: 'description', title: t('analysis.registry.columns.notes'), className: 'w-[120px]', render: (row) => row.description ?? emptyValue },
  ];

  const adapterColumns: DenseColumn<RegistryNetworkAdapter>[] = [
    { key: 'name', title: t('analysis.registry.columns.adapter'), className: 'w-[120px]', render: (row) => row.name },
    { key: 'description', title: t('analysis.registry.columns.deviceDescription'), className: 'w-[220px]', render: (row) => row.description ?? emptyValue },
    { key: 'macAddress', title: t('analysis.registry.columns.currentMac'), className: 'w-[128px] font-mono text-[10px]', render: (row) => row.macAddress ?? emptyValue },
    { key: 'permanentMacAddress', title: t('analysis.registry.columns.permanentMac'), className: 'w-[128px] font-mono text-[10px]', render: (row) => row.permanentMacAddress ?? emptyValue },
    { key: 'ipAddresses', title: t('analysis.registry.columns.ipAddresses'), className: 'w-[132px] font-mono text-[10px]', render: (row) => row.ipAddresses.join(', ') || emptyValue },
    { key: 'subnetMasks', title: t('analysis.registry.columns.subnetMasks'), className: 'w-[120px] font-mono text-[10px]', render: (row) => row.subnetMasks.join(', ') || emptyValue },
    { key: 'gateways', title: t('analysis.registry.columns.gateways'), className: 'w-[116px] font-mono text-[10px]', render: (row) => row.gateways.join(', ') || emptyValue },
    { key: 'dhcpEnabled', title: 'DHCP', className: 'w-[56px] text-center', render: (row) => row.dhcpEnabled == null ? emptyValue : row.dhcpEnabled ? t('analysis.registry.values.enabled') : t('analysis.registry.values.disabled') },
    { key: 'dhcpServer', title: t('analysis.registry.columns.dhcpServer'), className: 'w-[120px] font-mono text-[10px]', render: (row) => row.dhcpServer ?? emptyValue },
    { key: 'dnsServers', title: t('analysis.registry.columns.dnsServers'), className: 'w-[132px] font-mono text-[10px]', render: (row) => row.dnsServers.join(', ') || emptyValue },
    { key: 'pnpInstanceId', title: t('analysis.registry.columns.pnpInstance'), className: 'w-[260px] font-mono text-[10px]', render: (row) => row.pnpInstanceId ?? emptyValue },
    { key: 'serviceName', title: t('analysis.registry.columns.driverService'), className: 'w-[100px] font-mono text-[10px]', render: (row) => row.serviceName ?? emptyValue },
    { key: 'guid', title: t('analysis.registry.columns.interfaceGuid'), className: 'w-[180px] font-mono text-[10px]', render: (row) => row.guid },
  ];

  const softwareColumns: DenseColumn<InstalledSoftware>[] = [
    { key: 'displayName', title: t('analysis.registry.columns.softwareName'), className: 'min-w-[220px]', render: (row) => suspiciousLabel(row.displayName, row.isSuspicious) },
    { key: 'version', title: t('analysis.registry.columns.version'), className: 'w-[130px] font-mono text-[10px]', render: (row) => row.version },
    { key: 'publisher', title: t('analysis.registry.columns.publisher'), className: 'min-w-[200px]', render: (row) => row.publisher ?? <span className="text-forensics-error-text">{t('common.unknown')}</span> },
    { key: 'installDate', title: t('analysis.registry.columns.installDate'), className: 'w-[100px]', render: (row) => row.installDate ?? emptyValue },
    { key: 'estimatedSize', title: t('analysis.registry.columns.size'), className: 'w-[80px] text-right', render: (row) => row.estimatedSize ?? emptyValue },
  ];

  const usbColumns: DenseColumn<UsbDeviceHistory>[] = [
    { key: 'deviceName', title: t('analysis.registry.columns.deviceName'), className: 'min-w-[200px]', render: (row) => suspiciousLabel(row.deviceName, row.isSuspicious) },
    { key: 'serialNumber', title: t('analysis.registry.columns.serialNumber'), className: 'w-[180px] font-mono text-[10px]', render: (row) => row.serialNumber },
    { key: 'driveLetter', title: t('analysis.registry.columns.driveLetter'), className: 'w-[60px]', render: (row) => row.driveLetter ?? emptyValue },
    { key: 'fileSystem', title: t('analysis.registry.columns.fileSystem'), className: 'w-[80px]', render: (row) => row.fileSystem ?? emptyValue },
    { key: 'capacity', title: t('analysis.registry.columns.capacity'), className: 'w-[70px] text-right', render: (row) => row.capacity ?? emptyValue },
    { key: 'firstConnect', title: t('analysis.registry.columns.firstConnected'), className: 'w-[160px] font-mono text-[10px]', render: (row) => row.firstConnect ? row.firstConnect.replace('T', ' ').replace('Z', '') : emptyValue },
    { key: 'lastConnect', title: t('analysis.registry.columns.lastConnected'), className: 'w-[160px] font-mono text-[10px]', render: (row) => row.lastConnect ? row.lastConnect.replace('T', ' ').replace('Z', '') : emptyValue },
    { key: 'suspiciousReason', title: t('analysis.registry.columns.notes'), className: 'min-w-[220px]', render: (row) => row.suspiciousReason ?? '' },
  ];

  const rawColumns: DenseColumn<RegistryValue>[] = [
    { key: 'hivePath', title: 'Hive', className: 'w-[110px]', render: (row) => row.hivePath || emptyValue },
    { key: 'keyPath', title: 'Key', className: 'min-w-[260px]', render: (row) => row.keyPath || emptyValue },
    { key: 'valueName', title: 'Value', className: 'w-[180px]', render: (row) => row.valueName || emptyValue },
    { key: 'valueType', title: 'Type', className: 'w-[90px]', render: (row) => row.valueType || emptyValue },
    { key: 'data', title: 'Data', className: 'min-w-[220px]', render: (row) => row.data || emptyValue },
    { key: 'parser', title: 'Parser', className: 'w-[150px]', render: (row) => row.parser || emptyValue },
  ];

  return { samColumns, userAssistColumns, networkColumns, adapterColumns, softwareColumns, usbColumns, rawColumns };
}
