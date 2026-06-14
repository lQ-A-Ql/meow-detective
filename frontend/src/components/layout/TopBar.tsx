import { NavLink, useLocation, useNavigate } from 'react-router';
import { Search, Activity, Settings, AlertTriangle } from 'lucide-react';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  deriveEvidenceHashStatus,
  getCacheStateLabel,
  getEvidenceHashCaveatText,
  getEvidenceHashStatusLabel,
  getFreshnessLabel,
  getImportPhaseLabel,
  getImportPhaseStateLabel,
  getPartialKindLabel,
  useImportEventState,
} from '@/features/jobs/import-event-state';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import { apiMode } from '@/lib/api/client';
import { useUiStore } from '@/stores/ui-store';

const links = [
  { to: '/', label: '案件概览', page: 'home' as const, context: '案件状态、指标与近期对象' },
  { to: '/files', label: '文件浏览', page: 'files' as const, context: '目录树、文件表与取证查看器' },
  { to: '/analysis', label: '数据源分析', page: 'analysis' as const, context: '系统信息、文件分类与分析报告' },
  { to: '/v2', label: 'V2 治理', page: 'v2' as const, context: '可信验证、Benchmark、安全治理与发布评分' },
  { to: '/v3', label: 'V3', page: 'v3' as const, context: '图统计、平台覆盖、规则包状态与批处理状态' },
  { to: '/search', label: '全局搜索', page: 'search' as const, context: '关键字、结构化查询与命中详情' },
  { to: '/timeline', label: '时间线', page: 'timeline' as const, context: '事件聚合、筛选与时序检视' },
  { to: '/artifacts', label: '痕迹分析', page: 'artifacts' as const, context: 'Windows 痕迹家族与解析字段' },
  { to: '/reports', label: '报告导出', page: 'reports' as const, context: '模板、导出任务与报告产物' },
];

