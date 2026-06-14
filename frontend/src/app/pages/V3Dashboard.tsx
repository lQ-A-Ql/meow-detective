import type { LucideIcon } from 'lucide-react';
import { Activity, BarChart3, GitBranch, Layers, RefreshCw, Shield } from 'lucide-react';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/components/analysis/AnalysisPanels';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { useTimelineEvents } from '@/features/timeline/hooks';
import { useArtifactFamilyCounts } from '@/features/artifacts/hooks';
import { useCorrelationSnapshot } from '@/features/analysis/hooks';
import { Button } from '@/app/components/ui/button';
import { isApiErrorDto } from '@/lib/api/client';

function errorMessage(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function StatCard({
  title,
  value,
  subtitle,
  icon: Icon,
}: {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: LucideIcon;
}) {
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4">
      <div className="flex items-center justify-between">
        <div className="font-mono text-[11px] uppercase tracking-wider text-[#888]">{title}</div>
        {Icon ? <Icon size={16} className="text-[#aaa]" /> : null}
      </div>
      <div className="mt-1 font-mono text-2xl font-semibold text-[#111]">{value}</div>
      {subtitle ? <div className="mt-0.5 text-[11px] text-[#666]">{subtitle}</div> : null}
    </div>
  );
}

function BreakdownList({
  title,
  entries,
  formatValue = (v: number) => String(v),
}: {
  title: string;
  entries: Record<string, number>;
  formatValue?: (v: number) => string;
}) {
  const sorted = Object.entries(entries).sort(([, a], [, b]) => b - a);
  if (sorted.length === 0) {
    return null;
  }
  return (
    <div className="rounded border border-[#e0e0e0] bg-white p-4">
      <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">{title}</div>
      <div className="space-y-1">
        {sorted.map(([key, count]) => (
          <div key={key} className="flex items-center justify-between text-xs">
            <span className="font-mono text-[#111]">{key}</span>
            <span className="font-mono text-[#666]">{formatValue(count)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function SectionHeader({
  icon: Icon,
  title,
  subtitle,
}: {
  icon: LucideIcon;
  title: string;
  subtitle?: string;
}) {
  return (
    <div className="flex items-center gap-3 border-b border-[#eee] pb-2">
      <Icon size={18} className="text-[#555]" />
      <div>
        <div className="font-serif text-[15px] text-[#111]">{title}</div>
        {subtitle ? <div className="text-[11px] text-[#888]">{subtitle}</div> : null}
      </div>
    </div>
  );
}

export function V3Dashboard() {
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const caseId = currentCase.data?.id ?? '';
  const graph = useGraphSnapshot(caseId);
  const timeline = useTimelineEvents({ limit: 1 });
  const artifactCounts = useArtifactFamilyCounts();
  const correlation = useCorrelationSnapshot();

  const hasCase = Boolean(currentCase.data);
  const loading =
    currentCase.isLoading || graph.isLoading || timeline.isLoading || artifactCounts.isLoading || correlation.isLoading;
  const error = currentCase.error ?? graph.error ?? timeline.error ?? artifactCounts.error ?? correlation.error;

  async function refresh() {
    await Promise.all([
      graph.refetch(),
      timeline.refetch(),
      artifactCounts.refetch(),
      correlation.refetch(),
    ]);
  }

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-auto bg-white">
      <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-[#111]">V3 治理台</div>
            <div className="mt-1 font-mono text-[11px] text-[#666]">
              图统计 / 平台覆盖 / 规则包状态 / 批处理状态
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={refresh}
              disabled={!hasCase || loading}
              className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
            >
              <RefreshCw size={14} className={graph.isFetching || timeline.isFetching ? 'animate-spin' : ''} />
              刷新
            </Button>
          </div>
        </div>
      </div>

      {!hasCase && currentCase.isSuccess ? (
        <AnalysisEmptyState demoPending={false} onLoadDemoCase={() => {}} />
      ) : loading ? (
        <AnalysisLoadingPanel text="正在加载 V3 治理快照..." />
      ) : (
        <div className="flex-1 space-y-6 overflow-auto p-6">
          {error ? <AnalysisErrorBanner message={errorMessage(error)} onRetry={refresh} /> : null}

          {/* Graph Statistics */}
          <section>
            <SectionHeader icon={GitBranch} title="图统计" subtitle="节点与边按类型分布" />
            <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-5">
              {graph.data ? (
                <>
                  <StatCard title="节点总数" value={graph.data.totalNodes} icon={GitBranch} />
                  <StatCard title="边总数" value={graph.data.totalEdges} icon={Activity} />
                  <StatCard title="密度" value={graph.data.density.toFixed(4)} icon={Layers} />
                  <StatCard title="最大联通分量" value={graph.data.largestComponentSize} icon={BarChart3} />
                  <StatCard
                    title="节点类型数"
                    value={Object.keys(graph.data.nodeCountByType).length}
                    icon={Shield}
                  />
                </>
              ) : null}
            </div>
            {graph.data ? (
              <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                <BreakdownList title="按节点类型" entries={graph.data.nodeCountByType} />
                <BreakdownList title="按边类型" entries={graph.data.edgeCountByType} />
              </div>
            ) : null}
          </section>

          {/* Import / Data Source Stats */}
          <section>
            <SectionHeader icon={Layers} title="数据源覆盖" subtitle="导入来源及分区" />
            <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
              <StatCard
                title="数据源数量"
                value={dataSources?.length ?? 0}
                icon={Layers}
              />
              <StatCard
                title="分区总数"
                value={dataSources?.reduce((sum, ds) => sum + ds.partitions.length, 0) ?? 0}
                icon={GitBranch}
              />
              <StatCard
                title="E01 源"
                value={dataSources?.filter((ds) => ds.kind === 'e01').length ?? 0}
                subtitle="EWF 格式"
              />
              <StatCard
                title="RAW 源"
                value={dataSources?.filter((ds) => ds.kind === 'raw').length ?? 0}
                subtitle="原始镜像"
              />
            </div>
            {dataSources && dataSources.length > 0 ? (
              <div className="mt-3 rounded border border-[#e0e0e0] bg-white p-4">
                <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">源明细</div>
                <div className="space-y-2">
                  {dataSources.map((ds) => (
                    <div
                      key={ds.id}
                      className="flex items-center justify-between border-b border-[#f0f0f0] pb-2 text-xs last:border-none last:pb-0"
                    >
                      <div>
                        <span className="font-mono text-[#111]">{ds.name}</span>
                        <span className="ml-2 font-mono text-[#aaa]">{ds.kind}</span>
                      </div>
                      <div className="flex items-center gap-4 font-mono text-[#666]">
                        {ds.fileCount !== undefined ? <span>{ds.fileCount} 文件</span> : null}
                        <span>{ds.partitions.length} 分区</span>
                        {ds.readerKind ? <span className="text-[#888]">{ds.readerKind}</span> : null}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </section>

          {/* Timeline Stats */}
          <section>
            <SectionHeader icon={Activity} title="时间线概览" subtitle="事件总量与类型分布" />
            <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
              <StatCard
                title="事件总量"
                value={timeline.data?.total ?? 0}
                icon={Activity}
              />
              <StatCard
                title="时间线就绪"
                value={timeline.isSuccess ? 'Yes' : 'No'}
              />
              <StatCard
                title="时间线状态"
                value={timeline.isLoading ? 'Loading' : timeline.isError ? 'Error' : 'Ready'}
              />
            </div>
          </section>

          {/* Artifact Stats */}
          <section>
            <SectionHeader icon={Shield} title="痕迹统计" subtitle="Windows 痕迹家族枚举" />
            <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
              <StatCard
                title="家族数量"
                value={artifactCounts.data?.length ?? 0}
                icon={Shield}
              />
              <StatCard
                title="记录总量"
                value={artifactCounts.data?.reduce((sum, a) => sum + a.count, 0) ?? 0}
                icon={BarChart3}
              />
            </div>
            {artifactCounts.data && artifactCounts.data.length > 0 ? (
              <div className="mt-3 rounded border border-[#e0e0e0] bg-white p-4">
                <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">家族明细</div>
                <div className="flex flex-wrap gap-2">
                  {artifactCounts.data.map((a) => (
                    <div
                      key={a.family}
                      className="flex items-center gap-1.5 rounded border border-[#e0e0e0] bg-[#fafafa] px-2 py-1 text-xs"
                    >
                      <span className="font-mono text-[#111]">{a.family}</span>
                      <span className="font-mono text-[#888]">{a.count}</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </section>

          {/* Correlation Stats */}
          <section>
            <SectionHeader icon={BarChart3} title="关联统计" subtitle="关联分析快照" />
            <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
              <StatCard
                title="关联节点"
                value={correlation.data?.nodeCount ?? 0}
                icon={GitBranch}
              />
              <StatCard
                title="关联边"
                value={correlation.data?.edgeCount ?? 0}
                icon={Activity}
              />
              <StatCard
                title="聚合簇"
                value={correlation.data?.clusterCount ?? 0}
                icon={Layers}
              />
              <StatCard
                title="线索数"
                value={correlation.data?.leadCount ?? 0}
                icon={Shield}
              />
            </div>
            {correlation.data?.familyCoverage && correlation.data.familyCoverage.length > 0 ? (
              <div className="mt-3 rounded border border-[#e0e0e0] bg-white p-4">
                <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">家族覆盖</div>
                <div className="space-y-1">
                  {correlation.data.familyCoverage.map((fc, i) => (
                    <div key={fc.family ?? i} className="flex items-center justify-between text-xs">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[#111]">{fc.displayName}</span>
                        <span
                          className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${
                            fc.status === 'covered'
                              ? 'bg-[#e6f7e6] text-[#2d7d2d]'
                              : fc.status === 'review'
                                ? 'bg-[#fff3cd] text-[#856404]'
                                : fc.status === 'missing'
                                  ? 'bg-[#f8d7da] text-[#721c24]'
                                  : 'bg-[#f0f0f0] text-[#666]'
                          }`}
                        >
                          {fc.status}
                        </span>
                      </div>
                      <div className="flex items-center gap-3 font-mono text-[#666]">
                        <span>{fc.leadCount} 线索</span>
                        <span>{fc.clusterCount} 簇</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </section>

          {/* Platform Coverage (placeholder) */}
          <section>
            <SectionHeader icon={Layers} title="平台覆盖" subtitle="文件系统 / 镜像格式 / 痕迹家族" />
            <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
              平台覆盖矩阵将在规则包导入后动态生成。
            </div>
          </section>

          {/* Rule Pack Status (placeholder) */}
          <section>
            <SectionHeader icon={Shield} title="规则包状态" subtitle="规则版本、覆盖率、校验和" />
            <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
              规则包管理将在后续版本中实现。
            </div>
          </section>

          {/* Batch Status (placeholder) */}
          <section>
            <SectionHeader icon={Activity} title="批处理状态" subtitle="批量导入、批量提取、批量报告" />
            <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
              批处理状态将在后续版本中实现。
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
