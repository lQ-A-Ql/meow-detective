import { save } from '@tauri-apps/plugin-dialog';
import {
  FileChildrenPage,
  FileEntryRow,
  FileRowsPage,
  FileTreeNode,
  MediaRangeRequest,
  MediaRangeResponse,
  MediaUrl,
  ViewerRangeRequest,
} from '@/types/models';
import { apiClient } from './client';

export async function getFileTree(showHidden = false) {
  return apiClient.request(
    'get_file_tree_request',
    () => apiClient.getMockProvider().getFileTree(showHidden),
    { request: { showHidden } },
  );
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
): Promise<FileRowsPage> {
  return apiClient.request(
    'get_file_rows_request',
    async () => {
      const rows = await apiClient.getMockProvider().getFileRows(parentId, showHidden);
      return {
        rows: rows.slice(offset, offset + limit),
        totalCount: rows.length,
        offset,
        limit,
        truncated: offset + limit < rows.length,
      };
    },
    { request: { parentId: parentId ?? null, offset, limit, showHidden } },
  );
}

export async function importDataSource(sourcePath: string) {
  return apiClient.request('import_data_source', () =>
    apiClient.getMockProvider().importDataSource(sourcePath), { request: { sourcePath } });
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
  return apiClient.request('get_file_children_request', () =>
    apiClient.getMockProvider().getFileChildren(parentId, showHidden).then((children) => ({
      children: children.slice(offset, offset + limit),
      totalCount: children.length,
      offset,
      limit,
      truncated: offset + limit < children.length,
    })), { request: { parentId, offset, limit, showHidden } });
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
 * Returns an opaque media handle and, for small media only, an inline data URL.
 */
export async function getMediaUrl(fileId: string): Promise<MediaUrl> {
  return apiClient.request(
    'get_media_url',
    () => Promise.resolve({
      url: '',
      handleId: `file:${fileId}`,
      mimeType: 'video/mp4',
      size: 0,
      canReadRanges: false,
      mode: 'inline',
      previewMode: 'inline',
    }),
    { fileId },
  );
}

export async function readMediaRange(request: MediaRangeRequest): Promise<MediaRangeResponse> {
  return apiClient.request(
    'read_media_range',
    () => Promise.resolve({
      offset: request.offset,
      bytesBase64: '',
      bytesRead: 0,
      eof: true,
    }),
    { request },
  );
}

export async function extractFile(file: FileEntryRow) {
  if (apiClient.mode === 'mock') {
    const content = [
      'Forensics Workbench mock export',
      `fileId: ${file.id}`,
      `path: ${file.path}`,
      `name: ${file.name}`,
    ].join('\n');
    const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    try {
      const link = document.createElement('a');
      link.href = url;
      link.download = file.name || `${file.id}.txt`;
      document.body.appendChild(link);
      link.click();
      link.remove();
    } finally {
      URL.revokeObjectURL(url);
    }
    return 'Mock file exported';
  }

  const destinationPath = await save({ defaultPath: file.name || file.id });
  if (!destinationPath) {
    return 'Export cancelled';
  }
  return apiClient.request(
    'extract_file',
    () => Promise.resolve('Mock file exported'),
    { request: { fileId: file.id, destinationPath } },
  );
}
