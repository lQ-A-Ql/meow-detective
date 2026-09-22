import { Boxes, Database, GitBranch, Network, Server } from 'lucide-react';
import type { TFunction } from 'i18next';
import { MetricCard } from '@/components/data-display';
import type { InfrastructureGraphNode } from '@/types/models';

export function InfrastructureHeader({
  caseName,
  nodes,
  relationCount,
  networkLinkCount,
  t,
}: {
  caseName: string;
  nodes: InfrastructureGraphNode[];
  relationCount: number;
  networkLinkCount: number;
  t: TFunction;
}) {
  const hosts = nodes.filter((node) => node.kind === 'physical_host').length;
  const storage = nodes.filter((node) => node.domain === 'storage').length;
  const workloads = nodes.filter((node) => node.domain === 'analysis').length;
  const infrastructure = nodes.filter((node) => ['pve', 'kubernetes', 'ceph', 'ceph_cluster'].includes(node.kind)).length;
  return (
    <header className="shrink-0 border-b border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[10px] uppercase tracking-[0.16em] text-forensics-muted">{t('infrastructure.eyebrow')}</div>
          <h1 className="mt-1 text-xl font-light text-forensics-text">{t('infrastructure.title')}</h1>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-forensics-muted">{t('infrastructure.subtitle')}</p>
        </div>
        <div className="max-w-full border-l border-forensics-border pl-3 text-right text-xs text-forensics-text">
          <div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.caseLabel')}</div>
          <div className="mt-1 max-w-64 truncate">{caseName}</div>
        </div>
      </div>
      <div className="mt-5 grid grid-cols-2 border-y border-forensics-border sm:grid-cols-3 xl:grid-cols-6">
        <MetricCard icon={Server} label={t('infrastructure.metrics.hosts')} value={hosts} size="sm" className="border-0 border-r" />
        <MetricCard icon={Boxes} label={t('infrastructure.metrics.infrastructure')} value={infrastructure} size="sm" className="border-0 border-r" />
        <MetricCard icon={Database} label={t('infrastructure.metrics.storage')} value={storage} size="sm" className="border-0 border-r" />
        <MetricCard icon={Boxes} label={t('infrastructure.metrics.workloads')} value={workloads} size="sm" className="border-0 border-r" />
        <MetricCard icon={Network} label={t('infrastructure.metrics.relations')} value={relationCount} size="sm" className="border-0 border-r" />
        <MetricCard icon={GitBranch} label={t('infrastructure.metrics.hostLinks')} value={networkLinkCount} size="sm" className="border-0" />
      </div>
    </header>
  );
}
