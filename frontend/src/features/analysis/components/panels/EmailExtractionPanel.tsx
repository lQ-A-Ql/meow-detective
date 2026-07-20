import { useState } from 'react';
import type {
  EmailAttachment,
  EmailExtractionSummary,
  EmailHeader,
  EmailMessage,
} from '@/types/models';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { KeyValueField } from '@/components/data-display';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  DenseTableFrame,
  ExtractionTableSection,
  formatSize,
} from './helpers';
import {
  ChevronDown,
  ChevronUp,
  Mail,
  Paperclip,
} from 'lucide-react';

export function EmailExtractionPanel({
  summary,
}: {
  summary?: EmailExtractionSummary;
}) {
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [showHeaders, setShowHeaders] = useState(false);

  const info = summary ?? {
    status: 'unavailable' as const,
    total: 0,
    messages: [],
    generatedAt: '',
    warnings: ['邮件提取结果暂不可用。'],
  };

  const selectedMessage = info.messages.find(
    (msg) => msg.artifactId === selectedArtifactId,
  );

  const columns: DenseColumn<EmailMessage>[] = [
    {
      key: 'sentAt',
      title: '时间',
      className: 'w-[150px]',
      render: (row) => row.sentAt ?? '-',
    },
    {
      key: 'from',
      title: 'From',
      className: 'w-[180px]',
      render: (row) => row.from || '-',
    },
    {
      key: 'to',
      title: 'To',
      className: 'w-[160px]',
      render: (row) => joinAddresses(row.to),
    },
    {
      key: 'cc',
      title: 'Cc',
      className: 'w-[120px]',
      render: (row) => joinAddresses(row.cc),
    },
    {
      key: 'bcc',
      title: 'Bcc',
      className: 'w-[120px]',
      render: (row) => joinAddresses(row.bcc),
    },
    {
      key: 'subject',
      title: 'Subject',
      className: 'min-w-[200px]',
      render: (row) => row.subject || '-',
    },
    {
      key: 'attachmentCount',
      title: '附件',
      className: 'w-[100px]',
      render: (row) =>
        row.attachmentCount > 0 ? (
          <span className="inline-flex items-center gap-1">
            <Paperclip className="size-3" />
            {row.attachmentCount}
          </span>
        ) : (
          '-'
        ),
    },
    {
      key: 'sourcePath',
      title: 'Source',
      className: 'min-w-[180px]',
      render: (row) => row.sourcePath || '-',
    },
    {
      key: 'containerPath',
      title: 'Folder',
      className: 'min-w-[140px]',
      render: (row) => row.containerPath || '-',
    },
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
        [
          '含附件',
          info.messages.filter((item) => item.attachmentCount > 0).length.toString(),
        ],
      ]}
    >
      <DenseTableFrame>
        <DenseDataTable
          rows={info.messages}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          selectedRowKey={selectedArtifactId ?? undefined}
          onRowClick={(row) =>
            setSelectedArtifactId(
              row.artifactId === selectedArtifactId ? null : row.artifactId,
            )
          }
          emptyTitle="暂无邮件信息"
          emptyDescription="支持 EML/EMLX/MBOX/PST/OST 邮件解析：头字段、正文、附件、Cc/Bcc、Message-ID、References、容器路径与文件夹路径。"
        />
      </DenseTableFrame>
      {selectedMessage ? (
        <EmailDetailCard
          message={selectedMessage}
          showHeaders={showHeaders}
          onToggleHeaders={() => setShowHeaders((prev) => !prev)}
        />
      ) : null}
    </ExtractionTableSection>
  );
}

