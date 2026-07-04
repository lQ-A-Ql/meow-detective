import {
  CaseMetrics,
  CaseSummary,
  DataSourceSummary,
  RecentCase,
  RecentObject,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getCurrentCase(): Promise<CaseSummary | null> {
  return apiClient.request(COMMANDS.case.GET_CURRENT_CASE);
}

export async function getCaseMetrics(): Promise<CaseMetrics> {
  return apiClient.request(COMMANDS.case.GET_CASE_METRICS);
}

export async function getRecentObjects(): Promise<RecentObject[]> {
  return apiClient.request(COMMANDS.case.GET_RECENT_OBJECTS);
}

export async function getRecentCases(): Promise<RecentCase[]> {
  return apiClient.request(COMMANDS.case.GET_RECENT_CASES);
}

export async function getDataSources(): Promise<DataSourceSummary[]> {
  return apiClient.request(COMMANDS.case.GET_DATA_SOURCES);
}

export async function createCase(caseRoot: string, name: string, examiner?: string): Promise<CaseSummary> {
  return apiClient.request(COMMANDS.case.CREATE_CASE, { request: { caseRoot, name, examiner: examiner ?? null } });
}

export async function openCase(caseRoot: string): Promise<CaseSummary> {
  return apiClient.request(COMMANDS.case.OPEN_CASE, { request: { caseRoot } });
}

export async function closeCase(): Promise<void> {
  return apiClient.request(COMMANDS.case.CLOSE_CASE);
}

export async function renameDataSource(dataSourceId: string, name: string): Promise<void> {
  return apiClient.request(COMMANDS.case.RENAME_DATA_SOURCE, { request: { dataSourceId, name } });
}

export async function deleteCase(caseRoot: string): Promise<string> {
  return apiClient.request(COMMANDS.case.DELETE_CASE, { request: { caseRoot } });
}

export async function removeCaseFromList(caseRoot: string): Promise<string> {
  return apiClient.request(COMMANDS.case.REMOVE_CASE_FROM_LIST, { request: { caseRoot } });
}

export async function deleteDataSource(dataSourceId: string): Promise<string> {
  return apiClient.request(COMMANDS.case.DELETE_DATA_SOURCE, { request: { dataSourceId } });
}
