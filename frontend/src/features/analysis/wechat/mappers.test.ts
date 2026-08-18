import { describe, expect, it } from 'vitest';
import type { PluginArtifactEntry } from '@/types/models';
import {
  buildWeChatConversations,
  linkWeChatMessageMedia,
  linkWeChatMomentMedia,
  mapWeChatContacts,
  mapWeChatFavorites,
  mapWeChatMessages,
  mapWeChatMoments,
  mapWeChatSupplemental,
} from './mappers';

function entry(artifactId: string, attrs: Record<string, unknown>): PluginArtifactEntry {
  return {
    artifactId,
    fileId: `file-${artifactId}`,
    sourcePath: `[P2]/wechat/${artifactId}.db`,
    title: artifactId,
    summary: artifactId,
    attrs,
    createdAt: '2026-08-01T00:00:00Z',
  };
}

describe('WeChat artifact mappers', () => {
  it('joins contact names into sessions and orders loaded messages chronologically', () => {
    const contacts = mapWeChatContacts([
      entry('contact-1', { username: 'wxid_friend', nickName: '朋友昵称', remark: '案件联系人' }),
    ]);
    const messages = mapWeChatMessages([
      entry('message-2', {
        talker: 'wxid_friend',
        senderUsername: 'wxid_friend',
        isSend: false,
        localId: 2,
        localType: 1,
        localTypeLabel: '文本',
        contentText: '后发消息',
        createTimeUtc: '2026-08-01T10:02:00Z',
      }),
      entry('message-1', {
        talker: 'wxid_friend',
        senderUsername: 'wxid_owner',
        isSend: true,
        localId: 1,
        localType: 1,
        localTypeLabel: '文本',
        contentText: '先发消息',
        createTimeUtc: '2026-08-01T10:01:00Z',
      }),
    ]);
    const conversations = buildWeChatConversations([
      entry('session-1', {
        username: 'wxid_friend',
        summary: '会话摘要',
        unreadCount: 3,
        lastTimestampUtc: '2026-08-01T10:00:00Z',
      }),
    ], messages, contacts);

    expect(conversations).toHaveLength(1);
    expect(conversations[0]).toMatchObject({
      talker: 'wxid_friend',
      displayName: '案件联系人',
      unreadCount: 3,
      lastTimestampUtc: '2026-08-01T10:02:00Z',
    });
    expect(conversations[0].messages.map((message) => message.artifactId))
      .toEqual(['message-1', 'message-2']);
    expect(conversations[0].messages[1].senderUsername).toBe('wxid_friend');
    expect(conversations[0].messages[1].senderDisplayName).toBe('案件联系人');
  });

  it('creates conversations for messages that have no session row', () => {
    const messages = mapWeChatMessages([
      entry('message-1', { talker: 'filehelper', contentText: '孤立消息' }),
    ]);
    const conversations = buildWeChatConversations([], messages, []);
    expect(conversations[0]).toMatchObject({
      talker: 'filehelper',
      displayName: 'filehelper',
      summary: '孤立消息',
    });
  });

  it('maps moments and favorites with contact display names', () => {
    const contacts = mapWeChatContacts([
      entry('contact-1', { username: 'wxid_friend', remark: '证人甲' }),
    ]);
    const moments = mapWeChatMoments([
      entry('moment-1', {
        userName: 'wxid_friend',
        contentDesc: '朋友圈正文',
        hasMedia: true,
        mediaItems: [{ id: 'm1', url: 'https://media.invalid/1' }],
        likes: [{ username: 'wxid_like', nickname: '点赞者' }],
        comments: [{ username: 'wxid_comment', content: '评论正文' }],
        createTimeUtc: '2026-08-01T11:00:00Z',
      }),
    ], contacts);
    const favorites = mapWeChatFavorites([
      entry('favorite-1', {
        fromUsr: 'wxid_friend',
        contentText: '收藏正文',
        contentTruncated: true,
        updateTimeUtc: '2026-08-01T12:00:00Z',
      }),
    ], contacts);

    expect(moments[0]).toMatchObject({
      displayName: '证人甲',
      hasMedia: true,
      mediaItems: [{ id: 'm1', url: 'https://media.invalid/1' }],
      likes: [{ username: 'wxid_like', nickname: '点赞者' }],
      comments: [{ username: 'wxid_comment', content: '评论正文' }],
    });
    expect(favorites[0]).toMatchObject({
      fromDisplayName: '证人甲',
      content: '收藏正文',
      contentTruncated: true,
    });
  });

  it('finds inline media in supplemental resource values', () => {
    const records = mapWeChatSupplemental([entry('media-1', {
      table: 'MediaResource',
      values: {
        payload: {
          mimeType: 'image/png',
          inlineDataBase64: 'iVBORw0KGgo=',
          sha256: 'a'.repeat(64),
          sizeBytes: 8,
        },
      },
    })]);
    expect(records[0].inlineMedia).toEqual({
      mimeType: 'image/png',
      dataBase64: 'iVBORw0KGgo=',
      sha256: 'a'.repeat(64),
      sizeBytes: 8,
    });
  });

  it('links inline resource images to the matching message table and local id', () => {
    const tableHash = '0123456789abcdef0123456789abcdef';
    const messages = mapWeChatMessages([entry('message-1', {
      talker: 'gh_owner',
      talkerTable: `Msg_${tableHash}`,
      localId: 7,
      serverId: 99,
      localType: 3,
      mediaKind: 'image',
      sourceDisplayName: '案件号主',
      replyUsername: 'wxid_reader',
      xmlText: '图片答复',
    })]);
    const media = mapWeChatSupplemental([entry('media-1', {
      table: `Resource_${tableHash}`,
      values: {
        local_id: 7,
        payload: {
          mimeType: 'image/png',
          inlineDataBase64: 'iVBORw0KGgo=',
        },
      },
    })]);
    const linked = linkWeChatMessageMedia(messages, media);
    expect(linked[0]).toMatchObject({
      sourceDisplayName: '案件号主',
      replyUsername: 'wxid_reader',
      xmlText: '图片答复',
      inlineMedia: {
        mimeType: 'image/png',
        dataBase64: 'iVBORw0KGgo=',
      },
    });
  });

  it('links local media to chats and Moments by recovered media hash', () => {
    const md5 = 'c83987177522cc563fe6724f650b28fa';
    const messages = mapWeChatMessages([entry('message-1', {
      talker: 'wxid_friend',
      localType: 3,
      mediaKind: 'image',
      mediaMd5: md5,
    })]);
    const moments = mapWeChatMoments([entry('moment-1', {
      userName: 'wxid_friend',
      hasMedia: true,
      mediaItems: [{ id: 'm1', md5 }],
    })], []);
    const media = mapWeChatSupplemental([entry('media-1', {
      table: 'LocalMediaFile',
      values: {
        storageKey: '11111111111111111111111111111111',
        plainMd5: md5,
        media: {
          mimeType: 'image/jpeg',
          inlineDataBase64: '/9j/4A==',
        },
      },
    })]);

    expect(linkWeChatMessageMedia(messages, media)[0].inlineMedia).toMatchObject({
      mimeType: 'image/jpeg',
    });
    expect(linkWeChatMomentMedia(moments, media)[0].mediaItems[0].inlineMedia).toMatchObject({
      mimeType: 'image/jpeg',
    });
  });

  it('preserves an encrypted local cache link when the image key is unavailable', () => {
    const storageKey = '83d35dbfebf20beff6c1e711168205ee';
    const messages = mapWeChatMessages([entry('message-1', {
      talker: 'wxid_friend',
      localId: 15,
      localType: 3,
      mediaKind: 'image',
    })]);
    const media = mapWeChatSupplemental([
      entry('resource-1', {
        table: 'MessageResourceDetail',
        values: {
          localId: 15,
          storageKey,
        },
      }),
      entry('local-1', {
        table: 'LocalMediaFile',
        values: {
          localPathKind: 'messageAttachment',
          storageKey,
          encrypted: true,
          encryptedSizeBytes: 5864,
          encryptedSha256: 'b'.repeat(64),
        },
      }),
    ]);

    expect(linkWeChatMessageMedia(messages, media)[0].localMedia).toMatchObject({
      state: 'encrypted',
      storageKey,
      encryptedSizeBytes: 5864,
    });
  });

  it('keeps opaque hex locators out of displayable media URLs from old plugin records', () => {
    const locator = '30'.repeat(96);
    const message = mapWeChatMessages([entry('message-1', {
      mediaKind: 'image',
      mediaUrl: locator,
      mediaItems: [{ id: 'image-1', url: locator }],
    })])[0];

    expect(message.mediaUrl).toBeUndefined();
    expect(message.mediaLocator).toBe(locator);
    expect(message.mediaItems[0]).toMatchObject({ urlLocator: locator });
    expect(message.mediaItems[0].url).toBeUndefined();
  });
});
