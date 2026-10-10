import { useMemo } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
import { X, CircleDot, History } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { ContextBar } from '@/components/layout/ContextBar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { KeyValueField } from '@/components/data-display';
import type { ArtifactsWorkspaceModel } from '@/features/artifacts/use-artifacts-workspace-model';
import type { ArtifactRow } from '@/types/models';

interface ArtifactsWorkspaceProps {
  model: ArtifactsWorkspaceModel;
}

/** Pure artifact presentation surface. Query, selection, and navigation behavior stay in the model. */
export function ArtifactsWorkspace({ model }: ArtifactsWorkspaceProps) {
  const { t } = useTranslation();
  const columns = useMemo<DenseColumn<ArtifactRow>[]>(
    () => [
      { key: 'title', title: t('artifacts.columns.familyPath', { family: model.selectedArtifactFamily }), className: 'w-[38%] text-forensics-muted', render: (row) => row.title },
      { key: 'summary', title: t('artifacts.columns.targetPath'), className: 'w-[34%] text-forensics-text-secondary', render: (row) => row.summary.replace(`${t('artifacts.columns.targetPath')}: `, '') },
      { key: 'createdAt', title: t('artifacts.columns.createdAt'), className: 'w-40 text-forensics-muted-light', render: (row) => row.createdAt },
      { key: 'args', title: t('artifacts.columns.arguments'), className: 'text-forensics-text-tertiary', render: (row) => String(row.attrs.arguments ?? '-') },
    ],
    [model.selectedArtifactFamily, t],
  );

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <ContextBar>
        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {model.families.map(({ family, count }) => (
            <div key={family} role="button" tabIndex={0} onClick={() => model.selectArtifactFamily(family)} className="shrink-0 whitespace-nowrap font-mono">
              {family} <span className="text-forensics-muted-lighter">{count}</span>
            </div>
          ))}
        </div>
      </ContextBar>
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col border-r border-forensics-border">
          <DenseDataTableFrame layout="fill" variant="plain">
            <DenseDataTable<ArtifactRow> rows={model.tableRows} getRowKey={(row) => row.id} selectedRowKey={model.selectedArtifact?.id} onRowClick={model.onArtifactRowClick} emptyTitle={t('artifacts.emptyTitle')} emptyDescription={t('artifacts.emptyDescription')} columns={columns} loadContextKey={model.loadContextKey} loadStateKey={model.loadStateKey} onReachEnd={model.loadNextPage} onRetryLoadMore={model.retry} hasMore={model.hasMore} loadingMore={model.loadingMore} loadMoreFailed={model.loadMoreFailed} initialLoadFailed={model.initialLoadFailed} onRetryInitialLoad={model.retry} emptyAction={model.selectedArtifactFamily ? (<Tooltip content={t('artifacts.clearFamily')}><Button aria-label={t('artifacts.clearFamily')} type="button" variant="outline" size="sm" onClick={() => model.selectArtifactFamily('')}><ActionIcon name="close" /></Button></Tooltip>) : undefined} />
          </DenseDataTableFrame>
        </div>
        <InspectorPane kind={t('inspectorKind.artifact')} title={t('artifacts.inspector.title')} subtitle={model.selectedArtifact ? `${model.selectedArtifact.artifactType} / ${model.selectedArtifact.id}` : t('artifacts.inspector.none')} widthClassName="w-80">
          <div className="space-y-5">
            <InspectorSection title={t('artifacts.inspector.targetPath')}><InspectorValue value={String(model.selectedArtifact?.attrs.targetPath ?? '-')} mono strong /></InspectorSection>
            <InspectorSection title={t('artifacts.inspector.attributes')}><div className="space-y-2 font-mono text-[10px]"><KeyValueField label={t('artifacts.fields.driveType')} value={String(model.selectedArtifact?.attrs.driveType ?? '-')} /><KeyValueField label={t('artifacts.fields.volumeSerial')} value={String(model.selectedArtifact?.attrs.volumeSerial ?? '-')} /><KeyValueField label={t('artifacts.fields.machineId')} value={String(model.selectedArtifact?.attrs.machineId ?? '-')} /></div></InspectorSection>
            <InspectorSection title={t('artifacts.inspector.context')}><InspectorValue value={model.selectedArtifact?.title ?? '-'} mono /></InspectorSection>
            <InspectorSection title={t('artifacts.inspector.actions')}><div className="space-y-2"><Tooltip content={t('artifacts.actions.openSource')}><Button aria-label={t('artifacts.actions.openSource')} type="button" variant="forensicsSurface" size="xs" onClick={() => model.openArtifactSource(model.selectedArtifact)} disabled={!model.selectedArtifact?.sourceObjectId} className="font-light"><ActionIcon name="list" /></Button></Tooltip><Tooltip content={t('artifacts.actions.openTimeline')}><Button aria-label={t('artifacts.actions.openTimeline')} type="button" variant="forensicsLink" size="xs" onClick={() => model.openArtifactTimeline(model.selectedArtifact)} disabled={!model.selectedArtifact} ><ActionIcon name="history" /></Button></Tooltip></div></InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
