import { HardDrive } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetMemberSummary } from '@/types/linuxCluster';
import type { DataSourceSummary } from '@/types/dataSource';
import type { InfrastructureHostFact } from '../types';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import type { LinuxTopologyScopeSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function MemberEvidenceTable({ members, sourceMetadata, scopes, hostFacts, networkFacts, selectedIndex, onSelect, t }: { members: LinuxEvidenceSetMemberSummary[]; sourceMetadata: Map<string, DataSourceSummary>; scopes: LinuxTopologyScopeSummary[]; hostFacts: Map<string, InfrastructureHostFact>; networkFacts: InfrastructureNetworkFact[]; selectedIndex?: number; onSelect: (index: number) => void; t: TFunction }) {
  const roleBySource = new Map(scopes.flatMap((scope) => scope.memberRoles.map((member) => [member.dataSourceId, member.role] as const)));
  return (
    <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={HardDrive} title={t('infrastructure.workspace.members.title')} subtitle={t('infrastructure.workspace.members.description')} className="px-4 py-3" />
      <div className="overflow-x-auto">
        <Table className="min-w-[1060px] text-left text-xs">
          <TableHeader><TableRow><TableHead>{t('infrastructure.workspace.members.member')}</TableHead><TableHead>{t('infrastructure.workspace.members.kind')}</TableHead><TableHead>{t('infrastructure.workspace.members.import')}</TableHead><TableHead>{t('infrastructure.workspace.members.files')}</TableHead><TableHead>{t('infrastructure.workspace.members.hostname')}</TableHead><TableHead>{t('infrastructure.workspace.members.os')}</TableHead><TableHead>{t('infrastructure.workspace.members.address')}</TableHead><TableHead>{t('infrastructure.workspace.members.role')}</TableHead><TableHead>{t('infrastructure.workspace.members.hash')}</TableHead></TableRow></TableHeader>
          <TableBody>{members.map((member) => { const source = member.dataSourceId ? sourceMetadata.get(member.dataSourceId) : undefined; const hostFact = member.dataSourceId ? hostFacts.get(member.dataSourceId) : undefined; const addresses = member.addresses.length ? member.addresses : networkFacts.filter((fact) => fact.dataSourceId === member.dataSourceId && fact.factKind === 'interface_address').map((fact) => fact.value); const role = member.roles[0] ?? (member.dataSourceId ? roleBySource.get(member.dataSourceId) : undefined); const os = [member.operatingSystem ?? hostFact?.operatingSystem, member.osVersion ?? hostFact?.operatingSystemVersion, member.kernelVersion ?? hostFact?.kernelVersion].filter(Boolean).join(' / '); return <TableRow key={member.memberIndex} data-state={selectedIndex === member.memberIndex ? 'selected' : undefined} onClick={() => onSelect(member.memberIndex)} className="cursor-pointer"><TableCell className="max-w-64 truncate font-mono text-forensics-text" title={member.sourcePath}>{member.memberIndex + 1}. {member.sourceName}</TableCell><TableCell className="text-forensics-muted">{member.sourceKind.toUpperCase()}</TableCell><TableCell><StatusBadge label={statusLabel(member.importState, t)} variant={statusVariant(member.importState)} /></TableCell><TableCell className="font-mono text-forensics-text">{source?.fileCount?.toLocaleString() ?? '—'}</TableCell><TableCell className="max-w-36 truncate text-forensics-text" title={member.hostname ?? hostFact?.hostname}>{member.hostname ?? hostFact?.hostname ?? '—'}</TableCell><TableCell className="max-w-48 truncate text-[10px] text-forensics-muted" title={os}>{os || '—'}</TableCell><TableCell className="max-w-36 truncate font-mono text-[10px] text-forensics-muted" title={addresses.join(', ')}>{addresses.join(', ') || '—'}</TableCell><TableCell><StatusBadge label={t(`infrastructure.workspace.topology.roles.${role ?? 'unknown'}`, { defaultValue: role ?? 'unknown' })} variant="outline" /></TableCell><TableCell><StatusBadge label={statusLabel(member.hashStatus, t)} variant={statusVariant(member.hashStatus)} /></TableCell></TableRow>; })}</TableBody>
        </Table>
      </div>
      {members.length === 0 ? <EmptyState>{t('infrastructure.workspace.members.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
