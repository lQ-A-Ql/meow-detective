import type {
  EmailExtractionSummary,
  EmailMessage,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisExtractionProgress,
  type AnalysisExtractionProgressInfo,
  DenseTableFrame,
  ExtractionTableSection,
} from './helpers';

export function EmailExtractionPanel({
  summary,
  progress,
}: {
  summary?: EmailExtractionSummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    total: 0,
    messages: [],
    generatedAt: '',
    warnings: ['邮件提取结果暂不可用。'],
  };
  const columns: DenseColumn<EmailMessage>[] = [
    { key: 'sentAt', title: '时间', className: 'w-[170px]', render: (row) => row.sentAt ?? '-' },
    { key: 'from', title: 'From', className: 'w-[180px]', render: (row) => row.from || '-' },
    { key: 'to', title: 'To', className: 'w-[200px]', render: (row) => row.to.join(', ') || '-' },
    { key: 'subject', title: 'Subject', className: 'min-w-[240px]', render: (row) => row.subject || '-' },
    { key: 'attachments', title: '附件', className: 'w-[180px]', render: (row) => row.attachments.join(', ') || '-' },
    { key: 'sourcePath', title: 'Source', className: 'min-w-[260px]', render: (row) => row.sourcePath || '-' },
  ];

  return (
    <ExtractionTableSection
      title="邮件信息"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['邮件数', info.total.toString()],
        ['当前页', info.messages.length.toString()],
        ['含附件', info.messages.filter((item) => item.attachments.length > 0).length.toString()],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />
      <DenseTableFrame>
        <DenseDataTable
          rows={info.messages}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          emptyTitle="暂无邮件信息"
          emptyDescription="v1 支持 EML/EMLX 的头字段、附件名与正文预览。"
        />
      </DenseTableFrame>
      {info.messages[0] ? (
        <div className="mt-4 rounded border border-[#e0e0e0] bg-[#fcfcfc] p-3 text-[12px]">
          <div className="mb-1 font-semibold text-[#111]">正文预览</div>
          <div className="text-[#666]">{info.messages[0].bodyPreview}</div>
          <div className="mt-2 font-mono text-[10px] text-[#888]">
            Message-ID: {info.messages[0].messageId}
          </div>
        </div>
      ) : null}
    </ExtractionTableSection>
  );
}
