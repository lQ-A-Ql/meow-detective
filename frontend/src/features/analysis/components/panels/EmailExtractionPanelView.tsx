import { useMemo } from 'react';
import type { useEmailExtractionModel } from '../../use-email-extraction-model';
import { useTranslation } from 'react-i18next';
import type {
  EmailAttachment,
  EmailHeader,
  EmailMessage,
} from '@/types/models';
import { Badge } from '@/app/components/ui/badge';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Button } from '@/app/components/ui/button';
import { KeyValueField } from '@/components/data-display';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import {
  ExtractionTableSection,
  formatSize,
} from './helpers';
import {
  ChevronDown,
  ChevronUp,
  Mail,
  Paperclip,
} from 'lucide-react';

type Translate = (key: string, options?: Record<string, unknown>) => string;

// Column titles are translated, so the array is built from `t` and memoized in
// the panel; the stable reference keeps DenseDataTable row memoization intact
// across selection/renders.
function emailColumns(t: Translate): DenseColumn<EmailMessage>[] {
  return [
  {
    key: 'sentAt',
    title: t('linuxArtifacts.columns.timestamp'),
    className: 'w-[150px]',
    render: (row) => row.sentAt ?? '-',
  },
  {
    key: 'from',
    title: t('analysisPanels.email.columns.from'),
    className: 'w-[180px]',
    render: (row) => row.from || '-',
  },
  {
    key: 'to',
    title: t('analysisPanels.email.columns.to'),
    className: 'w-[160px]',
    render: (row) => joinAddresses(row.to),
  },
  {
    key: 'cc',
    title: t('analysisPanels.email.columns.cc'),
    className: 'w-[120px]',
    render: (row) => joinAddresses(row.cc),
  },
  {
    key: 'bcc',
    title: t('analysisPanels.email.columns.bcc'),
    className: 'w-[120px]',
    render: (row) => joinAddresses(row.bcc),
  },
  {
    key: 'subject',
    title: t('analysisPanels.email.columns.subject'),
    className: 'min-w-[200px]',
    render: (row) => row.subject || '-',
  },
  {
    key: 'attachmentCount',
    title: t('analysisPanels.email.attachments'),
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
}

export function EmailExtractionPanelView({ model }: { model: ReturnType<typeof useEmailExtractionModel> }) {
  const { t } = useTranslation();
  const columns = useMemo(() => emailColumns(t), [t]);
  const { info, selectedArtifactId, selectedMessage, showHeaders, handleRowClick, toggleHeaders, attachmentCount } = model;

  return (
    <ExtractionTableSection
      title={t('analysis.tabs.email')}
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        [t('analysisPanels.email.total'), info.total.toString()],
        [t('analysisPanels.email.messageCount'), info.messages.length.toString()],
        [
          t('analysisPanels.email.withAttachments'),
          attachmentCount.toString(),
        ],
      ]}
    >
      <DenseDataTableFrame rowCount={info.messages.length}>
        <DenseDataTable
          rows={info.messages}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          selectedRowKey={selectedArtifactId ?? undefined}
          onRowClick={handleRowClick}
          emptyTitle={t('analysisPanels.email.emptyTitle')}
          emptyDescription={t('analysisPanels.email.emptyDescription')}
        />
      </DenseDataTableFrame>
      {selectedMessage ? (
        <EmailDetailCard
          message={selectedMessage}
          showHeaders={showHeaders}
          onToggleHeaders={toggleHeaders}
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
  const { t } = useTranslation();
  const hasHtml = Boolean(message.bodyHtml);
  const hasPlain = Boolean(message.bodyPlain);
  const defaultTab = hasPlain ? 'plain' : hasHtml ? 'html' : 'preview';

  return (
    <div className="mt-4 rounded-none bg-transparent">
      <div className="flex items-start gap-3 border-b border-forensics-divider p-3">
        <div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-none bg-forensics-panel-strong">
          <Mail className="size-4 text-forensics-text-tertiary" />
        </div>
        <div className="min-w-0 flex-1 space-y-1.5">
          <div className="text-[13px] font-light text-forensics-text">
            {message.subject || '(no subject)'}
          </div>
          <div className="grid grid-cols-1 gap-x-6 gap-y-1 text-[11px] text-forensics-text-tertiary md:grid-cols-2">
            <KeyValueField layout="inline" label="From" value={message.from} />
            <KeyValueField layout="inline" label="To" value={joinAddresses(message.to)} />
            {message.cc.length > 0 ? (
              <KeyValueField layout="inline" label="Cc" value={joinAddresses(message.cc)} />
            ) : null}
            {message.bcc.length > 0 ? (
              <KeyValueField layout="inline" label="Bcc" value={joinAddresses(message.bcc)} />
            ) : null}
            {message.replyTo ? <KeyValueField layout="inline" label="Reply-To" value={message.replyTo} /> : null}
            {message.returnPath ? (
              <KeyValueField layout="inline" label="Return-Path" value={message.returnPath} />
            ) : null}
            {message.sentAt ? <KeyValueField layout="inline" label="Sent" value={message.sentAt} /> : null}
            {message.receivedAt ? (
              <KeyValueField layout="inline" label="Received" value={message.receivedAt} />
            ) : null}
            {message.containerPath ? (
              <KeyValueField layout="inline" label="Container" value={message.containerPath} />
            ) : null}
            {message.messageClass ? (
              <KeyValueField layout="inline" label="Message Class" value={message.messageClass} />
            ) : null}
            {message.isDeleted ? (
              <KeyValueField layout="inline" label="Deleted" value={t('common.yes')} />
            ) : null}
            {message.messageId ? (
              <KeyValueField layout="inline" label="Message-ID" value={message.messageId} mono />
            ) : null}
            {message.inReplyTo ? (
              <KeyValueField layout="inline" label="In-Reply-To" value={message.inReplyTo} mono />
            ) : null}
            {message.references.length > 0 ? (
              <KeyValueField
                layout="inline"
                label="References"
                value={message.references.join(', ')}
                mono
                className="md:col-span-2"
              />
            ) : null}
            {message.xMailer ? <KeyValueField layout="inline" label="X-Mailer" value={message.xMailer} /> : null}
            {message.xOriginatingIp ? (
              <KeyValueField layout="inline" label="X-Originating-IP" value={message.xOriginatingIp} />
            ) : null}
          </div>
        </div>
      </div>

      {message.attachmentDetails.length > 0 ? (
        <div className="border-b border-forensics-divider p-3">
          <div className="mb-2 text-[11px] font-light text-forensics-text">
            {t('analysisPanels.email.attachmentsTitle', { count: message.attachmentDetails.length })}
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
            { value: 'preview', label: t('settings.sections.preview.title') },
            ...(hasPlain ? [{ value: 'plain', label: t('analysisPanels.email.tabPlain') }] : []),
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
        <div className="border-t border-forensics-line p-3">
          <Button
            variant="forensicsGhost"
            shape="row"
            aria-expanded={showHeaders}
            onClick={onToggleHeaders}
            className="h-7 gap-1 text-[11px] text-forensics-ink-3"
          >
            {showHeaders ? (
              <ChevronUp className="size-3" />
            ) : (
              <ChevronDown className="size-3" />
            )}
            {t('analysisPanels.email.rawHeaders', { count: message.headers.length })}
          </Button>
          {showHeaders ? <HeaderList headers={message.headers} /> : null}
        </div>
      ) : null}
    </div>
  );
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
  const { t } = useTranslation();
  return (
    <ScrollArea className="max-h-[240px] rounded-none bg-transparent" viewportClassName="p-3 text-[12px] leading-relaxed text-forensics-text-secondary">
      {text?.trim() ? text : <span className="text-forensics-muted-lighter">{t('analysisPanels.email.noBody')}</span>}
    </ScrollArea>
  );
}

function HtmlPreview({ html }: { html?: string }) {
  const { t } = useTranslation();
  if (!html) {
    return (
      <div className="rounded-none bg-transparent p-3 text-[12px] text-forensics-muted-lighter">
        {t('analysisPanels.email.noHtml')}
      </div>
    );
  }
  return (
    <ScrollArea className="max-h-[240px] rounded-none bg-transparent" viewportClassName="p-3">
      <pre className="whitespace-pre-wrap break-all font-mono text-[11px] text-forensics-text-secondary">
        {html}
      </pre>
    </ScrollArea>
  );
}

function HeaderList({ headers }: { headers: EmailHeader[] }) {
  return (
    <ScrollArea className="mt-2 max-h-[240px] rounded-none bg-transparent" viewportClassName="p-2">
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
    </ScrollArea>
  );
}

function joinAddresses(addresses: string[]) {
  if (!addresses || addresses.length === 0) return '-';
  return addresses.join(', ');
}
