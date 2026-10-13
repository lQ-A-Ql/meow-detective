import type { RegistryRecovery } from '@/types/registryBrowser';

/** Only typed, bounded recovery details are allowed into the inspector. */
export function recoveryFailureDetails(error: unknown): RegistryRecovery | undefined {
  if (!error || typeof error !== 'object' || !('details' in error) || !error.details || typeof error.details !== 'object' || !('recovery' in error.details)) return undefined;
  const recovery = error.details.recovery;
  if (!recovery || typeof recovery !== 'object' || !('status' in recovery) || recovery.status !== 'failed' || !('logs' in recovery) || !Array.isArray(recovery.logs) || recovery.logs.length > 2) return undefined;
  return recovery as RegistryRecovery;
}
