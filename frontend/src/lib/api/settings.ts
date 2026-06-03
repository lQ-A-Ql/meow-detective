import { AppSettings } from '@/types/models';
import { apiClient } from './client';

export async function getAppSettings(): Promise<AppSettings> {
  return apiClient.request(
    'get_app_settings',
    () => Promise.resolve({
      caseRoot: 'C:\\Cases',
      imageSearchPaths: [],
      theme: 'light',
      devEventTrace: false,
      maxImportWorkers: undefined,
      maxAnalysisWorkers: undefined,
      importAnalysisMode: 'metadataOnly',
    }),
  );
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return apiClient.request(
    'save_app_settings',
    () => Promise.resolve(settings),
    { settings },
  );
}
