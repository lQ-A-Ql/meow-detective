import { Activity, AlertTriangle, CheckCircle2, Clock, Database, FileText, FolderOpen, PencilLine, Trash2, Upload } from 'lucide-react';
import { useMemo, useState, type ReactNode } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { InlineProgressRow } from '@/components/status/InlineProgressRow';
import {
  useCreateCase,
  useCurrentCase,
  useOpenCase,
  useCaseMetrics,
  useDataSources,
  useDeleteCase,
  useDeleteDataSource,
  useRecentCases,
  useRecentObjects,
  useRemoveCaseFromList,
  useRenameDataSource,
} from '@/features/case/hooks';
import { useImportDataSource } from '@/features/files/hooks';
import { cancelImport } from '@/lib/api/files';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import type { JobSnapshot } from '@/types/models';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

const importJobPattern = /导入|加载|镜像|数据源|datasource|data source|import|ingest|image|e01|raw|dd|img/i;

function isImportJob(job: JobSnapshot) {
  return job.status === 'running' && importJobPattern.test(`${job.name} ${job.scope} ${job.detail}`);
}

function isImportRelatedJob(job: JobSnapshot) {
  return importJobPattern.test(`${job.name} ${job.scope} ${job.detail}`);
}

export function CaseHome() {
  const { data: currentCase } = useCurrentCase();
  const { data: metrics } = useCaseMetrics();
  const { data: dataSources } = useDataSources();
  const { data: recentCases } = useRecentCases();
  const { data: recentObjects } = useRecentObjects();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const qc = useQueryClient();
  const importMutation = useImportDataSource();
  const cancelImportMutation = useMutation({
    mutationFn: (jobId: string) => cancelImport(jobId),
    onSuccess: () => { toast.success('导入已取消'); qc.invalidateQueries({ queryKey: ['jobs'] }); },
    onError: (e: Error) => { toast.error('取消失败', { description: e.message }); },
  });
  const createCaseMutation = useCreateCase();
  const openCaseMutation = useOpenCase();
  const renameDataSourceMutation = useRenameDataSource();
  const deleteCaseMutation = useDeleteCase();
  const deleteDataSourceMutation = useDeleteDataSource();
  const removeCaseFromListMutation = useRemoveCaseFromList();

  const [importPath, setImportPath] = useState('');
  const [showImport, setShowImport] = useState(false);
  const [caseRoot, setCaseRoot] = useState('C:\\Cases');
  const [caseName, setCaseName] = useState('');
  const [openCasePath, setOpenCasePath] = useState('C:\\Cases\\case-001');
  const [editingDataSourceId, setEditingDataSourceId] = useState<string | undefined>();
  const [editingDataSourceName, setEditingDataSourceName] = useState('');

  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const runningJob = runningJobs[0];
  const importJob = runningJobs.find(isImportJob);
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const partialJobCount = jobs?.filter((job) => job.partial).length ?? 0;
  const failedImportJob = jobs?.find((job) => job.status === 'failed' && isImportRelatedJob(job));
  const sortedRecentCases = useMemo(() => recentCases ?? [], [recentCases]);

  if (!currentCase) {
    return (
      <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
        <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-8">
          <div className="font-serif text-3xl text-[#111] tracking-tight mb-3">Forensics Workbench</div>
          <div className="max-w-3xl text-[14px] text-[#666] leading-7">
            当前没有活动案件。先创建或打开案件目录，接着导入逻辑目录、RAW/DD/IMG 或 E01 镜像，即可进入可运行 demo 的真实文件浏览链路。
          </div>
        </div>

        <div className="grid grid-cols-2 gap-6 p-8">
          <div className="border border-[#e0e0e0] bg-white p-5">
            <div className="text-[13px] font-semibold text-[#333] mb-3">新建案件</div>
            <div className="space-y-2 mb-3">
              <input
                type="text"
                value={caseRoot}
                onChange={(e) => setCaseRoot(e.target.value)}
                placeholder="案件根目录"
                className="w-full border border-[#ccc] px-2 py-1 text-[12px] font-mono"
              />
              <input
                type="text"
                value={caseName}
                onChange={(e) => setCaseName(e.target.value)}
                placeholder="案件名称"
                className="w-full border border-[#ccc] px-2 py-1 text-[12px]"
              />
            </div>
            <button
              onClick={() => createCaseMutation.mutate({ caseRoot, name: caseName })}
              disabled={createCaseMutation.isPending || !caseRoot || !caseName}
              className="bg-[#111] text-white px-4 py-1.5 text-[12px] hover:bg-[#333] disabled:opacity-50"
            >
              {createCaseMutation.isPending ? '创建中...' : '创建案件'}
            </button>
            {createCaseMutation.isError ? (
              <div className="mt-2 text-[11px] text-red-600">{(createCaseMutation.error as Error)?.message}</div>
            ) : null}
          </div>

          <div className="border border-[#e0e0e0] bg-white p-5">
            <div className="text-[13px] font-semibold text-[#333] mb-3">打开已有案件</div>
            <div className="space-y-2 mb-3">
              <input
                type="text"
                value={openCasePath}
                onChange={(e) => setOpenCasePath(e.target.value)}
                placeholder="案件路径"
                className="w-full border border-[#ccc] px-2 py-1 text-[12px] font-mono"
              />
            </div>
            <button
              onClick={() => openCaseMutation.mutate(openCasePath)}
              disabled={openCaseMutation.isPending || !openCasePath}
              className="bg-[#111] text-white px-4 py-1.5 text-[12px] hover:bg-[#333] disabled:opacity-50"
            >
              {openCaseMutation.isPending ? '打开中...' : '打开案件'}
            </button>
            {openCaseMutation.isError ? (
              <div className="mt-2 text-[11px] text-red-600">{(openCaseMutation.error as Error)?.message}</div>
            ) : null}
          </div>
        </div>

        <div className="px-8 pb-8">
          <div className="border border-[#e0e0e0] bg-white">
            <div className="border-b border-[#e0e0e0] bg-[#fafafa] px-5 py-3 flex items-center justify-between">
              <div className="text-[13px] font-semibold text-[#333]">最近打开案件</div>
              <div className="text-[10px] font-mono text-[#888]">{sortedRecentCases.length} 项</div>
            </div>
            {sortedRecentCases.length ? (
              <div className="divide-y divide-[#eee]">
                {sortedRecentCases.map((item) => (
                  <div
                    key={`${item.caseRoot}-${item.openedAt}`}
                    className="flex items-center px-5 py-3 text-left hover:bg-[#f7f7f7] cursor-pointer"
                    onClick={() => openCaseMutation.mutate(item.caseRoot)}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="text-[13px] text-[#111] font-medium truncate">{item.name}</div>
                      <div className="text-[11px] text-[#666] font-mono truncate mt-1">{item.caseRoot}</div>
                    </div>
                    <div className="text-[10px] text-[#888] font-mono shrink-0 mr-3">{item.openedAt}</div>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (window.confirm(`确定删除案件 "${item.name}"？\n\n该操作将删除案件目录及其所有数据，且不可撤销。`)) {
                          deleteCaseMutation.mutate(item.caseRoot, {
                            onSuccess: () => {
                              removeCaseFromListMutation.mutate(item.caseRoot);
                            },
                          });
                        }
                      }}
                      className="text-[#999] hover:text-red-600 shrink-0"
                      title="删除案件"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="px-5 py-6 text-[12px] text-[#777]">这里会保留最近打开过的案件，便于重新进入分析现场。</div>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="font-serif text-2xl text-[#111] mb-1 tracking-tight">案件 #{currentCase.number ?? '-'}</div>
            <div className="text-[#666] font-mono text-[11px]">{currentCase.name}</div>
            <div className="mt-3 flex flex-wrap gap-2 text-[10px] uppercase tracking-wider text-[#666]">
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">当前状态: 活跃</span>
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">数据源: {metrics?.dataSourceCount ?? 0}</span>
              <span className="border border-[#e7d9b4] bg-white px-2 py-1">告警: {warnings?.length ?? 0}</span>
              <span className="border border-[#e7d9b4] bg-white px-2 py-1">部分完成任务: {partialJobCount}</span>
            </div>
          </div>
          <div className="flex gap-8 text-right">
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">状态</div>
              <div className="flex items-center gap-1.5 text-[#111] text-[13px] justify-end">
                <div className="w-1.5 h-1.5 rounded-full bg-[#111]"></div> 活跃
              </div>
            </div>
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">创建时间</div>
              <div className="text-[#111] text-[13px] font-mono">{currentCase.createdAt}</div>
            </div>
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">检验人</div>
              <div className="text-[#111] text-[13px]">{currentCase.examiner ?? '-'}</div>
            </div>
            <button
              onClick={() => setShowImport((value) => !value)}
              className="flex items-center gap-1.5 border border-[#111] px-3 py-1.5 text-[12px] hover:bg-[#111] hover:text-white transition-colors"
            >
              <Upload size={12} /> 导入数据源
            </button>
          </div>
        </div>
      </div>

      {showImport ? (
        <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-4 shrink-0">
          <div className="flex items-center gap-3">
            <input
              type="text"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              placeholder="镜像路径或逻辑目录路径"
              className="flex-1 border border-[#ccc] px-3 py-1.5 text-[12px] font-mono"
            />
            <button
              onClick={async () => {
                try {
                  const path = await open({
                    directory: false,
                    multiple: false,
                    filters: [{ name: 'Data Sources', extensions: ['e01', 'E01', 'dd', 'raw', 'img'] }],
                  });
                  if (path) {
                    setImportPath(path as string);
                  }
                } catch {
                  // Tauri dialog may be unavailable in non-tauri mode.
                }
              }}
              className="border border-[#ccc] px-3 py-1.5 text-[12px] hover:bg-[#eee] flex items-center gap-1"
            >
              <FolderOpen size={12} /> 文件
            </button>
            <button
              onClick={async () => {
                try {
                  const path = await open({ directory: true, multiple: false });
                  if (path) {
                    setImportPath(path as string);
                  }
                } catch {
                  // Tauri dialog may be unavailable in non-tauri mode.
                }
              }}
              className="border border-[#ccc] px-3 py-1.5 text-[12px] hover:bg-[#eee] flex items-center gap-1"
            >
              <FolderOpen size={12} /> 目录
            </button>
            <button
              onClick={() => {
                if (importPath.trim()) {
                  importMutation.mutate(importPath.trim());
                }
              }}
              disabled={importMutation.isPending || Boolean(importJob)}
              className="bg-[#111] text-white px-4 py-1.5 text-[12px] hover:bg-[#333] disabled:opacity-50"
            >
              {importMutation.isPending ? '提交中...' : importJob ? '后台导入中...' : '导入'}
            </button>
            <button
              onClick={() => {
                setShowImport(false);
                setImportPath('');
              }}
              className="text-[#888] text-[12px] hover:text-[#111]"
            >
              取消
            </button>
          </div>
          {importMutation.isPending ? (
            <div className="mt-2 flex items-center gap-2 text-[11px] text-[#666]">
              <div className="w-3 h-3 border-2 border-[#666] border-t-transparent rounded-full animate-spin" />
              正在提交导入任务，后台进度会在任务列表中持续更新。
            </div>
          ) : null}
          {importJob ? (
            <div className="mt-2 text-[11px] text-[#555] font-mono bg-white border border-[#ddd] p-2">
              <div>后台导入进行中: {importJob.name} · {importJob.progress}% · {importJob.detail}</div>
              <button
                onClick={() => cancelImportMutation.mutate(importJob.id)}
                disabled={cancelImportMutation.isPending}
                className="mt-1 text-red-600 hover:text-red-800 text-[10px] underline disabled:opacity-50"
              >
                {cancelImportMutation.isPending ? '取消中...' : '取消导入'}
              </button>
            </div>
          ) : null}
          {importMutation.isSuccess ? (
            <div className="mt-2 text-[11px] text-green-700 font-mono bg-green-50 border border-green-200 p-2">
              {importMutation.data}
            </div>
          ) : null}
          {importMutation.isError ? (
            <div className="mt-2 text-[11px] text-red-600 font-mono bg-red-50 border border-red-200 p-2">
              导入失败: {(importMutation.error as Error)?.message || '未知错误'}
            </div>
          ) : null}
          {failedImportJob ? (
            <div className="mt-2 text-[11px] text-red-700 font-mono bg-red-50 border border-red-200 p-2">
              后台导入失败: {failedImportJob.detail || failedImportJob.name}
            </div>
          ) : null}
        </div>
      ) : null}

      <div className="border-b border-[#e0e0e0] shrink-0">
        <div className="grid grid-cols-4 divide-x divide-[#e0e0e0]">
          <MetricBlock icon={<Database size={12} />} title="数据源" value={metrics?.dataSourceCount ?? 0} />
          <MetricBlock icon={<FileText size={12} />} title="已索引文件" value={metrics?.indexedFileCount ?? 0} />
          <MetricBlock icon={<Clock size={12} />} title="时间线事件" value={metrics?.timelineEventCount ?? 0} />
          <MetricBlock icon={<AlertTriangle size={12} />} title="提取痕迹" value={metrics?.artifactCount ?? 0} />
        </div>
      </div>

      <div className="flex-1 flex min-h-0">
        <div className="w-1/2 border-r border-[#e0e0e0] flex flex-col min-h-0 bg-white">
          <div className="h-8 border-b border-[#e0e0e0] bg-[#fafafa] flex items-center justify-between px-4 text-[11px] font-semibold uppercase text-[#555] tracking-wider shrink-0">
            <span>最近任务</span>
            <span className="font-mono text-[10px] text-[#888]">
              完成 {completedJobs.length} / 部分 {partialJobCount} / 运行 {runningJob ? 1 : 0}
            </span>
          </div>
          <div className="flex-1 overflow-auto p-4 space-y-3">
            {runningJob ? (
              <InlineProgressRow
                title={runningJob.name}
                subtitle={runningJob.scope}
                detail={runningJob.detail}
                progress={runningJob.progress}
              />
            ) : null}
            {completedJobs.map((job) => (
              <div key={job.id} className="border-t border-[#eee] pt-3 flex items-start gap-3">
                <CheckCircle2 size={14} className="text-[#888] mt-0.5" />
                <div className="flex-1">
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-[#333] text-[13px]">{job.name}</div>
                    <div className="text-[#888] font-mono text-[10px]">{job.detail}</div>
                  </div>
                  <div className="text-[#666] text-[11px] mt-0.5">{job.scope}</div>
                  {job.partial ? (
                    <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] font-mono">
                      <span className="border border-[#e7d9b4] bg-[#fff9ec] px-1.5 py-0.5 text-[#8a5a00]">
                        PARTIAL
                      </span>
                      <span className="border border-[#e7d9b4] bg-white px-1.5 py-0.5 text-[#6f4d00]">
                        warnings {job.warningCount}
                      </span>
                      <span className="border border-[#d9d9d9] bg-white px-1.5 py-0.5 text-[#555]">
                        skipped {job.skippedCount}
                      </span>
                      <span className="border border-red-200 bg-white px-1.5 py-0.5 text-red-700">
                        failed {job.failedCount}
                      </span>
                    </div>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="w-1/2 flex flex-col min-h-0 bg-[#fafafa]">
          <div className="h-8 border-b border-[#e0e0e0] bg-[#fafafa] flex items-center justify-between px-4 text-[11px] font-semibold uppercase text-[#555] tracking-wider shrink-0">
            <span>已有数据源</span>
            <span className="font-mono text-[10px] text-[#888]">{dataSources?.length ?? 0} 个</span>
          </div>
          <div className="max-h-64 overflow-auto border-b border-[#e0e0e0] bg-white">
            {dataSources?.length ? (
              dataSources.map((source) => {
                const isEditing = editingDataSourceId === source.id;
                const partitionCount = source.partitions?.length ?? 0;
                return (
                  <div key={source.id} className="border-b border-[#eee] px-4 py-3 last:border-b-0">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        {isEditing ? (
                          <div className="flex items-center gap-2">
                            <input
                              type="text"
                              value={editingDataSourceName}
                              onChange={(e) => setEditingDataSourceName(e.target.value)}
                              className="flex-1 border border-[#ccc] px-2 py-1 text-[12px] bg-white"
                            />
                            <button
                              onClick={() =>
                                renameDataSourceMutation.mutate(
                                  { dataSourceId: source.id, name: editingDataSourceName.trim() },
                                  {
                                    onSuccess: () => {
                                      setEditingDataSourceId(undefined);
                                      setEditingDataSourceName('');
                                    },
                                  },
                                )
                              }
                              className="border border-[#111] px-2 py-1 text-[11px] hover:bg-[#111] hover:text-white"
                            >
                              保存
                            </button>
                            <button
                              onClick={() => {
                                setEditingDataSourceId(undefined);
                                setEditingDataSourceName('');
                              }}
                              className="text-[11px] text-[#666] hover:text-[#111]"
                            >
                              取消
                            </button>
                          </div>
                        ) : (
                          <div className="flex items-center gap-2">
                            <div className="text-[13px] text-[#111] font-medium truncate">{source.name}</div>
                            <button
                              type="button"
                              onClick={() => {
                                setEditingDataSourceId(source.id);
                                setEditingDataSourceName(source.name);
                              }}
                              className="text-[#777] hover:text-[#111]"
                            >
                              <PencilLine size={12} />
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                if (window.confirm(`确定删除数据源 "${source.name}"？\n\n该操作将级联删除其下的所有文件条目、时间线事件和提取痕迹，且不可撤销。`)) {
                                  deleteDataSourceMutation.mutate(source.id);
                                }
                              }}
                              className="text-[#999] hover:text-red-600"
                            >
                              <Trash2 size={12} />
                            </button>
                          </div>
                        )}
                        <div className="mt-1 text-[10px] uppercase tracking-wider text-[#888]">{source.kind}</div>
                        <div className="mt-1 text-[11px] text-[#666] font-mono break-all">{source.sourcePath}</div>
                        {partitionCount > 0 ? (
                          <div className="mt-3 space-y-2">
                            <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-[#777]">
                              <span>分区结构</span>
                              <span className="font-mono">{partitionCount} 项</span>
                            </div>
                            <div className="space-y-2">
                              {source.partitions.map((partition) => {
                                const statusTone =
                                  partition.status === 'supported'
                                    ? 'border-[#d7e7d7] bg-[#f7fbf7] text-[#234b23]'
                                    : partition.status === 'locked'
                                      ? 'border-[#ead8ab] bg-[#fff9ec] text-[#8a5a00]'
                                      : 'border-[#e3e3e3] bg-[#fafafa] text-[#666]';
                                const statusLabel =
                                  partition.status === 'supported'
                                    ? '可浏览'
                                    : partition.status === 'locked'
                                      ? '需要解锁'
                                      : '暂不支持';

                                return (
                                  <div key={`${source.id}-${partition.index}`} className={`border px-3 py-2 text-[11px] ${statusTone}`}>
                                    <div className="flex items-start justify-between gap-3">
                                      <div className="min-w-0">
                                        <div className="text-[#111] font-medium">
                                          Partition {partition.index} / {partition.kindLabel}
                                        </div>
                                        <div className="mt-1 text-[#555]">{partition.name}</div>
                                        <div className="mt-1 font-mono text-[10px] break-all text-[#777]">
                                          offset {partition.offset} / length {partition.length}
                                        </div>
                                        {partition.typeGuid ? (
                                          <div className="mt-1 font-mono text-[10px] break-all text-[#888]">
                                            GUID {partition.typeGuid}
                                          </div>
                                        ) : null}
                                        {partition.unlockHint ? (
                                          <div className="mt-2 text-[10px] font-medium text-[#8a5a00]">
                                            {partition.unlockHint}
                                          </div>
                                        ) : null}
                                      </div>
                                      <div className="shrink-0 text-right">
                                        <div className="text-[10px] uppercase tracking-wider font-semibold">
                                          {statusLabel}
                                        </div>
                                        <div className="mt-1 font-mono text-[10px] text-[#888]">
                                          {partition.filesystem ?? partition.kindLabel}
                                        </div>
                                      </div>
                                    </div>
                                  </div>
                                );
                              })}
                            </div>
                          </div>
                        ) : null}
                      </div>
                      <div className="text-right shrink-0">
                        <div className="text-[11px] text-[#111] font-mono">{source.fileCount ?? 0}</div>
                        <div className="text-[10px] text-[#888]">objects</div>
                      </div>
                    </div>
                  </div>
                );
              })
            ) : (
              <div className="px-4 py-6 text-[12px] text-[#777]">导入数据源后，这里会展示当前案件中的全部证据源，并允许重命名。</div>
            )}
          </div>

          <div className="h-8 border-b border-[#e0e0e0] bg-[#fafafa] flex items-center justify-between px-4 text-[11px] font-semibold uppercase text-[#555] tracking-wider shrink-0">
            <span>高价值对象</span>
            <span className="font-mono text-[10px] text-[#888]">最近发现 {recentObjects?.length ?? 0} 项</span>
          </div>
          <div className="flex-1 overflow-auto">
            <div className="flex flex-col border-b border-[#e0e0e0]">
              {recentObjects?.length ? (
                recentObjects.map((item) => (
                  <div key={item.id} className="flex items-center px-4 py-2 border-b border-[#eee] hover:bg-[#f0f0f0] cursor-pointer bg-white">
                    <div className="w-8">
                      {item.kind === 'file' ? <FileText size={12} className="text-[#888]" /> : <Activity size={12} className="text-[#888]" />}
                    </div>
                    <div className="flex-1 font-mono text-[11px]">
                      <div className="text-[#111] font-medium">{item.title}</div>
                      <div className="text-[#666] text-[10px] mt-0.5 font-sans">{item.detail}</div>
                    </div>
                    <div className="text-[#888] text-[11px]">{item.time}</div>
                  </div>
                ))
              ) : (
                <div className="px-4 py-6 text-[12px] text-[#777]">导入并完成初步解析后，这里会展示最近发现的高价值对象。</div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function MetricBlock({
  icon,
  title,
  value,
}: {
  icon: ReactNode;
  title: string;
  value: number;
}) {
  return (
    <div className="p-4 flex flex-col gap-3 bg-white">
      <div className="text-[#888] text-[11px] uppercase tracking-wider flex items-center gap-1.5">
        {icon} {title}
      </div>
      <div className="text-2xl font-serif text-[#111]">{value.toLocaleString()}</div>
    </div>
  );
}
