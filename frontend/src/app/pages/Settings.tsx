import { useState, useEffect } from 'react';
import {
  applyTheme,
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
          theme: remote.theme,
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
        // Standalone mock/dev mode keeps local settings as the fallback.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    applyTheme(settings.theme);
  }, [settings.theme]);

  async function saveSettings() {
    if (!settings.caseRoot.trim()) {
      setSettingsMessage('案件默认存储路径不能为空。');
      return;
    }
    if (!validatePathList(settings.imageSearchPaths)) {
      setSettingsMessage('镜像搜索路径包含非法字符。');
      return;
    }
    const maxImportWorkers = parseOptionalPositiveInt(settings.maxImportWorkers);
    const maxAnalysisWorkers = parseOptionalPositiveInt(settings.maxAnalysisWorkers);
    if (maxImportWorkers === 0 || maxAnalysisWorkers === 0) {
      setSettingsMessage('Worker 数必须为空或大于 0。');
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
      setSettingsMessage('预览大小/块大小必须为正整数。');
      return;
    }
    setSavingSettings(true);
    setSettingsMessage('');
    try {
      const saved = await saveAppSettings({
        caseRoot: settings.caseRoot,
        imageSearchPaths: parsePathList(settings.imageSearchPaths),
        theme: settings.theme,
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
        theme: saved.theme,
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
      setSettingsMessage('设置已保存。');
    } catch (error) {
      const message = error instanceof Error ? error.message : '设置保存失败。';
      setSettingsMessage(message);
    } finally {
      setSavingSettings(false);
    }
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="font-serif text-xl text-[#111] tracking-tight">设置</div>
        <div className="text-[#666] text-[11px] font-mono mt-1">应用配置与数据目录</div>
      </div>

      <div className="p-6 space-y-8">
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
          theme={settings.theme}
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