export function TopBar() {
  const navigate = useNavigate();
  const location = useLocation();
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const currentPage = useUiStore((state) => state.currentPage);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const globalSearchQuery = useUiStore((state) => state.globalSearchQuery);
  const setGlobalSearchQuery = useUiStore((state) => state.setGlobalSearchQuery);
  const currentApiMode = apiMode();
  const isMockMode = currentApiMode === 'mock';
  const importSignals = useImportEventState();

  const runningCount = jobs?.filter((job) => job.status === 'running').length ?? 0;
  const warningCount = warnings?.length ?? 0;
  const partialCount = importSignals.partialResults.length;
  const freshestPartial = importSignals.partialResults[0];
  const cacheSummary = importSignals.cacheStatuses[0];
  const cancellation = importSignals.latestCancellation;
  const phase = importSignals.latestPhase;
  const report = importSignals.latestReport;
  const evidenceHashStatus = deriveEvidenceHashStatus(importSignals.partialResults, dataSources ?? []);
  const activeLink =
    links.find((link) => link.to === location.pathname)
    ?? links.find((link) => link.page === currentPage)
    ?? links[0];

  return (
    <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] px-4 py-2 text-xs">
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-6">
          <div className="flex items-center gap-5 min-w-0">
            {links.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                onClick={() => setCurrentPage(link.page)}
                className={({ isActive }) =>
                  `whitespace-nowrap hover:text-black ${isActive ? 'text-black font-semibold' : 'text-[#666]'}`
                }
              >
                {link.label}
              </NavLink>
            ))}
          </div>
          <div className="hidden xl:flex items-center gap-2 min-w-0 border-l border-[#e0e0e0] pl-4">
            <span className="text-[10px] uppercase tracking-wider text-[#888]">当前页</span>
            <span className="text-[11px] text-[#111] font-medium">{activeLink.context}</span>
          </div>
        </div>

        <div className="hidden lg:flex min-w-0 items-center gap-2 text-[12px] text-[#111]">
          <span className="font-serif">案件 #{currentCase?.number ?? '----'}</span>
          <span className="max-w-[220px] truncate text-[#666]">{currentCase?.name ?? '未选择案件'}</span>
          <span className="text-[#ccc]">|</span>
          <span className="text-[#666]">检验人 {currentCase?.examiner ?? '-'}</span>
          <span className="text-[#ccc]">|</span>
          <span className="font-mono text-[#888]">更新于 {currentCase?.updatedAt ?? '-'}</span>
        </div>

        <div className="flex shrink-0 items-center gap-3 text-[#666]">
          {isMockMode ? (
            <div
              role="status"
              aria-label="Mock mode data label"
              className="flex items-center gap-2 border border-[#d7c7a0] bg-[#fff8e8] px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-[#7a5600]"
            >
              <span className="text-[#111]">Mock Mode</span>
              <span className="font-mono text-[#7a5600]">显示演示取证数据</span>
            </div>
          ) : null}
          {phase ? (
            <SignalChip
              label="Import"
              value={`${getImportPhaseLabel(phase.phase)} ${phase.percent}%`}
              detail={`${getImportPhaseStateLabel(phase.state)} · ${phase.detail}`}
            />
          ) : null}
          {cancellation ? (
            <SignalChip
              label="Cancel"
              value={cancellation.safeToClose ? 'Safe To Close' : getCacheStateLabel(cancellation.state)}
              detail={cancellation.detail}
            />
          ) : null}
          {freshestPartial ? (
            <SignalChip
              label="Partial"
              value={`${getFreshnessLabel(freshestPartial.freshness)} ${partialCount}`}
              detail={`${getPartialKindLabel(freshestPartial.kind)} ${freshestPartial.readyCount}${freshestPartial.totalEstimate ? `/${freshestPartial.totalEstimate}` : ''}`}
            />
          ) : null}
          {evidenceHashStatus ? (
            <SignalChip
              label="Hash"
              value={getEvidenceHashStatusLabel(evidenceHashStatus)}
              detail={getEvidenceHashCaveatText(evidenceHashStatus)}
            />
          ) : null}
          {cacheSummary ? (
            <SignalChip
              label="Cache"
              value={getCacheStateLabel(cacheSummary.state)}
              detail={cacheSummary.message ?? cacheSummary.cacheKey}
            />
          ) : null}
          {report ? (
            <SignalChip
              label="Perf"
              value={`${report.summary.elapsedMs}ms`}
              detail={report.summary.summary}
            />
          ) : null}
          <div className="flex items-center gap-2 border border-[#e0e0e0] bg-white px-2 py-1 rounded-sm">
            <Search size={12} className="text-[#888]" />
            <input
              value={globalSearchQuery}
              onChange={(event) => setGlobalSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && globalSearchQuery.trim()) {
                  navigate(`/search?q=${encodeURIComponent(globalSearchQuery.trim())}`);
                }
              }}
              className="w-40 xl:w-56 bg-transparent border-none outline-none text-[#111] placeholder-[#aaa] text-xs font-mono"
              placeholder="输入全局检索语句或 IOC"
            />
          </div>
          <button
            onClick={toggleDrawer}
            className="flex items-center gap-1.5 border border-transparent px-2 py-1 hover:border-[#d8d8d8] hover:bg-white text-[#555] hover:text-black"
          >
            <Activity size={12} />
            <span>{runningCount} 运行中</span>
            {warningCount > 0 ? (
              <span className="flex items-center gap-1 text-[#9a6700]">
                <AlertTriangle size={11} /> {warningCount}
              </span>
            ) : null}
          </button>
          <div className="h-4 border-l border-[#e0e0e0]" />
          <Settings size={14} className="cursor-pointer hover:text-black" onClick={() => navigate('/settings')} />
        </div>
      </div>
    </div>
  );
}

function SignalChip({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div
      className="hidden 2xl:flex max-w-[220px] items-center gap-2 border border-[#e0e0e0] bg-white px-2 py-1"
      title={detail}
    >
      <span className="text-[10px] uppercase tracking-wider text-[#888]">{label}</span>
      <span className="truncate text-[11px] font-medium text-[#111]">{value}</span>
    </div>
  );
}
