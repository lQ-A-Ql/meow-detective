import { Check, CircleAlert, Minus, Waves } from 'lucide-react';
import type { TFunction } from 'i18next';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { statusLabel, statusVariant, type EvidenceStatus } from '../model/cluster-status-map';
import { StatusBadge } from '@/components/status/StatusBadge';

export function EvidenceTrustBar({ stages, t }: { stages: Array<{ key: string; status: EvidenceStatus }>; t: TFunction }) {
  return (
    <PanelFrame className="bg-forensics-surface">
      <SectionHeader icon={Waves} title={t('infrastructure.workspace.trust.title')} subtitle={t('infrastructure.workspace.trust.description')} />
      <div className="mt-4 grid gap-2 sm:grid-cols-3 xl:grid-cols-6">
        {stages.map((stage) => {
          const Icon = stage.status === 'complete' ? Check : stage.status === 'indeterminate' || stage.status === 'unknown' ? Minus : CircleAlert;
          return <div key={stage.key} className="flex min-w-0 items-center gap-2 border border-forensics-border px-3 py-2"><Icon size={14} className="shrink-0 text-forensics-muted" /><div className="min-w-0"><div className="truncate text-[11px] text-forensics-text">{t(`infrastructure.workspace.trust.stages.${stage.key}`)}</div><StatusBadge label={statusLabel(stage.status, t)} variant={statusVariant(stage.status)} /></div></div>;
        })}
      </div>
    </PanelFrame>
  );
}
