import type { FilePreviewKind, PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import type { FileEntryRow } from '@/types/models';

const IMAGE_EXTENSIONS = new Set(['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'ico']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'webm', 'avi', 'mkv']);
const AUDIO_EXTENSIONS = new Set(['mp3', 'wav', 'flac', 'aac', 'ogg']);
const DOCUMENT_EXTENSIONS = new Set([
  'pdf', 'docx', 'xlsx', 'pptx', 'sqlite', 'sqlite3', 'db', 'db3',
  'doc', 'xls', 'ppt',
]);
const TEXT_EXTENSIONS = new Set([
  'txt', 'log', 'md', 'csv', 'json', 'jsonl', 'xml', 'html', 'htm',
  'yaml', 'yml', 'toml', 'ini', 'conf', 'cfg', 'ps1', 'bat', 'cmd',
  'sh', 'py', 'js', 'jsx', 'ts', 'tsx', 'css', 'sql',
]);

type PreviewFileIdentity = Pick<FileEntryRow, 'entryType' | 'ext' | 'name'>;

function extensionFor(file: Pick<PreviewFileIdentity, 'ext' | 'name'>): string {
  const extension = file.ext?.trim() || file.name.split('.').pop();
  return extension?.toLowerCase().replace(/^\.+/, '') ?? '';
}

export function getFilePreviewKind(
  file: Pick<PreviewFileIdentity, 'ext' | 'name'>,
): FilePreviewKind | undefined {
  const extension = extensionFor(file);
  if (IMAGE_EXTENSIONS.has(extension)) return 'image';
  if (VIDEO_EXTENSIONS.has(extension)) return 'video';
  if (AUDIO_EXTENSIONS.has(extension)) return 'audio';
  if (DOCUMENT_EXTENSIONS.has(extension)) return 'document';
  return undefined;
}

export function getDefaultFilePreviewTab(file: PreviewFileIdentity): PreviewViewerTab {
  if (file.entryType === 'directory') return 'metadata';
  if (getFilePreviewKind(file)) return 'preview';
  return TEXT_EXTENSIONS.has(extensionFor(file)) ? 'text' : 'hex';
}
