import { Copy, ExternalLink } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { KeyValueField } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { kindLabel, stateLabel } from '../logic/labels';
import { sourceId } from '../logic/cluster-presentation';

export function InfrastructureInspector({
  node,
  edges,
  onOpenFiles,
  onClose,
  t,
}: {
  node: InfrastructureGraphNode;
  edges: InfrastructureGraphEdge[];
  onOpenFiles: () => void;
  onClose: () => void;
  t: TFunction;
}) {
  const provenance = parseJson(node.provenanceJson);
  const linked = edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id);
  const nodeSourceId = sourceId(node);
  return (
    <aside className="shrink-0 border-t border-forensics-border bg-forensics-surface px-5 py-4 lg:px-7">
      <div className="mx-auto flex max-w-[1500px] flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="text-sm text-forensics-text">{node.name}</span><StatusBadge label={stateLabel(node.status, t)} variant={statusVariant(node.status)} /></div><div className="mt-1 text-xs text-forensics-muted">{kindLabel(node.kind, t)} · {t('infrastructure.inspector.linkedRelations', { count: linked.length })}</div><div className="mt-3 grid gap-2 text-xs sm:grid-cols-4"><KeyValueField layout="inline" label={t('infrastructure.columns.confidence')} value={stateLabel(node.confidence, t)} /><KeyValueField layout="inline" label={t('infrastructure.inspector.source')} value={nodeSourceId ?? t('infrastructure.values.unavailable')} /><KeyValueField layout="inline" label={t('infrastructure.inspector.parser')} value={typeof provenance.parser === 'string' ? provenance.parser : t('infrastructure.values.unavailable')} /><KeyValueField layout="inline" label={t('infrastructure.clusterInfo.version')} value={node.version ?? t('infrastructure.values.unavailable')} /></div></div>
        <div className="flex shrink-0 items-center gap-2"><Button size="xs" variant="forensicsOutline" onClick={() => void navigator.clipboard?.writeText(node.id)} title={t('infrastructure.actions.copyId')}><Copy size={12} />{t('infrastructure.actions.copyId')}</Button>{nodeSourceId ? <Button size="xs" variant="forensicsOutline" onClick={onOpenFiles}><ExternalLink size={12} />{t('infrastructure.actions.openFiles')}</Button> : null}<Button size="xs" variant="forensicsGhost" onClick={onClose}>{t('infrastructure.actions.close')}</Button></div>
      </div>
    </aside>
  );
}

function statusVariant(value: string) { return value === 'ready' || value === 'complete' || value === 'verified' ? 'default' : 'outline'; }

function parseJson(value: string) { try { return JSON.parse(value) as Record<string, unknown>; } catch { return {}; } }
