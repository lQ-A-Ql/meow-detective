import { FileTreeNode, ViewerRangeRequest } from '@/types/models';
import { apiClient } from './client';

export async function getFileTree() {
  return apiClient.request('get_file_tree', () => apiClient.getMockProvider().getFileTree());
}

export async function getFileRows() {
  return apiClient.request('get_file_rows', () => apiClient.getMockProvider().getFileRows());
}

export async function importDataSource(sourcePath: string) {
  return apiClient.request('import_data_source', () =>
    apiClient.getMockProvider().importDataSource(sourcePath), { sourcePath });
}

export async function getFileChildren(parentId: string): Promise<FileTreeNode[]> {
  return apiClient.request('get_file_children', () =>
    apiClient.getMockProvider().getFileChildren(parentId), { parentId });
}

export async function openFileHandle(fileId: string) {
  return apiClient.request(
    'open_file_handle',
    () => apiClient.getMockProvider().openFileHandle(fileId),
    { fileId },
  );
}

export async function readFileRange(request: ViewerRangeRequest) {
  return apiClient.request(
    'read_file_range',
    () => apiClient.getMockProvider().readFileRange(request),
    { request },
  );
}
