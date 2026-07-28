import { useTranslation } from 'react-i18next';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { useSettingsPageModel } from '@/features/settings/use-settings-page-model';
import { StoragePathsSection } from '@/features/settings/components/StoragePathsSection';
import { ImportPerformanceSection } from '@/features/settings/components/ImportPerformanceSection';
import { PreviewSection } from '@/features/settings/components/PreviewSection';
import { UiDebugSection } from '@/features/settings/components/UiDebugSection';
import { SystemInfoSection } from '@/features/settings/components/SystemInfoSection';
import { McpSection } from '@/features/mcp/components/McpSection';

export function Settings() {
  const { t } = useTranslation();
  const {
    settings,
    setSettings,
    settingsMessage,
    savingSettings,
    saveSettings,
  } = useSettingsPageModel();

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-forensics-surface overflow-hidden">
      <div className="border-b border-forensics-border bg-forensics-panel p-6 shrink-0">
        <div className="font-serif text-xl text-forensics-text tracking-tight">{t('settings.title')}</div>
        <div className="text-forensics-muted text-[11px] font-mono mt-1">{t('settings.subtitle')}</div>
      </div>

      <ScrollArea className="min-h-0 flex-1" viewportClassName="mx-auto w-full max-w-5xl space-y-8 p-6">
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
      </ScrollArea>
    </div>
  );
}
