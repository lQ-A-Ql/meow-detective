import { Download, FileText, RefreshCw, Shield } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import type {
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
          <h3 className="text-[14px] font-semibold text-[#111]">证据语义分类</h3>
          <div className="mt-1 text-[11px] leading-5 text-[#666]">
            候选来自文件树 metadata；结构化提取会读取 Registry、浏览器记录与邮件候选并返回可审计来源。
          </div>
        </div>
        <Button
          type="button"
          onClick={onRun}
          disabled={pending}
          className="h-8 rounded border border-[#111] bg-[#111] px-4 text-[12px] text-white hover:bg-[#333]"
        >
          {pending ? <RefreshCw size={14} className="animate-spin" /> : <Shield size={14} />}
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
            const color = CATEGORY_COLORS[category.category] || '#344054';
            return (
              <section key={category.category} className="rounded border border-[#e0e0e0] bg-[#fcfcfc] p-4">
                <div className="mb-3 flex items-start justify-between gap-3">
                  <div className="flex items-center gap-2">
                    <Icon size={17} style={{ color }} />
                    <div>
                      <h4 className="text-[13px] font-semibold text-[#111]">{category.displayName}</h4>
                      <div className="font-mono text-[10px] text-[#888]">{category.category}</div>
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
                      <div key={source.fileId} className="rounded border border-[#e5e5e5] bg-white px-3 py-2">
                        <div className="flex items-center justify-between gap-2">
                          <div className="truncate font-mono text-[11px] text-[#333]">{source.path}</div>
                          <span className="shrink-0 text-[10px] text-[#777]">{statusLabel(source.status)}</span>
                        </div>
                        <div className="mt-1 flex flex-wrap gap-2 font-mono text-[10px] text-[#888]">
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

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          ['文件总数', totalFiles.toString()],
          ['文件总大小', formatSize(totalSize)],
          ['分类数', classifications.length.toString()],
        ]}
      />
      <div className="rounded border border-[#e0e0e0] bg-[#fcfcfc] px-3 py-2 text-[11px] leading-5 text-[#666]">
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
                  <h3 className="text-[14px] font-semibold text-[#111]">{category.category}</h3>
                  <span className="text-[11px] text-[#999]">
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
                <div className="overflow-hidden rounded border border-[#e0e0e0] bg-[#f8f8f8]">
                  <table className="w-full text-[11px]">
                    <thead>
                      <tr className="bg-[#f0f0f0]">
                        <th className="px-3 py-2 text-left font-medium">文件名</th>
                        <th className="px-3 py-2 text-left font-medium">类型</th>
                        <th className="px-3 py-2 text-left font-medium">来源</th>
                        <th className="px-3 py-2 text-right font-medium">大小</th>
                      </tr>
                    </thead>
                    <tbody>
                      {category.files.slice(0, 20).map((file) => (
                        <tr key={file.fileId} className="border-t border-[#e0e0e0]">
                          <td className="max-w-[300px] truncate px-3 py-1.5 font-mono">{file.name}</td>
                          <td className="px-3 py-1.5 text-[#666]">{file.magicDescription}</td>
                          <td className="max-w-[260px] truncate px-3 py-1.5 text-[#666]">
                            {formatProvenanceSummary(file.provenance)}
                          </td>
                          <td className="px-3 py-1.5 text-right text-[#666]">{formatSize(file.size)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
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
      <FileText size={48} className="text-[#ccc]" />
      <div className="text-[14px] text-[#666]">生成分析报告</div>
      <Button
        type="button"
        onClick={onDownload}
        disabled={pending}
        className="h-9 rounded bg-[#111] px-6 text-[12px] text-white hover:bg-[#333]"
      >
        {pending ? <RefreshCw size={14} className="animate-spin" /> : <Download size={14} />}
        下载 Markdown 报告
      </Button>
    </div>
  );
}
