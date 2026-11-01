import type { TFunction } from 'i18next';
import { Database, ShieldAlert } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from '@/app/components/ui/button';
import { PanelFrame } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import { formatBytes } from '@/lib/format-bytes';
import type { LinuxMdRaidArray } from '@/types/linuxCluster';

export function MdRaidEvidencePanel({ arrays, t, onImport, onOpenFiles }: { arrays: LinuxMdRaidArray[]; t: TFunction; onImport?: () => void; onOpenFiles?: () => void }) {
  if (arrays.length === 0) {
    return <PanelFrame className="bg-forensics-surface p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-sm text-forensics-text">{t('infrastructure.workspace.mdraid.empty')}</div><p className="mt-2 text-xs leading-5 text-forensics-muted">{t('infrastructure.workspace.mdraid.emptyDescription')}</p></div><ImportButton t={t} onImport={onImport} /></div></PanelFrame>;
  }
  return <div className="space-y-3"><div className="flex justify-end gap-2"><BrowseButton t={t} onOpenFiles={onOpenFiles} /><ImportButton t={t} onImport={onImport} /></div>{arrays.map((array) => <PanelFrame key={array.arrayUuid} className="bg-forensics-surface p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="flex items-center gap-2 text-sm text-forensics-text"><Database size={15} />{array.name}</div><div className="mt-1 break-all font-mono text-[10px] text-forensics-muted">{array.arrayUuid}</div></div><StatusBadge label={array.capacityMismatch ? t('infrastructure.workspace.mdraid.capacityMismatch') : t('infrastructure.workspace.mdraid.consistent')} variant={array.capacityMismatch ? 'secondary' : 'default'} /></div><div className="mt-4 grid gap-2 text-xs sm:grid-cols-4"><Metric label={t('infrastructure.workspace.mdraid.level')} value={`RAID${array.level}`} /><Metric label={t('infrastructure.workspace.mdraid.chunk')} value={formatBytes(array.chunkSectors * 512)} /><Metric label={t('infrastructure.workspace.mdraid.members')} value={`${array.members.length}/${array.raidDisks}`} /><Metric label={t('infrastructure.workspace.mdraid.filesystem')} value={t(`infrastructure.workspace.mdraid.filesystemStatus.${array.filesystem}`, { defaultValue: array.filesystem })} /></div><div className="mt-4 space-y-2">{array.members.map((member) => <div key={member.devNumber} className="grid gap-1 border border-forensics-border p-3 text-xs sm:grid-cols-[80px_1fr_1fr_1fr]"><span className="text-forensics-muted">{t('infrastructure.workspace.mdraid.device')} {member.devNumber}</span><span className="text-forensics-text">{member.vgName}/{member.lvName}</span><span className="text-forensics-muted">{t('infrastructure.workspace.mdraid.dataSize')}: {formatBytes(member.dataSizeSectors * 512)}</span><span className="font-mono text-[10px] text-forensics-muted">{t('infrastructure.workspace.mdraid.dataOffset')}: {member.dataOffsetSectors}</span></div>)}</div>{array.capacityMismatch ? <div className="mt-3 flex items-start gap-2 text-[11px] leading-5 text-forensics-muted"><ShieldAlert size={14} className="mt-0.5 shrink-0" />{t('infrastructure.workspace.mdraid.mismatchDescription')}</div> : null}<div className="mt-3 text-[11px] text-forensics-muted">{t('infrastructure.workspace.mdraid.readOnlyDescription')}</div></PanelFrame>)}</div>;
}

function BrowseButton({ t, onOpenFiles }: { t: TFunction; onOpenFiles?: () => void }) {
  if (!onOpenFiles) return null;
  return <Button type="button" shape="row" className="h-8 w-auto justify-center rounded-ctl px-3" variant="forensicsOutline" size="sm" onClick={onOpenFiles}><ActionIcon name="folder-open" /><span>{t('infrastructure.workspace.mdraid.openFiles')}</span></Button>;
}

function ImportButton({ t, onImport }: { t: TFunction; onImport?: () => void }) {
  if (!onImport) return null;
  return <Button type="button" shape="row" className="h-8 w-auto justify-center rounded-ctl px-3" variant="forensicsOutline" size="sm" onClick={onImport}><ActionIcon name="upload" /><span>{t('infrastructure.workspace.mdraid.import')}</span></Button>;
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="border border-forensics-border p-2"><div className="text-[10px] text-forensics-muted">{label}</div><div className="mt-1 text-forensics-text">{value}</div></div>;
}
