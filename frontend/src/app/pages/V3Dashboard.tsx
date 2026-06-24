import { Activity, BarChart3, GitBranch, Layers, Monitor, Globe, Server, Apple, RefreshCw, Shield } from 'lucide-react';
import { AnalysisEmptyState, AnalysisErrorBanner, AnalysisLoadingPanel } from '@/components/analysis/AnalysisPanels';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { useTimelineEvents } from '@/features/timeline/hooks';
import { useArtifactFamilyCounts } from '@/features/artifacts/hooks';
import { useCorrelationSnapshot, useV3GovernanceSnapshot } from '@/features/analysis/hooks';
import { Button } from '@/app/components/ui/button';
import { errorMessage, StatCard, BreakdownList, SectionHeader } from './V3ScoreCards';

export function V3Dashboard() {
  const currentCase = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const caseId = currentCase.data?.id ?? '';
  const graph = useGraphSnapshot(caseId);
  const timeline = useTimelineEvents({ limit: 1 });
  const artifactCounts = useArtifactFamilyCounts();
  const correlation = useCorrelationSnapshot();
  const v3Governance = useV3GovernanceSnapshot();

  const hasCase = Boolean(currentCase.data);
  const loading =
    currentCase.isLoading || graph.isLoading || timeline.isLoading || artifactCounts.isLoading || correlation.isLoading || v3Governance.isLoading;
  const error = currentCase.error ?? graph.error ?? timeline.error ?? artifactCounts.error ?? correlation.error ?? v3Governance.error;

  async function refresh() {
    await Promise.all([
      graph.refetch(),
      timeline.refetch(),
      artifactCounts.refetch(),
      correlation.refetch(),
      v3Governance.refetch(),
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
                value={dataSources?.reduce((sum, ds) => sum + (ds.partitions?.length ?? 0), 0) ?? 0}
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
                        <span>{ds.partitions?.length ?? 0} 分区</span>
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

          {/* Platform Coverage */}
          <section>
            <SectionHeader icon={Layers} title="平台覆盖" subtitle="痕迹家族按目标平台分布" />
            {v3Governance.data?.platformCoverage ? (
              <>
                <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
                  <StatCard
                    title="Windows"
                    value={v3Governance.data.platformCoverage.windowsArtifactFamilies}
                    subtitle="个家族"
                    icon={Monitor}
                  />
                  <StatCard
                    title="Linux"
                    value={v3Governance.data.platformCoverage.linuxArtifactFamilies}
                    subtitle="个家族"
                    icon={Server}
                  />
                  <StatCard
                    title="macOS"
                    value={v3Governance.data.platformCoverage.macosArtifactFamilies}
                    subtitle="个家族"
                    icon={Apple}
                  />
                  <StatCard
                    title="跨平台"
                    value={v3Governance.data.platformCoverage.crossPlatformArtifactFamilies}
                    subtitle="个家族"
                    icon={Globe}
                  />
                </div>
                <div className="mt-3 rounded border border-[#e0e0e0] bg-white p-4">
                  <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">家族明细</div>
                  <div className="space-y-3">
                    {v3Governance.data.platformCoverage.windowsFamilies.length > 0 && (
                      <div>
                        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-[#555]">
                          <Monitor size={12} /> Windows
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {v3Governance.data.platformCoverage.windowsFamilies.map((f) => (
                            <span key={f} className="rounded border border-[#e0e0e0] bg-[#f5f5f5] px-2 py-0.5 font-mono text-[10px] text-[#333]">
                              {f}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    {v3Governance.data.platformCoverage.crossPlatformFamilies.length > 0 && (
                      <div>
                        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-[#555]">
                          <Globe size={12} /> 跨平台
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {v3Governance.data.platformCoverage.crossPlatformFamilies.map((f) => (
                            <span key={f} className="rounded border border-[#e0e0e0] bg-[#e8f4fd] px-2 py-0.5 font-mono text-[10px] text-[#1a5c8a]">
                              {f}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    {v3Governance.data.platformCoverage.linuxFamilies.length > 0 && (
                      <div>
                        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-[#555]">
                          <Server size={12} /> Linux
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {v3Governance.data.platformCoverage.linuxFamilies.map((f) => (
                            <span key={f} className="rounded border border-[#e0e0e0] bg-[#f5f5f5] px-2 py-0.5 font-mono text-[10px] text-[#333]">
                              {f}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    {v3Governance.data.platformCoverage.macosFamilies.length > 0 && (
                      <div>
                        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-[#555]">
                          <Apple size={12} /> macOS
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {v3Governance.data.platformCoverage.macosFamilies.map((f) => (
                            <span key={f} className="rounded border border-[#e0e0e0] bg-[#f5f5f5] px-2 py-0.5 font-mono text-[10px] text-[#333]">
                              {f}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              </>
            ) : (
              <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
                暂无平台覆盖数据。导入数据源并运行痕迹提取后生成。
              </div>
            )}
          </section>

          {/* Rule Pack Status */}
          <section>
            <SectionHeader icon={Shield} title="规则包状态" subtitle="规则版本、覆盖率、执行状态" />
            {v3Governance.data?.rulePackCoverage ? (
              <>
                <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
                  <StatCard
                    title="已加载规则包"
                    value={v3Governance.data.rulePackCoverage.loadedPacks.length}
                    icon={Shield}
                  />
                  <StatCard
                    title="规则总数"
                    value={v3Governance.data.rulePackCoverage.totalRuleCount}
                    icon={BarChart3}
                  />
                  <StatCard
                    title="执行状态"
                    value={v3Governance.data.rulePackCoverage.executionStatus}
                    icon={Activity}
                  />
                </div>
                {v3Governance.data.rulePackCoverage.loadedPacks.length > 0 && (
                  <div className="mt-3 rounded border border-[#e0e0e0] bg-white p-4">
                    <div className="mb-2 font-mono text-[11px] uppercase tracking-wider text-[#888]">已加载规则包</div>
                    <div className="space-y-1">
                      {v3Governance.data.rulePackCoverage.loadedPacks.map((pack) => (
                        <div key={pack.name} className="flex items-center justify-between text-xs">
                          <div className="flex items-center gap-2">
                            <span className="font-mono text-[#111]">{pack.name}</span>
                            <span className="font-mono text-[#aaa]">v{pack.version}</span>
                          </div>
                          <div className="flex items-center gap-3 font-mono text-[#666]">
                            <span>{pack.ruleCount} 规则</span>
                            <span className="text-[#888]">{pack.author}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </>
            ) : (
              <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
                规则包数据将在导入数据源后加载。
              </div>
            )}
          </section>

          {/* Batch Status */}
          <section>
            <SectionHeader icon={Activity} title="批处理状态" subtitle="批量导入、批量提取、批量报告" />
            {v3Governance.data?.batchStatus ? (
              <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-5">
                <StatCard
                  title="进行中"
                  value={v3Governance.data.batchStatus.activeJobs}
                  icon={Activity}
                />
                <StatCard
                  title="已完成"
                  value={v3Governance.data.batchStatus.completedJobs}
                  icon={Shield}
                />
                <StatCard
                  title="失败"
                  value={v3Governance.data.batchStatus.failedJobs}
                  icon={BarChart3}
                />
                <StatCard
                  title="排队中"
                  value={v3Governance.data.batchStatus.queuedJobs}
                  icon={Layers}
                />
                <StatCard
                  title="总计"
                  value={v3Governance.data.batchStatus.totalJobs}
                  icon={GitBranch}
                />
              </div>
            ) : (
              <div className="mt-3 rounded border border-dashed border-[#ccc] bg-[#fafafa] p-6 text-center text-[12px] text-[#999]">
                暂无批处理作业。
              </div>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
