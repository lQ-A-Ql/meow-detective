import {
  CaseMetrics,
  CaseSummary,
  DataSourceSummary,
  RecentCase,
  RecentObject,
} from '@/types/models';
import { apiClient } from './client';

export async function getCurrentCase(): Promise<CaseSummary | null> {
  return apiClient.request('get_current_case');
}

export async function getCaseMetrics(): Promise<CaseMetrics> {
  return apiClient.request('get_case_metrics');
}

export async function getRecentObjects(): Promise<RecentObject[]> {
  return apiClient.request('get_recent_objects');
}

export async function getRecentCases(): Promise<RecentCase[]> {
  return apiClient.request('get_recent_cases');
}

export async function getDataSources(): Promise<DataSourceSummary[]> {
  return apiClient.request('get_data_sources');
}

export async function createCase(caseRoot: string, name: string, examiner?: string): Promise<CaseSummary> {
  return apiClient.request('create_case', { request: { caseRoot, name, examiner: examiner ?? null } });
}

export async function createAnalysisDemoCase(): Promise<CaseSummary> {
  return apiClient.request('create_analysis_demo_case');
}

export async function openCase(caseRoot: string): Promise<CaseSummary> {
  return apiClient.request('open_case', { request: { caseRoot } });
}

export async function closeCase(): Promise<void> {
  return apiClient.request('close_case');
}

export async function renameDataSource(dataSourceId: string, name: string): Promise<DataSourceSummary> {
  return apiClient.request('rename_data_source', { request: { dataSourceId, name } });
}

export async function deleteCase(caseRoot: string): Promise<string> {
  return apiClient.request('delete_case', { request: { caseRoot } });
}

export async function removeCaseFromList(caseRoot: string): Promise<string> {
  return apiClient.request('remove_case_from_list', { request: { caseRoot } });
}

export async function deleteDataSource(dataSourceId: string): Promise<string> {
  return apiClient.request('delete_data_source', { request: { dataSourceId } });
}
