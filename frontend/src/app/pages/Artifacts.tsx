import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { useArtifactFamilies, useArtifactRows } from '@/features/artifacts/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { ArtifactRow } from '@/types/models';

export function Artifacts() {
  const selectedArtifactFamily = useSelectionStore((state) => state.selectedArtifactFamily);
  const setSelectedArtifactFamily = useSelectionStore((state) => state.setSelectedArtifactFamily);
  const selectedArtifactId = useSelectionStore((state) => state.selectedArtifactId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);
  const { data: families } = useArtifactFamilies();
  const { data: rows } = useArtifactRows(selectedArtifactFamily);
  const selectedArtifact = rows?.find((row) => row.id === selectedArtifactId) ?? rows?.[0];

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="痕迹家族控制" meta={`Family ${selectedArtifactFamily} / 记录 ${rows?.length ?? 0} 条`}>
        <div className="h-10 shrink-0 flex items-center px-2 gap-1 overflow-x-auto">
          {families?.map((family) => {
            const isSelected = selectedArtifactFamily === family;
            const count = family === 'LNK' ? rows?.length ?? 0 : family === 'Prefetch' ? 12 : family === 'Amcache' ? 36 : 8;
            return (
              <button
                key={family}
                onClick={() => setSelectedArtifactFamily(family === 'LNK' ? 'LNK' : family)}
                className={`px-3 py-1.5 text-[11px] font-mono transition-colors whitespace-nowrap ${isSelected ? 'bg-white text-[#111] border border-[#ccc] rounded-[2px] font-medium' : 'text-[#666] hover:text-[#111]'}`}
              >
                {family} <span className="text-[#999]">{count}</span>
              </button>
            );
          })}
          <div className="ml-auto pr-2 text-[11px] font-mono text-[#888]">来源范围: Windows 用户活动 / 壳对象</div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 min-w-0 border-r border-[#e0e0e0]">
          <DenseDataTable<ArtifactRow>
            rows={rows ?? []}
            getRowKey={(row) => row.id}
            selectedRowKey={selectedArtifact?.id}
            onRowClick={(row) => setSelectedArtifactId(row.id)}
            emptyTitle="当前痕迹家族无记录"
            emptyDescription="请切换 family 或等待解析任务完成。"
            columns={[
              { key: 'title', title: `${selectedArtifactFamily} 路径`, className: 'w-[38%] text-[#666]', render: (row) => row.title },
              { key: 'summary', title: '目标路径', className: 'w-[34%] text-[#333]', render: (row) => row.summary.replace('目标路径: ', '') },
              { key: 'createdAt', title: '创建时间', className: 'w-40 text-[#888]', render: (row) => row.createdAt },
              { key: 'args', title: '参数', className: 'text-[#555]', render: (row) => String(row.attrs.arguments ?? '-') },
            ]}
          />
        </div>

        <InspectorPane
          title="痕迹属性"
          subtitle={selectedArtifact ? `${selectedArtifact.artifactType} / ${selectedArtifact.id}` : '未选择痕迹'}
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="目标路径">
              <InspectorValue value={String(selectedArtifact?.attrs.targetPath ?? '-')} mono strong />
            </InspectorSection>

            <InspectorSection title="属性字段">
              <div className="font-mono text-[10px] space-y-2">
                <ArtifactField label="驱动器类型" value={String(selectedArtifact?.attrs.driveType ?? '-')} />
                <ArtifactField label="卷序列号" value={String(selectedArtifact?.attrs.volumeSerial ?? '-')} />
                <ArtifactField label="机器 ID" value={String(selectedArtifact?.attrs.machineId ?? '-')} />
              </div>
            </InspectorSection>

            <InspectorSection title="来源上下文">
              <InspectorValue value={selectedArtifact?.title ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="关联动作">
              <div className="space-y-2">
                <button className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium">
                  在文件浏览中定位目标
                </button>
                <button className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline">
                  查看关联时间线事件
                </button>
              </div>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}

function ArtifactField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5 border border-[#e0e0e0] bg-white p-2">
      <span className="text-[#888]">{label}</span>
      <span className="text-[#333]">{value}</span>
    </div>
  );
}
