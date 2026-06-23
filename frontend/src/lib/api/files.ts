import { save } from '@tauri-apps/plugin-dialog';
import {
  FileChildrenPage,
  FileEntryRow,
  FileJumpContext,
  FileRowsPage,
  FileTreeNode,
  ImagePreviewResponse,
  MediaRangeRequest,
  MediaRangeResponse,
  MediaUrl,
  TextPreviewResponse,
  ViewerHandle,
  ViewerRangeRequest,
} from '@/types/models';
import { type FileSortKey, type FileSortDirection } from '@/lib/file-sort';
import { apiClient } from './client';

export async function getFileTree(showHidden = false) {
  return apiClient.request<FileTreeNode[]>('get_file_tree_request', { request: { showHidden } });
}

export async function getFileRows(parentId?: string, showHidden = false) {
  const rows = await getFileRowsPage(parentId, 0, 500, showHidden);
  return rows.rows;
}

export async function getFileRowsPage(
  parentId?: string,
  offset = 0,
  limit = 500,
  showHidden = false,
  sortKey: FileSortKey = 'name',
  sortDirection: FileSortDirection = 'asc',
): Promise<FileRowsPage> {
  return apiClient.request<FileRowsPage>('get_file_rows_request', {
    request: {
      parentId: parentId ?? null,
      offset,
      limit,
      showHidden,
      sortKey,
      sortDirection,
    },
  });
}

export async function importDataSource(sourcePath: string): Promise<string> {
  return apiClient.request('import_data_source', { request: { sourcePath } });
}

export async function getFileChildren(parentId: string, showHidden = false): Promise<FileTreeNode[]> {
  const page = await getFileChildrenPage(parentId, 0, 500, showHidden);
  return page.children;
}

export async function getFileChildrenPage(
  parentId: string,
  offset = 0,
  limit = 500,
  showHidden = false,
): Promise<FileChildrenPage> {
  return apiClient.request<FileChildrenPage>('get_file_children_request', {
    request: { parentId, offset, limit, showHidden },
  });
}

export async function openFileHandle(fileId: string): Promise<ViewerHandle> {
  return apiClient.request('open_file_handle_request', { request: { fileId } });
}

export async function readFileRange(request: ViewerRangeRequest): Promise<import('@/types/models').ViewerRangeResponse> {
  return apiClient.request('read_file_range', { request });
}

export async function cancelImport(jobId: string) {
  return apiClient.request('cancel_import', { jobId });
}

/**
 * Get text preview for a file.
 * Returns text content with encoding detection.
 */
export async function getTextPreview(fileId: string, maxBytes?: number): Promise<TextPreviewResponse> {
  return apiClient.request('get_text_preview', { fileId, maxBytes: maxBytes ?? 1024 * 1024 });
}

/**
 * Get image preview for a file.
 * Returns base64-encoded image data.
 */
export async function getImagePreview(fileId: string): Promise<ImagePreviewResponse> {
  return apiClient.request('get_image_preview', { fileId });
}

/**
 * Get media URL for video/audio playback.
 * Returns an opaque media handle and, for small media only, an inline data URL.
 */
export async function getMediaUrl(fileId: string): Promise<MediaUrl> {
  return apiClient.request<MediaUrl>('get_media_url', { fileId });
}

export async function readMediaRange(request: MediaRangeRequest): Promise<MediaRangeResponse> {
  return apiClient.request<MediaRangeResponse>('read_media_range', { request });
}

export async function extractFile(file: FileEntryRow) {
  const destinationPath = await save({ defaultPath: file.name || file.id });
  if (!destinationPath) {
    return 'Export cancelled';
  }
  return apiClient.request('extract_file', {
    request: { fileId: file.id, destinationPath, overwrite: false },
  });
}

export async function getFileJumpContext(
  fileId: string,
  options: {
    showHidden?: boolean;
    pageLimit?: number;
    sortKey?: FileSortKey;
    sortDirection?: FileSortDirection;
  } = {},
): Promise<FileJumpContext> {
  const {
    showHidden = false,
    pageLimit = 500,
    sortKey = 'name',
    sortDirection = 'asc',
  } = options;
  return apiClient.request<FileJumpContext>('get_file_jump_context', {
    request: { fileId, showHidden, pageLimit, sortKey, sortDirection },
  });
}
