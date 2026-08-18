import type { PluginArtifactEntry } from '@/types/models';
import type {
  WeChatContact,
  WeChatConversation,
  WeChatFavorite,
  WeChatMessage,
  WeChatMediaReference,
  WeChatMoment,
  WeChatSupplementalRecord,
  WeChatInteraction,
  WeChatInlineMedia,
  WeChatLocalMediaEvidence,
} from './types';

function textAttr(entry: PluginArtifactEntry, key: string): string | undefined {
  const value = entry.attrs[key];
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

function numberAttr(entry: PluginArtifactEntry, key: string): number | undefined {
  const value = entry.attrs[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function booleanAttr(entry: PluginArtifactEntry, key: string): boolean | undefined {
  const value = entry.attrs[key];
  return typeof value === 'boolean' ? value : undefined;
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

function booleanValue(value: unknown): boolean | undefined {
  return typeof value === 'boolean' ? value : undefined;
}

function finiteNumberValue(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function opaqueLocator(value: string | undefined): string | undefined {
  if (!value) return undefined;
  const normalized = value.trim();
  return normalized.length >= 64
    && normalized.length % 2 === 0
    && /^[a-f\d]+$/i.test(normalized)
    ? normalized
    : undefined;
}

function recordArray<T>(value: unknown, map: (record: Record<string, unknown>) => T): T[] {
  return Array.isArray(value)
    ? value.map(objectValue).filter((record): record is Record<string, unknown> => Boolean(record)).map(map)
    : [];
}

function mediaReference(record: Record<string, unknown>): WeChatMediaReference {
  const url = stringValue(record.url);
  const thumbUrl = stringValue(record.thumbUrl);
  const urlLocator = stringValue(record.urlLocator) ?? opaqueLocator(url);
  const thumbLocator = stringValue(record.thumbLocator) ?? opaqueLocator(thumbUrl);
  return {
    id: stringValue(record.id),
    type: stringValue(record.type),
    title: stringValue(record.title),
    description: stringValue(record.description),
    url: urlLocator ? undefined : url,
    thumbUrl: thumbLocator ? undefined : thumbUrl,
    urlLocator,
    thumbLocator,
    md5: stringValue(record.md5),
  };
}

function interaction(record: Record<string, unknown>): WeChatInteraction {
  return {
    commentId: stringValue(record.commentId),
    username: stringValue(record.username),
    nickname: stringValue(record.nickname),
    replyUsername: stringValue(record.replyUsername),
    replyNickname: stringValue(record.replyNickname),
    replyCommentId: stringValue(record.replyCommentId),
    content: stringValue(record.content),
    createTime: stringValue(record.createTime),
    source: stringValue(record.source),
  };
}

function timestampValue(value?: string): number {
  if (!value) return 0;
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function preferredContactName(entry: PluginArtifactEntry, username: string): string {
  return textAttr(entry, 'remark')
    ?? textAttr(entry, 'nickName')
    ?? textAttr(entry, 'alias')
    ?? username;
}

export function mapWeChatContacts(entries: readonly PluginArtifactEntry[]): WeChatContact[] {
  return entries
    .map((entry) => {
      const username = textAttr(entry, 'username') ?? entry.artifactId;
      return {
        artifactId: entry.artifactId,
        username,
        displayName: preferredContactName(entry, username),
        alias: textAttr(entry, 'alias'),
        nickName: textAttr(entry, 'nickName'),
        remark: textAttr(entry, 'remark'),
        description: textAttr(entry, 'description'),
        deleted: booleanAttr(entry, 'deleted') ?? false,
        sourcePath: entry.sourcePath,
      };
    })
    .sort((left, right) => left.displayName.localeCompare(right.displayName, 'zh-CN'));
}

export function mapWeChatMessages(entries: readonly PluginArtifactEntry[]): WeChatMessage[] {
  return entries.map((entry) => {
    const rawMediaUrl = textAttr(entry, 'mediaUrl');
    const rawThumbnailUrl = textAttr(entry, 'thumbnailUrl');
    const mediaLocator = textAttr(entry, 'mediaLocator') ?? opaqueLocator(rawMediaUrl);
    const thumbnailLocator = textAttr(entry, 'thumbnailLocator') ?? opaqueLocator(rawThumbnailUrl);
    return {
      artifactId: entry.artifactId,
      talker: textAttr(entry, 'talker') ?? textAttr(entry, 'talkerTable') ?? entry.artifactId,
      senderUsername: textAttr(entry, 'senderUsername'),
      isSend: booleanAttr(entry, 'isSend'),
      localId: numberAttr(entry, 'localId'),
      serverId: numberAttr(entry, 'serverId'),
      talkerTable: textAttr(entry, 'talkerTable'),
      localType: numberAttr(entry, 'localType'),
      typeLabel: textAttr(entry, 'localTypeLabel') ?? '未知类型',
      contentText: textAttr(entry, 'contentText'),
      contentTruncated: booleanAttr(entry, 'contentTruncated') ?? false,
      mediaKind: textAttr(entry, 'mediaKind'),
      mediaUrl: mediaLocator ? undefined : rawMediaUrl,
      thumbnailUrl: thumbnailLocator ? undefined : rawThumbnailUrl,
      mediaLocator,
      thumbnailLocator,
      mediaMd5: textAttr(entry, 'mediaMd5'),
      mediaItems: recordArray(entry.attrs.mediaItems, mediaReference),
      voiceDurationMs: textAttr(entry, 'voiceDurationMs'),
      videoDurationSeconds: textAttr(entry, 'videoDurationSeconds'),
      appTitle: textAttr(entry, 'appTitle'),
      appDescription: textAttr(entry, 'appDescription'),
      appUrl: textAttr(entry, 'appUrl'),
      sourceUsername: textAttr(entry, 'sourceUsername'),
      sourceDisplayName: textAttr(entry, 'sourceDisplayName'),
      replyUsername: textAttr(entry, 'replyUsername'),
      replyNickname: textAttr(entry, 'replyNickname'),
      xmlText: textAttr(entry, 'xmlText'),
      sourceXmlText: textAttr(entry, 'sourceXmlText'),
      packedInfoXmlText: textAttr(entry, 'packedInfoXmlText'),
      sourceContent: textAttr(entry, 'sourceContent'),
      packedInfoText: textAttr(entry, 'packedInfoText'),
      xmlFields: objectValue(entry.attrs.xmlFields),
      sourceXmlFields: objectValue(entry.attrs.sourceXmlFields),
      packedInfoXmlFields: objectValue(entry.attrs.packedInfoXmlFields),
      referencedMessage: objectValue(entry.attrs.referencedMessage),
      attrs: entry.attrs,
      createTimeUtc: textAttr(entry, 'createTimeUtc'),
      sourcePath: entry.sourcePath,
    };
  });
}

export function buildWeChatConversations(
  sessionEntries: readonly PluginArtifactEntry[],
  messages: readonly WeChatMessage[],
  contacts: readonly WeChatContact[],
): WeChatConversation[] {
  const contactsByUsername = new Map(contacts.map((contact) => [contact.username, contact]));
  const conversations = new Map<string, WeChatConversation>();
  for (const entry of sessionEntries) {
    const talker = textAttr(entry, 'username') ?? entry.artifactId;
    conversations.set(talker, {
      talker,
      displayName: contactsByUsername.get(talker)?.displayName ?? talker,
      summary: textAttr(entry, 'summary') ?? '',
      unreadCount: numberAttr(entry, 'unreadCount') ?? 0,
      hidden: booleanAttr(entry, 'isHidden') ?? false,
      lastTimestampUtc: textAttr(entry, 'lastTimestampUtc'),
      messages: [],
    });
  }
  for (const message of messages) {
    const senderDisplayName = message.sourceDisplayName ?? (message.senderUsername
      ? contactsByUsername.get(message.senderUsername)?.displayName ?? message.senderUsername
      : undefined);
    const displayedMessage = { ...message, senderDisplayName };
    const conversation = conversations.get(message.talker) ?? {
      talker: message.talker,
      displayName: contactsByUsername.get(message.talker)?.displayName ?? message.talker,
      summary: '',
      unreadCount: 0,
      hidden: false,
      messages: [],
    };
    conversation.messages.push(displayedMessage);
    if (timestampValue(message.createTimeUtc) > timestampValue(conversation.lastTimestampUtc)) {
      conversation.lastTimestampUtc = message.createTimeUtc;
    }
    conversations.set(message.talker, conversation);
  }
  for (const conversation of conversations.values()) {
    conversation.messages.sort((left, right) => {
      const timeOrder = timestampValue(left.createTimeUtc) - timestampValue(right.createTimeUtc);
      return timeOrder || (left.localId ?? 0) - (right.localId ?? 0);
    });
    const lastMessage = conversation.messages.at(-1);
    if (!conversation.summary && lastMessage) {
      conversation.summary = lastMessage.contentText ?? lastMessage.typeLabel;
    }
  }
  return [...conversations.values()].sort((left, right) => {
    const timeOrder = timestampValue(right.lastTimestampUtc) - timestampValue(left.lastTimestampUtc);
    return timeOrder || left.displayName.localeCompare(right.displayName, 'zh-CN');
  });
}

export function mapWeChatMoments(
  entries: readonly PluginArtifactEntry[],
  contacts: readonly WeChatContact[],
): WeChatMoment[] {
  const names = new Map(contacts.map((contact) => [contact.username, contact.displayName]));
  return entries
    .map((entry) => {
      const userName = textAttr(entry, 'userName') ?? '';
      return {
        artifactId: entry.artifactId,
        userName,
        displayName: names.get(userName) ?? userName,
        content: textAttr(entry, 'contentDesc'),
        createTimeUtc: textAttr(entry, 'createTimeUtc'),
        hasMedia: booleanAttr(entry, 'hasMedia') ?? false,
        mediaItems: recordArray(entry.attrs.mediaItems, mediaReference),
        likes: recordArray(entry.attrs.likes, interaction),
        comments: recordArray(entry.attrs.comments, interaction),
        sourcePath: entry.sourcePath,
      };
    })
    .sort((left, right) => timestampValue(right.createTimeUtc) - timestampValue(left.createTimeUtc));
}

export function mapWeChatSupplemental(
  entries: readonly PluginArtifactEntry[],
): WeChatSupplementalRecord[] {
  return entries.map((entry) => {
    const values = objectValue(entry.attrs.values) ?? entry.attrs;
    const inlineMedia = findInlineMedia(values);
    return {
      artifactId: entry.artifactId,
      title: entry.title,
      table: textAttr(entry, 'table') ?? '',
      values,
      sourcePath: entry.sourcePath,
      inlineMedia,
      localMedia: localMediaEvidence(
        values,
        textAttr(entry, 'table') ?? '',
        entry.sourcePath,
        inlineMedia,
      ),
    };
  });
}

function localMediaEvidence(
  values: Record<string, unknown>,
  table: string,
  sourcePath: string,
  inlineMedia?: WeChatInlineMedia,
): WeChatLocalMediaEvidence | undefined {
  const pathKind = stringNestedField(values, ['localPathKind']);
  const storageKey = stringNestedField(values, ['storageKey'])?.toLocaleLowerCase();
  const cacheKey = stringNestedField(values, ['cacheKey'])?.toLocaleLowerCase();
  if (table !== 'LocalMediaFile' && !pathKind) return undefined;
  if (!storageKey && !cacheKey && !pathKind) return undefined;
  const encrypted = booleanValue(values.encrypted);
  return {
    state: inlineMedia ? 'decoded' : encrypted === true ? 'encrypted' : 'unavailable',
    storageKey,
    cacheKey,
    pathKind,
    encryptedSizeBytes: finiteNumberValue(values.encryptedSizeBytes),
    encryptedSha256: stringValue(values.encryptedSha256),
    plainMd5: stringValue(values.plainMd5)?.toLocaleLowerCase(),
    sourcePath,
  };
}

function findInlineMedia(value: unknown, depth = 0): WeChatSupplementalRecord['inlineMedia'] {
  if (depth > 4) return undefined;
  const object = objectValue(value);
  if (object) {
    const mimeType = stringValue(object.mimeType);
    const dataBase64 = stringValue(object.inlineDataBase64);
    if (mimeType && dataBase64) {
      return {
        mimeType,
        dataBase64,
        sha256: stringValue(object.sha256),
        sizeBytes: typeof object.sizeBytes === 'number' ? object.sizeBytes : undefined,
      };
    }
    for (const nested of Object.values(object)) {
      const found = findInlineMedia(nested, depth + 1);
      if (found) return found;
    }
  }
  if (Array.isArray(value)) {
    for (const nested of value) {
      const found = findInlineMedia(nested, depth + 1);
      if (found) return found;
    }
  }
  return undefined;
}

function nestedField(value: unknown, names: readonly string[], depth = 0): unknown {
  if (depth > 4) return undefined;
  const object = objectValue(value);
  if (object) {
    const match = Object.entries(object).find(([key]) =>
      names.some((name) => key.toLocaleLowerCase() === name.toLocaleLowerCase()));
    if (match) return match[1];
    for (const nested of Object.values(object)) {
      const found = nestedField(nested, names, depth + 1);
      if (found !== undefined) return found;
    }
  }
  if (Array.isArray(value)) {
    for (const nested of value) {
      const found = nestedField(nested, names, depth + 1);
      if (found !== undefined) return found;
    }
  }
  return undefined;
}

function numericNestedField(value: unknown, names: readonly string[]): number | undefined {
  const found = nestedField(value, names);
  if (typeof found === 'number' && Number.isFinite(found)) return found;
  if (typeof found === 'string' && /^\d+$/.test(found.trim())) return Number(found);
  return undefined;
}

function stringNestedField(value: unknown, names: readonly string[]): string | undefined {
  return stringValue(nestedField(value, names));
}

function tableHash(value: string | undefined): string | undefined {
  return value?.match(/[a-f\d]{32}/i)?.[0]?.toLocaleLowerCase();
}

function addUniqueIndex<K>(map: Map<K, WeChatSupplementalRecord | null>, key: K | undefined, record: WeChatSupplementalRecord) {
  if (key === undefined) return;
  if (!map.has(key)) {
    map.set(key, record);
    return;
  }
  if (map.get(key) !== record) map.set(key, null);
}

function recordStorageKey(record: WeChatSupplementalRecord): string | undefined {
  return record.localMedia?.storageKey
    ?? stringNestedField(record.values, ['storageKey'])?.toLocaleLowerCase();
}

function linkLocalMediaRecords(
  records: readonly WeChatSupplementalRecord[],
): WeChatSupplementalRecord[] {
  const byStorageKey = new Map<string, WeChatSupplementalRecord | null>();
  for (const record of records) {
    if (!record.localMedia?.storageKey) continue;
    addUniqueIndex(byStorageKey, record.localMedia.storageKey, record);
  }
  return records.map((record) => {
    const storageKey = recordStorageKey(record);
    const localRecord = storageKey ? byStorageKey.get(storageKey) : undefined;
    if (!localRecord || localRecord === record) return record;
    return {
      ...record,
      inlineMedia: record.inlineMedia ?? localRecord.inlineMedia,
      localMedia: record.localMedia ?? localRecord.localMedia,
    };
  });
}

export function linkWeChatMessageMedia(
  messages: readonly WeChatMessage[],
  records: readonly WeChatSupplementalRecord[],
): WeChatMessage[] {
  const linkedRecords = linkLocalMediaRecords(records);
  const byServerId = new Map<number, WeChatSupplementalRecord | null>();
  const byLocalId = new Map<number, WeChatSupplementalRecord | null>();
  const byLocalAndTable = new Map<string, WeChatSupplementalRecord | null>();
  const byLocalAndTalker = new Map<string, WeChatSupplementalRecord | null>();
  const byMd5 = new Map<string, WeChatSupplementalRecord | null>();
  for (const record of linkedRecords) {
    const localId = numericNestedField(record.values, ['local_id', 'localId', 'msg_local_id', 'messageLocalId']);
    const serverId = numericNestedField(record.values, ['server_id', 'serverId', 'msg_server_id', 'messageServerId']);
    const hash = tableHash(record.table)
      ?? tableHash(stringNestedField(record.values, ['table', 'table_name', 'message_table']));
    const talker = stringNestedField(record.values, ['talker', 'username'])?.toLocaleLowerCase();
    addUniqueIndex(byServerId, serverId, record);
    addUniqueIndex(byLocalId, localId, record);
    addUniqueIndex(byLocalAndTable, localId !== undefined && hash ? `${hash}:${localId}` : undefined, record);
    addUniqueIndex(byLocalAndTalker, localId !== undefined && talker ? `${talker}:${localId}` : undefined, record);
    for (const key of ['md5', 'media_md5', 'mediaMd5', 'plainMd5', 'storageKey', 'cacheKey']) {
      const md5 = stringNestedField(record.values, [key])?.toLocaleLowerCase();
      addUniqueIndex(byMd5, md5, record);
    }
  }
  return messages.map((message) => {
    const hash = tableHash(message.talkerTable);
    const localTableKey = message.localId !== undefined && hash ? `${hash}:${message.localId}` : undefined;
    const record = (message.mediaMd5 ? byMd5.get(message.mediaMd5.toLocaleLowerCase()) : undefined)
      ?? (message.serverId !== undefined ? byServerId.get(message.serverId) : undefined)
      ?? (localTableKey ? byLocalAndTable.get(localTableKey) : undefined)
      ?? (message.localId !== undefined
        ? byLocalAndTalker.get(`${message.talker.toLocaleLowerCase()}:${message.localId}`)
        : undefined)
      ?? (message.localId !== undefined ? byLocalId.get(message.localId) : undefined);
    return record
      ? {
          ...message,
          ...(record.inlineMedia ? { inlineMedia: record.inlineMedia } : {}),
          ...(record.localMedia ? { localMedia: record.localMedia } : {}),
        }
      : { ...message };
  });
}

export function linkWeChatMomentMedia(
  moments: readonly WeChatMoment[],
  records: readonly WeChatSupplementalRecord[],
): WeChatMoment[] {
  const byMd5 = new Map<string, WeChatSupplementalRecord | null>();
  for (const record of linkLocalMediaRecords(records)) {
    for (const key of ['plainMd5', 'storageKey', 'cacheKey']) {
      const md5 = stringNestedField(record.values, [key])?.toLocaleLowerCase();
      addUniqueIndex(byMd5, md5, record);
    }
  }
  return moments.map((moment) => ({
    ...moment,
    mediaItems: moment.mediaItems.map((item) => {
      const record = item.md5 ? byMd5.get(item.md5.toLocaleLowerCase()) : undefined;
      return record
        ? {
            ...item,
            ...(record.inlineMedia ? { inlineMedia: record.inlineMedia } : {}),
            ...(record.localMedia ? { localMedia: record.localMedia } : {}),
          }
        : item;
    }),
  }));
}

export function mapWeChatFavorites(
  entries: readonly PluginArtifactEntry[],
  contacts: readonly WeChatContact[],
): WeChatFavorite[] {
  const names = new Map(contacts.map((contact) => [contact.username, contact.displayName]));
  return entries
    .map((entry) => {
      const fromUsername = textAttr(entry, 'fromUsr');
      return {
        artifactId: entry.artifactId,
        localId: numberAttr(entry, 'localId'),
        type: numberAttr(entry, 'type'),
        fromUsername,
        fromDisplayName: (fromUsername && names.get(fromUsername)) ?? fromUsername ?? '',
        realChatName: textAttr(entry, 'realChatName'),
        content: textAttr(entry, 'contentText'),
        contentTruncated: booleanAttr(entry, 'contentTruncated') ?? false,
        updateTimeUtc: textAttr(entry, 'updateTimeUtc'),
        sourcePath: entry.sourcePath,
      };
    })
    .sort((left, right) => timestampValue(right.updateTimeUtc) - timestampValue(left.updateTimeUtc));
}
