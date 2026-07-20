import { Download, FileText, RefreshCw, Shield } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import type {
  AnalysisClassifiedFile,
  AnalysisFileClassification,
  EvidenceClassificationSummary,
} from '@/types/models';
import {
  CATEGORY_COLORS,
  CATEGORY_ICONS,
  EmptyLine,
  formatProvenanceSummary,
  formatSize,
  Metric,
  ProvenancePanel,
  DenseTableFrame,
  StatusPill,
  statusLabel,
  SummaryStrip,
  WarningList,
} from './helpers';

export function EvidenceClassificationPanel({
  summary,
  pending,
  onRun,
}: {
  summary?: EvidenceClassificationSummary;
  pending: boolean;
  onRun: () => void;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    categories: [],
    generatedAt: '',
    warnings: ['证据分类摘要暂不可用。'],
    totals: {
      categoryCount: 0,
      candidateFileCount: 0,
      totalSize: 0,
      artifactCount: 0,
    },
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-[14px] font-light text-forensics-text">证据语义分类</h3>
          <div className="mt-1 text-[11px] leading-5 text-forensics-muted">
            候选来自文件树 metadata；结构化提取会读取 Registry、浏览器记录与邮件候选并返回可审计来源。
          </div>
        </div>
        <Button
          type="button"
          onClick={onRun}
          disabled={pending}
          className="h-8 rounded-none border border-forensics-text bg-forensics-text px-4 text-[12px] text-white hover:bg-forensics-text-secondary"
        >
          {pending ? <RefreshCw size={14} className="opacity-70" /> : <Shield size={14} />}
          {pending ? '分类中...' : '开始证据分类'}
        </Button>
      </div>

      <SummaryStrip
        items={[
          ['证据族', info.totals.categoryCount.toString()],
          ['候选文件', info.totals.candidateFileCount.toString()],
          ['候选总大小', formatSize(info.totals.totalSize)],
          ['已解析 Artifact', info.totals.artifactCount.toString()],
        ]}
      />

      {info.warnings.length > 0 ? <WarningList warnings={info.warnings} /> : null}

      {info.categories.length === 0 ? (
        <EmptyLine text="未发现证据语义分类数据。" />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          {info.categories.map((category) => {
            const Icon = CATEGORY_ICONS[category.category] || Shield;
            const color = CATEGORY_COLORS[category.category] || 'var(--forensics-text-secondary)';
            return (
              <section key={category.category} className="rounded-none border border-forensics-border bg-forensics-surface p-4">
                <div className="mb-3 flex items-start justify-between gap-3">
                  <div className="flex items-center gap-2">
                    <Icon size={17} style={{ color }} />
                    <div>
                      <h4 className="text-[13px] font-light text-forensics-text">{category.displayName}</h4>
                      <div className="font-mono text-[10px] text-forensics-muted-light">{category.category}</div>
                    </div>
                  </div>
                  <StatusPill status={category.status} />
                </div>

                <div className="mb-3 grid grid-cols-2 gap-2 text-[11px]">
                  <Metric label="候选" value={`${category.fileCount} 个`} />
                  <Metric label="大小" value={formatSize(category.totalSize)} />
                  <Metric label="Artifact" value={`${category.artifactCount} 条`} />
                  <Metric label="置信度" value={`${Math.round(category.confidence * 100)}%`} />
                </div>

                {category.warnings.length > 0 ? <WarningList warnings={category.warnings.slice(0, 2)} /> : null}

                {category.sources.length > 0 ? (
                  <div className="space-y-2">
                    {category.sources.slice(0, 4).map((source) => (
                      <div key={source.fileId} className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="truncate font-mono text-[11px] text-forensics-text-secondary">{source.path}</div>
                          <span className="shrink-0 text-[10px] text-forensics-muted">{statusLabel(source.status)}</span>
                        </div>
                        <div className="mt-1 flex flex-wrap gap-2 font-mono text-[10px] text-forensics-muted-light">
                          <span>{source.evidenceKind}</span>
                          <span>{source.parser}</span>
                          <span>{formatSize(source.size)}</span>
                          <span>artifacts={source.artifactCount}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <EmptyLine text="该证据族暂无代表来源文件。" />
                )}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function FileClassificationPanel({
  classifications,
}: {
  classifications: AnalysisFileClassification[];
}) {
  const totalFiles = classifications.reduce((sum, category) => sum + category.fileCount, 0);
  const totalSize = classifications.reduce((sum, category) => sum + category.totalSize, 0);
  const columns: DenseColumn<AnalysisClassifiedFile>[] = [
    {
      key: 'name',
      title: '文件名',
      className: 'min-w-[220px]',
      render: (file) => file.name,
    },
    {
      key: 'type',
      title: '类型',
      className: 'min-w-[180px]',
      render: (file) => file.magicDescription,
    },
    {
      key: 'source',
      title: '来源',
      className: 'min-w-[240px]',
      render: (file) => formatProvenanceSummary(file.provenance),
    },
    {
      key: 'size',
      title: '大小',
      className: 'w-[100px] text-right',
      render: (file) => formatSize(file.size),
    },
  ];

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          ['文件总数', totalFiles.toString()],
          ['文件总大小', formatSize(totalSize)],
          ['分类数', classifications.length.toString()],
        ]}
      />
      <div className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] leading-5 text-forensics-muted">
        当前为 metadata-only 分类：总数/大小来自数据库全量聚合，文件列表仅展示抽样；不读取 E01/RAW 文件正文。
      </div>

      {classifications.length === 0 ? (
        <EmptyLine text="未发现可分类文件。" />
      ) : (
        <div className="space-y-4">
          {classifications.map((category) => {
            const Icon = CATEGORY_ICONS[category.category] || FileText;
            const color = CATEGORY_COLORS[category.category] || CATEGORY_COLORS.Other;
            return (
              <section key={category.category}>
                <div className="mb-2 flex items-center gap-2">
                  <Icon size={16} style={{ color }} />
                  <h3 className="text-[14px] font-light text-forensics-text">{category.category}</h3>
                  <span className="text-[11px] text-forensics-muted-lighter">
                    总计 {category.fileCount} 个 · 抽样 {category.files.length} 个 · {formatSize(category.totalSize)} · {statusLabel(category.status)}
                  </span>
                </div>
                {category.warnings.length > 0 ? <WarningList warnings={category.warnings} /> : null}
                <ProvenancePanel
                  title="分类来源"
                  provenance={category.provenance}
                  compact
                  fallback="分类来源暂不可用。"
                />
                <DenseTableFrame>
                  <DenseDataTable
                    rows={category.files.slice(0, 20)}
                    columns={columns}
                    getRowKey={(file) => file.fileId}
                    emptyTitle="暂无抽样文件"
                    emptyDescription="该分类当前没有可展示的抽样文件。"
                  />
                </DenseTableFrame>
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function AnalysisReportPanel({
  pending,
  onDownload,
}: {
  pending: boolean;
  onDownload: () => void;
}) {
  return (
    <div className="flex h-64 flex-col items-center justify-center gap-4">
      <FileText size={48} className="text-forensics-muted-lighter" />
      <div className="text-[14px] text-forensics-muted">生成分析报告</div>
      <Button
        type="button"
        onClick={onDownload}
        disabled={pending}
        className="h-9 rounded-none bg-forensics-text px-6 text-[12px] text-white hover:bg-forensics-text-secondary"
      >
        {pending ? <RefreshCw size={14} className="opacity-70" /> : <Download size={14} />}
        下载 Markdown 报告
      </Button>
    </div>
  );
}
