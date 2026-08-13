import { useState, type ComponentType } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ChevronDown,
  ChevronRight,
  Database,
  FileClock,
  FileText,
  FileX2,
  Globe,
  Mail,
  Monitor,
  Puzzle,
  Server,
  Shield,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { cn } from '@/app/components/ui/utils';
import { TreeConnector } from '@/components/tree/TreeConnector';
import { dataSourcePlatformLabel, sourceKindIconLarge } from '@/lib/data-source-utils';
import type { DataSourceSummary, PluginModule } from '@/types/models';
import type {
  AnalysisExtractionProgressInfo,
  AnalysisTabKey,
  ExtractionCategory,
  LinuxAnalysisTabKey,
} from '@/features/analysis/types';

type SourceTreeNode = {
  label: string;
  icon: ComponentType<{ size?: number | string; className?: string }>;
  category?: ExtractionCategory;
  windowsTab?: AnalysisTabKey;
  linuxTab?: LinuxAnalysisTabKey;
};

const WINDOWS_NODES: SourceTreeNode[] = [
  { label: '系统信息', icon: Monitor, windowsTab: 'system' },
  { label: '证据分类', icon: Shield, windowsTab: 'evidence' },
  { label: '注册表', icon: Database, category: 'Registry', windowsTab: 'registry' },
  { label: '浏览器记录', icon: Globe, category: 'BrowserHistory', windowsTab: 'browser' },
  { label: '邮件信息', icon: Mail, category: 'Email', windowsTab: 'email' },
  { label: '事件日志', icon: FileClock, category: 'EventLogs', windowsTab: 'eventlogs' },
  { label: '文件分类', icon: FileText, windowsTab: 'files' },
  { label: '分析报告', icon: FileText, windowsTab: 'report' },
];

const WINDOWS_DELETED_RECOVERY_NODE: SourceTreeNode = {
  label: '删除恢复',
  icon: FileX2,
  windowsTab: 'deletedRecovery',
};

const LINUX_NODES: SourceTreeNode[] = [
  { label: '概览', icon: Server, category: 'LinuxArtifacts', linuxTab: 'overview' },
  { label: '系统日志', icon: FileClock, category: 'LinuxJournal', linuxTab: 'journal' },
  { label: '登录记录', icon: Monitor, category: 'LinuxLogin', linuxTab: 'login' },
  { label: '命令历史', icon: FileText, category: 'LinuxCommands', linuxTab: 'commands' },
  { label: '软件包', icon: Database, category: 'LinuxPackages', linuxTab: 'packages' },
  { label: '定时任务', icon: FileClock, category: 'LinuxCron', linuxTab: 'cron' },
  { label: 'Sudo', icon: Shield, category: 'LinuxSudo', linuxTab: 'sudo' },
  { label: '系统配置', icon: Database, category: 'LinuxSystemConfig', linuxTab: 'systemConfig' },
  { label: 'Web 服务', icon: Globe, category: 'LinuxWebServices', linuxTab: 'webServices' },
  { label: 'MySQL 服务', icon: Database, category: 'LinuxMysqlServices', linuxTab: 'mysqlServices' },
  { label: '删除恢复', icon: FileX2, linuxTab: 'deletedRecovery' },
];

export interface AnalysisSourceSidebarProps {
  dataSources: DataSourceSummary[];
  selectedDataSourceId?: string;
  disabled?: boolean;
  progress: Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
  linuxNodeCounts?: Partial<Record<LinuxAnalysisTabKey, number>>;
  activeWindowsTab: AnalysisTabKey;
  activeLinuxTab: LinuxAnalysisTabKey;
  /** Plugin modules of the currently selected data source (already fetched). */
  pluginModules?: PluginModule[];
  activePluginId?: string;
  onSelectDataSource: (id: string) => void;
  onWindowsTabChange: (tab: AnalysisTabKey) => void;
  onLinuxTabChange: (tab: LinuxAnalysisTabKey) => void;
  onSelectPluginModule?: (pluginId: string) => void;
}

