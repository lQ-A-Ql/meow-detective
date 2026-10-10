import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';

import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Button } from '@/app/components/ui/button';
import { ActionBar } from '@/components/layout/ActionBar';
import { AnchorNav } from '@/components/layout/AnchorNav';
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
  const anchors = [
    { id: 'settings-storage', label: t('settings.anchors.storage') },
    { id: 'settings-performance', label: t('settings.anchors.performance') },
    { id: 'settings-preview', label: t('settings.anchors.preview') },
    { id: 'settings-ui', label: t('settings.anchors.ui') },
    { id: 'settings-mcp', label: t('settings.anchors.mcp') },
    { id: 'settings-system', label: t('settings.anchors.system') },
  ];

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden bg-forensics-surface">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <AnchorNav items={anchors} label={t('settings.anchors.label')} className="hidden w-52 border-r border-forensics-divider px-2 py-4 xl:block" />
        <ScrollArea className="min-h-0 flex-1" viewportClassName="mx-auto w-full max-w-5xl space-y-8 p-6">
    <section id="settings-storage" className="scroll-mt-4">
            <StoragePathsSection caseRoot={settings.caseRoot} imageSearchPaths={settings.imageSearchPaths} setSettings={setSettings} />
    </section>
    <section id="settings-performance" className="scroll-mt-4">
            <ImportPerformanceSection maxImportWorkers={settings.maxImportWorkers} maxAnalysisWorkers={settings.maxAnalysisWorkers} importAnalysisMode={settings.importAnalysisMode} setSettings={setSettings} />
    </section>
    <section id="settings-preview" className="scroll-mt-4">
            <PreviewSection hexChunkBytes={settings.hexChunkBytes} maxViewerRangeLength={settings.maxViewerRangeLength} maxInlineImagePreviewBytes={settings.maxInlineImagePreviewBytes} maxInlineMediaPreviewBytes={settings.maxInlineMediaPreviewBytes} setSettings={setSettings} />
    </section>
    <section id="settings-ui" className="scroll-mt-4">
            <UiDebugSection devEventTrace={settings.devEventTrace} setSettings={setSettings} />
    </section>
    <section id="settings-mcp" className="scroll-mt-4">
            <McpSectionContainer />
    </section>
    <section id="settings-system" className="scroll-mt-4">
            <SystemInfoSection />
    </section>
        </ScrollArea>
      </div>
      <ActionBar note={settingsMessage}>
        <Tooltip content={t('settings.actions.save')}><Button aria-label={t('settings.actions.save')} type="button" variant="outline" size="sm" onClick={() => void saveSettings()} disabled={savingSettings}><ActionIcon name="save" /></Button></Tooltip>
      </ActionBar>
    </div>
  );
}
