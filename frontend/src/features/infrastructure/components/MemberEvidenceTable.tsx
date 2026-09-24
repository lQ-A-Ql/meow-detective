import { HardDrive } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetMemberSummary } from '@/types/linuxCluster';
import type { DataSourceSummary } from '@/types/dataSource';
import { formatBytes } from '@/lib/format-bytes';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function MemberEvidenceTable({ members, sourceMetadata, selectedIndex, onSelect, t }: { members: LinuxEvidenceSetMemberSummary[]; sourceMetadata: Map<string, DataSourceSummary>; selectedIndex?: number; onSelect: (index: number) => void; t: TFunction }) {
  return (
    <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={HardDrive} title={t('infrastructure.workspace.members.title')} subtitle={t('infrastructure.workspace.members.description')} className="px-4 py-3" />
      <div className="overflow-x-auto">
        <Table className="min-w-[1060px] text-left text-xs">
          <TableHeader><TableRow><TableHead>{t('infrastructure.workspace.members.member')}</TableHead><TableHead>{t('infrastructure.workspace.members.kind')}</TableHead><TableHead>{t('infrastructure.workspace.members.import')}</TableHead><TableHead>{t('infrastructure.workspace.members.files')}</TableHead><TableHead>{t('infrastructure.workspace.members.size')}</TableHead><TableHead>{t('infrastructure.workspace.members.hash')}</TableHead><TableHead>{t('infrastructure.workspace.members.provenance')}</TableHead><TableHead>{t('infrastructure.workspace.members.source')}</TableHead></TableRow></TableHeader>
          <TableBody>{members.map((member) => { const source = member.dataSourceId ? sourceMetadata.get(member.dataSourceId) : undefined; return <TableRow key={member.memberIndex} data-state={selectedIndex === member.memberIndex ? 'selected' : undefined} onClick={() => onSelect(member.memberIndex)} className="cursor-pointer"><TableCell className="max-w-72 truncate font-mono text-forensics-text" title={member.sourcePath}>{member.memberIndex + 1}. {member.sourceName}</TableCell><TableCell className="text-forensics-muted">{member.sourceKind.toUpperCase()}</TableCell><TableCell><StatusBadge label={statusLabel(member.importState, t)} variant={statusVariant(member.importState)} /></TableCell><TableCell className="font-mono text-forensics-text">{source?.fileCount?.toLocaleString() ?? '—'}</TableCell><TableCell className="font-mono text-forensics-muted">{source?.evidenceSize === undefined ? '—' : formatBytes(source.evidenceSize)}</TableCell><TableCell><StatusBadge label={statusLabel(member.hashStatus, t)} variant={statusVariant(member.hashStatus)} /></TableCell><TableCell><StatusBadge label={statusLabel(member.provenanceStatus, t)} variant={statusVariant(member.provenanceStatus)} /></TableCell><TableCell className="max-w-80 truncate font-mono text-[10px] text-forensics-muted" title={member.sourcePath}>{member.sourcePath}</TableCell></TableRow>; })}</TableBody>
        </Table>
      </div>
      {members.length === 0 ? <EmptyState>{t('infrastructure.workspace.members.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