export function AnalysisSourceSidebar({
  dataSources,
  selectedDataSourceId,
  disabled = false,
  progress,
  linuxNodeCounts,
  activeWindowsTab,
  activeLinuxTab,
  pluginModules,
  activePluginId,
  onSelectDataSource,
  onWindowsTabChange,
  onLinuxTabChange,
  onSelectPluginModule,
}: AnalysisSourceSidebarProps) {
  const { t } = useTranslation();
  const [collapsedSourceIds, setCollapsedSourceIds] = useState<Set<string>>(() => new Set());

  function toggleSource(sourceId: string) {
    setCollapsedSourceIds((current) => {
      const next = new Set(current);
      if (next.has(sourceId)) {
        next.delete(sourceId);
      } else {
        next.add(sourceId);
      }
      return next;
    });
  }

  function selectOrToggleSource(sourceId: string, selected: boolean, expanded: boolean) {
    if (!selected) {
      onSelectDataSource(sourceId);
      if (!expanded) {
        toggleSource(sourceId);
      }
      return;
    }

    toggleSource(sourceId);
  }

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-forensics-border bg-forensics-panel" aria-label="数据源树">
      <div className="border-b border-forensics-border px-3 py-3">
        <div className="text-[13px] font-light text-forensics-text">数据源</div>
        <div className="mt-1 text-[11px] text-forensics-muted">按来源展开提取结果</div>
      </div>

      <ScrollArea className="min-h-0 flex-1" viewportClassName="p-2">
        <div className="space-y-2">
          {dataSources.map((source) => {
            const selected = source.id === selectedDataSourceId;
            const expanded = !collapsedSourceIds.has(source.id);
            const nodes = source.platform === 'windows'
              ? [...WINDOWS_NODES, WINDOWS_DELETED_RECOVERY_NODE]
              : LINUX_NODES;
            const SourceIcon = sourceKindIconLarge(source.kind);
            // Plugin modules are fetched for the selected source only; the
            // platform filter keeps e.g. windows-only plugins off linux sources.
            const pluginNodes = selected
              ? (pluginModules ?? []).filter((module) => module.evidencePlatform === source.platform)
              : [];

            return (
              <section key={source.id} className="min-w-0">
                <Button
                  type="button"
                  variant="forensicsGhost"
                  size="inline"
                  disabled={disabled}
                  aria-label={source.name}
                  aria-current={selected ? 'true' : undefined}
                  aria-expanded={expanded}
                  title={expanded ? '收起数据源' : '展开数据源'}
                  onClick={() => selectOrToggleSource(source.id, selected, expanded)}
                  className={cn(
                    'h-8 w-full min-w-0 justify-start gap-2 border border-transparent px-2 text-left text-[12px] hover:border-forensics-border',
                    selected && 'border-forensics-border bg-forensics-surface text-forensics-text',
                  )}
                >
                  {expanded ? (
                    <ChevronDown size={12} className="shrink-0 text-forensics-muted-light" />
                  ) : (
                    <ChevronRight size={12} className="shrink-0 text-forensics-muted-light" />
                  )}
                  <SourceIcon size={14} className="shrink-0 text-forensics-muted-light" />
                  <span className="min-w-0 flex-1 truncate" title={source.name}>{source.name}</span>
                  {source.fileCount !== undefined ? (
                    <span className="shrink-0 font-mono text-[10px] text-forensics-muted-light">{source.fileCount}</span>
                  ) : null}
                </Button>
                {expanded ? (
                  <div className="ml-4 border-l border-forensics-border-light py-1 pl-1">
                    <div className="px-2 pb-1 text-[10px] text-forensics-muted-light">{dataSourcePlatformLabel(source)}</div>
                    {nodes.map((node, index) => {
                      const active = selected
                        && (source.platform === 'windows'
                          ? node.windowsTab === activeWindowsTab
                          : node.linuxTab === activeLinuxTab);
                      const nodeProgress = selected && node.category ? progress[node.category] : undefined;
                      const summaryCount = selected && source.platform === 'linux' && node.linuxTab
                        ? linuxNodeCounts?.[node.linuxTab]
                        : undefined;
                      const resultCount = summaryCount
                        ?? (nodeProgress && nodeProgress.status !== 'idle'
                          ? nodeProgress.artifactCount
                          : undefined);
                      const Icon = node.icon;

                      return (
                        <Button
                          key={node.label}
                          type="button"
                          variant="forensicsGhost"
                          size="inline"
                          disabled={disabled}
                          aria-label={`${source.name} / ${node.label}`}
                          aria-current={active ? 'true' : undefined}
                          onClick={() => {
                            if (source.id !== selectedDataSourceId) {
                              onSelectDataSource(source.id);
                            }
                            if (source.platform === 'windows' && node.windowsTab) {
                              onWindowsTabChange(node.windowsTab);
                            }
                            if (source.platform === 'linux' && node.linuxTab) {
                              onLinuxTabChange(node.linuxTab);
                            }
                          }}
                          className={cn(
                            'h-7 w-full min-w-0 justify-start gap-1 px-1 text-left text-[11px] text-forensics-muted hover:text-forensics-text',
                            active && 'bg-forensics-surface text-forensics-text',
                          )}
                        >
                          <TreeConnector depth={1} isLast={index === nodes.length - 1 && pluginNodes.length === 0} />
                          <Icon size={12} className="shrink-0 text-forensics-muted-light" />
                          <span className="min-w-0 flex-1 truncate">
                            {resultCount === undefined ? node.label : `${node.label}(${resultCount})`}
                          </span>
                        </Button>
                      );
                    })}
                    {pluginNodes.length > 0 ? (
                      <>
                        <div className="px-2 pb-1 pt-1 text-[10px] text-forensics-muted-light">
                          {t('pluginModule.groupTitle')}
                        </div>
                        {pluginNodes.map((module, index) => {
                          const pluginActive = activePluginId === module.pluginId
                            && (source.platform === 'windows'
                              ? activeWindowsTab === 'plugin'
                              : activeLinuxTab === 'plugin');
                          return (
                            <Button
                              key={module.pluginId}
                              type="button"
                              variant="forensicsGhost"
                              size="inline"
                              disabled={disabled}
                              aria-label={`${source.name} / ${module.displayName}`}
                              aria-current={pluginActive ? 'true' : undefined}
                              onClick={() => {
                                if (source.id !== selectedDataSourceId) {
                                  onSelectDataSource(source.id);
                                }
                                onSelectPluginModule?.(module.pluginId);
                              }}
                              className={cn(
                                'h-7 w-full min-w-0 justify-start gap-1 px-1 text-left text-[11px] text-forensics-muted hover:text-forensics-text',
                                pluginActive && 'bg-forensics-surface text-forensics-text',
                              )}
                            >
                              <TreeConnector depth={1} isLast={index === pluginNodes.length - 1} />
                              <Puzzle size={12} className="shrink-0 text-forensics-muted-light" />
                              <span className="min-w-0 flex-1 truncate" title={module.displayName}>
                                {`${module.displayName}(${module.totalCount})`}
                              </span>
                            </Button>
                          );
                        })}
                      </>
                    ) : null}
                  </div>
                ) : null}
              </section>
            );
          })}
        </div>
      </ScrollArea>
    </aside>
  );
}
