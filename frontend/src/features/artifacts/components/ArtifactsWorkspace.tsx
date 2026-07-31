import { useMemo } from 'react';
import { Button } from '@/app/components/ui/button';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { ArtifactField } from '@/features/artifacts/components/ArtifactField';
import type { ArtifactsWorkspaceModel } from '@/features/artifacts/use-artifacts-workspace-model';
import type { ArtifactRow } from '@/types/models';

interface ArtifactsWorkspaceProps {
  model: ArtifactsWorkspaceModel;
}

/** Pure artifact presentation surface. Query, selection, and navigation behavior stay in the model. */
export function ArtifactsWorkspace({ model }: ArtifactsWorkspaceProps) {
  const columns = useMemo<DenseColumn<ArtifactRow>[]>(
    () => [
      { key: 'title', title: `${model.selectedArtifactFamily} 路径`, className: 'w-[38%] text-forensics-muted', render: (row) => row.title },
      { key: 'summary', title: '目标路径', className: 'w-[34%] text-forensics-text-secondary', render: (row) => row.summary.replace('目标路径: ', '') },
      { key: 'createdAt', title: '创建时间', className: 'w-40 text-forensics-muted-light', render: (row) => row.createdAt },
      { key: 'args', title: '参数', className: 'text-forensics-text-tertiary', render: (row) => String(row.attrs.arguments ?? '-') },
    ],
    [model.selectedArtifactFamily],
  );

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title="痕迹家族控制" meta={`Family ${model.selectedArtifactFamily} / 记录 ${model.tableRows.length}/${model.totalRows} 条 / 来源范围：Windows 用户活动 / 壳对象`}>
        <div className="flex h-10 shrink-0 items-center gap-1 overflow-x-auto px-2">
          {model.families.map(({ family, count }) => (
            <Button type="button" key={family} variant={model.selectedArtifactFamily === family ? 'forensicsSurface' : 'forensicsGhost'} size="xs" onClick={() => model.selectArtifactFamily(family)} className="shrink-0 whitespace-nowrap font-mono">
              {family} <span className="text-forensics-muted-lighter">{count}</span>
            </Button>
          ))}
        </div>
      </PageSubbar>
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col border-r border-forensics-border">
          <DenseDataTableFrame layout="fill" variant="plain">
            <DenseDataTable<ArtifactRow> rows={model.tableRows} getRowKey={(row) => row.id} selectedRowKey={model.selectedArtifact?.id} onRowClick={model.onArtifactRowClick} emptyTitle="当前痕迹家族无记录" emptyDescription="请切换 family 或等待解析任务完成。" columns={columns} loadContextKey={model.loadContextKey} loadStateKey={model.loadStateKey} onReachEnd={model.loadNextPage} onRetryLoadMore={model.retry} hasMore={model.hasMore} loadingMore={model.loadingMore} loadMoreFailed={model.loadMoreFailed} initialLoadFailed={model.initialLoadFailed} onRetryInitialLoad={model.retry} />
          </DenseDataTableFrame>
        </div>
        <InspectorPane title="痕迹属性" subtitle={model.selectedArtifact ? `${model.selectedArtifact.artifactType} / ${model.selectedArtifact.id}` : '未选择痕迹'} widthClassName="w-80">
          <div className="space-y-5">
            <InspectorSection title="目标路径"><InspectorValue value={String(model.selectedArtifact?.attrs.targetPath ?? '-')} mono strong /></InspectorSection>
            <InspectorSection title="属性字段"><div className="space-y-2 font-mono text-[10px]"><ArtifactField label="驱动器类型" value={String(model.selectedArtifact?.attrs.driveType ?? '-')} /><ArtifactField label="卷序列号" value={String(model.selectedArtifact?.attrs.volumeSerial ?? '-')} /><ArtifactField label="机器 ID" value={String(model.selectedArtifact?.attrs.machineId ?? '-')} /></div></InspectorSection>
            <InspectorSection title="来源上下文"><InspectorValue value={model.selectedArtifact?.title ?? '-'} mono /></InspectorSection>
            <InspectorSection title="关联动作"><div className="space-y-2"><Button type="button" variant="forensicsSurface" size="xs" onClick={() => model.openArtifactSource(model.selectedArtifact)} disabled={!model.selectedArtifact?.sourceObjectId} className="w-full font-light">在文件浏览中定位目标</Button><Button type="button" variant="forensicsLink" size="xs" onClick={() => model.openArtifactTimeline(model.selectedArtifact)} disabled={!model.selectedArtifact} className="w-full">查看关联时间线事件</Button></div></InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
