export type ImportAnalysisMode = 'metadataOnly' | 'budgetedContent' | 'fullContent';

export interface LocalSettings {
  caseRoot: string;
  imageSearchPaths: string;
  devEventTrace: boolean;
  maxImportWorkers: string;
  maxAnalysisWorkers: string;
  importAnalysisMode: ImportAnalysisMode;
  hexChunkBytes: string;
  maxViewerRangeLength: string;
  maxInlineImagePreviewBytes: string;
  maxInlineMediaPreviewBytes: string;
}

const STORAGE_KEY = 'forensics.localSettings';

export const defaultSettings: LocalSettings = {
  caseRoot: 'C:\\ForensicsWorkbench\\cases',
  imageSearchPaths: 'E:\\cases\\; D:\\images\\',
  devEventTrace: false,
  maxImportWorkers: '',
  maxAnalysisWorkers: '',
  importAnalysisMode: 'metadataOnly',
  hexChunkBytes: '65536',
  maxViewerRangeLength: '1048576',
  maxInlineImagePreviewBytes: '5242880',
  maxInlineMediaPreviewBytes: '20971520',
};

export function readLocalSettings(): LocalSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaultSettings;
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return defaultSettings;
  }
}

export function writeLocalSettings(settings: LocalSettings) {
  const normalized = normalizeSettings(settings);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}

export function validatePathList(value: string) {
  const paths = value
    .split(';')
    .map((item) => item.trim())
    .filter(Boolean);
  return paths.every((path) => !path.includes('\0'));
}

export function parsePathList(value: string) {
  return value
    .split(';')
    .map((item) => item.trim())
    .filter(Boolean);
}

export function formatPathList(paths: string[]) {
  return paths.join('; ');
}

function normalizeSettings(value: unknown): LocalSettings {
  if (!value || typeof value !== 'object') {
    return defaultSettings;
  }
  const candidate = value as Partial<LocalSettings>;
  return {
    caseRoot: typeof candidate.caseRoot === 'string' ? candidate.caseRoot : defaultSettings.caseRoot,
    imageSearchPaths:
      typeof candidate.imageSearchPaths === 'string'
        ? candidate.imageSearchPaths
        : defaultSettings.imageSearchPaths,
    devEventTrace: candidate.devEventTrace === true,
    maxImportWorkers:
      typeof candidate.maxImportWorkers === 'string'
        ? candidate.maxImportWorkers
        : defaultSettings.maxImportWorkers,
    maxAnalysisWorkers:
      typeof candidate.maxAnalysisWorkers === 'string'
        ? candidate.maxAnalysisWorkers
        : defaultSettings.maxAnalysisWorkers,
    importAnalysisMode: isImportAnalysisMode(candidate.importAnalysisMode)
      ? candidate.importAnalysisMode
      : defaultSettings.importAnalysisMode,
    hexChunkBytes:
      typeof candidate.hexChunkBytes === 'string'
        ? candidate.hexChunkBytes
        : defaultSettings.hexChunkBytes,
    maxViewerRangeLength:
      typeof candidate.maxViewerRangeLength === 'string'
        ? candidate.maxViewerRangeLength
        : defaultSettings.maxViewerRangeLength,
    maxInlineImagePreviewBytes:
      typeof candidate.maxInlineImagePreviewBytes === 'string'
        ? candidate.maxInlineImagePreviewBytes
        : defaultSettings.maxInlineImagePreviewBytes,
    maxInlineMediaPreviewBytes:
      typeof candidate.maxInlineMediaPreviewBytes === 'string'
        ? candidate.maxInlineMediaPreviewBytes
        : defaultSettings.maxInlineMediaPreviewBytes,
  };
}

function isImportAnalysisMode(value: unknown): value is ImportAnalysisMode {
  return value === 'metadataOnly' || value === 'budgetedContent' || value === 'fullContent';
}

export interface PreviewSettings {
  hexChunkBytes: number;
  maxViewerRangeLength: number;
  maxInlineImagePreviewBytes: number;
  maxInlineMediaPreviewBytes: number;
}

export function getPreviewSettings(): PreviewSettings {
  const local = readLocalSettings();
  const parse = (value: string, fallback: number) => {
    const trimmed = value.trim();
    if (!trimmed) return fallback;
    const parsed = Number(trimmed);
    return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
  };
  return {
    hexChunkBytes: parse(local.hexChunkBytes, 64 * 1024),
    maxViewerRangeLength: parse(local.maxViewerRangeLength, 1024 * 1024),
    maxInlineImagePreviewBytes: parse(local.maxInlineImagePreviewBytes, 5 * 1024 * 1024),
    maxInlineMediaPreviewBytes: parse(local.maxInlineMediaPreviewBytes, 20 * 1024 * 1024),
  };
}
