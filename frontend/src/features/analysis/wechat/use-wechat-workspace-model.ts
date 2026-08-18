import { useMemo, useState } from 'react';
import { usePluginFamilyEntries } from '@/features/analysis/hooks';
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
import type { WeChatLoadState, WeChatWorkspaceModel, WeChatWorkspaceView } from './types';

const WECHAT_PLUGIN_ID = 'meow.plugin.wechat';

type FamilyQuery = ReturnType<typeof usePluginFamilyEntries>;

function familyLoadState(query: FamilyQuery): WeChatLoadState {
  return {
    loaded: query.data?.entries.length ?? 0,
    total: query.data?.totalCount ?? 0,
    hasMore: Boolean(query.hasNextPage),
    loadingMore: query.isFetchingNextPage,
    failed: query.isError || query.isFetchNextPageError,
    loadMore: () => {
      void query.fetchNextPage();
    },
    retry: () => {
      void query.refetch();
    },
  };
}

export function useWeChatWorkspaceModel(dataSourceId: string): WeChatWorkspaceModel {
  const [activeView, setActiveView] = useState<WeChatWorkspaceView>('chats');
  const [search, setSearch] = useState('');
  const [requestedTalker, setRequestedTalker] = useState<string>();
  const contactsQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatContact',
  });
  const sessionsQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatSession',
  });
  const messagesQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatMessage',
  });
  const momentsQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatMoment',
  });
  const favoritesQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatFavorite',
  });
  const mediaQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatMedia',
  });
  const searchRecordsQuery = usePluginFamilyEntries({
    dataSourceId,
    pluginId: WECHAT_PLUGIN_ID,
    family: 'WeChatSearchRecord',
  });

  const contacts = useMemo(
    () => mapWeChatContacts(contactsQuery.data?.entries ?? []),
    [contactsQuery.data],
  );
  const media = useMemo(
    () => mapWeChatSupplemental(mediaQuery.data?.entries ?? []),
    [mediaQuery.data],
  );
  const messages = useMemo(
    () => linkWeChatMessageMedia(mapWeChatMessages(messagesQuery.data?.entries ?? []), media),
    [media, messagesQuery.data],
  );
  const conversations = useMemo(
    () => buildWeChatConversations(sessionsQuery.data?.entries ?? [], messages, contacts),
    [contacts, messages, sessionsQuery.data],
  );
  const moments = useMemo(
    () => linkWeChatMomentMedia(
      mapWeChatMoments(momentsQuery.data?.entries ?? [], contacts),
      media,
    ),
    [contacts, media, momentsQuery.data],
  );
  const favorites = useMemo(
    () => mapWeChatFavorites(favoritesQuery.data?.entries ?? [], contacts),
    [contacts, favoritesQuery.data],
  );
  const searchRecords = useMemo(
    () => mapWeChatSupplemental(searchRecordsQuery.data?.entries ?? []),
    [searchRecordsQuery.data],
  );
  const normalizedSearch = search.trim().toLocaleLowerCase();
  const filteredConversations = normalizedSearch
    ? conversations.filter((conversation) =>
        [conversation.displayName, conversation.talker, conversation.summary]
          .some((value) => value.toLocaleLowerCase().includes(normalizedSearch)))
    : conversations;
  const selectedConversation = filteredConversations.find(
    (conversation) => conversation.talker === requestedTalker,
  ) ?? filteredConversations[0];
  const messageLoad = familyLoadState(messagesQuery);
  const sessionLoad = familyLoadState(sessionsQuery);
  const momentLoad = familyLoadState(momentsQuery);
  const mediaLoad = familyLoadState(mediaQuery);

  return {
    activeView,
    setActiveView,
    search,
    setSearch,
    loading: [
      contactsQuery,
      sessionsQuery,
      messagesQuery,
      momentsQuery,
      favoritesQuery,
      mediaQuery,
      searchRecordsQuery,
    ]
      .some((query) => query.isLoading),
    conversations: filteredConversations,
    selectedConversation,
    selectConversation: setRequestedTalker,
    contacts,
    moments,
    favorites,
    media,
    searchRecords,
    chatLoad: {
      loaded: messageLoad.loaded,
      total: messageLoad.total,
      hasMore: messageLoad.hasMore || sessionLoad.hasMore || mediaLoad.hasMore,
      loadingMore: messageLoad.loadingMore || sessionLoad.loadingMore || mediaLoad.loadingMore,
      failed: messageLoad.failed || sessionLoad.failed || mediaLoad.failed,
      loadMore: () => {
        if (messageLoad.hasMore) messageLoad.loadMore();
        if (sessionLoad.hasMore) sessionLoad.loadMore();
        if (mediaLoad.hasMore) mediaLoad.loadMore();
      },
      retry: () => {
        messageLoad.retry();
        sessionLoad.retry();
        mediaLoad.retry();
      },
    },
    contactLoad: familyLoadState(contactsQuery),
    momentLoad: {
      loaded: momentLoad.loaded,
      total: momentLoad.total,
      hasMore: momentLoad.hasMore || mediaLoad.hasMore,
      loadingMore: momentLoad.loadingMore || mediaLoad.loadingMore,
      failed: momentLoad.failed || mediaLoad.failed,
      loadMore: () => {
        if (momentLoad.hasMore) momentLoad.loadMore();
        if (mediaLoad.hasMore) mediaLoad.loadMore();
      },
      retry: () => {
        momentLoad.retry();
        mediaLoad.retry();
      },
    },
    favoriteLoad: familyLoadState(favoritesQuery),
    mediaLoad,
    searchRecordLoad: familyLoadState(searchRecordsQuery),
  };
}
