import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  defaultSettings,
  formatPathList,
  parsePathList,
  readLocalSettings,
  validatePathList,
  writeLocalSettings,
} from '@/lib/settings';
import { useAppSettings, useSaveAppSettings } from '@/features/settings/hooks';
import { useMcpStore } from '@/stores/mcp-store';

function parseOptionalPositiveInt(value: string): number | undefined {
  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return 0;
  }
  return parsed;
}

function parseRequiredPositiveInt(value: string): number {
  const trimmed = value.trim();
  if (!trimmed) {
    return 0;
  }
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return 0;
  }
  return parsed;
}

export function useSettingsPageModel() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState(() => readLocalSettings());
  const [settingsMessage, setSettingsMessage] = useState('');
  const appSettings = useAppSettings();
  const saveAppSettings = useSaveAppSettings();
  const { loadConfig } = useMcpStore();

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    const remote = appSettings.data;
    if (!remote) {
      return;
    }
    setSettings((current) => ({
      ...current,
      caseRoot: remote.caseRoot,
      imageSearchPaths: formatPathList(remote.imageSearchPaths),
      devEventTrace: remote.devEventTrace,
      maxImportWorkers: remote.maxImportWorkers?.toString() ?? '',
      maxAnalysisWorkers: remote.maxAnalysisWorkers?.toString() ?? '',
      importAnalysisMode: remote.importAnalysisMode ?? 'metadataOnly',
      hexChunkBytes: remote.hexChunkBytes?.toString() ?? defaultSettings.hexChunkBytes,
      maxViewerRangeLength: remote.maxViewerRangeLength?.toString() ?? defaultSettings.maxViewerRangeLength,
      maxInlineImagePreviewBytes:
        remote.maxInlineImagePreviewBytes?.toString() ?? defaultSettings.maxInlineImagePreviewBytes,
      maxInlineMediaPreviewBytes:
        remote.maxInlineMediaPreviewBytes?.toString() ?? defaultSettings.maxInlineMediaPreviewBytes,
    }));
  }, [appSettings.data]);

  async function saveSettings() {
    if (!settings.caseRoot.trim()) {
      setSettingsMessage(t('settings.validation.caseRootEmpty'));
      return;
    }
    if (!validatePathList(settings.imageSearchPaths)) {
      setSettingsMessage(t('settings.validation.imageSearchPathsInvalid'));
      return;
    }
    const maxImportWorkers = parseOptionalPositiveInt(settings.maxImportWorkers);
    const maxAnalysisWorkers = parseOptionalPositiveInt(settings.maxAnalysisWorkers);
    if (maxImportWorkers === 0 || maxAnalysisWorkers === 0) {
      setSettingsMessage(t('settings.validation.workersPositive'));
      return;
    }
    const hexChunkBytes = parseRequiredPositiveInt(settings.hexChunkBytes);
    const maxViewerRangeLength = parseRequiredPositiveInt(settings.maxViewerRangeLength);
    const maxInlineImagePreviewBytes = parseRequiredPositiveInt(settings.maxInlineImagePreviewBytes);
    const maxInlineMediaPreviewBytes = parseRequiredPositiveInt(settings.maxInlineMediaPreviewBytes);
    if (
      hexChunkBytes === 0 ||
      maxViewerRangeLength === 0 ||
      maxInlineImagePreviewBytes === 0 ||
      maxInlineMediaPreviewBytes === 0
    ) {
      setSettingsMessage(t('settings.validation.previewPositive'));
      return;
    }
    setSettingsMessage('');
    try {
      const saved = await saveAppSettings.mutateAsync({
        caseRoot: settings.caseRoot,
        imageSearchPaths: parsePathList(settings.imageSearchPaths),
        devEventTrace: settings.devEventTrace,
        maxImportWorkers,
        maxAnalysisWorkers,
        importAnalysisMode: settings.importAnalysisMode,
        hexChunkBytes,
        maxViewerRangeLength,
        maxInlineImagePreviewBytes,
        maxInlineMediaPreviewBytes,
      });
      const normalized = writeLocalSettings({
        caseRoot: saved.caseRoot,
        imageSearchPaths: formatPathList(saved.imageSearchPaths),
        devEventTrace: saved.devEventTrace,
        maxImportWorkers: saved.maxImportWorkers?.toString() ?? '',
        maxAnalysisWorkers: saved.maxAnalysisWorkers?.toString() ?? '',
        importAnalysisMode: saved.importAnalysisMode ?? 'metadataOnly',
        hexChunkBytes: saved.hexChunkBytes?.toString() ?? defaultSettings.hexChunkBytes,
        maxViewerRangeLength: saved.maxViewerRangeLength?.toString() ?? defaultSettings.maxViewerRangeLength,
        maxInlineImagePreviewBytes:
          saved.maxInlineImagePreviewBytes?.toString() ?? defaultSettings.maxInlineImagePreviewBytes,
        maxInlineMediaPreviewBytes:
          saved.maxInlineMediaPreviewBytes?.toString() ?? defaultSettings.maxInlineMediaPreviewBytes,
      });
      setSettings(normalized);
      setSettingsMessage(t('settings.saved'));
    } catch (error) {
      const message = error instanceof Error ? error.message : t('settings.saveFailed');
      setSettingsMessage(message);
    }
  }

  return {
    settings,
    setSettings,
    settingsMessage,
    savingSettings: saveAppSettings.isPending,
    saveSettings,
  };
}
