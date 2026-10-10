import { HardDrive } from 'lucide-react';
import type { TFunction } from 'i18next';
import { SummaryTable, type SummaryTableColumn, type SummaryTableRow } from '@/components/tables/SummaryTable';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetMemberSummary, LinuxTopologyScopeSummary } from '@/types/linuxCluster';
import type { DataSourceSummary } from '@/types/dataSource';
import type { InfrastructureHostFact } from '../types';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function MemberEvidenceTable({ members, sourceMetadata, scopes, hostFacts, networkFacts, selectedIndex, onSelect, t }: { members: LinuxEvidenceSetMemberSummary[]; sourceMetadata: Map<string, DataSourceSummary>; scopes: LinuxTopologyScopeSummary[]; hostFacts: Map<string, InfrastructureHostFact>; networkFacts: InfrastructureNetworkFact[]; selectedIndex?: number; onSelect: (index: number) => void; t: TFunction }) {
  const roleBySource = new Map(scopes.flatMap((scope) => scope.memberRoles.map((member) => [member.dataSourceId, member.role] as const)));
  const columns: SummaryTableColumn[] = [
    { key: 'member', label: t('infrastructure.workspace.members.member'), className: 'max-w-64 truncate font-mono text-forensics-text' },
    { key: 'kind', label: t('infrastructure.workspace.members.kind'), className: 'text-forensics-muted' },
    { key: 'import', label: t('infrastructure.workspace.members.import') },
    { key: 'files', label: t('infrastructure.workspace.members.files'), className: 'font-mono text-forensics-text' },
    { key: 'hostname', label: t('infrastructure.workspace.members.hostname'), className: 'max-w-36 truncate text-forensics-text' },
    { key: 'os', label: t('infrastructure.workspace.members.os'), className: 'max-w-48 truncate text-[10px] text-forensics-muted' },
    { key: 'address', label: t('infrastructure.workspace.members.address'), className: 'max-w-36 truncate font-mono text-[10px] text-forensics-muted' },
    { key: 'role', label: t('infrastructure.workspace.members.role') },
    { key: 'hash', label: t('infrastructure.workspace.members.hash') },
  ];
  const rows: SummaryTableRow[] = members.map((member) => {
    const source = member.dataSourceId ? sourceMetadata.get(member.dataSourceId) : undefined;
    const hostFact = member.dataSourceId ? hostFacts.get(member.dataSourceId) : undefined;
    const addresses = member.addresses.length ? member.addresses : networkFacts.filter((fact) => fact.dataSourceId === member.dataSourceId && fact.factKind === 'interface_address').map((fact) => fact.value);
    const role = member.roles[0] ?? (member.dataSourceId ? roleBySource.get(member.dataSourceId) : undefined);
    const os = [member.operatingSystem ?? hostFact?.operatingSystem, member.osVersion ?? hostFact?.operatingSystemVersion, member.kernelVersion ?? hostFact?.kernelVersion].filter(Boolean).join(' / ');
    const hostname = member.hostname ?? hostFact?.hostname;
    return {
      key: member.memberIndex,
      selected: selectedIndex === member.memberIndex,
      onSelect: () => onSelect(member.memberIndex),
      className: 'cursor-pointer',
      cells: [
        { content: `${member.memberIndex + 1}. ${member.sourceName}`, title: member.sourcePath },
        member.sourceKind.toUpperCase(),
        <StatusBadge key="import" label={statusLabel(member.importState, t)} variant={statusVariant(member.importState)} />,
        source?.fileCount?.toLocaleString() ?? '—',
        { content: hostname ?? '—', title: hostname },
        { content: os || '—', title: os },
        { content: addresses.join(', ') || '—', title: addresses.join(', ') },
        <StatusBadge key="role" label={t(`infrastructure.workspace.topology.roles.${role ?? 'unknown'}`, { defaultValue: role ?? 'unknown' })} variant="outline" />,
        <StatusBadge key="hash" label={statusLabel(member.hashStatus, t)} variant={statusVariant(member.hashStatus)} />,
      ],
    };
  });
  return (
    <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={HardDrive} title={t('infrastructure.workspace.members.title')} subtitle={t('infrastructure.workspace.members.description')} className="px-4 py-3" />
      <div className="overflow-x-auto">
        <SummaryTable columns={columns} rows={rows} minWidthClass="min-w-[1060px]" />
      </div>
      {members.length === 0 ? <EmptyState>{t('infrastructure.workspace.members.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
