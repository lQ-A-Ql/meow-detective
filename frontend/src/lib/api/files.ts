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
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getFileTree(showHidden = false) {
  return apiClient.request<FileTreeNode[]>(COMMANDS.files.GET_FILE_TREE_REQUEST, { request: { showHidden } });
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
  return apiClient.request<FileRowsPage>(COMMANDS.files.GET_FILE_ROWS_REQUEST, {
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
  return apiClient.request(COMMANDS.files.IMPORT_DATA_SOURCE, { request: { sourcePath } });
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
  return apiClient.request<FileChildrenPage>(COMMANDS.files.GET_FILE_CHILDREN_REQUEST, {
    request: { parentId, offset, limit, showHidden },
  });
}

export async function openFileHandle(fileId: string): Promise<ViewerHandle> {
  return apiClient.request(COMMANDS.files.OPEN_FILE_HANDLE_REQUEST, { request: { fileId } });
}

export async function readFileRange(request: ViewerRangeRequest): Promise<import('@/types/models').ViewerRangeResponse> {
  return apiClient.request(COMMANDS.files.READ_FILE_RANGE, { request });
}

export async function cancelImport(jobId: string) {
  return apiClient.request(COMMANDS.files.CANCEL_IMPORT, { jobId });
}

/**
 * Get text preview for a file.
 * Returns text content with encoding detection.
 */
export async function getTextPreview(fileId: string, maxBytes?: number): Promise<TextPreviewResponse> {
  return apiClient.request(COMMANDS.files.GET_TEXT_PREVIEW, { fileId, maxBytes: maxBytes ?? 1024 * 1024 });
}

/**
 * Get image preview for a file.
 * Returns base64-encoded image data.
 */
export async function getImagePreview(fileId: string): Promise<ImagePreviewResponse> {
  return apiClient.request(COMMANDS.files.GET_IMAGE_PREVIEW, { fileId });
}

/**
 * Get media URL for video/audio playback.
 * Returns an opaque media handle and, for small media only, an inline data URL.
 */
export async function getMediaUrl(fileId: string): Promise<MediaUrl> {
  return apiClient.request<MediaUrl>(COMMANDS.files.GET_MEDIA_URL, { fileId });
}

export async function readMediaRange(request: MediaRangeRequest): Promise<MediaRangeResponse> {
  return apiClient.request<MediaRangeResponse>(COMMANDS.files.READ_MEDIA_RANGE, { request });
}

export async function extractFile(file: FileEntryRow) {
  const destinationPath = await save({ defaultPath: file.name || file.id });
  if (!destinationPath) {
    return 'Export cancelled';
  }
  return apiClient.request(COMMANDS.files.EXTRACT_FILE, {
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
  return apiClient.request<FileJumpContext>(COMMANDS.files.GET_FILE_JUMP_CONTEXT, {
    request: { fileId, showHidden, pageLimit, sortKey, sortDirection },
  });
}
