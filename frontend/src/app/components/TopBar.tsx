import { NavLink } from 'react-router';
import { Search, Activity, Settings, AlertTriangle } from 'lucide-react';
import { useCurrentCase } from '@/features/case/hooks';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import { useUiStore } from '@/stores/ui-store';

const links = [
  { to: '/', label: '案件概览', page: 'home' as const, context: '案件状态、指标与近期对象' },
  { to: '/files', label: '文件浏览', page: 'files' as const, context: '目录树、文件表与取证查看器' },
  { to: '/search', label: '全局搜索', page: 'search' as const, context: '关键字、结构化查询与命中详情' },
  { to: '/timeline', label: '时间线', page: 'timeline' as const, context: '事件聚合、筛选与时序检视' },
  { to: '/artifacts', label: '痕迹分析', page: 'artifacts' as const, context: 'Windows 痕迹家族与解析字段' },
  { to: '/reports', label: '报告导出', page: 'reports' as const, context: '模板、导出任务与报告产物' },
];

export function TopBar() {
  const { data: currentCase } = useCurrentCase();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);
  const currentPage = useUiStore((state) => state.currentPage);
  const setCurrentPage = useUiStore((state) => state.setCurrentPage);
  const globalSearchQuery = useUiStore((state) => state.globalSearchQuery);
  const setGlobalSearchQuery = useUiStore((state) => state.setGlobalSearchQuery);

  const runningCount = jobs?.filter((job) => job.status === 'running').length ?? 0;
  const warningCount = warnings?.length ?? 0;
  const currentLink = links.find((link) => link.page === currentPage) ?? links[0];

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
            <span className="text-[11px] text-[#111] font-medium">{currentLink.context}</span>
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
          <div className="flex items-center gap-2 border border-[#e0e0e0] bg-white px-2 py-1 rounded-sm">
            <Search size={12} className="text-[#888]" />
            <input
              value={globalSearchQuery}
              onChange={(event) => setGlobalSearchQuery(event.target.value)}
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
          <Settings size={14} className="cursor-pointer hover:text-black" />
        </div>
      </div>
    </div>
  );
}
