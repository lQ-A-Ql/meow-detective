import type {
  CorrelationCluster,
} from '@/types/models';
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
  const primaryJump =
    cluster.primaryFileId
      ? { route: '/files', targetId: cluster.primaryFileId, label: '查看主文件' }
      : undefined;

  return (
    <div className="rounded border border-[#e0e0e0] bg-[#fcfcfc] p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-semibold text-[#111]">{cluster.title}</div>
          <div className="mt-1 text-[11px] text-[#555]">{cluster.summary}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(cluster.confidence)}`}>
          {confidenceLabel(cluster.confidence)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="Artifact" value={cluster.artifactCount.toString()} />
        <Metric label="Timeline" value={cluster.timelineCount.toString()} />
        <Metric label="节点" value={cluster.nodeIds.length.toString()} />
        <Metric label="边" value={cluster.edgeIds.length.toString()} />
      </div>
      <FamilyPills families={cluster.families} testId={`cluster-families-${cluster.id}`} />
      {cluster.edgeIds.length > 0 ? (
        <div className="mt-3 text-[11px] text-[#555]">Edge IDs: {cluster.edgeIds.join(', ')}</div>
      ) : null}
      {primaryJump ? (
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => onJump(primaryJump.route, primaryJump.targetId)}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
          >
            {primaryJump.label}
          </button>
        </div>
      ) : null}
    </div>
  );
}
