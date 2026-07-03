import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  defaultSettings,
  formatPathList,
  parsePathList,
  readLocalSettings,
  validatePathList,
  writeLocalSettings,
} from '@/lib/settings';
import { getAppSettings, saveAppSettings } from '@/lib/api/settings';
import { useMcpStore } from '@/stores/mcp-store';
import { StoragePathsSection } from './settings/StoragePathsSection';
import { ImportPerformanceSection } from './settings/ImportPerformanceSection';
import { PreviewSection } from './settings/PreviewSection';
import { UiDebugSection } from './settings/UiDebugSection';
import { McpSection } from './settings/McpSection';
import { SystemInfoSection } from './settings/SystemInfoSection';

export function Settings() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState(() => readLocalSettings());
  const [settingsMessage, setSettingsMessage] = useState('');
  const [savingSettings, setSavingSettings] = useState(false);
  const { loadConfig } = useMcpStore();

  useEffect(() => {
    loadConfig();
  }, []);

  useEffect(() => {
    let cancelled = false;
    getAppSettings()
      .then((remote) => {
        if (cancelled) {
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
      })
      .catch(() => {
        // Keep local settings as the fallback when the desktop settings command is unavailable.
      });
    return () => {
      cancelled = true;
    };
  }, []);

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
    setSavingSettings(true);
    setSettingsMessage('');
    try {
      const saved = await saveAppSettings({
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
    } finally {
      setSavingSettings(false);
    }
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-forensics-surface overflow-hidden">
      <div className="border-b border-forensics-border bg-forensics-panel p-6 shrink-0">
        <div className="font-serif text-xl text-forensics-text tracking-tight">{t('settings.title')}</div>
        <div className="text-forensics-muted text-[11px] font-mono mt-1">{t('settings.subtitle')}</div>
      </div>

      <div className="mx-auto w-full max-w-5xl flex-1 space-y-8 overflow-auto p-6">
        <StoragePathsSection
          caseRoot={settings.caseRoot}
          imageSearchPaths={settings.imageSearchPaths}
          setSettings={setSettings}
        />
        <ImportPerformanceSection
          maxImportWorkers={settings.maxImportWorkers}
          maxAnalysisWorkers={settings.maxAnalysisWorkers}
          importAnalysisMode={settings.importAnalysisMode}
          setSettings={setSettings}
        />
        <PreviewSection
          hexChunkBytes={settings.hexChunkBytes}
          maxViewerRangeLength={settings.maxViewerRangeLength}
          maxInlineImagePreviewBytes={settings.maxInlineImagePreviewBytes}
          maxInlineMediaPreviewBytes={settings.maxInlineMediaPreviewBytes}
          setSettings={setSettings}
        />
        <UiDebugSection
          devEventTrace={settings.devEventTrace}
          savingSettings={savingSettings}
          settingsMessage={settingsMessage}
          setSettings={setSettings}
          onSave={saveSettings}
        />
        <McpSection />
        <SystemInfoSection />
      </div>
    </div>
  );
}

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
