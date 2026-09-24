import { Activity, ExternalLink } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { MetricCard } from '@/components/data-display';
import type { LinuxEvidenceEvent } from '@/types/linuxCluster';

export function TimelineContextCard({ events, memberCount, onOpen, t }: { events: LinuxEvidenceEvent[]; memberCount: number; onOpen: () => void; t: TFunction }) {
  const sourceCount = new Set(events.map((event) => event.dataSourceId)).size;
  const uncertain = events.filter((event) => event.completeness !== 'complete').length;
  return <section className="border border-forensics-border bg-forensics-surface p-4"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="flex items-center gap-2"><Activity size={16} className="text-forensics-muted" /><h2 className="font-serif text-[15px] font-light text-forensics-text">{t('infrastructure.workspace.timeline.title')}</h2></div><p className="mt-1 text-[11px] text-forensics-muted">{t('infrastructure.workspace.timeline.description')}</p></div><Button type="button" variant="forensicsOutline" size="sm" onClick={onOpen}>{t('infrastructure.workspace.timeline.open')}<ExternalLink size={13} /></Button></div><div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-4"><MetricCard label={t('infrastructure.workspace.timeline.events')} value={events.length} size="sm" /><MetricCard label={t('infrastructure.workspace.timeline.members')} value={sourceCount || memberCount} size="sm" /><MetricCard label={t('infrastructure.workspace.timeline.uncertain')} value={uncertain} size="sm" /><MetricCard label={t('infrastructure.workspace.timeline.sequence')} value={t('infrastructure.workspace.timeline.native')} size="sm" /></div></section>;
}
