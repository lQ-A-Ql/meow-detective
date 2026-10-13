export function registryBrowserErrorCode(error: unknown): string {
  if (typeof error !== 'object' || error === null || !('code' in error)) return 'failed';
  const code = String(error.code);
  return ['sourceUnavailable', 'corruptKey', 'lockedHive', 'keyNotFound', 'expiredSession', 'invalidRequest', 'unsupportedHive', 'browserBusy', 'recoveryFailed'].includes(code) ? code : 'failed';
}
