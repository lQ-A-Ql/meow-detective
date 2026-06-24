import { AppSettings } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getAppSettings(): Promise<AppSettings> {
  return apiClient.request(COMMANDS.settings.GET_APP_SETTINGS);
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return apiClient.request(COMMANDS.settings.SAVE_APP_SETTINGS, { settings });
}
