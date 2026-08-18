import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { WeChatWorkspaceModel } from '@/features/analysis/wechat/types';
import { WeChatWorkspace } from './WeChatWorkspace';

const loadState = {
  loaded: 1,
  total: 3,
  hasMore: true,
  loadingMore: false,
  failed: false,
  loadMore: vi.fn(),
  retry: vi.fn(),
};

function modelFixture(overrides: Partial<WeChatWorkspaceModel> = {}): WeChatWorkspaceModel {
  const conversation = {
    talker: 'wxid_friend',
    displayName: '案件联系人',
    summary: '最近一条消息',
    unreadCount: 2,
    hidden: false,
    lastTimestampUtc: '2026-08-01T10:02:00Z',
    messages: [{
      artifactId: 'message-1',
      talker: 'wxid_friend',
      senderUsername: 'wxid_friend',
      isSend: false,
      localId: 1,
      localType: 1,
      typeLabel: '文本',
      contentText: '测试聊天正文',
      contentTruncated: false,
      mediaItems: [],
      attrs: {},
      createTimeUtc: '2026-08-01T10:02:00Z',
      sourcePath: '[P2]/wechat/message_0.db',
    }],
  };
  return {
    activeView: 'chats',
    setActiveView: vi.fn(),
    search: '',
    setSearch: vi.fn(),
    loading: false,
    conversations: [conversation],
    selectedConversation: conversation,
    selectConversation: vi.fn(),
    contacts: [{
      artifactId: 'contact-1',
      username: 'wxid_friend',
      displayName: '案件联系人',
      remark: '案件联系人',
      deleted: false,
      sourcePath: '[P2]/wechat/contact.db',
    }],
    moments: [],
    favorites: [],
    media: [],
    searchRecords: [],
    chatLoad: loadState,
    contactLoad: loadState,
    momentLoad: { ...loadState, loaded: 0, total: 0, hasMore: false },
    favoriteLoad: { ...loadState, loaded: 0, total: 0, hasMore: false },
    mediaLoad: { ...loadState, loaded: 0, total: 0, hasMore: false },
    searchRecordLoad: { ...loadState, loaded: 0, total: 0, hasMore: false },
    ...overrides,
  };
}

describe('WeChatWorkspace', () => {
  it('renders reconstructed conversations and loaded-record completeness', () => {
    render(<WeChatWorkspace model={modelFixture()} />);
    expect(screen.getAllByText('案件联系人').length).toBeGreaterThan(0);
    expect(screen.getByText('测试聊天正文')).toBeInTheDocument();
    expect(screen.getByText('已加载 1 / 3')).toBeInTheDocument();
  });

  it('forwards tab and search interactions to the workspace model', () => {
    const setActiveView = vi.fn();
    const setSearch = vi.fn();
    render(<WeChatWorkspace model={modelFixture({ setActiveView, setSearch })} />);

    fireEvent.change(screen.getByRole('textbox', { name: '搜索会话' }), {
      target: { value: '证人' },
    });
    expect(setSearch).toHaveBeenCalledWith('证人');
    fireEvent.mouseDown(screen.getByRole('tab', { name: '联系人' }));
    fireEvent.click(screen.getByRole('tab', { name: '联系人' }));
    expect(setActiveView).toHaveBeenCalledWith('contacts');
  });

  it('requests the next evidence page through the public button primitive', () => {
    const loadMore = vi.fn();
    render(<WeChatWorkspace model={modelFixture({
      chatLoad: { ...loadState, loadMore },
    })} />);
    fireEvent.click(screen.getByRole('button', { name: '加载更多' }));
    expect(loadMore).toHaveBeenCalledOnce();
  });

  it('renders an inline chat image and the official-account reply target', () => {
    const base = modelFixture();
    const conversation = {
      ...base.conversations[0],
      messages: [{
        ...base.conversations[0].messages[0],
        localType: 3,
        typeLabel: '图片',
        mediaKind: 'image',
        contentText: '<msg><img/></msg>',
        xmlText: '图片答复',
        replyNickname: '提问者',
        inlineMedia: {
          mimeType: 'image/png',
          dataBase64: 'iVBORw0KGgo=',
        },
      }],
    };
    render(<WeChatWorkspace model={modelFixture({
      conversations: [conversation],
      selectedConversation: conversation,
    })} />);

    expect(screen.getByRole('img', { name: '图片' })).toHaveAttribute(
      'src',
      'data:image/png;base64,iVBORw0KGgo=',
    );
    expect(screen.getByText('回复 提问者')).toBeInTheDocument();
  });

  it('shows a VoIP message as parsed text and labels a Moment CDN image as unverified', () => {
    const base = modelFixture();
    const conversation = {
      ...base.conversations[0],
      messages: [{
        ...base.conversations[0].messages[0],
        contentText: '<voipmsg><VoIPBubbleMsg><msg>已在其设备拒绝</msg></VoIPBubbleMsg></voipmsg>',
        xmlText: '已在其设备拒绝',
      }],
    };
    const moment = {
      artifactId: 'moment-1',
      userName: 'wxid_friend',
      displayName: '案件联系人',
      hasMedia: true,
      mediaItems: [{ id: 'media-1', url: 'http://mmsns.qpic.cn/mmsns/example/0' }],
      likes: [],
      comments: [],
      sourcePath: '[P2]/wechat/sns.db',
    };
    const { rerender } = render(<WeChatWorkspace model={modelFixture({
      conversations: [conversation],
      selectedConversation: conversation,
    })} />);
    expect(screen.getByText('已在其设备拒绝')).toBeInTheDocument();
    expect(screen.queryByText('<voipmsg><VoIPBubbleMsg><msg>已在其设备拒绝</msg></VoIPBubbleMsg></voipmsg>')).not.toBeInTheDocument();

    rerender(<WeChatWorkspace model={modelFixture({
      activeView: 'moments',
      moments: [moment],
    })} />);
    expect(screen.getByText('外部网络候选图像，未验证（非本地证据）')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'media-1' })).toHaveAttribute(
      'src',
      'http://mmsns.qpic.cn/mmsns/example/0',
    );
  });

  it('does not load a Moment URL outside the permitted media host', () => {
    const moment = {
      artifactId: 'moment-1',
      userName: 'wxid_friend',
      displayName: '案件联系人',
      hasMedia: true,
      mediaItems: [{ id: 'media-1', url: 'https://media.invalid/image.jpg' }],
      likes: [],
      comments: [],
      sourcePath: '[P2]/wechat/sns.db',
    };
    render(<WeChatWorkspace model={modelFixture({
      activeView: 'moments',
      moments: [moment],
    })} />);

    expect(screen.getByText('外部来源未获允许')).toBeInTheDocument();
    expect(document.querySelector('img[src="https://media.invalid/image.jpg"]')).toBeNull();
  });
});
