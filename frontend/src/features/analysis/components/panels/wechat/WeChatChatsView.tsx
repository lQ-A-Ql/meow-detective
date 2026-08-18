import { Image, Link, MessageSquare, Mic, Search, Smile, Video } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { InlineMedia } from '@/app/components/ui/inline-media';
import { ExternalMediaPreview } from '@/app/components/ui/external-media-preview';
import { MediaEvidenceStatus } from '@/app/components/ui/media-evidence-status';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type {
  WeChatConversation,
  WeChatLoadState,
  WeChatMessage,
} from '@/features/analysis/wechat/types';
import { WeChatIdentity } from './WeChatIdentity';
import { WeChatLoadFooter } from './WeChatLoadFooter';

function formatTime(value: string | undefined, locale: string, dateOnly = false): string {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, dateOnly
    ? { year: 'numeric', month: '2-digit', day: '2-digit' }
    : { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(date);
}

function messageIcon(message: WeChatMessage) {
  switch (message.localType) {
    case 3:
      return <Image />;
    case 34:
      return <Mic />;
    case 43:
      return <Video />;
    case 47:
      return <Smile />;
    case 49:
      return <Link />;
    default:
      return <MessageSquare />;
  }
}

function MessageBody({ message }: { message: WeChatMessage }) {
  const { t } = useTranslation();
  const replyTarget = message.replyNickname ?? message.replyUsername;
  const readableText = message.xmlText ?? message.sourceXmlText ?? message.contentText;
  const supplementalReplies = [message.sourceXmlText, message.packedInfoXmlText]
    .filter((value): value is string => Boolean(value && value !== readableText))
    .filter((value, index, values) => values.indexOf(value) === index);
  const referencedContent = typeof message.referencedMessage?.content === 'string'
    ? message.referencedMessage.content
    : undefined;
  const opaqueLocator = message.mediaLocator ?? message.thumbnailLocator;
  if (message.mediaKind) {
    const detail = message.appTitle
      ?? message.appDescription
      ?? message.mediaItems[0]?.title
      ?? message.mediaItems[0]?.description;
    const reference = message.appUrl
      ?? message.mediaUrl
      ?? message.mediaItems[0]?.url
      ?? message.thumbnailUrl
      ?? message.mediaMd5;
    const externalMediaUrl = message.thumbnailUrl ?? message.mediaUrl;
    const localMediaStatus = message.localMedia
      ? message.localMedia.state === 'encrypted' ? 'linked-encrypted' : 'linked-unavailable'
      : undefined;
    const localMediaStatusLabel = message.localMedia
      ? message.localMedia.state === 'encrypted'
        ? t('wechatWorkspace.localMediaLinkedEncrypted')
        : t('wechatWorkspace.localMediaLinkedUnavailable')
      : undefined;
    const duration = message.voiceDurationMs
      ? `${message.voiceDurationMs} ms`
      : message.videoDurationSeconds
        ? `${message.videoDurationSeconds} s`
        : undefined;
    return (
      <div className="min-w-36 text-left text-[11px]">
        {message.inlineMedia ? (
          <InlineMedia
            media={message.inlineMedia}
            alt={detail ?? message.typeLabel}
            className="mb-2"
            mediaClassName="max-h-72 w-full"
          />
        ) : localMediaStatus ? (
          <>
            <MediaEvidenceStatus
              status={localMediaStatus}
              label={localMediaStatusLabel ?? t('wechatWorkspace.localMediaLinkedUnavailable')}
              detail={message.localMedia?.storageKey}
              className="mb-2"
            />
            {externalMediaUrl ? (
              <ExternalMediaPreview
                sourceUrl={externalMediaUrl}
                alt={detail ?? message.typeLabel}
                warningLabel={t('wechatWorkspace.externalMediaWarning')}
                unavailableLabel={t('wechatWorkspace.externalMediaUnavailable')}
                blockedLabel={t('wechatWorkspace.externalMediaBlocked')}
                className="mb-2"
                mediaClassName="max-h-72 w-full"
              />
            ) : null}
          </>
        ) : externalMediaUrl ? (
          <ExternalMediaPreview
            sourceUrl={externalMediaUrl}
            alt={detail ?? message.typeLabel}
            warningLabel={t('wechatWorkspace.externalMediaWarning')}
            unavailableLabel={t('wechatWorkspace.externalMediaUnavailable')}
            blockedLabel={t('wechatWorkspace.externalMediaBlocked')}
            className="mb-2"
            mediaClassName="max-h-72 w-full"
          />
        ) : null}
        <div className="flex items-center gap-2 [&_svg]:size-4">
          {messageIcon(message)}
          <span>{message.typeLabel}</span>
          {duration ? <span className="font-mono text-[9px] opacity-70">{duration}</span> : null}
        </div>
        {detail ? <div className="mt-1 break-words text-[12px]">{detail}</div> : null}
        {!message.inlineMedia && !externalMediaUrl && !localMediaStatus ? (
          <MediaEvidenceStatus
            status="unlinked"
            label={t('wechatWorkspace.localMediaUnlinked')}
            className="mt-2"
          />
        ) : null}
        {replyTarget ? (
          <div className="mt-1 text-[10px] text-forensics-muted">
            {t('wechatWorkspace.replyTo', { name: replyTarget })}
          </div>
        ) : null}
        {referencedContent ? (
          <div className="mt-1 border-l border-forensics-border-strong pl-2 text-[10px] opacity-80">
            {referencedContent}
          </div>
        ) : null}
        {supplementalReplies.map((reply) => (
          <p key={reply} className="mt-1 whitespace-pre-wrap break-words text-[12px] leading-5">
            {reply}
          </p>
        ))}
        {reference ? <div className="mt-1 break-all font-mono text-[9px] opacity-70">{reference}</div> : null}
        {opaqueLocator ? (
          <details className="mt-1 text-[9px] opacity-70">
            <summary>{t('wechatWorkspace.opaqueLocator')}</summary>
            <code className="mt-1 block max-h-16 overflow-auto break-all">{opaqueLocator}</code>
          </details>
        ) : null}
      </div>
    );
  }
  if (readableText) {
    return (
      <div>
        {replyTarget ? (
          <div className="mb-1 text-[10px] text-forensics-muted">
            {t('wechatWorkspace.replyTo', { name: replyTarget })}
          </div>
        ) : null}
        <p className="whitespace-pre-wrap break-words text-[12px] leading-5">
          {readableText}
          {message.contentTruncated ? (
            <span className="ml-1 text-forensics-muted">{t('wechatWorkspace.truncated')}</span>
          ) : null}
        </p>
        {supplementalReplies.map((reply) => (
          <p key={reply} className="mt-1 whitespace-pre-wrap break-words border-l border-forensics-border-strong pl-2 text-[11px] leading-5">
            {reply}
          </p>
        ))}
      </div>
    );
  }
  return (
    <div className="flex items-center gap-2 text-[11px] text-forensics-muted [&_svg]:size-4">
      {messageIcon(message)}
      <span>{message.typeLabel}</span>
    </div>
  );
}

function MessageStream({ conversation }: { conversation: WeChatConversation }) {
  const { t, i18n } = useTranslation();
  if (conversation.messages.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-[12px] text-forensics-muted">
        {t('wechatWorkspace.noLoadedMessages')}
      </div>
    );
  }
  let previousDay = '';
  return (
    <div className="space-y-3 p-4">
      {conversation.messages.map((message) => {
        const day = formatTime(message.createTimeUtc, i18n.language, true);
        const showDay = Boolean(day && day !== previousDay);
        const direction = message.isSend === true
          ? 'outgoing'
          : message.isSend === false
            ? 'incoming'
            : 'unknown';
        previousDay = day || previousDay;
        return (
          <div key={message.artifactId}>
            {showDay ? (
              <div className="mb-3 text-center font-mono text-[10px] text-forensics-muted">{day}</div>
            ) : null}
            <article
              className={`flex ${direction === 'outgoing' ? 'justify-end' : direction === 'incoming' ? 'justify-start' : 'justify-center'}`}
              title={`${message.sourcePath}\n${message.artifactId}`}
            >
              <div className={`max-w-[78%] ${direction === 'outgoing' ? 'text-right' : 'text-left'}`}>
                {direction === 'incoming' && message.senderUsername ? (
                  <div className="mb-1 text-[10px] text-forensics-muted">
                    {message.senderDisplayName ?? message.senderUsername}
                  </div>
                ) : null}
                {direction === 'unknown' ? (
                  <div className="mb-1 text-center text-[9px] text-forensics-muted">{t('wechatWorkspace.unknownDirection')}</div>
                ) : null}
                <div className={direction === 'outgoing'
                  ? 'border border-forensics-border-strong bg-forensics-800 px-3 py-2 text-white'
                  : direction === 'incoming'
                    ? 'border border-forensics-border bg-forensics-surface px-3 py-2 text-forensics-text'
                    : 'border border-dashed border-forensics-border-strong bg-forensics-panel px-3 py-2 text-forensics-text'}>
                  <MessageBody message={message} />
                </div>
                <div className="mt-1 font-mono text-[9px] text-forensics-muted">
                  {formatTime(message.createTimeUtc, i18n.language)}
                  {message.localId !== undefined ? `  #${message.localId}` : ''}
                </div>
              </div>
            </article>
          </div>
        );
      })}
    </div>
  );
}

