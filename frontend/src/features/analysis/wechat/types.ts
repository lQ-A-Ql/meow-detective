export type WeChatWorkspaceView = 'chats' | 'contacts' | 'moments' | 'favorites' | 'media' | 'index';

export interface WeChatMediaReference {
  id?: string;
  type?: string;
  title?: string;
  description?: string;
  url?: string;
  thumbUrl?: string;
  urlLocator?: string;
  thumbLocator?: string;
  md5?: string;
  inlineMedia?: WeChatInlineMedia;
  localMedia?: WeChatLocalMediaEvidence;
}

export interface WeChatInteraction {
  commentId?: string;
  username?: string;
  nickname?: string;
  replyUsername?: string;
  replyNickname?: string;
  replyCommentId?: string;
  content?: string;
  createTime?: string;
  source?: string;
}

export interface WeChatInlineMedia {
  mimeType: string;
  dataBase64: string;
  sha256?: string;
  sizeBytes?: number;
}

export type WeChatLocalMediaState = 'decoded' | 'encrypted' | 'unavailable';

export interface WeChatLocalMediaEvidence {
  state: WeChatLocalMediaState;
  storageKey?: string;
  cacheKey?: string;
  pathKind?: string;
  encryptedSizeBytes?: number;
  encryptedSha256?: string;
  plainMd5?: string;
  sourcePath: string;
}

export interface WeChatContact {
  artifactId: string;
  username: string;
  displayName: string;
  alias?: string;
  nickName?: string;
  remark?: string;
  description?: string;
  deleted: boolean;
  sourcePath: string;
}

export interface WeChatMessage {
  artifactId: string;
  talker: string;
  senderUsername?: string;
  senderDisplayName?: string;
  isSend?: boolean;
  localId?: number;
  serverId?: number;
  talkerTable?: string;
  localType?: number;
  typeLabel: string;
  contentText?: string;
  contentTruncated: boolean;
  mediaKind?: string;
  mediaUrl?: string;
  thumbnailUrl?: string;
  mediaLocator?: string;
  thumbnailLocator?: string;
  mediaMd5?: string;
  mediaItems: WeChatMediaReference[];
  voiceDurationMs?: string;
  videoDurationSeconds?: string;
  appTitle?: string;
  appDescription?: string;
  appUrl?: string;
  sourceUsername?: string;
  sourceDisplayName?: string;
  replyUsername?: string;
  replyNickname?: string;
  xmlText?: string;
  sourceXmlText?: string;
  packedInfoXmlText?: string;
  sourceContent?: string;
  packedInfoText?: string;
  xmlFields?: Record<string, unknown>;
  sourceXmlFields?: Record<string, unknown>;
  packedInfoXmlFields?: Record<string, unknown>;
  referencedMessage?: Record<string, unknown>;
  inlineMedia?: WeChatInlineMedia;
  localMedia?: WeChatLocalMediaEvidence;
  attrs: Record<string, unknown>;
  createTimeUtc?: string;
  sourcePath: string;
}

export interface WeChatConversation {
  talker: string;
  displayName: string;
  summary: string;
  unreadCount: number;
  hidden: boolean;
  lastTimestampUtc?: string;
  messages: WeChatMessage[];
}

export interface WeChatMoment {
  artifactId: string;
  userName: string;
  displayName: string;
  content?: string;
  createTimeUtc?: string;
  hasMedia: boolean;
  mediaItems: WeChatMediaReference[];
  likes: WeChatInteraction[];
  comments: WeChatInteraction[];
  sourcePath: string;
}

export interface WeChatSupplementalRecord {
  artifactId: string;
  title: string;
  table: string;
  values: Record<string, unknown>;
  sourcePath: string;
  inlineMedia?: WeChatInlineMedia;
  localMedia?: WeChatLocalMediaEvidence;
}

export interface WeChatFavorite {
  artifactId: string;
  localId?: number;
  type?: number;
  fromUsername?: string;
  fromDisplayName: string;
  realChatName?: string;
  content?: string;
  contentTruncated: boolean;
  updateTimeUtc?: string;
  sourcePath: string;
}

export interface WeChatLoadState {
  loaded: number;
  total: number;
  hasMore: boolean;
  loadingMore: boolean;
  failed: boolean;
  loadMore: () => void;
  retry: () => void;
}

export interface WeChatWorkspaceModel {
  activeView: WeChatWorkspaceView;
  setActiveView: (view: WeChatWorkspaceView) => void;
  search: string;
  setSearch: (value: string) => void;
  loading: boolean;
  conversations: WeChatConversation[];
  selectedConversation?: WeChatConversation;
  selectConversation: (talker: string) => void;
  contacts: WeChatContact[];
  moments: WeChatMoment[];
  favorites: WeChatFavorite[];
  media: WeChatSupplementalRecord[];
  searchRecords: WeChatSupplementalRecord[];
  chatLoad: WeChatLoadState;
  contactLoad: WeChatLoadState;
  momentLoad: WeChatLoadState;
  favoriteLoad: WeChatLoadState;
  mediaLoad: WeChatLoadState;
  searchRecordLoad: WeChatLoadState;
}
