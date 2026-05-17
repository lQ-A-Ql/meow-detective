import { Terminal, AlertCircle, ChevronUp, ChevronDown, Clock3 } from 'lucide-react';
import { useJobsSnapshot, useTraceItems, useWarnings } from '@/features/jobs/hooks';
import { useUiStore } from '@/stores/ui-store';

export function BottomDrawer() {
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const { data: trace } = useTraceItems();
  const drawerOpen = useUiStore((state) => state.drawerOpen);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);

  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const runningCount = runningJobs.length;

  return (
    <div
      className={`shrink-0 border-t border-[#e0e0e0] bg-[#fafafa] z-10 transition-[height] duration-150 ${drawerOpen ? 'h-56' : 'h-8'}`}
    >
      <div className="h-8 flex items-center px-4 text-[#666] text-[11px] font-mono justify-between">
        <div className="flex items-center gap-4 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <Terminal size={12} className="text-[#888]" />
            <span className="truncate">[SYSTEM] 运行时缓存稳定，当前案件对象索引可用。</span>
          </div>
          <div className="px-3 border-l border-[#e0e0e0] flex items-center gap-3 text-[#555]">
            <span>
              <span className="text-[#111]">{runningCount}</span> 运行中
            </span>
            <span>
              <span className="text-[#111]">{warnings?.length ?? 0}</span> 警告
            </span>
            <span>
              <span className="text-[#111]">{trace?.length ?? 0}</span> Trace
            </span>
          </div>
        </div>
        <div className="flex items-center gap-4">
          <button onClick={toggleDrawer} className="flex items-center gap-1.5 text-[#555] hover:text-[#111]">
            <span>{drawerOpen ? '收起任务抽屉' : '展开任务抽屉'}</span>
            {drawerOpen ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          </button>
          <div>
            数据库大小: <span className="text-[#111]">1.2 GB</span>
          </div>
          <div className="border-l border-[#e0e0e0] pl-4">
            内存: <span className="text-[#111]">4.2GB</span> / CPU: <span className="text-[#111]">12%</span>
          </div>
        </div>
      </div>
      {drawerOpen ? (
        <div className="grid h-[calc(100%-2rem)] grid-cols-3 border-t border-[#e0e0e0]">
          <div className="overflow-auto border-r border-[#e0e0e0] p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-[#555]">
              <span>JOBS</span>
              <span className="font-mono text-[#888]">{runningCount} 运行 / {completedJobs.length} 完成</span>
            </div>
            <div className="space-y-3">
              {runningJobs.map((job) => (
                <div key={job.id} className="border border-[#e0e0e0] bg-white p-3 text-[11px]">
                  <div className="flex items-center justify-between gap-3 text-[#111]">
                    <span className="font-medium">{job.name}</span>
                    <span className="text-[#888]">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-[#666]">{job.scope}</div>
                  <div className="mt-2 h-1 overflow-hidden border border-[#e0e0e0] bg-[#eee]">
                    <div className="h-full bg-[#111]" style={{ width: `${job.progress}%` }} />
                  </div>
                </div>
              ))}
              {completedJobs.map((job) => (
                <div key={job.id} className="border-b border-[#ececec] pb-2 text-[11px] text-[#555]">
                  <div className="flex items-center justify-between gap-3">
                    <span>{job.name}</span>
                    <span className="text-[#888]">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-[#888]">{job.scope}</div>
                </div>
              ))}
            </div>
          </div>
          <div className="overflow-auto border-r border-[#e0e0e0] p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-[#555]">
              <span>WARNINGS</span>
              <span className="font-mono text-[#888]">{warnings?.length ?? 0} 条</span>
            </div>
            <div className="space-y-2">
              {warnings?.map((warning) => (
                <div key={warning.id} className="border border-[#e7d9b4] bg-white p-3 text-[11px]">
                  <div className="flex items-start gap-2 text-[#111]">
                    <AlertCircle size={12} className="mt-0.5 text-[#b7791f] shrink-0" />
                    <div>
                      <div className="font-medium">{warning.title}</div>
                      <div className="mt-1 text-[#666]">{warning.detail}</div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
          <div className="overflow-auto p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-[#555]">
              <span>TRACE</span>
              <span className="font-mono text-[#888]">最近事件流</span>
            </div>
            <div className="space-y-2 text-[11px]">
              {trace?.map((item) => (
                <div key={item.id} className="border-b border-[#ececec] pb-2 text-[#555] flex gap-2">
                  <Clock3 size={11} className="mt-0.5 shrink-0 text-[#999]" />
                  <div>
                    <div className="text-[#888] font-mono">{item.ts}</div>
                    <div>{item.message}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
