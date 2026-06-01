import { Terminal, AlertCircle, ChevronUp, ChevronDown, Clock3 } from 'lucide-react';
import { useJobsSnapshot, useTraceItems, useWarnings } from '@/features/jobs/hooks';
import { apiMode } from '@/lib/api/client';
import { useUiStore } from '@/stores/ui-store';
import type { JobSnapshot } from '@/types/models';

export function BottomDrawer() {
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const { data: trace } = useTraceItems();
  const drawerOpen = useUiStore((state) => state.drawerOpen);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);

  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const failedJobs = jobs?.filter((job) => job.status === 'failed') ?? [];
  const partialJobs = jobs?.filter((job) => job.partial) ?? [];
  const jobWarningCount = jobs?.reduce((sum, job) => sum + job.warningCount, 0) ?? 0;
  const jobSkippedCount = jobs?.reduce((sum, job) => sum + job.skippedCount, 0) ?? 0;
  const runningCount = runningJobs.length;
  const currentApiMode = apiMode();
  const headline =
    runningJobs[0]?.detail ||
    failedJobs[0]?.detail ||
    completedJobs[0]?.detail ||
    '等待任务执行';

  return (
    <div
      className={`shrink-0 border-t border-[#e0e0e0] bg-[#fafafa] z-10 transition-[height] duration-150 ${drawerOpen ? 'h-56' : 'h-8'}`}
    >
      <div className="h-8 flex items-center px-4 text-[#666] text-[11px] font-mono justify-between">
        <div className="flex items-center gap-4 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <Terminal size={12} className="text-[#888]" />
            <span className="truncate">[JOBS] {headline}</span>
          </div>
          <div className="px-3 border-l border-[#e0e0e0] flex items-center gap-3 text-[#555]">
            <span>
              <span className="text-[#111]">{runningCount}</span> 运行中
            </span>
            <span>
              <span className="text-[#111]">{(warnings?.length ?? 0) + jobWarningCount}</span> 警告
            </span>
            <span>
              <span className="text-[#111]">{jobSkippedCount}</span> 跳过
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
            API: <span className="text-[#111] uppercase">{currentApiMode}</span>
          </div>
          <div className="border-l border-[#e0e0e0] pl-4">
            最近状态: <span className="text-[#111]">{runningJobs[0]?.scope || failedJobs[0]?.scope || completedJobs[0]?.scope || '空闲'}</span>
          </div>
        </div>
      </div>
      {drawerOpen ? (
        <div className="grid h-[calc(100%-2rem)] grid-cols-3 border-t border-[#e0e0e0]">
          <div className="overflow-auto border-r border-[#e0e0e0] p-3">
            <div className="mb-2 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-[#555]">
              <span>JOBS</span>
              <span className="font-mono text-[#888]">
                {runningCount} 运行 / {completedJobs.length} 完成 / {partialJobs.length} 部分 / {failedJobs.length} 失败
              </span>
            </div>
            <div className="space-y-3">
              {runningJobs.map((job) => (
                <div key={job.id} className="border border-[#e0e0e0] bg-white p-3 text-[11px]">
                  <div className="flex items-center justify-between gap-3 text-[#111]">
                    <span className="font-medium">{job.name}</span>
                    <span className="text-[#888]">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-[#666]">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                  {job.currentPartition ? (
                    <div className="mt-2 border border-[#ececec] bg-[#fafafa] px-2 py-2">
                      <div className="flex items-center justify-between gap-3 text-[10px] uppercase tracking-wider text-[#666]">
                        <span>分区进度</span>
                        <span className="font-mono text-[#111]">
                          {(job.completedPartitions ?? 0)}/{job.totalPartitions ?? '?'} 完成
                        </span>
                      </div>
                      <div className="mt-1.5 flex items-center gap-2">
                        <div className="flex-1 h-1.5 overflow-hidden border border-[#e0e0e0] bg-white">
                          <div
                            className="h-full transition-all duration-300"
                            style={{
                              width: `${job.partitionProgress ?? 0}%`,
                              backgroundColor: (job.partitionProgress ?? 0) >= 100 ? '#22c55e' : '#666',
                            }}
                          />
                        </div>
                        <span className="text-[10px] font-mono text-[#555] w-8 text-right">
                          {job.partitionProgress ?? 0}%
                        </span>
                      </div>
                      <div className="mt-1 text-[11px] text-[#333] font-medium">
                        {job.currentPartition}
                      </div>
                    </div>
                  ) : null}
                  <div className="mt-2 flex items-center gap-2">
                    <div className="flex-1 h-1 overflow-hidden border border-[#e0e0e0] bg-[#eee]">
                      <div className="h-full bg-[#111]" style={{ width: `${job.progress}%` }} />
                    </div>
                    <span className="text-[10px] font-mono text-[#888]">{job.progress}%</span>
                  </div>
                </div>
              ))}
              {completedJobs.map((job) => (
                <div key={job.id} className="border-b border-[#ececec] pb-2 text-[11px] text-[#555]">
                  <div className="flex items-center justify-between gap-3">
                    <span className="flex items-center gap-2">
                      {job.name}
                      {job.partial ? (
                        <span className="border border-[#e7d9b4] bg-[#fff9ec] px-1.5 py-0.5 text-[9px] font-semibold text-[#8a5a00]">
                          PARTIAL
                        </span>
                      ) : null}
                    </span>
                    <span className="text-[#888]">{job.detail}</span>
                  </div>
                  <div className="mt-1 text-[#888]">{job.scope}</div>
                  <JobOutcomeBadges job={job} />
                </div>
              ))}
              {failedJobs.map((job) => (
                <div key={job.id} className="border border-red-200 bg-red-50 p-3 text-[11px] text-red-700">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{job.name}</span>
                    <span>{job.detail}</span>
                  </div>
                  <div className="mt-1 text-red-600/80">{job.scope || '任务执行失败'}</div>
                  <JobOutcomeBadges job={job} />
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

function JobOutcomeBadges({ job }: { job: JobSnapshot }) {
  if (!job.partial && job.warningCount === 0 && job.skippedCount === 0 && job.failedCount === 0) {
    return null;
  }

  return (
    <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[10px] font-mono">
      {job.partial ? (
        <span className="border border-[#e7d9b4] bg-[#fff9ec] px-1.5 py-0.5 font-semibold text-[#8a5a00]">
          PARTIAL
        </span>
      ) : null}
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
  );
}
