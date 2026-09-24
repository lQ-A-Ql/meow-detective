import { X } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { KeyValueField, PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetMemberSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function EvidenceInspector({ member, onClose, t }: { member?: LinuxEvidenceSetMemberSummary; onClose: () => void; t: TFunction }) {
  if (!member) return null;
  return <aside className="w-full shrink-0 xl:w-80"><PanelFrame className="bg-forensics-surface"><div className="flex items-start justify-between gap-3"><SectionHeader title={t('infrastructure.workspace.inspector.title')} subtitle={member.sourceName} /><Button type="button" variant="forensicsGhost" size="iconXs" aria-label={t('infrastructure.workspace.inspector.close')} onClick={onClose}><X size={14} /></Button></div><div className="mt-4 space-y-3"><KeyValueField layout="stacked" label={t('infrastructure.workspace.inspector.source')} value={member.sourcePath} /><KeyValueField layout="stacked" label={t('infrastructure.workspace.inspector.kind')} value={member.sourceKind.toUpperCase()} /><div className="grid grid-cols-2 gap-2"><div><div className="text-[10px] text-forensics-muted">{t('infrastructure.workspace.members.import')}</div><StatusBadge label={statusLabel(member.importState, t)} variant={statusVariant(member.importState)} /></div><div><div className="text-[10px] text-forensics-muted">{t('infrastructure.workspace.members.hash')}</div><StatusBadge label={statusLabel(member.hashStatus, t)} variant={statusVariant(member.hashStatus)} /></div></div><KeyValueField layout="stacked" label={t('infrastructure.workspace.inspector.provenance')} value={member.provenanceStatus} /></div></PanelFrame></aside>;
}
