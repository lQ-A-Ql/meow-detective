import { Clock, Monitor, Network, RefreshCw, Shield } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { BrandEmptyState } from '@/components/brand';
import type {
  AnalysisExtractionRun,
  AnalysisSystemInfo,
} from '@/types/models';
import { DataSourceSelector } from '@/features/analysis/components/DataSourceSelector';
import {
  EmptyLine,
  AnalysisExtractionProgress,
  type AnalysisExtractionProgressInfo,
  FieldProvenancePanel,
  formatProvenanceSummary,
  InfoCard,
  ProvenancePanel,
  RunMetric,
  StatusPill,
  WarningList,
} from './helpers';

export function AnalysisProgressOverview({
  progress,
  expanded,
  onExpandedChange,
}: {
  progress: AnalysisExtractionProgressInfo[];
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      data-testid="analysis-progress-overview"
      className="shrink-0 border-b border-forensics-border bg-forensics-panel"
    >
      <div className="flex items-center justify-between border-b border-forensics-border px-6 py-2">
        <span className="text-xs font-medium text-forensics-text-secondary">
          {t('analysis.progressDrawer.title')}
        </span>
        <Button
          type="button"
          variant="forensicsSurface"
          size="compact"
          onClick={() => onExpandedChange(!expanded)}
        >
          {expanded ? t('analysis.progressDrawer.collapse') : t('analysis.progressDrawer.expand')}
        </Button>
      </div>
      {expanded ? (
        <div className="grid grid-cols-1 gap-3 px-6 py-4 lg:grid-cols-3">
          {progress.map((item, index) => (
            <AnalysisExtractionProgress key={`${item.label}-${index}`} progress={item} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function SystemInfoPanel({ systemInfo }: { systemInfo?: AnalysisSystemInfo }) {
  const info = systemInfo ?? {
    networkAdapters: [],
    bootHistory: [],
    status: 'unavailable' as const,
    warnings: ['系统信息暂不可用。'],
    provenance: [],
    fieldProvenance: [],
  };
  const parserFailures = info.provenance.filter(
    (item) => item.status !== 'parsed' && item.warnings.length > 0,
  );

  return (
    <div className="space-y-6">
      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Monitor size={16} />
          系统信息
        </h3>
        <div className="mb-3 flex items-center gap-2 text-[12px] text-[#666]">
          <StatusPill status={info.status} />
          {info.warnings[0] ? <span>{info.warnings[0]}</span> : null}
        </div>
        {parserFailures.length > 0 ? (
          <WarningList warnings={['已发现 Registry/EVTX 候选文件，但部分解析器失败；下方 provenance 已列出具体原因。']} />
        ) : null}
        {info.warnings.length > 1 ? <WarningList warnings={info.warnings.slice(0, 3)} /> : null}
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <InfoCard label="计算机名" value={info.computerName} />
          <InfoCard label="操作系统" value={info.osVersion} />
          <InfoCard label="Build 号" value={info.buildNumber} />
          <InfoCard label="注册用户" value={info.registeredOwner} />
          <InfoCard label="时区" value={info.timezone} />
          <InfoCard label="安装日期" value={info.installDate} />
        </div>
      </section>

      <ProvenancePanel
        title="解析来源"
        provenance={info.provenance}
        fallback="Registry/EVTX 解析来源暂不可用。"
      />

      <FieldProvenancePanel fieldProvenance={info.fieldProvenance} />

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Network size={16} />
          网络适配器
        </h3>
        {info.networkAdapters.length > 0 ? (
          <div className="space-y-2">
            {info.networkAdapters.map((adapter) => (
              <div key={adapter.name} className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-3">
                <div className="text-[12px] font-medium">{adapter.name}</div>
                <div className="mt-1 font-mono text-[11px] text-[#666]">
                  MAC: {adapter.macAddress ?? '-'}
                </div>
                <div className="font-mono text-[11px] text-[#666]">
                  IP: {adapter.ipAddresses.join(', ') || '-'}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text="未解析到网络适配器。" />
        )}
      </section>

      <section>
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-semibold text-[#111]">
          <Clock size={16} />
          开关机历史
        </h3>
        {info.bootHistory.length > 0 ? (
          <div className="space-y-1">
            {info.bootHistory.map((boot) => (
              <div key={`${boot.timestamp}-${boot.source}`} className="rounded border border-[#e0e0e0] bg-[#f8f8f8] p-3 text-[12px]">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="font-mono text-[#666]">{boot.timestamp}</span>
                  <span className="rounded bg-[#f0f0f0] px-2 py-0.5 text-[10px]">
                    {boot.bootType}
                  </span>
                  {boot.eventId ? (
                    <span className="rounded bg-[#f0f0f0] px-2 py-0.5 font-mono text-[10px]">
                      EventID {boot.eventId}
                    </span>
                  ) : null}
                  <span className="text-[#999]">{boot.source}</span>
                </div>
                {boot.note ? <div className="mt-2 text-[11px] text-[#666]">{boot.note}</div> : null}
                <div className="mt-2 text-[11px] text-[#777]">
                  {formatProvenanceSummary(boot.provenance)}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine text="未解析到开关机历史。" />
        )}
      </section>
    </div>
  );
}

export function AnalysisHeader({
  loading,
  hasCase,
  extractionPending,
  dataSourceSwitchDisabled = extractionPending,
  extractionRun,
  onRefresh,
  onRunExtraction,
  dataSources,
  selectedDataSourceId,
  onSelectDataSource,
}: {
  loading: boolean;
  hasCase: boolean;
  extractionPending: boolean;
  dataSourceSwitchDisabled?: boolean;
  extractionRun?: AnalysisExtractionRun;
  onRefresh: () => void;
  onRunExtraction: () => void;
  dataSources?: import('@/types/models').DataSourceSummary[];
  selectedDataSourceId?: string;
  onSelectDataSource?: (id: string) => void;
}) {
  return (
    <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="font-serif text-xl tracking-tight text-[#111]">数据源分析</div>
          <div className="mt-1 font-mono text-[11px] text-[#666]">
            证据分类 · 注册表提取 · 浏览器记录 · 邮件信息
          </div>
          {dataSources && dataSources.length > 0 && onSelectDataSource ? (
            <>
              <DataSourceSelector
                dataSources={dataSources}
                selectedId={selectedDataSourceId}
                onSelect={onSelectDataSource}
                disabled={dataSourceSwitchDisabled}
                className="mt-3"
              />
              <div className="mt-2 max-w-md text-[11px] leading-5 text-[#999]">
                {selectedDataSourceId
                  ? '分析结果绑定当前数据源，Windows/Linux 视图会按数据源平台独立执行。'
                  : '请选择一个数据源后再刷新或运行提取。'}
              </div>
            </>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onRefresh}
            disabled={!hasCase || loading || !selectedDataSourceId}
            className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            刷新
          </Button>
          <Button
            type="button"
            onClick={onRunExtraction}
            disabled={!hasCase || extractionPending || !selectedDataSourceId}
            className="h-8 rounded border border-[#111] bg-[#111] px-3 text-[12px] text-white hover:bg-[#333]"
          >
            {extractionPending ? <RefreshCw size={14} className="animate-spin" /> : <Shield size={14} />}
            {extractionPending ? '提取中...' : '运行提取'}
          </Button>
        </div>
      </div>
      {extractionRun ? (
        <div className="mt-4 grid min-w-[360px] grid-cols-3 rounded border border-[#e0e0e0] bg-white text-center">
          <RunMetric label="扫描" value={extractionRun.scannedCount.toString()} />
          <RunMetric label="Artifact" value={extractionRun.artifactCount.toString()} />
          <RunMetric label="Timeline" value={extractionRun.timelineEventCount.toString()} />
        </div>
      ) : null}
    </div>
  );
}

export function AnalysisEmptyState() {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <BrandEmptyState
        variant="investigate"
        title="请先创建或打开案件"
        description="数据源分析依赖当前案件中的文件目录和数据源记录。未选择案件时不会发起分析请求。"
        className="max-w-md"
      />
    </div>
  );
}

export function AnalysisErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="mb-4 flex items-center justify-between gap-3 rounded border border-red-200 bg-red-50 p-3 text-[12px] text-red-700">
      <span>{message}</span>
      <Button
        type="button"
        variant="outline"
        onClick={onRetry}
        className="h-7 shrink-0 rounded border-red-200 bg-white px-3 text-[12px] text-red-700 hover:bg-red-100"
      >
        重试
      </Button>
    </div>
  );
}

export function AnalysisLoadingPanel({ text }: { text: string }) {
  return (
    <div className="flex h-64 items-center justify-center text-[#999]">
      <RefreshCw size={24} className="mr-2 animate-spin" />
      {text}
    </div>
  );
}
