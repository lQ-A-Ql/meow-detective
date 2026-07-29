import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { McpSectionContainer } from '@/features/mcp/containers/McpSectionContainer';
import { ImportPerformanceSection } from '@/features/settings/components/ImportPerformanceSection';
import { PreviewSection } from '@/features/settings/components/PreviewSection';
import { StoragePathsSection } from '@/features/settings/components/StoragePathsSection';
import { SystemInfoSection } from '@/features/settings/components/SystemInfoSection';
import { UiDebugSection } from '@/features/settings/components/UiDebugSection';
import type { useSettingsPageModel } from '@/features/settings/use-settings-page-model';

interface SettingsWorkspaceProps {
  model: ReturnType<typeof useSettingsPageModel>;
}

/** Pure settings presentation surface. Settings persistence belongs to the page model. */
export function SettingsWorkspace({ model }: SettingsWorkspaceProps) {
  const { t } = useTranslation();
  const { settings, setSettings, settingsMessage, savingSettings, saveSettings } = model;

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="font-serif text-xl tracking-tight text-forensics-text">{t('settings.title')}</div>
        <div className="mt-1 font-mono text-[11px] text-forensics-muted">{t('settings.subtitle')}</div>
      </div>
      <ScrollArea className="min-h-0 flex-1" viewportClassName="mx-auto w-full max-w-5xl space-y-8 p-6">
        <StoragePathsSection caseRoot={settings.caseRoot} imageSearchPaths={settings.imageSearchPaths} setSettings={setSettings} />
        <ImportPerformanceSection maxImportWorkers={settings.maxImportWorkers} maxAnalysisWorkers={settings.maxAnalysisWorkers} importAnalysisMode={settings.importAnalysisMode} setSettings={setSettings} />
        <PreviewSection hexChunkBytes={settings.hexChunkBytes} maxViewerRangeLength={settings.maxViewerRangeLength} maxInlineImagePreviewBytes={settings.maxInlineImagePreviewBytes} maxInlineMediaPreviewBytes={settings.maxInlineMediaPreviewBytes} setSettings={setSettings} />
        <UiDebugSection devEventTrace={settings.devEventTrace} savingSettings={savingSettings} settingsMessage={settingsMessage} setSettings={setSettings} onSave={saveSettings} />
        <McpSectionContainer />
        <SystemInfoSection />
      </ScrollArea>
    </div>
  );
}
