import type {
  RegistryHiveCloseRequest, RegistryHiveListRequest, RegistryHiveOpenRequest, RegistryHivePage,
  RegistryHiveSession, RegistryKeyMetadata, RegistryKeyResolveRequest, RegistrySubkeyPage,
  RegistrySubkeysRequest, RegistryValuePage, RegistryValueRange, RegistryValueRangeRequest,
  RegistryValuesRequest,
} from '@/types/registryBrowser';

/** Read-only registry browsing client contract. */
export interface RegistryBrowserClient {
  listHives: (request: RegistryHiveListRequest) => Promise<RegistryHivePage>;
  openHive: (request: RegistryHiveOpenRequest) => Promise<RegistryHiveSession>;
  resolveKey: (request: RegistryKeyResolveRequest) => Promise<RegistryKeyMetadata>;
  listSubkeys: (request: RegistrySubkeysRequest) => Promise<RegistrySubkeyPage>;
  listValues: (request: RegistryValuesRequest) => Promise<RegistryValuePage>;
  readValueRange: (request: RegistryValueRangeRequest) => Promise<RegistryValueRange>;
  closeHive: (request: RegistryHiveCloseRequest) => Promise<boolean>;
}
