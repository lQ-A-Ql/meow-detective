import { apiClient } from '@/lib/api/client';
import { COMMANDS } from '@/lib/api/commands';
import type {
  EmulationBypassAccount,
  EmulationBypassApplyRequest,
  EmulationBypassResult,
  EmulationLinuxAccount,
  EmulationLinuxBypassRequest,
  EmulationLinuxBypassResult,
  EmulationOsdataCleanupRequest,
  EmulationOsdataCleanupResult,
  EmulationPreflight,
  EmulationSessionStatus,
  PrepareEmulationRequest,
} from '@/types/models';

export async function prepareEmulation(
  request: PrepareEmulationRequest,
): Promise<EmulationSessionStatus> {
  return apiClient.request<EmulationSessionStatus>(COMMANDS.emulation.PREPARE, { request });
}

export async function getEmulationPreflight(dataSourceId: string): Promise<EmulationPreflight> {
  return apiClient.request<EmulationPreflight>(COMMANDS.emulation.GET_PREFLIGHT, { dataSourceId });
}

export async function getEmulationBypassAccounts(
  dataSourceId: string,
  partitionIndex: number,
): Promise<EmulationBypassAccount[]> {
  return apiClient.request<EmulationBypassAccount[]>(COMMANDS.emulation.BYPASS_ACCOUNTS, {
    dataSourceId,
    partitionIndex,
  });
}

export async function applyEmulationBypass(
  request: EmulationBypassApplyRequest,
): Promise<EmulationBypassResult> {
  return apiClient.request<EmulationBypassResult>(COMMANDS.emulation.APPLY_BYPASS, { request });
}

export async function cleanupEmulationOsdata(
  request: EmulationOsdataCleanupRequest,
): Promise<EmulationOsdataCleanupResult> {
  return apiClient.request<EmulationOsdataCleanupResult>(COMMANDS.emulation.CLEANUP_OSDATA, {
    request,
  });
}

export async function getEmulationLinuxAccounts(
  dataSourceId: string,
  partitionIndex: number,
): Promise<EmulationLinuxAccount[]> {
  return apiClient.request<EmulationLinuxAccount[]>(COMMANDS.emulation.LINUX_ACCOUNTS, {
    dataSourceId,
    partitionIndex,
  });
}

export async function applyEmulationLinuxBypass(
  request: EmulationLinuxBypassRequest,
): Promise<EmulationLinuxBypassResult> {
  return apiClient.request<EmulationLinuxBypassResult>(COMMANDS.emulation.APPLY_LINUX_BYPASS, {
    request,
  });
}

export async function launchEmulation(sessionId: string): Promise<EmulationSessionStatus> {
  return apiClient.request<EmulationSessionStatus>(COMMANDS.emulation.LAUNCH, { sessionId });
}

export async function getEmulationStatus(sessionId: string): Promise<EmulationSessionStatus> {
  return apiClient.request<EmulationSessionStatus>(COMMANDS.emulation.GET_STATUS, { sessionId });
}

export async function listEmulationSessions(): Promise<EmulationSessionStatus[]> {
  return apiClient.request<EmulationSessionStatus[]>(COMMANDS.emulation.LIST_SESSIONS);
}

export async function releaseEmulation(sessionId: string): Promise<EmulationSessionStatus> {
  return apiClient.request<EmulationSessionStatus>(COMMANDS.emulation.RELEASE, { sessionId });
}
