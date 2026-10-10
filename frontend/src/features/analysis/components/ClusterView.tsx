import { useTranslation } from 'react-i18next';
import type {
  CorrelationCluster,
} from '@/types/models';
import { Button } from '@/app/components/ui/button';
import {
  confidenceLabel,
  confidenceTone,
  FamilyPills,
  Metric,
} from './correlation-helpers';

export function ClusterCard({
  cluster,
  onJump,
}: {
  cluster: CorrelationCluster;
  onJump: (route: string, targetId: string) => void;
}) {
  const { t } = useTranslation();
  const primaryJump =
    cluster.primaryFileId
      ? { route: '/files', targetId: cluster.primaryFileId, label: t('correlation.cluster.viewPrimaryFile') }
      : undefined;

  return (
    <div className="rounded-none bg-transparent p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-light text-forensics-text">{cluster.title}</div>
          <div className="mt-1 text-[11px] text-forensics-text-tertiary">{cluster.summary}</div>
        </div>
        <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(cluster.confidence)}`}>
          {confidenceLabel(cluster.confidence)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="Artifact" value={cluster.artifactCount.toString()} />
        <Metric label="Timeline" value={cluster.timelineCount.toString()} />
        <Metric label={t('graph.stats.nodes')} value={cluster.nodeIds.length.toString()} />
        <Metric label={t('graph.stats.edges')} value={cluster.edgeIds.length.toString()} />
      </div>
      <FamilyPills families={cluster.families} testId={`cluster-families-${cluster.id}`} />
      {cluster.edgeIds.length > 0 ? (
        <div className="mt-3 text-[11px] text-forensics-text-tertiary">Edge IDs: {cluster.edgeIds.join(', ')}</div>
      ) : null}
      {primaryJump ? (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            type="button"
            variant="forensicsOutline"
            size="compact"
            onClick={() => onJump(primaryJump.route, primaryJump.targetId)}
            className="text-[10px]"
          >
            {primaryJump.label}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
