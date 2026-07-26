import { useEffect, useMemo } from 'react';
import { Button } from '@/app/components/ui/button';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import {
  useArtifactById,
  useArtifactFamilies,
  useArtifactFamilyCounts,
  useInfiniteArtifactRows,
} from '@/features/artifacts/hooks';
import { ArtifactField } from '@/features/artifacts/components/ArtifactField';
import { useArtifactsSelectionModel } from '@/features/artifacts/use-artifacts-page-model';
import { ArtifactRow } from '@/types/models';

export function Artifacts() {
  const {
    openArtifactSource,
    openArtifactTimeline,
    selectedArtifactFamily,
    selectedArtifactId,
    setSelectedArtifactFamily,
    setSelectedArtifactId,
  } = useArtifactsSelectionModel();

  const { data: families } = useArtifactFamilies();
  const { data: familyCounts } = useArtifactFamilyCounts();
  const artifactRowsQuery = useInfiniteArtifactRows(selectedArtifactFamily);
  const rows = useMemo(
    () => artifactRowsQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [artifactRowsQuery.data],
  );
  const totalRows = artifactRowsQuery.data?.pages[0]?.total ?? 0;
  const selectedArtifactQuery = useArtifactById(selectedArtifactId);

  useEffect(() => {
    if (
      selectedArtifactQuery.data?.artifactType &&
      selectedArtifactQuery.data.artifactType !== selectedArtifactFamily
    ) {
      setSelectedArtifactFamily(selectedArtifactQuery.data.artifactType);
    }
  }, [
    selectedArtifactFamily,
    selectedArtifactQuery.data,
    setSelectedArtifactFamily,
  ]);

  const tableRows = useMemo(() => {
    if (!selectedArtifactQuery.data) {
      return rows;
    }
    if (rows.some((row) => row.id === selectedArtifactQuery.data?.id)) {
      return rows;
    }
    return [selectedArtifactQuery.data, ...rows];
  }, [rows, selectedArtifactQuery.data]);

  const selectedArtifact =
    selectedArtifactQuery.data ??
    tableRows.find((row) => row.id === selectedArtifactId) ??
    tableRows[0];

  // The first column title embeds the family name, so the array is memoized
  // on the family instead of being rebuilt on every render.
  const tableColumns = useMemo<DenseColumn<ArtifactRow>[]>(
    () => [
      {
        key: 'title',
        title: `${selectedArtifactFamily} 路径`,
        className: 'w-[38%] text-forensics-muted',
        render: (row) => row.title,
      },
      {
        key: 'summary',
        title: '目标路径',
        className: 'w-[34%] text-forensics-text-secondary',
        render: (row) => row.summary.replace('目标路径: ', ''),
      },
      {
        key: 'createdAt',
        title: '创建时间',
        className: 'w-40 text-forensics-muted-light',
        render: (row) => row.createdAt,
      },
      {
        key: 'args',
        title: '参数',
        className: 'text-forensics-text-tertiary',
        render: (row) => String(row.attrs.arguments ?? '-'),
      },
    ],
    [selectedArtifactFamily],
  );

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar
        title="痕迹家族控制"
        meta={`Family ${selectedArtifactFamily} / 记录 ${tableRows.length}/${totalRows} 条 / 来源范围：Windows 用户活动 / 壳对象`}
      >
        <div className="flex h-10 shrink-0 items-center gap-1 overflow-x-auto px-2">
          {families?.map((family) => {
            const isSelected = selectedArtifactFamily === family;
            const count =
              familyCounts?.find((item) => item.family === family)?.count ??
              tableRows.length;
            return (
              <Button
                type="button"
                key={family}
                variant={isSelected ? 'forensicsSurface' : 'forensicsGhost'}
                size="xs"
                onClick={() => setSelectedArtifactFamily(family)}
                className="shrink-0 whitespace-nowrap font-mono"
              >
                {family} <span className="text-forensics-muted-lighter">{count}</span>
              </Button>
            );
          })}
        </div>
      </PageSubbar>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="min-w-0 flex-1 border-r border-forensics-border">
          <DenseDataTable<ArtifactRow>
            rows={tableRows}
            getRowKey={(row) => row.id}
            selectedRowKey={selectedArtifact?.id}
            onRowClick={(row) => setSelectedArtifactId(row.id)}
            emptyTitle="当前痕迹家族无记录"
            emptyDescription="请切换 family 或等待解析任务完成。"
            columns={tableColumns}
            loadContextKey={selectedArtifactFamily}
            loadStateKey={artifactRowsQuery.dataUpdatedAt}
            onReachEnd={() => { void artifactRowsQuery.fetchNextPage(); }}
            onRetryLoadMore={() => artifactRowsQuery.refetch()}
            hasMore={artifactRowsQuery.hasNextPage}
            loadingMore={artifactRowsQuery.isFetchingNextPage}
            loadMoreFailed={artifactRowsQuery.isFetchNextPageError}
            initialLoadFailed={artifactRowsQuery.isError && rows.length === 0}
            onRetryInitialLoad={() => { void artifactRowsQuery.refetch(); }}
          />
        </div>

        <InspectorPane
          title="痕迹属性"
          subtitle={
            selectedArtifact
              ? `${selectedArtifact.artifactType} / ${selectedArtifact.id}`
              : '未选择痕迹'
          }
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="目标路径">
              <InspectorValue
                value={String(selectedArtifact?.attrs.targetPath ?? '-')}
                mono
                strong
              />
            </InspectorSection>

            <InspectorSection title="属性字段">
              <div className="space-y-2 font-mono text-[10px]">
                <ArtifactField
                  label="驱动器类型"
                  value={String(selectedArtifact?.attrs.driveType ?? '-')}
                />
                <ArtifactField
                  label="卷序列号"
                  value={String(selectedArtifact?.attrs.volumeSerial ?? '-')}
                />
                <ArtifactField
                  label="机器 ID"
                  value={String(selectedArtifact?.attrs.machineId ?? '-')}
                />
              </div>
            </InspectorSection>

            <InspectorSection title="来源上下文">
              <InspectorValue value={selectedArtifact?.title ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="关联动作">
              <div className="space-y-2">
                <Button
                  type="button"
                  variant="forensicsSurface"
                  size="xs"
                  onClick={() => openArtifactSource(selectedArtifact)}
                  disabled={!selectedArtifact?.sourceObjectId}
                  className="w-full font-light"
                >
                  在文件浏览中定位目标
                </Button>
                <Button
                  type="button"
                  variant="forensicsLink"
                  size="xs"
                  onClick={() => openArtifactTimeline(selectedArtifact)}
                  disabled={!selectedArtifact}
                  className="w-full"
                >
                  查看关联时间线事件
                </Button>
              </div>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
