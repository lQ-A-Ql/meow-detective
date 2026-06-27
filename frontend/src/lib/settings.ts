export type ThemeMode = 'light' | 'dark';
export type ImportAnalysisMode = 'metadataOnly' | 'budgetedContent' | 'fullContent';

export interface LocalSettings {
  caseRoot: string;
  imageSearchPaths: string;
  theme: ThemeMode;
  devEventTrace: boolean;
  maxImportWorkers: string;
  maxAnalysisWorkers: string;
  importAnalysisMode: ImportAnalysisMode;
}

const STORAGE_KEY = 'forensics.localSettings';

export const defaultSettings: LocalSettings = {
  caseRoot: 'C:\\ForensicsWorkbench\\cases',
  imageSearchPaths: 'E:\\cases\\; D:\\images\\',
  theme: 'light',
  devEventTrace: false,
  maxImportWorkers: '',
  maxAnalysisWorkers: '',
  importAnalysisMode: 'metadataOnly',
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
  applyTheme(normalized.theme);
  return normalized;
}

export function applyTheme(theme: ThemeMode) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.classList.toggle('dark', theme === 'dark');
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
    theme: candidate.theme === 'dark' ? 'dark' : 'light',
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
  };
}

function isImportAnalysisMode(value: unknown): value is ImportAnalysisMode {
  return value === 'metadataOnly' || value === 'budgetedContent' || value === 'fullContent';
}
