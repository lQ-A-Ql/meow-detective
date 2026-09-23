import { Boxes } from 'lucide-react';
import type { TFunction } from 'i18next';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import { kindLabel } from '../logic/labels';

export function KubernetesEvidencePanel({ facts, t }: { facts: InfrastructureNetworkFact[]; t: TFunction }) {
  const evidence = facts.filter((fact) => fact.factKind.startsWith('kubernetes_') || fact.factKind === 'cni_network');
  const groups = [...new Set(evidence.map((fact) => fact.factKind))].sort();
  return (
    <PanelFrame className="bg-forensics-surface">
      <SectionHeader icon={Boxes} title={t('infrastructure.kubernetesEvidence.title')} subtitle={t('infrastructure.kubernetesEvidence.description')} />
      <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {groups.map((kind) => {
          const items = evidence.filter((fact) => fact.factKind === kind);
          return <div key={kind} className="border border-forensics-border bg-forensics-panel px-3 py-3"><div className="flex items-center justify-between gap-3 text-xs text-forensics-text"><span>{t(`infrastructure.networkFacts.kinds.${kind}`, { defaultValue: kindLabel(kind, t) })}</span><span className="font-mono text-[10px] text-forensics-muted">{items.length}</span></div><div className="mt-3 space-y-2">{items.slice(0, 8).map((fact) => <div key={fact.id} className="border-l-2 border-forensics-primary-blue/50 pl-2 text-[11px]"><div className="truncate text-forensics-text" title={fact.subject}>{fact.subject}</div><div className="truncate font-mono text-forensics-text-secondary" title={fact.value}>{fact.value}</div><div className="truncate text-[10px] text-forensics-muted" title={fact.sourcePath}>{fact.sourcePath}</div></div>)}</div></div>;
        })}
      </div>
      {groups.length === 0 ? <EmptyState className="mt-4">{t('infrastructure.kubernetesEvidence.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
