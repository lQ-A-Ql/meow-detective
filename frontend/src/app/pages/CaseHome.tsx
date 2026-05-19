import { Activity, AlertTriangle, Clock, Database, FileText, CheckCircle2, Upload, FolderOpen } from 'lucide-react';
import { useState } from 'react';
import { useCaseMetrics, useCurrentCase, useRecentObjects } from '@/features/case/hooks';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import { useImportDataSource } from '@/features/files/hooks';
import { useCreateCase, useOpenCase } from '@/features/case/hooks';
import { InlineProgressRow } from '@/components/status/InlineProgressRow';
import { open } from '@tauri-apps/plugin-dialog';

export function CaseHome() {
  const { data: currentCase } = useCurrentCase();
  const { data: metrics } = useCaseMetrics();
  const { data: recentObjects } = useRecentObjects();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const importMutation = useImportDataSource();
  const createCaseMutation = useCreateCase();
  const openCaseMutation = useOpenCase();
  const [importPath, setImportPath] = useState('');
  const [showImport, setShowImport] = useState(false);
  const [showNewCase, setShowNewCase] = useState(false);
  const [caseRoot, setCaseRoot] = useState('C:\\Cases');
  const [caseName, setCaseName] = useState('');
  const [openCasePath, setOpenCasePath] = useState('C:\\Cases\\case-001');

  const runningJob = jobs?.find((job) => job.status === 'running');
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      {!currentCase && (
        <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
          <div className="font-serif text-xl text-[#111] mb-4 tracking-tight">案件管理</div>
          <div className="flex gap-6">
            <div className="flex-1 border border-[#e0e0e0] bg-white p-4">
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
              {createCaseMutation.isError && (
                <div className="mt-2 text-[11px] text-red-600">{(createCaseMutation.error as Error)?.message}</div>
              )}
            </div>
            <div className="flex-1 border border-[#e0e0e0] bg-white p-4">
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
              {openCaseMutation.isError && (
                <div className="mt-2 text-[11px] text-red-600">{(openCaseMutation.error as Error)?.message}</div>
              )}
            </div>
          </div>
        </div>
      )}

      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="font-serif text-2xl text-[#111] mb-1 tracking-tight">案件 #{currentCase?.number}</div>
            <div className="text-[#666] font-mono text-[11px]">C:\Cases\WannaCry_Root\case.db</div>
            <div className="mt-3 flex flex-wrap gap-2 text-[10px] uppercase tracking-wider text-[#666]">
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">当前状态: 活跃</span>
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">最近导出: 2 份</span>
              <span className="border border-[#e7d9b4] bg-white px-2 py-1">告警: {warnings?.length ?? 0}</span>
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
              <div className="text-[#111] text-[13px] font-mono">{currentCase?.createdAt}</div>
            </div>
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">检验人</div>
              <div className="text-[#111] text-[13px]">{currentCase?.examiner}</div>
            </div>
            <button
              onClick={() => setShowImport(!showImport)}
              className="flex items-center gap-1.5 border border-[#111] px-3 py-1.5 text-[12px] hover:bg-[#111] hover:text-white transition-colors"
            >
              <Upload size={12} /> 导入数据源
            </button>
          </div>
        </div>
      </div>

      {showImport && (
        <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-4 shrink-0">
          <div className="flex items-center gap-3">
            <input
              type="text"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              placeholder="镜像路径 (例: E:\cases\image.E01) 或点击浏览..."
              className="flex-1 border border-[#ccc] px-3 py-1.5 text-[12px] font-mono"
            />
            <button
              onClick={async () => {
                try {
                  const path = await open({
                    filters: [{ name: 'Disk Images', extensions: ['e01', 'E01', 'dd', 'raw', 'img', 'vmdk', '*'] }],
                  });
                  if (path) setImportPath(path as string);
                } catch {
                  // mock/dev mode — dialog not available
                }
              }}
              className="border border-[#ccc] px-3 py-1.5 text-[12px] hover:bg-[#eee] flex items-center gap-1"
            >
              <FolderOpen size={12} /> 浏览
            </button>
            <button
              onClick={() => {
                if (importPath.trim()) {
                  importMutation.mutate(importPath.trim());
                }
              }}
              disabled={importMutation.isPending}
              className="bg-[#111] text-white px-4 py-1.5 text-[12px] hover:bg-[#333] disabled:opacity-50"
            >
              {importMutation.isPending ? '导入中...' : '导入'}
            </button>
            <button onClick={() => { setShowImport(false); setImportPath(''); }} className="text-[#888] text-[12px] hover:text-[#111]">
              取消
            </button>
          </div>
          {importMutation.isPending && (
            <div className="mt-2 flex items-center gap-2 text-[11px] text-[#666]">
              <div className="w-3 h-3 border-2 border-[#666] border-t-transparent rounded-full animate-spin" />
              正在导入数据源，请等待...
            </div>
          )}
          {importMutation.isSuccess && (
            <div className="mt-2 text-[11px] text-green-700 font-mono bg-green-50 border border-green-200 p-2">
              {importMutation.data}
            </div>
          )}
          {importMutation.isError && (
            <div className="mt-2 text-[11px] text-red-600 font-mono bg-red-50 border border-red-200 p-2">
              导入失败: {(importMutation.error as Error)?.message || '未知错误'}
            </div>
          )}
        </div>
      )}

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
            <span className="font-mono text-[10px] text-[#888]">完成 {completedJobs.length} / 运行 {runningJob ? 1 : 0}</span>
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
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="w-1/2 flex flex-col min-h-0 bg-[#fafafa]">
          <div className="h-8 border-b border-[#e0e0e0] bg-[#fafafa] flex items-center justify-between px-4 text-[11px] font-semibold uppercase text-[#555] tracking-wider shrink-0">
            <span>高价值对象</span>
            <span className="font-mono text-[10px] text-[#888]">最近发现 {recentObjects?.length ?? 0} 项</span>
          </div>
          <div className="flex-1 overflow-auto">
            <div className="flex flex-col border-b border-[#e0e0e0]">
              {recentObjects?.map((item) => (
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
              ))}
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
  lines = [],
}: {
  icon: React.ReactNode;
  title: string;
  value: number;
  lines?: Array<[string, string]>;
}) {
  return (
    <div className="p-4 flex flex-col gap-3 bg-white">
      <div className="text-[#888] text-[11px] uppercase tracking-wider flex items-center gap-1.5">
        {icon} {title}
      </div>
      <div className="text-2xl font-serif text-[#111]">{value.toLocaleString()}</div>
      <div className="text-[#666] font-mono text-[10px] space-y-1 mt-1">
        {lines.map(([label, amount]) => (
          <div key={label} className="flex justify-between">
            <span>{label}</span>
            <span className="text-[#888]">{amount}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
