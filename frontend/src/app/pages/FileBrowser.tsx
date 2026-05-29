import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { ChevronDown, ChevronRight, File, Folder, HardDrive } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { useCurrentCase } from '@/features/case/hooks';
import { useFileChildren, useFileRows, useFileTree, useFileViewer } from '@/features/files/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import { FileEntryRow, FileTreeNode } from '@/types/models';

export function FileBrowser() {
  const { data: currentCase } = useCurrentCase();
  const { data: rootTree } = useFileTree();
  const selectedDirectoryId = useSelectionStore((state) => state.selectedDirectoryId);
  const setSelectedDirectoryId = useSelectionStore((state) => state.setSelectedDirectoryId);
  const selectedFileId = useSelectionStore((state) => state.selectedFileId);
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const viewerTab = useUiStore((state) => state.viewerTab);
  const setViewerTab = useUiStore((state) => state.setViewerTab);
  const navigate = useNavigate();
  const [expandedDirectoryIds, setExpandedDirectoryIds] = useState<string[]>([]);
  const [treeChildren, setTreeChildren] = useState<Record<string, FileTreeNode[]>>({});

  const activeDirectoryId =
    selectedDirectoryId ?? rootTree?.[0]?.id;
  const { data: rows } = useFileRows(activeDirectoryId);
  const { data: activeChildren } = useFileChildren(activeDirectoryId);

  useEffect(() => {
    if (!activeDirectoryId || !activeChildren) {
      return;
    }
    setTreeChildren((current) => ({
      ...current,
      [activeDirectoryId]: activeChildren,
    }));
  }, [activeChildren, activeDirectoryId]);

  useEffect(() => {
    if (!selectedDirectoryId && rootTree?.[0]?.id) {
      setSelectedDirectoryId(rootTree[0].id);
      setExpandedDirectoryIds((current) =>
        current.includes(rootTree[0].id) ? current : [...current, rootTree[0].id],
      );
    }
  }, [rootTree, selectedDirectoryId, setSelectedDirectoryId]);

  useEffect(() => {
    const visibleIds = new Set<string>();
    const collect = (nodes: FileTreeNode[]) => {
      for (const node of nodes) {
        visibleIds.add(node.id);
        const children = treeChildren[node.id];
        if (children?.length) {
          collect(children);
        }
      }
    };
    collect(rootTree ?? []);

    if (selectedDirectoryId && !visibleIds.has(selectedDirectoryId)) {
      setSelectedDirectoryId(rootTree?.[0]?.id);
      setSelectedFileId(undefined);
    }
  }, [rootTree, selectedDirectoryId, setSelectedDirectoryId, setSelectedFileId, treeChildren]);

  useEffect(() => {
    if (selectedFileId && (!rows || !rows.some((row) => row.id === selectedFileId))) {
      setSelectedFileId(undefined);
    }
  }, [rows, selectedFileId, setSelectedFileId]);

  const selectedFile = rows?.find((row) => row.id === selectedFileId);
  const { data: viewer } = useFileViewer(selectedFile?.id);
  const flatTree = useMemo(() => {
    const visible: FileTreeNode[] = [];
    const roots = rootTree ?? [];

    const appendNodes = (nodes: FileTreeNode[]) => {
      for (const node of nodes) {
        visible.push(node);
        if (expandedDirectoryIds.includes(node.id)) {
          appendNodes(treeChildren[node.id] ?? []);
        }
      }
    };

    appendNodes(roots);
    return visible;
  }, [expandedDirectoryIds, rootTree, treeChildren]);
  const currentDirectory = flatTree.find((node) => node.id === activeDirectoryId);
  const activeRootNode = useMemo(() => {
    if (!activeDirectoryId || !rootTree?.length) {
      return rootTree?.[0];
    }

    for (const root of rootTree) {
      if (root.id === activeDirectoryId) {
        return root;
      }

      const stack: FileTreeNode[] = [...(treeChildren[root.id] ?? [])];
      while (stack.length > 0) {
        const next: FileTreeNode = stack.pop()!;
        if (next.id === activeDirectoryId) {
          return root;
        }
        stack.push(...(treeChildren[next.id] ?? []));
      }
    }

    return rootTree[0];
  }, [activeDirectoryId, rootTree, treeChildren]);
  const executableCount =
    rows?.filter((row) => ['exe', 'dll'].includes((row.ext ?? row.name.split('.').pop() ?? '').toLowerCase()))
      .length ?? 0;

  const treeNodes = useMemo(
    () =>
      flatTree.map((node) => ({
        ...node,
        active: node.id === activeDirectoryId,
        expanded: expandedDirectoryIds.includes(node.id),
      })),
    [activeDirectoryId, expandedDirectoryIds, flatTree],
  );
  const activeDirectoryPath = rows?.find((row) => row.id === activeDirectoryId)?.path;

  function toggleDirectory(node: FileTreeNode) {
    setSelectedDirectoryId(node.id);
    setSelectedFileId(undefined);
    setExpandedDirectoryIds((current) =>
      current.includes(node.id) ? current.filter((id) => id !== node.id) : [...current, node.id],
    );
  }

  if (!currentCase) {
    return (
      <div className="flex-1 flex items-center justify-center bg-white">
        <div className="w-full max-w-xl border border-[#e0e0e0] bg-[#fafafa] p-8 text-center">
          <div className="font-serif text-2xl text-[#111] mb-3">文件浏览待激活</div>
          <div className="text-[13px] text-[#666] leading-6">
            先在案件概览页创建或打开案件，再导入镜像或逻辑目录，即可在这里浏览目录树和文件内容。
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="文件浏览控制" meta={`当前目录 ${rows?.length ?? 0} 项 / 可执行对象 ${executableCount} 项`}>
        <div className="h-10 flex items-center px-4 gap-4 text-xs shrink-0">
          <div className="flex items-center gap-1.5 text-[#666] font-mono text-[11px] min-w-0">
            <HardDrive size={12} />
            {treeNodes.length > 0 ? (
              <>
                <span className="text-[#111] font-semibold">{activeRootNode?.name || '/'}</span>
                {currentDirectory && currentDirectory.id !== activeRootNode?.id ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold">{currentDirectory.name}</span>
                  </>
                ) : null}
                {selectedFile ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold">{selectedFile.name}</span>
                  </>
                ) : null}
              </>
            ) : (
              <span className="text-[#aaa]">无数据源</span>
            )}
          </div>
          <div className="h-4 border-l border-[#e0e0e0]" />
          <div className="text-[#666] flex items-center gap-2">
            过滤:
            <input
              type="text"
              className="bg-white border border-[#ccc] px-2 py-0.5 text-[#111] font-mono text-[11px] rounded-[2px] outline-none w-40 focus:border-[#666]"
              defaultValue="*"
            />
          </div>
          <div className="text-[11px] text-[#888] font-mono">viewer: metadata / hex 已启用</div>
          <div className="ml-auto text-[#888] text-[11px]">显示 {rows?.length ?? 0} 个项目</div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="w-56 border-r border-[#e0e0e0] bg-[#fafafa] flex flex-col shrink-0">
          <div className="h-7 border-b border-[#e0e0e0] flex items-center px-3 text-[10px] font-semibold text-[#555] uppercase tracking-wider bg-[#f5f5f5]">
            目录树
          </div>
          <div className="flex-1 overflow-auto py-1 font-mono text-[11px] select-none">
            {treeNodes.length === 0 ? (
              <div className="px-3 py-4 text-[#888]">导入数据源后显示目录树。</div>
            ) : null}
            {treeNodes.map((node) => (
              <button
                key={node.id}
                type="button"
                onClick={() => toggleDirectory(node)}
                className={`w-full flex items-center gap-1.5 px-2 py-1 cursor-pointer text-left ${
                  node.active ? 'bg-[#e0e0e0] text-[#111] font-medium' : 'text-[#555] hover:bg-[#eaeaea]'
                }`}
                style={{ paddingLeft: `${8 + node.depth * 16}px` }}
              >
                {node.expanded ? <ChevronDown size={12} className="text-[#888]" /> : <ChevronRight size={12} className="text-[#aaa]" />}
                <Folder
                  size={12}
                  className={
                    node.status === 'locked'
                      ? 'text-amber-600'
                      : node.status === 'unsupported'
                        ? 'text-[#999]'
                        : node.status === 'queued'
                          ? 'text-sky-700'
                          : 'text-[#888]'
                  }
                />
                <span className="truncate">{node.name}</span>
                {node.status && node.status !== 'ready' ? (
                  <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-[#888]">
                    {node.status}
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </div>

        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="flex-1 flex flex-col border-b border-[#e0e0e0] bg-white min-h-0">
            <DenseDataTable<FileEntryRow>
              rows={rows ?? []}
              getRowKey={(row) => row.id}
              selectedRowKey={selectedFile?.id}
              onRowClick={(row) => {
                if (row.entryType === 'directory') {
                  setSelectedDirectoryId(row.id);
                  setSelectedFileId(undefined);
                  setExpandedDirectoryIds((current) =>
                    current.includes(row.id) ? current : [...current, row.id],
                  );
                  return;
                }
                setSelectedFileId(row.id);
              }}
              emptyTitle="当前目录为空"
              emptyDescription="所选路径下没有可展示的文件对象。"
              columns={[
                {
                  key: 'name',
                  title: '名称',
                  className: 'w-[34%]',
                  render: (row) => (
                    <div className="flex items-center gap-2 min-w-0">
                      {row.entryType === 'directory' ? (
                        <Folder size={12} className="text-[#888]" />
                      ) : (
                        <File size={12} className="text-[#888]" />
                      )}
                      <span className="truncate">{row.name}</span>
                    </div>
                  ),
                },
                {
                  key: 'size',
                  title: '大小',
                  className: 'w-28 text-[#666]',
                  render: (row) => (row.entryType === 'directory' ? '-' : row.size ? `${Math.round(row.size / 1000)} KB` : '0 KB'),
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
                  render: (row) => (row.entryType === 'directory' ? 'DIR' : row.deleted ? 'DEL' : 'A--'),
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
                    <div className="font-mono text-[11px] text-[#333] whitespace-pre-wrap">
                      {viewer.range.lines.map((line, index) => (
                        <div key={index}>{line}</div>
                      ))}
                    </div>
                  ) : (
                    <div className="text-[#666]">选择文件后显示十六进制预览。</div>
                  ),
                },
                {
                  value: 'text',
                  label: '文本',
                  content: (
                    <div className="space-y-2 font-mono text-[11px] text-[#444]">
                      <div className="text-[#888]">文本预览尚未实现，当前 demo 只保证 metadata 与 hex 可用。</div>
                      <div className="border border-[#e0e0e0] bg-white p-3 text-[#666]">选择文本文件后可继续扩展编码识别与字符串提取。</div>
                    </div>
                  ),
                },
                {
                  value: 'preview',
                  label: '预览',
                  content: (
                    <div className="space-y-2 text-[11px] text-[#555]">
                      <div className="text-[#888] font-mono">预览状态: 降级模式</div>
                      <div className="border border-dashed border-[#d7d7d7] bg-white p-4">当前 demo 暂未生成媒体预览，但不会影响文件浏览与 hex 查看。</div>
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
              <InspectorValue value={selectedFile?.path ?? activeDirectoryPath ?? currentDirectory?.name ?? '-'} mono />
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
                <button
                  onClick={() => {
                    if (selectedFile) {
                      useSelectionStore.getState().setSelectedTimelineId(selectedFile.id);
                      navigate("/timeline");
                    }
                  }}
                  className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline"
                >
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
