import { Bookmark, Image, Search, UserRoundCheck, UserRoundX } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Input } from '@/app/components/ui/input';
import { InlineMedia } from '@/app/components/ui/inline-media';
import { ExternalMediaPreview } from '@/app/components/ui/external-media-preview';
import { MediaEvidenceStatus } from '@/app/components/ui/media-evidence-status';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import type {
  WeChatContact,
  WeChatFavorite,
  WeChatLoadState,
  WeChatMoment,
  WeChatSupplementalRecord,
} from '@/features/analysis/wechat/types';
import { WeChatIdentity } from './WeChatIdentity';
import { WeChatLoadFooter } from './WeChatLoadFooter';

function formatDate(value: string | undefined, locale: string): string {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function EmptyCollection({ text }: { text: string }) {
  return <div className="px-4 py-12 text-center text-[12px] text-forensics-muted">{text}</div>;
}

export function WeChatContactsView({
  contacts,
  search,
  onSearchChange,
  loadState,
}: {
  contacts: WeChatContact[];
  search: string;
  onSearchChange: (value: string) => void;
  loadState: WeChatLoadState;
}) {
  const { t } = useTranslation();
  const normalized = search.trim().toLocaleLowerCase();
  const visible = normalized
    ? contacts.filter((contact) => [
        contact.displayName,
        contact.username,
        contact.alias,
        contact.nickName,
        contact.remark,
      ].some((value) => value?.toLocaleLowerCase().includes(normalized)))
    : contacts;
  return (
    <div className="flex h-[min(68vh,680px)] min-h-[520px] flex-col border border-forensics-border">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-forensics-border bg-forensics-panel px-3">
        <Search className="size-4 text-forensics-muted" />
        <Input
          variant="search"
          inputSize="compact"
          aria-label={t('wechatWorkspace.searchContacts')}
          placeholder={t('wechatWorkspace.searchContacts')}
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
        />
      </div>
      <ScrollArea className="min-h-0 flex-1 bg-forensics-surface">
        {visible.map((contact) => (
          <article
            key={contact.artifactId}
            className="flex min-h-16 items-center gap-3 border-b border-forensics-border-light px-4 py-2"
            title={`${contact.sourcePath}\n${contact.artifactId}`}
          >
            <WeChatIdentity name={contact.displayName} large />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[12px] text-forensics-text">{contact.displayName}</span>
                {contact.deleted ? (
                  <Badge variant="outline" className="gap-1 text-[9px]">
                    <UserRoundX /> {t('wechatWorkspace.deleted')}
                  </Badge>
                ) : (
                  <UserRoundCheck className="size-3.5 text-forensics-muted" />
                )}
              </div>
              <div className="mt-0.5 truncate font-mono text-[10px] text-forensics-muted">
                {contact.username}{contact.alias ? ` · ${contact.alias}` : ''}
              </div>
              {contact.description ? (
                <div className="mt-0.5 truncate text-[10px] text-forensics-text-tertiary">{contact.description}</div>
              ) : null}
            </div>
          </article>
        ))}
        {visible.length === 0 ? <EmptyCollection text={t('wechatWorkspace.noContacts')} /> : null}
      </ScrollArea>
      <WeChatLoadFooter state={loadState} />
    </div>
  );
}

export function WeChatMomentsView({
  moments,
  loadState,
}: {
  moments: WeChatMoment[];
  loadState: WeChatLoadState;
}) {
  const { t, i18n } = useTranslation();
  return (
    <div className="flex h-[min(68vh,680px)] min-h-[520px] flex-col border border-forensics-border">
      <ScrollArea className="min-h-0 flex-1 bg-forensics-surface">
        <div className="mx-auto max-w-3xl">
          {moments.map((moment) => (
            <article
              key={moment.artifactId}
              className="flex gap-3 border-b border-forensics-border-light px-4 py-4"
              title={`${moment.sourcePath}\n${moment.artifactId}`}
            >
              <WeChatIdentity name={moment.displayName} large />
              <div className="min-w-0 flex-1">
                <div className="text-[12px] text-forensics-text">{moment.displayName}</div>
                {moment.content ? (
                  <p className="mt-1 whitespace-pre-wrap break-words text-[12px] leading-5 text-forensics-text-secondary">
                    {moment.content}
                  </p>
                ) : null}
                {moment.hasMedia ? (
                  <div className="mt-2 grid gap-1 border-l-2 border-forensics-border pl-2 text-[10px] text-forensics-muted">
                    {moment.mediaItems.length > 0 ? moment.mediaItems.map((media, index) => (
                      <div key={`${media.id ?? index}-${media.url ?? ''}`} className="min-w-0">
                        {media.inlineMedia ? (
                          <InlineMedia
                            media={media.inlineMedia}
                            alt={media.title ?? media.description ?? ''}
                            mediaClassName="max-h-72 w-full"
                          />
                        ) : media.localMedia ? (
                          <>
                            <MediaEvidenceStatus
                              status={media.localMedia.state === 'encrypted' ? 'linked-encrypted' : 'linked-unavailable'}
                              label={media.localMedia.state === 'encrypted'
                                ? t('wechatWorkspace.localMediaLinkedEncrypted')
                                : t('wechatWorkspace.localMediaLinkedUnavailable')}
                              detail={media.localMedia.storageKey ?? media.localMedia.cacheKey}
                              className="mb-2"
                            />
                            {media.thumbUrl || media.url ? (
                              <ExternalMediaPreview
                                sourceUrl={media.thumbUrl ?? media.url ?? ''}
                                alt={media.title ?? media.description ?? media.id ?? t('wechatWorkspace.mediaItem')}
                                warningLabel={t('wechatWorkspace.externalMediaWarning')}
                                unavailableLabel={t('wechatWorkspace.externalMediaUnavailable')}
                                blockedLabel={t('wechatWorkspace.externalMediaBlocked')}
                                mediaClassName="max-h-72 w-full"
                              />
                            ) : null}
                          </>
                        ) : media.thumbUrl || media.url ? (
                          <ExternalMediaPreview
                            sourceUrl={media.thumbUrl ?? media.url ?? ''}
                            alt={media.title ?? media.description ?? media.id ?? t('wechatWorkspace.mediaItem')}
                            warningLabel={t('wechatWorkspace.externalMediaWarning')}
                            unavailableLabel={t('wechatWorkspace.externalMediaUnavailable')}
                            blockedLabel={t('wechatWorkspace.externalMediaBlocked')}
                            mediaClassName="max-h-72 w-full"
                          />
                        ) : (
                          <MediaEvidenceStatus
                            status="unlinked"
                            label={media.title ?? media.description ?? media.id ?? t('wechatWorkspace.mediaItem')}
                            detail={t('wechatWorkspace.localMediaUnlinked')}
                          />
                        )}
                        {media.url || media.thumbUrl || media.urlLocator || media.thumbLocator ? (
                          <details className="mt-1 text-[9px] opacity-70">
                            <summary>{t('wechatWorkspace.mediaReference')}</summary>
                            <code className="mt-1 block max-h-16 overflow-auto break-all">
                              {media.url ?? media.thumbUrl ?? media.urlLocator ?? media.thumbLocator}
                            </code>
                          </details>
                        ) : null}
                      </div>
                    )) : (
                      <div className="flex items-center gap-2"><Image className="size-3.5" />{t('wechatWorkspace.mediaItem')}</div>
                    )}
                  </div>
                ) : null}
                {moment.likes.length > 0 ? (
                  <div className="mt-2 text-[10px] text-forensics-muted">
                    {t('wechatWorkspace.likes')}: {moment.likes.map((like) => like.nickname ?? like.username).filter(Boolean).join(', ')}
                  </div>
                ) : null}
                {moment.comments.map((comment, index) => (
                  <div key={comment.commentId ?? `${comment.username}-${index}`} className="mt-1 border-l border-forensics-border pl-2 text-[10px] text-forensics-text-secondary">
                    <span className="text-forensics-muted">
                      {comment.nickname ?? comment.username ?? t('wechatWorkspace.unknownSource')}
                      {comment.replyNickname ?? comment.replyUsername
                        ? ` ${t('wechatWorkspace.replyTo', { name: comment.replyNickname ?? comment.replyUsername })}`
                        : ''}: {' '}
                    </span>
                    {comment.content}
                  </div>
                ))}
                <div className="mt-2 font-mono text-[9px] text-forensics-muted">
                  {formatDate(moment.createTimeUtc, i18n.language)}
                  {moment.userName ? ` · ${moment.userName}` : ''}
                </div>
              </div>
            </article>
          ))}
          {moments.length === 0 ? <EmptyCollection text={t('wechatWorkspace.noMoments')} /> : null}
        </div>
      </ScrollArea>
      <WeChatLoadFooter state={loadState} />
    </div>
  );
}

export function WeChatSupplementalView({
  records,
  loadState,
  emptyText,
}: {
  records: WeChatSupplementalRecord[];
  loadState: WeChatLoadState;
  emptyText: string;
}) {
  return (
    <div className="flex h-[min(68vh,680px)] min-h-[520px] flex-col border border-forensics-border">
      <ScrollArea className="min-h-0 flex-1 bg-forensics-surface">
        <div className="grid grid-cols-1 gap-px bg-forensics-border-light md:grid-cols-2 xl:grid-cols-3">
          {records.map((record) => {
            const media = record.inlineMedia;
            return (
              <article key={record.artifactId} className="min-w-0 bg-forensics-surface p-3" title={`${record.sourcePath}\n${record.artifactId}`}>
                {media ? (
                  <InlineMedia media={media} className="mb-2" mediaClassName="h-36 w-full" />
                ) : null}
                <div className="truncate text-[11px] text-forensics-text">{record.title}</div>
                <div className="mt-1 font-mono text-[9px] text-forensics-muted">{record.table}</div>
                {media ? (
                  <div className="mt-1 break-all font-mono text-[9px] text-forensics-muted">
                    {media.mimeType}{media.sizeBytes !== undefined ? ` · ${media.sizeBytes} B` : ''}
                    {media.sha256 ? ` · ${media.sha256}` : ''}
                  </div>
                ) : (
                  <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-all font-mono text-[9px] text-forensics-text-tertiary">
                    {JSON.stringify(record.values, null, 2)}
                  </pre>
                )}
              </article>
            );
          })}
        </div>
        {records.length === 0 ? <EmptyCollection text={emptyText} /> : null}
      </ScrollArea>
      <WeChatLoadFooter state={loadState} />
    </div>
  );
}

export function WeChatFavoritesView({
  favorites,
  loadState,
}: {
  favorites: WeChatFavorite[];
  loadState: WeChatLoadState;
}) {
  const { t, i18n } = useTranslation();
  return (
    <div className="flex h-[min(68vh,680px)] min-h-[520px] flex-col border border-forensics-border">
      <ScrollArea className="min-h-0 flex-1 bg-forensics-surface">
        {favorites.map((favorite) => (
          <article
            key={favorite.artifactId}
            className="flex gap-3 border-b border-forensics-border-light px-4 py-3"
            title={`${favorite.sourcePath}\n${favorite.artifactId}`}
          >
            <div className="flex size-10 shrink-0 items-center justify-center border border-forensics-border bg-forensics-panel">
              <Bookmark className="size-4 text-forensics-muted" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 text-[11px] text-forensics-muted">
                <span>{favorite.fromDisplayName || t('wechatWorkspace.unknownSource')}</span>
                {favorite.type !== undefined ? <Badge variant="outline">{t('wechatWorkspace.type', { type: favorite.type })}</Badge> : null}
              </div>
              <p className="mt-1 whitespace-pre-wrap break-words text-[12px] leading-5 text-forensics-text-secondary">
                {favorite.content || t('wechatWorkspace.noTextContent')}
                {favorite.contentTruncated ? <span className="ml-1 text-forensics-muted">{t('wechatWorkspace.truncated')}</span> : null}
              </p>
              <div className="mt-2 font-mono text-[9px] text-forensics-muted">
                {formatDate(favorite.updateTimeUtc, i18n.language)}
                {favorite.localId !== undefined ? ` · #${favorite.localId}` : ''}
              </div>
            </div>
          </article>
        ))}
        {favorites.length === 0 ? <EmptyCollection text={t('wechatWorkspace.noFavorites')} /> : null}
      </ScrollArea>
      <WeChatLoadFooter state={loadState} />
    </div>
  );
}
