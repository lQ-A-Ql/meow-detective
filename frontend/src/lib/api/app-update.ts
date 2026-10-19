import { apiClient, Channel } from './client';
import { COMMANDS } from './commands';
import type { AppUpdateInfo, AppUpdateProgress } from '@/types/appUpdate';

export const getAppUpdateInfo = () => apiClient.request<AppUpdateInfo>(COMMANDS.appUpdate.INFO);
export const checkAppUpdate = () => apiClient.request<AppUpdateInfo>(COMMANDS.appUpdate.CHECK);
export function installAppUpdate(version: string, onProgress: (progress: AppUpdateProgress) => void) {
  const progress = new Channel<AppUpdateProgress>();
  progress.onmessage = onProgress;
  return apiClient.request<void>(COMMANDS.appUpdate.INSTALL, { version, progress });
}
