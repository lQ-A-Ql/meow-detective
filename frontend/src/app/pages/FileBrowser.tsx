import { ChevronDown, ChevronRight, File, Folder, HardDrive } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { useFileRows, useFileTree, useFileViewer } from '@/features/files/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import { FileEntryRow } from '@/types/models';

export function FileBrowser() {
  const { data: tree } = useFileTree();
  const { data: rows } = useFileRows();
  const selectedFileId = useSelectionStore((state) => state.selectedFileId);
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const viewerTab = useUiStore((state) => state.viewerTab);
  const setViewerTab = useUiStore((state) => state.setViewerTab);
  const { data: viewer } = useFileViewer(selectedFileId);
  const selectedFile = rows?.find((row) => row.id === selectedFileId) ?? rows?.[0];
  const executableCount = rows?.filter((row) => ['exe', 'dll'].includes(row.ext ?? row.name.split('.').pop() ?? '')).length ?? 0;

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="文件浏览控制" meta={`当前目录 ${rows?.length ?? 0} 项 / 可执行对象 ${executableCount} 项`}>
        <div className="h-10 flex items-center px-4 gap-4 text-xs shrink-0">
          <div className="flex items-center gap-1.5 text-[#666] font-mono text-[11px] min-w-0">
            <HardDrive size={12} />
            <span className="hover:text-black cursor-pointer">Vol_1</span>
            <ChevronRight size={12} className="text-[#aaa]" />
            <span className="hover:text-black cursor-pointer">Windows</span>
            <ChevronRight size={12} className="text-[#aaa]" />
            <span className="text-[#111] font-semibold">System32</span>
          </div>
          <div className="h-4 border-l border-[#e0e0e0]" />
          <div className="text-[#666] flex items-center gap-2">
            过滤:
            <input
              type="text"
              className="bg-white border border-[#ccc] px-2 py-0.5 text-[#111] font-mono text-[11px] rounded-[2px] outline-none w-40 focus:border-[#666]"
              defaultValue="*.dll, *.exe"
            />
          </div>
          <div className="text-[11px] text-[#888] font-mono">命中目录缓存: runtime.db / files.view</div>
          <div className="ml-auto text-[#888] text-[11px]">显示 {rows?.length ?? 0} 个项目</div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="w-56 border-r border-[#e0e0e0] bg-[#fafafa] flex flex-col shrink-0">
          <div className="h-7 border-b border-[#e0e0e0] flex items-center px-3 text-[10px] font-semibold text-[#555] uppercase tracking-wider bg-[#f5f5f5]">
            目录树
          </div>
          <div className="flex-1 overflow-auto py-1 font-mono text-[11px] select-none">
            {tree?.map((node) => (
              <div
                key={node.id}
                className={`flex items-center gap-1.5 px-2 py-1 cursor-pointer ${node.active ? 'bg-[#e0e0e0] text-[#111] font-medium' : 'text-[#555] hover:bg-[#eaeaea]'}`}
                style={{ paddingLeft: `${8 + node.depth * 16}px` }}
              >
                {node.expanded ? <ChevronDown size={12} className="text-[#888]" /> : <ChevronRight size={12} className="text-[#aaa]" />}
                <Folder size={12} className="text-[#888]" />
                <span>{node.name}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="flex-1 flex flex-col border-b border-[#e0e0e0] bg-white min-h-0">
            <DenseDataTable<FileEntryRow>
              rows={rows ?? []}
              getRowKey={(row) => row.id}
              selectedRowKey={selectedFile?.id}
              onRowClick={(row) => setSelectedFileId(row.id)}
              emptyTitle="当前目录为空"
              emptyDescription="所选路径下没有可展示的文件对象。"
              columns={[
                {
                  key: 'name',
                  title: '名称',
                  className: 'w-[34%]',
                  render: (row) => (
                    <div className="flex items-center gap-2 min-w-0">
                      <File size={12} className="text-[#888]" />
                      <span className="truncate">{row.name}</span>
                    </div>
                  ),
                },
                {
                  key: 'size',
                  title: '大小',
                  className: 'w-28 text-[#666]',
                  render: (row) => (row.size ? `${Math.round(row.size / 1000)} KB` : '-'),
                },
                {
                  key: 'modifiedAt',
                  title: '修改时间',
                  className: 'w-44 text-[#666]',
                  render: (row) => row.modifiedAt ?? '-',
                },
                {
                  key: 'attr',
                  title: '属性',
                  className: 'text-[#888]',
                  render: (row) => (row.deleted ? 'DEL' : 'A--'),
                },
              ]}
            />
          </div>

          <div className="h-72 bg-[#fcfcfc] shrink-0 min-h-0">
            <ViewerTabs
              value={viewerTab}
              onValueChange={(value) => setViewerTab(value as 'metadata' | 'text' | 'hex' | 'preview')}
              tabs={[
                {
                  value: 'hex',
                  label: '十六进制',
                  content: viewer?.range.lines?.length ? (
                    <div className="flex gap-4 font-mono text-[11px] text-[#444] leading-[1.6]">
                      <div className="text-[#aaa] select-none text-right border-r border-[#ddd] pr-3">
                        {viewer.range.lines.map((_, index) => (
                          <div key={index}>{(index * 16).toString(16).padStart(8, '0').toUpperCase()}</div>
                        ))}
                      </div>
                      <div className="text-[#333] whitespace-pre-wrap">
                        {viewer.range.lines.map((line, index) => (
                          <div key={index}>{line}</div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <div className="text-[#666]">当前文件暂无十六进制预览。</div>
                  ),
                },
                {
                  value: 'text',
                  label: '文本',
                  content: (
                    <div className="space-y-2 font-mono text-[11px] text-[#444]">
                      <div className="text-[#888]">文本提取状态: 未命中 UTF-8/UTF-16 连续片段。</div>
                      <div className="border border-[#e0e0e0] bg-white p-3 text-[#666]">当前文件无可提取文本预览。</div>
                    </div>
                  ),
                },
                {
                  value: 'preview',
                  label: '预览',
                  content: (
                    <div className="space-y-2 text-[11px] text-[#555]">
                      <div className="text-[#888] font-mono">预览状态: 待生成</div>
                      <div className="border border-dashed border-[#d7d7d7] bg-white p-4">媒体预览需通过 runtime cache 生成临时资源。</div>
                    </div>
                  ),
                },
                {
                  value: 'metadata',
                  label: '元数据',
                  content: (
                    <div className="space-y-2 font-mono text-[11px] text-[#444]">
                      <div>handle_id: {viewer?.handle.handleId ?? '-'}</div>
                      <div>size: {viewer?.handle.size ?? '-'}</div>
                      <div>mime: {viewer?.handle.mime ?? '-'}</div>
                      <div>path: {selectedFile?.path ?? '-'}</div>
                    </div>
                  ),
                },
              ]}
            />
          </div>
        </div>

        <InspectorPane
          title="对象检查器"
          subtitle={selectedFile ? `已选对象 ${selectedFile.name}` : '未选中文件对象'}
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="对象标识">
              <InspectorValue value={selectedFile?.name ?? '-'} mono strong />
            </InspectorSection>

            <InspectorSection title="来源路径">
              <InspectorValue value={selectedFile?.path ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="时间戳 (MACB)">
              <div className="font-mono text-[11px] grid grid-cols-[30px_1fr] gap-1">
                <div className="text-[#888]">M</div>
                <div className="text-[#333]">{selectedFile?.modifiedAt ?? '-'}</div>
                <div className="text-[#888]">A</div>
                <div className="text-[#333]">{selectedFile?.accessedAt ?? '-'}</div>
                <div className="text-[#888]">C</div>
                <div className="text-[#333]">{selectedFile?.changedAt ?? '-'}</div>
                <div className="text-[#888]">B</div>
                <div className="text-[#333]">{selectedFile?.createdAt ?? '-'}</div>
              </div>
            </InspectorSection>

            <InspectorSection title="摘要字段">
              <InspectorValue value={selectedFile?.hashSha256 ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="操作">
              <div className="flex flex-col gap-2">
                <button className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium">
                  提取文件
                </button>
                <button className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline">
                  在时间线中查看
                </button>
              </div>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
