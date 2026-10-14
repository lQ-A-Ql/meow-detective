
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { KeyValueField } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { kindLabel, stateLabel } from '../logic/labels';
import { sourceId } from '../logic/cluster-presentation';
import type { infrastructureInspectorModel } from '../logic/inspector-model';

export function InfrastructureInspector({
  node,
  onOpenFiles,
  onClose,
  t,
  presentation,
  onCopyId,
}: {
  node: InfrastructureGraphNode;
  edges: InfrastructureGraphEdge[];
  onOpenFiles: () => void;
  onClose: () => void;
  t: TFunction;
  presentation: ReturnType<typeof infrastructureInspectorModel>;
  onCopyId: () => void;
}) {
  const nodeSourceId = sourceId(node);
  return (
    <aside className="shrink-0 border-t border-forensics-line bg-forensics-surface px-5 py-4 lg:px-7">
      <div className="mx-auto flex max-w-[1500px] flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="text-sm text-forensics-text">{node.name}</span><StatusBadge label={stateLabel(node.status, t)} variant={presentation.statusVariant} /></div><div className="mt-1 text-xs text-forensics-muted">{kindLabel(node.kind, t)} · {t('infrastructure.inspector.linkedRelations', { count: presentation.linkedCount })}</div><div className="mt-3 grid gap-2 text-xs sm:grid-cols-4"><KeyValueField layout="inline" label={t('infrastructure.columns.confidence')} value={stateLabel(node.confidence, t)} /><KeyValueField layout="inline" label={t('infrastructure.inspector.source')} value={nodeSourceId ?? t('infrastructure.values.unavailable')} /><KeyValueField layout="inline" label={t('infrastructure.inspector.parser')} value={presentation.parser ? presentation.parser : t('infrastructure.values.unavailable')} /><KeyValueField layout="inline" label={t('infrastructure.clusterInfo.version')} value={node.version ?? t('infrastructure.values.unavailable')} /></div></div>
        <div className="flex shrink-0 items-center gap-2"><Tooltip content={t('infrastructure.actions.copyId')}><Button aria-label={t('infrastructure.actions.copyId')} size="xs" variant="forensicsOutline" onClick={onCopyId} title={t('infrastructure.actions.copyId')}><ActionIcon name="copy" /></Button></Tooltip>{nodeSourceId ? <Tooltip content={t('infrastructure.actions.openFiles')}><Button aria-label={t('infrastructure.actions.openFiles')} size="xs" variant="forensicsOutline" onClick={onOpenFiles}><ActionIcon name="share" /></Button></Tooltip> : null}<Tooltip content={t('infrastructure.actions.close')}><Button aria-label={t('infrastructure.actions.close')} size="xs" variant="forensicsGhost" onClick={onClose}><ActionIcon name="close" /></Button></Tooltip></div>
      </div>
    </aside>
  );
}
