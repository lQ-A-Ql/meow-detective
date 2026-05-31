import { FileTreeNode, ViewerRangeRequest } from '@/types/models';
import { apiClient } from './client';

export async function getFileTree() {
  return apiClient.request('get_file_tree', () => apiClient.getMockProvider().getFileTree());
}

export async function getFileRows(parentId?: string) {
  return apiClient.request(
    'get_file_rows_request',
    () => apiClient.getMockProvider().getFileRows(parentId),
    { request: { parentId: parentId ?? null } },
  );
}

export async function importDataSource(sourcePath: string) {
  return apiClient.request('import_data_source', () =>
    apiClient.getMockProvider().importDataSource(sourcePath), { request: { sourcePath } });
}

export async function getFileChildren(parentId: string): Promise<FileTreeNode[]> {
  return apiClient.request('get_file_children_request', () =>
    apiClient.getMockProvider().getFileChildren(parentId), { request: { parentId } });
}

export async function openFileHandle(fileId: string) {
  return apiClient.request(
    'open_file_handle_request',
    () => apiClient.getMockProvider().openFileHandle(fileId),
    { request: { fileId } },
  );
}

export async function readFileRange(request: ViewerRangeRequest) {
  return apiClient.request(
    'read_file_range',
    () => apiClient.getMockProvider().readFileRange(request),
    { request },
  );
}

export async function cancelImport(jobId: string) {
  return apiClient.request('cancel_import', () => Promise.resolve('Cancel not available in mock mode'), { jobId });
}

/**
 * Get text preview for a file.
 * Returns text content with encoding detection.
 */
export async function getTextPreview(fileId: string, maxBytes?: number) {
  return apiClient.request(
    'get_text_preview',
    () => Promise.resolve({
      content: '',
      encoding: 'UTF-8',
      isTruncated: false,
      lineNumber: 0,
      isBinary: false,
      language: null,
    }),
    { fileId, maxBytes: maxBytes ?? 1024 * 1024 },
  );
}

/**
 * Get image preview for a file.
 * Returns base64-encoded image data.
 */
export async function getImagePreview(fileId: string) {
  return apiClient.request(
    'get_image_preview',
    () => Promise.resolve({
      dataUrl: '',
      mimeType: 'image/png',
      width: 0,
      height: 0,
      size: 0,
    }),
    { fileId },
  );
}

/**
 * Get media URL for video/audio playback.
 * Returns a local file URL.
 */
export async function getMediaUrl(fileId: string) {
  return apiClient.request(
    'get_media_url',
    () => Promise.resolve({
      url: '',
      mimeType: 'video/mp4',
      size: 0,
    }),
    { fileId },
  );
}
