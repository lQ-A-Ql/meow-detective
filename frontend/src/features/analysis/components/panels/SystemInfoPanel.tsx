import { Clock, Monitor, Network, RefreshCw, Shield } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { BrandEmptyState } from '@/components/brand';
import type {
  AnalysisSystemInfo,
} from '@/types/models';
import {
  EmptyLine,
  FieldProvenancePanel,
  formatProvenanceSummary,
  InfoCard,
  ProvenancePanel,
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
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Monitor size={16} />
          系统信息
        </h3>
        <div className="mb-3 flex items-center gap-2 text-[12px] text-forensics-muted">
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
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Network size={16} />
          网络适配器
        </h3>
        {info.networkAdapters.length > 0 ? (
          <div className="space-y-2">
            {info.networkAdapters.map((adapter) => (
              <div key={adapter.name} className="rounded-none border border-forensics-border bg-forensics-panel p-3">
                <div className="text-[12px] font-light">{adapter.name}</div>
                <div className="mt-1 font-mono text-[11px] text-forensics-muted">
                  MAC: {adapter.macAddress ?? '-'}
                </div>
                <div className="font-mono text-[11px] text-forensics-muted">
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
        <h3 className="mb-3 flex items-center gap-2 text-[14px] font-light text-forensics-text">
          <Clock size={16} />
          开关机历史
        </h3>
        {info.bootHistory.length > 0 ? (
          <div className="space-y-1">
            {info.bootHistory.map((boot) => (
              <div key={`${boot.timestamp}-${boot.source}`} className="rounded-none border border-forensics-border bg-forensics-panel p-3 text-[12px]">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="font-mono text-forensics-muted">{boot.timestamp}</span>
                  <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 text-[10px]">
                    {boot.bootType}
                  </span>
                  {boot.eventId ? (
                    <span className="rounded-none bg-forensics-panel-strong px-2 py-0.5 font-mono text-[10px]">
                      EventID {boot.eventId}
                    </span>
                  ) : null}
                  <span className="text-forensics-muted-lighter">{boot.source}</span>
                </div>
                {boot.note ? <div className="mt-2 text-[11px] text-forensics-muted">{boot.note}</div> : null}
                <div className="mt-2 text-[11px] text-forensics-muted">
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
  onRefresh,
  onRunExtraction,
  selectedDataSourceId,
}: {
  loading: boolean;
  hasCase: boolean;
  extractionPending: boolean;
  onRefresh: () => void;
  onRunExtraction: () => void;
  selectedDataSourceId?: string;
}) {
  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel px-6 py-3">
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
        <div className="min-w-0">
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <div className="font-serif text-lg tracking-wide text-forensics-text">数据源分析</div>
            <div className="font-mono text-[10px] text-forensics-muted">
              证据分类 · 注册表提取 · 浏览器记录 · 邮件信息
            </div>
            <div className="text-[10px] leading-4 text-forensics-muted-lighter">
              {selectedDataSourceId
                ? '分析结果绑定当前数据源，Windows/Linux 视图会按数据源平台独立执行。'
                : '请从左侧数据源树选择一个来源后再刷新或运行提取。'}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={onRefresh}
            disabled={!hasCase || loading || !selectedDataSourceId}
            className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"
          >
            <RefreshCw size={14} className={loading ? 'opacity-70' : ''} />
            刷新
          </Button>
          <Button
            type="button"
            onClick={onRunExtraction}
            disabled={!hasCase || extractionPending || !selectedDataSourceId}
            className="h-8 rounded-none border border-forensics-text bg-forensics-text px-3 text-[12px] text-white hover:bg-forensics-text-secondary"
          >
            {extractionPending ? <RefreshCw size={14} className="opacity-70" /> : <Shield size={14} />}
            {extractionPending ? '提取中...' : '运行提取'}
          </Button>
        </div>
      </div>
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
    <div className="mb-4 flex items-center justify-between gap-3 rounded-none border border-forensics-error-border bg-forensics-error-bg p-3 text-[12px] text-forensics-error-text">
      <span>{message}</span>
      <Button
        type="button"
        variant="outline"
        onClick={onRetry}
        className="h-7 shrink-0 rounded-none border-forensics-error-border bg-forensics-surface px-3 text-[12px] text-forensics-error-text hover:bg-forensics-error-bg"
      >
        重试
      </Button>
    </div>
  );
}

export function AnalysisLoadingPanel({ text }: { text: string }) {
  return (
    <div className="flex h-64 items-center justify-center text-forensics-muted-lighter">
      <RefreshCw size={24} className="mr-2 opacity-70" />
      {text}
    </div>
  );
}
