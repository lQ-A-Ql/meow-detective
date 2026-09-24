import { Activity, ChevronRight, Database, FileCheck2, ShieldCheck } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { MetricCard } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetListItem, LinuxEvidenceSetSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function ClusterEvidenceHeader({
  caseName,
  sets,
  selectedSetId,
  summary,
  provenance,
  onSelectSet,
  onOpenTimeline,
  eventCount,
  t,
}: {
  caseName: string;
  sets: LinuxEvidenceSetListItem[];
  selectedSetId?: string;
  summary?: LinuxEvidenceSetSummary;
  provenance: string;
  onSelectSet: (id: string) => void;
  onOpenTimeline: () => void;
  eventCount: number;
  t: TFunction;
}) {
  return (
    <header className="shrink-0 border-b border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[10px] uppercase tracking-[0.16em] text-forensics-muted">{t('infrastructure.workspace.eyebrow')}</div>
          <h1 className="mt-1 text-xl font-light text-forensics-text">{t('infrastructure.workspace.title')}</h1>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-forensics-muted">{t('infrastructure.workspace.subtitle')}</p>
        </div>
        <div className="flex max-w-full flex-wrap items-end justify-end gap-2">
          <label className="grid gap-1 text-left text-[10px] uppercase tracking-wide text-forensics-muted">
            <span>{t('infrastructure.workspace.evidenceSet')}</span>
            <select
              aria-label={t('infrastructure.workspace.evidenceSet')}
              value={selectedSetId ?? ''}
              onChange={(event) => onSelectSet(event.target.value)}
              className="h-8 min-w-56 border border-forensics-border-strong bg-forensics-panel px-2 text-xs text-forensics-text outline-none focus:border-forensics-primary-blue"
            >
              {sets.map((item) => <option key={item.importSetId} value={item.importSetId}>{item.name} · {item.state}</option>)}
            </select>
          </label>
          <Button type="button" variant="forensicsOutline" size="sm" onClick={onOpenTimeline}><Activity size={14} />{t('infrastructure.workspace.openTimeline')}</Button>
        </div>
      </div>
      <div className="mt-5 grid grid-cols-2 border-y border-forensics-border sm:grid-cols-4 xl:grid-cols-7">
        <MetricCard icon={Database} label={t('infrastructure.workspace.metrics.members')} value={summary?.memberCount ?? 0} size="sm" className="border-0 border-r" />
        <MetricCard icon={FileCheck2} label={t('infrastructure.workspace.metrics.ready')} value={summary?.readyCount ?? 0} size="sm" className="border-0 border-r" />
        <MetricCard icon={ShieldCheck} label={t('infrastructure.workspace.metrics.provenance')} value={<StatusBadge label={statusLabel(provenance, t)} variant={statusVariant(provenance)} />} size="sm" className="border-0 border-r" />
        <MetricCard label={t('infrastructure.workspace.metrics.report')} value={provenance === 'complete' ? t('infrastructure.workspace.reportReady') : t('infrastructure.workspace.reportPartial')} size="sm" className="border-0 border-r" />
        <MetricCard label={t('infrastructure.workspace.metrics.scopes')} value={summary?.scopes.length ?? 0} size="sm" className="border-0 border-r" />
        <MetricCard label={t('infrastructure.workspace.metrics.derived')} value={summary?.derivedSources.length ?? 0} size="sm" className="border-0 border-r" />
        <MetricCard label={t('infrastructure.workspace.metrics.events')} value={eventCount} size="sm" className="border-0" />
      </div>
      {summary?.diagnostics.length ? <div className="mt-3 flex items-start gap-2 text-xs text-forensics-muted"><ChevronRight size={14} className="mt-0.5 shrink-0" /><span>{summary.diagnostics[0]}</span></div> : null}
      {summary?.manifestDigest ? <div className="mt-2 text-[10px] text-forensics-muted">{t('infrastructure.workspace.manifest')}: <span className="font-mono text-forensics-text-secondary">schema {summary.manifestSchemaVersion ?? '?'} · {summary.manifestDigest.slice(0, 16)}…</span></div> : null}
      <div className="mt-2 text-right text-[10px] text-forensics-muted">{t('infrastructure.workspace.caseLabel')}: <span className="font-mono text-forensics-text-secondary">{caseName}</span></div>
    </header>
  );
}
