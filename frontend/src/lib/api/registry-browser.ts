import type {
  RegistryHiveCloseRequest, RegistryHiveListRequest, RegistryHiveOpenRequest, RegistryHivePage,
  RegistryHiveSession, RegistryKeyMetadata, RegistryKeyResolveRequest, RegistrySubkeyPage,
  RegistrySubkeysRequest, RegistryValuePage, RegistryValueRange, RegistryValueRangeRequest,
  RegistryValuesRequest, RegistrySearchPage,
} from '@/types/registryBrowser';
import { apiClient } from './client';
import { COMMANDS } from './commands';

/** Read-only registry browsing client contract. */
export interface RegistryBrowserClient {
  listHives: (request: RegistryHiveListRequest) => Promise<RegistryHivePage>;
  openHive: (request: RegistryHiveOpenRequest) => Promise<RegistryHiveSession>;
  resolveKey: (request: RegistryKeyResolveRequest) => Promise<RegistryKeyMetadata>;
  listSubkeys: (request: RegistrySubkeysRequest) => Promise<RegistrySubkeyPage>;
  listValues: (request: RegistryValuesRequest) => Promise<RegistryValuePage>;
  readValueRange: (request: RegistryValueRangeRequest) => Promise<RegistryValueRange>;
  closeHive: (request: RegistryHiveCloseRequest) => Promise<boolean>;
  searchHive: (request: { sessionId: string; query: string }) => Promise<RegistrySearchPage>;
}

export const registryBrowserClient: RegistryBrowserClient = {
  listHives: (request) => apiClient.request(COMMANDS.registryBrowser.LIST_HIVES, { request }),
  openHive: (request) => apiClient.request(COMMANDS.registryBrowser.OPEN_HIVE, { request }),
  resolveKey: (request) => apiClient.request(COMMANDS.registryBrowser.RESOLVE_KEY, { request }),
  listSubkeys: (request) => apiClient.request(COMMANDS.registryBrowser.LIST_SUBKEYS, { request }),
  listValues: (request) => apiClient.request(COMMANDS.registryBrowser.LIST_VALUES, { request }),
  readValueRange: (request) => apiClient.request(COMMANDS.registryBrowser.READ_VALUE_RANGE, { request }),
  closeHive: async (request) => { await apiClient.request(COMMANDS.registryBrowser.CLOSE_HIVE, { request }); return true; },
  searchHive: (request) => apiClient.request(COMMANDS.registryBrowser.SEARCH_HIVE, { request }),
};
export async function searchRegistryHive(request: { sessionId: string; query: string }): Promise<import('@/types/registryBrowser').RegistrySearchPage> {
  return apiClient.request('search_registry_hive', { request });
}
