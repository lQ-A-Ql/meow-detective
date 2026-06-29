import { Clock, Database, Monitor, Network, RefreshCw, Shield } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import type {
  AnalysisExtractionRun,
  AnalysisSystemInfo,
} from '@/types/models';
import {
  EmptyLine,
  FieldProvenancePanel,
  formatProvenanceSummary,
  InfoCard,
  ProvenancePanel,
  RunMetric,
  StatusPill,
  WarningList,
} from './helpers';

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
  demoPending,
  extractionPending,
  extractionRun,
  onLoadDemoCase,
  onRefresh,
  onRunExtraction,
}: {
  loading: boolean;
  hasCase: boolean;
  demoPending: boolean;
  extractionPending: boolean;
  extractionRun?: AnalysisExtractionRun;
  onLoadDemoCase: () => void;
  onRefresh: () => void;
  onRunExtraction: () => void;
}) {
  return (
    <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="font-serif text-xl tracking-tight text-[#111]">数据源分析</div>
          <div className="mt-1 font-mono text-[11px] text-[#666]">
            证据分类 · 注册表提取 · 浏览器记录 · 邮件信息
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            onClick={onLoadDemoCase}
            disabled={loading}
            className="h-8 rounded border border-[#111] bg-[#111] px-3 text-[12px] text-white hover:bg-[#333]"
          >
            {demoPending ? <RefreshCw size={14} className="animate-spin" /> : <Database size={14} />}
            加载演示案件
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={onRefresh}
            disabled={!hasCase || loading}
            className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
          >
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            刷新
          </Button>
          <Button
            type="button"
            onClick={onRunExtraction}
            disabled={!hasCase || extractionPending}
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

export function AnalysisEmptyState({
  demoPending,
  onLoadDemoCase,
}: {
  demoPending: boolean;
  onLoadDemoCase: () => void;
}) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center">
        <Monitor size={40} className="mx-auto mb-4 text-[#bbb]" />
        <div className="text-[15px] font-semibold text-[#111]">请先创建或打开案件</div>
        <div className="mt-2 text-[12px] leading-6 text-[#666]">
          数据源分析依赖当前案件中的文件目录和数据源记录。未选择案件时不会发起分析请求。
        </div>
        <Button
          type="button"
          onClick={onLoadDemoCase}
          disabled={demoPending}
          className="mt-5 h-8 rounded bg-[#111] px-5 text-[12px] text-white hover:bg-[#333]"
        >
          {demoPending ? <RefreshCw size={14} className="animate-spin" /> : <Database size={14} />}
          加载演示案件
        </Button>
      </div>
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