function EmailDetailCard({
  message,
  showHeaders,
  onToggleHeaders,
}: {
  message: EmailMessage;
  showHeaders: boolean;
  onToggleHeaders: () => void;
}) {
  const hasHtml = Boolean(message.bodyHtml);
  const hasPlain = Boolean(message.bodyPlain);
  const defaultTab = hasPlain ? 'plain' : hasHtml ? 'html' : 'preview';

  return (
    <div className="mt-4 rounded-none border border-forensics-border bg-forensics-surface">
      <div className="flex items-start gap-3 border-b border-forensics-border p-3">
        <div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-none bg-forensics-panel-strong">
          <Mail className="size-4 text-forensics-text-tertiary" />
        </div>
        <div className="min-w-0 flex-1 space-y-1.5">
          <div className="text-[13px] font-light text-forensics-text">
            {message.subject || '(no subject)'}
          </div>
          <div className="grid grid-cols-1 gap-x-6 gap-y-1 text-[11px] text-forensics-text-tertiary md:grid-cols-2">
            <Field label="From" value={message.from} />
            <Field label="To" value={joinAddresses(message.to)} />
            {message.cc.length > 0 ? (
              <Field label="Cc" value={joinAddresses(message.cc)} />
            ) : null}
            {message.bcc.length > 0 ? (
              <Field label="Bcc" value={joinAddresses(message.bcc)} />
            ) : null}
            {message.replyTo ? <Field label="Reply-To" value={message.replyTo} /> : null}
            {message.returnPath ? (
              <Field label="Return-Path" value={message.returnPath} />
            ) : null}
            {message.sentAt ? <Field label="Sent" value={message.sentAt} /> : null}
            {message.receivedAt ? (
              <Field label="Received" value={message.receivedAt} />
            ) : null}
            {message.containerPath ? (
              <Field label="Container" value={message.containerPath} />
            ) : null}
            {message.messageClass ? (
              <Field label="Message Class" value={message.messageClass} />
            ) : null}
            {message.isDeleted ? (
              <Field label="Deleted" value="是" />
            ) : null}
            {message.messageId ? (
              <Field label="Message-ID" value={message.messageId} monospace />
            ) : null}
            {message.inReplyTo ? (
              <Field label="In-Reply-To" value={message.inReplyTo} monospace />
            ) : null}
            {message.references.length > 0 ? (
              <Field
                label="References"
                value={message.references.join(', ')}
                monospace
                className="md:col-span-2"
              />
            ) : null}
            {message.xMailer ? <Field label="X-Mailer" value={message.xMailer} /> : null}
            {message.xOriginatingIp ? (
              <Field label="X-Originating-IP" value={message.xOriginatingIp} />
            ) : null}
          </div>
        </div>
      </div>

      {message.attachmentDetails.length > 0 ? (
        <div className="border-b border-forensics-border p-3">
          <div className="mb-2 text-[11px] font-light text-forensics-text">
            附件 ({message.attachmentDetails.length})
          </div>
          <div className="flex flex-wrap gap-2">
            {message.attachmentDetails.map((att) => (
              <AttachmentBadge key={att.fileName} attachment={att} />
            ))}
          </div>
        </div>
      ) : null}

      <div className="p-3">
        <PanelTabs
          defaultValue={defaultTab}
          variant="compact"
          tabs={[
            { value: 'preview', label: '预览' },
            ...(hasPlain ? [{ value: 'plain', label: '纯文本' }] : []),
            ...(hasHtml ? [{ value: 'html', label: 'HTML' }] : []),
          ]}
        >
          <TabsContent value="preview">
            <BodyPreview text={message.bodyPreview} />
          </TabsContent>
          {hasPlain ? (
            <TabsContent value="plain">
              <BodyPreview text={message.bodyPlain} />
            </TabsContent>
          ) : null}
          {hasHtml ? (
            <TabsContent value="html">
              <HtmlPreview html={message.bodyHtml} />
            </TabsContent>
          ) : null}
        </PanelTabs>
      </div>

      {message.headers.length > 0 ? (
        <div className="border-t border-forensics-border p-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={onToggleHeaders}
            className="h-7 gap-1 px-2 text-[11px] text-forensics-text-tertiary"
          >
            {showHeaders ? (
              <ChevronUp className="size-3" />
            ) : (
              <ChevronDown className="size-3" />
            )}
            原始头字段 ({message.headers.length})
          </Button>
          {showHeaders ? <HeaderList headers={message.headers} /> : null}
        </div>
      ) : null}
    </div>
  );
}

function Field({
  label,
  value,
  monospace,
  className,
}: {
  label: string;
  value: string;
  monospace?: boolean;
  className?: string;
}) {
  return <KeyValueField label={label} value={value} mono={monospace} layout="inline" className={className} />;
}

function AttachmentBadge({ attachment }: { attachment: EmailAttachment }) {
  const sizeText =
    attachment.size !== undefined ? formatSize(attachment.size) : undefined;
  return (
    <Badge variant="outline" className="gap-1 px-2 py-0.5 text-[10px]">
      <Paperclip className="size-3" />
      <span className="max-w-[200px] truncate">{attachment.fileName}</span>
      {sizeText ? <span className="text-forensics-muted-light">({sizeText})</span> : null}
      {attachment.mimeType ? (
        <span className="text-forensics-muted-light">· {attachment.mimeType}</span>
      ) : null}
    </Badge>
  );
}

function BodyPreview({ text }: { text?: string }) {
  return (
    <div className="max-h-[240px] overflow-auto rounded-none border border-forensics-border bg-forensics-surface p-3 text-[12px] leading-relaxed text-forensics-text-secondary">
      {text?.trim() ? text : <span className="text-forensics-muted-lighter">无正文内容</span>}
    </div>
  );
}

function HtmlPreview({ html }: { html?: string }) {
  if (!html) {
    return (
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-3 text-[12px] text-forensics-muted-lighter">
        无 HTML 内容
      </div>
    );
  }
  return (
    <div className="max-h-[240px] overflow-auto rounded-none border border-forensics-border bg-forensics-surface p-3">
      <pre className="whitespace-pre-wrap break-all font-mono text-[11px] text-forensics-text-secondary">
        {html}
      </pre>
    </div>
  );
}

function HeaderList({ headers }: { headers: EmailHeader[] }) {
  return (
    <div className="mt-2 max-h-[240px] overflow-auto rounded-none border border-forensics-border bg-forensics-surface p-2">
      <div className="divide-y divide-forensics-border-light text-[11px]">
        {headers.map((header, index) => (
          <div key={index} className="grid grid-cols-[160px_minmax(0,1fr)] gap-3 py-1">
            <div className="font-light text-forensics-muted">{header.name}</div>
            <div className="break-all font-mono text-[10px] text-forensics-text-secondary">
              {header.value}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function joinAddresses(addresses: string[]) {
  if (!addresses || addresses.length === 0) return '-';
  return addresses.join(', ');
}
