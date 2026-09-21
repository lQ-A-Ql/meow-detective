import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { formatSize } from '@/features/analysis/components/panels/helpers';
import { InfoCard, TableBlock } from '@/features/analysis/components/panels/helpers';
import type { KubernetesClusterArtifact, KubernetesClusterNode, KubernetesClusterSummary } from '@/types/models';

export function KubernetesClusterPanel({ summary }: { summary?: KubernetesClusterSummary }) {
  const { t } = useTranslation();
  const nodeColumns = useMemo<DenseColumn<KubernetesClusterNode>[]>(() => [
    { key: 'sourceName', title: t('analysis.kubernetes.columns.source'), render: (row) => row.sourceName, text: (row) => row.sourceName },
    { key: 'nodeName', title: t('analysis.kubernetes.columns.node'), render: (row) => row.nodeName ?? t('analysis.kubernetes.values.unproven'), text: (row) => row.nodeName ?? '' },
    { key: 'role', title: t('analysis.kubernetes.columns.role'), render: (row) => row.controlPlane ? t('analysis.kubernetes.roles.controlPlane') : t('analysis.kubernetes.roles.node'), text: (row) => row.controlPlane ? 'control-plane' : 'node' },
    { key: 'artifactCount', title: t('analysis.kubernetes.columns.artifacts'), render: (row) => row.artifactCount.toString(), text: (row) => row.artifactCount.toString() },
    { key: 'status', title: t('analysis.kubernetes.columns.status'), render: (row) => <Badge variant={row.status === 'candidateFound' ? 'default' : 'outline'}>{t(`analysis.kubernetes.status.${row.status}`)}</Badge>, text: (row) => row.status },
  ], [t]);
  const artifactColumns = useMemo<DenseColumn<KubernetesClusterArtifact>[]>(() => [
    { key: 'kind', title: t('analysis.kubernetes.columns.kind'), render: (row) => row.kind, text: (row) => row.kind },
    { key: 'path', title: t('analysis.kubernetes.columns.path'), className: 'min-w-[360px]', render: (row) => row.path, text: (row) => row.path },
    { key: 'size', title: t('analysis.kubernetes.columns.size'), render: (row) => formatSize(row.size), text: (row) => row.size.toString() },
    { key: 'status', title: t('analysis.kubernetes.columns.status'), render: (row) => <Badge variant="outline">{t(`analysis.kubernetes.status.${row.status}`)}</Badge>, text: (row) => row.status },
  ], [t]);

  if (!summary || summary.status === 'notFound') {
    return <div className="border border-forensics-border p-6 text-sm text-forensics-muted">{t('analysis.kubernetes.empty')}</div>;
  }

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <InfoCard label={t('analysis.kubernetes.cards.cluster')} value={summary.clusterName ?? t('analysis.kubernetes.values.unproven')} />
        <InfoCard label={t('analysis.kubernetes.cards.members')} value={`${summary.readyMemberCount}/${summary.expectedMemberCount}`} />
        <InfoCard label={t('analysis.kubernetes.cards.controlPlane')} value={summary.controlPlaneMemberCount.toString()} />
        <InfoCard label={t('analysis.kubernetes.cards.artifacts')} value={summary.artifactCount.toString()} />
      </div>
      {summary.diagnostics.length > 0 ? (
        <div className="border border-forensics-border bg-forensics-panel px-3 py-2 text-xs text-forensics-muted">
          {summary.diagnostics.map((diagnostic) => <div key={diagnostic}>{diagnostic}</div>)}
        </div>
      ) : null}
      <TableBlock title={t('analysis.kubernetes.nodesTitle')}>
        <DenseDataTableFrame rowCount={summary.nodes.length}>
          <DenseDataTable rows={summary.nodes} columns={nodeColumns} getRowKey={(row) => row.dataSourceId} emptyTitle={t('analysis.kubernetes.nodesTitle')} emptyDescription={t('analysis.kubernetes.empty')} />
        </DenseDataTableFrame>
      </TableBlock>
      <TableBlock title={t('analysis.kubernetes.artifactsTitle')}>
        <DenseDataTableFrame rowCount={summary.artifacts.length}>
          <DenseDataTable rows={summary.artifacts} columns={artifactColumns} getRowKey={(row) => row.fileId} emptyTitle={t('analysis.kubernetes.artifactsTitle')} emptyDescription={t('analysis.kubernetes.empty')} filterable />
        </DenseDataTableFrame>
      </TableBlock>
    </div>
  );
}
