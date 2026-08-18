import { Bookmark, ContactRound, FileSearch, Image, Images, LoaderCircle, MessageCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import type { WeChatWorkspaceModel, WeChatWorkspaceView } from '@/features/analysis/wechat/types';
import { WeChatChatsView } from './WeChatChatsView';
import {
  WeChatContactsView,
  WeChatFavoritesView,
  WeChatMomentsView,
  WeChatSupplementalView,
} from './WeChatCollectionViews';

export function WeChatWorkspace({ model }: { model: WeChatWorkspaceModel }) {
  const { t } = useTranslation();
  if (model.loading && model.conversations.length === 0 && model.contacts.length === 0) {
    return (
      <div className="flex h-40 items-center justify-center gap-2 text-[12px] text-forensics-muted">
        <LoaderCircle className="size-4 animate-spin" />
        {t('wechatWorkspace.loading')}
      </div>
    );
  }
  return (
    <Tabs
      value={model.activeView}
      onValueChange={(value) => model.setActiveView(value as WeChatWorkspaceView)}
      className="gap-3"
    >
      <TabsList className="w-full justify-start overflow-x-auto">
        <TabsTrigger value="chats">
          <MessageCircle />
          {t('wechatWorkspace.tabs.chats')}
        </TabsTrigger>
        <TabsTrigger value="contacts">
          <ContactRound />
          {t('wechatWorkspace.tabs.contacts')}
        </TabsTrigger>
        <TabsTrigger value="moments">
          <Image />
          {t('wechatWorkspace.tabs.moments')}
        </TabsTrigger>
        <TabsTrigger value="favorites">
          <Bookmark />
          {t('wechatWorkspace.tabs.favorites')}
        </TabsTrigger>
        <TabsTrigger value="media">
          <Images />
          {t('wechatWorkspace.tabs.media')}
        </TabsTrigger>
        <TabsTrigger value="index">
          <FileSearch />
          {t('wechatWorkspace.tabs.index')}
        </TabsTrigger>
      </TabsList>
      <TabsContent value="chats" className="m-0">
        <WeChatChatsView
          conversations={model.conversations}
          selectedConversation={model.selectedConversation}
          search={model.search}
          onSearchChange={model.setSearch}
          onSelectConversation={model.selectConversation}
          loadState={model.chatLoad}
        />
      </TabsContent>
      <TabsContent value="contacts" className="m-0">
        <WeChatContactsView
          contacts={model.contacts}
          search={model.search}
          onSearchChange={model.setSearch}
          loadState={model.contactLoad}
        />
      </TabsContent>
      <TabsContent value="moments" className="m-0">
        <WeChatMomentsView moments={model.moments} loadState={model.momentLoad} />
      </TabsContent>
      <TabsContent value="favorites" className="m-0">
        <WeChatFavoritesView favorites={model.favorites} loadState={model.favoriteLoad} />
      </TabsContent>
      <TabsContent value="media" className="m-0">
        <WeChatSupplementalView
          records={model.media}
          loadState={model.mediaLoad}
          emptyText={t('wechatWorkspace.noMedia')}
        />
      </TabsContent>
      <TabsContent value="index" className="m-0">
        <WeChatSupplementalView
          records={model.searchRecords}
          loadState={model.searchRecordLoad}
          emptyText={t('wechatWorkspace.noSearchRecords')}
        />
      </TabsContent>
    </Tabs>
  );
}
