import { apiClient } from './client';

export async function getCurrentCase() {
  return apiClient.request('get_current_case', () => apiClient.getMockProvider().getCurrentCase());
}

export async function getCaseMetrics() {
  return apiClient.request('get_case_metrics', () => apiClient.getMockProvider().getCaseMetrics());
}

export async function getRecentObjects() {
  return apiClient.request('get_recent_objects', () => apiClient.getMockProvider().getRecentObjects());
}

export async function getRecentCases() {
  return apiClient.request('get_recent_cases', () => apiClient.getMockProvider().getRecentCases());
}

export async function getDataSources() {
  return apiClient.request('get_data_sources', () => apiClient.getMockProvider().getDataSources());
}

export async function createCase(caseRoot: string, name: string, examiner?: string) {
  return apiClient.request('create_case', () =>
    apiClient.getMockProvider().createCase(caseRoot, name, examiner), { request: { caseRoot, name, examiner: examiner ?? null } });
}

export async function openCase(caseRoot: string) {
  return apiClient.request('open_case', () => apiClient.getMockProvider().openCase(caseRoot), { request: { caseRoot } });
}

export async function closeCase() {
  return apiClient.request('close_case', () => apiClient.getMockProvider().closeCase());
}

export async function renameDataSource(dataSourceId: string, name: string) {
  return apiClient.request(
    'rename_data_source',
    () => apiClient.getMockProvider().renameDataSource(dataSourceId, name),
    { request: { dataSourceId, name } },
  );
}

export async function deleteCase(caseRoot: string) {
  return apiClient.request(
    'delete_case',
    () => Promise.resolve(`Case deleted: ${caseRoot}`),
    { request: { caseRoot } },
  );
}

export async function removeCaseFromList(caseRoot: string) {
  return apiClient.request(
    'remove_case_from_list',
    () => Promise.resolve(`Removed from list: ${caseRoot}`),
    { request: { caseRoot } },
  );
}

export async function deleteDataSource(dataSourceId: string) {
  return apiClient.request(
    'delete_data_source',
    () => Promise.resolve(`Data source deleted: ${dataSourceId}`),
    { request: { dataSourceId } },
  );
}
