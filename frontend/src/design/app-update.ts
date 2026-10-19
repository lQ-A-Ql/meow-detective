export const UPDATE_RELEASES_URL = 'https://github.com/lQ-A-Ql/meow-detective/releases/latest';
export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
export const UPDATE_STORAGE_KEY = 'Meow_Detective.updateCheckAt';
export const UPDATE_ERROR_KEYS: Record<string, string> = {
  UPDATE_CLOSE_CASE: 'appUpdate.errors.closeCase',
  UPDATE_CASE_BUSY: 'appUpdate.errors.busy',
  UPDATE_TASKS_BUSY: 'appUpdate.errors.busy',
  UPDATE_MOUNTS_BUSY: 'appUpdate.errors.busy',
  UPDATE_SCHEMA_MANUAL: 'appUpdate.errors.schema',
  UPDATE_SIGNATURE: 'appUpdate.errors.signature',
  UPDATE_NETWORK: 'appUpdate.errors.network',
  UPDATE_PENDING_RECOVERY: 'appUpdate.errors.recovery',
};