export function WeChatChatsView({
  conversations,
  selectedConversation,
  search,
  onSearchChange,
  onSelectConversation,
  loadState,
}: {
  conversations: WeChatConversation[];
  selectedConversation?: WeChatConversation;
  search: string;
  onSearchChange: (value: string) => void;
  onSelectConversation: (talker: string) => void;
  loadState: WeChatLoadState;
}) {
  const { t, i18n } = useTranslation();
  return (
    <div className="grid h-[min(68vh,680px)] min-h-[520px] grid-cols-1 grid-rows-[220px_minmax(0,1fr)] border border-forensics-border lg:grid-cols-[280px_minmax(0,1fr)] lg:grid-rows-1">
      <aside className="flex min-h-0 flex-col border-b border-forensics-border bg-forensics-panel lg:border-r lg:border-b-0">
        <div className="flex h-10 items-center gap-2 border-b border-forensics-border px-3">
          <Search className="size-4 shrink-0 text-forensics-muted" />
          <Input
            variant="search"
            inputSize="compact"
            aria-label={t('wechatWorkspace.searchChats')}
            placeholder={t('wechatWorkspace.searchChats')}
            value={search}
            onChange={(event) => onSearchChange(event.target.value)}
          />
        </div>
        <ScrollArea className="min-h-0 flex-1">
          <div className="py-1">
            {conversations.map((conversation) => (
              <Button
                key={conversation.talker}
                variant="treeControl"
                size="menuItem"
                data-active={conversation.talker === selectedConversation?.talker}
                className="h-14 w-full justify-start px-3"
                onClick={() => onSelectConversation(conversation.talker)}
              >
                <WeChatIdentity name={conversation.displayName} />
                <span className="min-w-0 flex-1 text-left">
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate text-[12px] text-forensics-text">{conversation.displayName}</span>
                    <span className="shrink-0 font-mono text-[9px] text-forensics-muted">
                      {formatTime(conversation.lastTimestampUtc, i18n.language)}
                    </span>
                  </span>
                  <span className="mt-0.5 flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-[10px] text-forensics-muted">
                      {conversation.summary || conversation.talker}
                    </span>
                    {conversation.unreadCount > 0 ? (
                      <span className="min-w-4 border border-forensics-border-strong px-1 text-center font-mono text-[9px] text-forensics-text-secondary">
                        {conversation.unreadCount}
                      </span>
                    ) : null}
                  </span>
                </span>
              </Button>
            ))}
            {conversations.length === 0 ? (
              <div className="px-3 py-6 text-center text-[11px] text-forensics-muted">
                {t('wechatWorkspace.noConversations')}
              </div>
            ) : null}
          </div>
        </ScrollArea>
      </aside>
      <section className="flex min-h-0 flex-col bg-forensics-panel-strong">
        {selectedConversation ? (
          <>
            <header className="flex h-12 shrink-0 items-center gap-3 border-b border-forensics-border bg-forensics-surface px-4">
              <WeChatIdentity name={selectedConversation.displayName} />
              <div className="min-w-0">
                <div className="truncate text-[12px] text-forensics-text">{selectedConversation.displayName}</div>
                <div className="truncate font-mono text-[9px] text-forensics-muted">
                  {selectedConversation.talker} · {t('wechatWorkspace.messageCount', { count: selectedConversation.messages.length })}
                </div>
              </div>
            </header>
            <ScrollArea className="min-h-0 flex-1">
              <MessageStream conversation={selectedConversation} />
            </ScrollArea>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center text-[12px] text-forensics-muted">
            {t('wechatWorkspace.noConversations')}
          </div>
        )}
        <WeChatLoadFooter state={loadState} />
      </section>
    </div>
  );
}
