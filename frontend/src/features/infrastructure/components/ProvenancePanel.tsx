import { ShieldCheck } from 'lucide-react';
import type { TFunction } from 'i18next';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxEvidenceSetSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant, trustStages } from '../model/cluster-status-map';

export function ProvenancePanel({ summary, provenance, t }: { summary: LinuxEvidenceSetSummary; provenance: string; t: TFunction }) {
  const stages = trustStages(summary);
  return <PanelFrame className="bg-forensics-surface"><SectionHeader icon={ShieldCheck} title={t('infrastructure.workspace.provenance.title')} subtitle={t('infrastructure.workspace.provenance.description')} /><div className="mt-4 grid gap-2 md:grid-cols-2 xl:grid-cols-3">{stages.map((stage) => <div key={stage.key} className="border border-forensics-border p-3"><div className="flex items-center justify-between gap-2"><span className="text-xs text-forensics-text">{t(`infrastructure.workspace.trust.stages.${stage.key}`)}</span><StatusBadge label={statusLabel(stage.status, t)} variant={statusVariant(stage.status)} /></div><p className="mt-2 text-[11px] leading-5 text-forensics-muted">{t(`infrastructure.workspace.provenance.details.${stage.key}`)}</p></div>)}</div><div className="mt-4 border-t border-forensics-border pt-3 text-xs text-forensics-muted"><span>{t('infrastructure.workspace.provenance.reportState')}: </span><StatusBadge label={statusLabel(provenance, t)} variant={statusVariant(provenance)} /></div>{summary.diagnostics.length ? <div className="mt-3 space-y-1 text-[11px] text-forensics-muted">{summary.diagnostics.map((diagnostic) => <div key={diagnostic}>· {diagnostic}</div>)}</div> : null}</PanelFrame>;
}
