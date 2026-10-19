import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';

import { ScrollArea } from '@/app/components/ui/scroll-area';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { ActionBar } from '@/components/layout/ActionBar';
import { AnchorNav } from '@/components/layout/AnchorNav';
import { McpSectionContainer } from '@/features/mcp/containers/McpSectionContainer';
import { ImportPerformanceSection } from '@/features/settings/components/ImportPerformanceSection';
import { PreviewSection } from '@/features/settings/components/PreviewSection';
import { StoragePathsSection } from '@/features/settings/components/StoragePathsSection';
import { SystemInfoSection } from '@/features/settings/components/SystemInfoSection';
import { UiDebugSection } from '@/features/settings/components/UiDebugSection';
import type { useSettingsPageModel } from '@/features/settings/use-settings-page-model';
import { BrandWatermark } from '@/components/brand';
import { UpdateSection } from '@/features/app-update/components/UpdateSection';
import type { useUpdateModel } from '@/features/app-update/use-update-model';

interface SettingsWorkspaceProps {
  model: ReturnType<typeof useSettingsPageModel>;
  update: ReturnType<typeof useUpdateModel>;
}

/** Pure settings presentation surface. Settings persistence belongs to the page model. */
export function SettingsWorkspace({ model, update }: SettingsWorkspaceProps) {
  const { t } = useTranslation();
  const { settings, setSettings, settingsMessage, savingSettings, saveSettings } = model;
  const anchors = [
    { id: 'settings-storage', label: t('settings.anchors.storage') },
    { id: 'settings-performance', label: t('settings.anchors.performance') },
    { id: 'settings-preview', label: t('settings.anchors.preview') },
    { id: 'settings-ui', label: t('settings.anchors.ui') },
    { id: 'settings-mcp', label: t('settings.anchors.mcp') },
    { id: 'settings-system', label: t('settings.anchors.system') },
    { id: 'settings-updates', label: t('appUpdate.title') },
  ];

  return (
    <WorkspaceFrame>
      <WorkspaceCard className="relative hidden w-52 shrink-0 xl:block">
        <AnchorNav items={anchors} label={t('settings.anchors.label')} className="workspace-content" />
        <BrandWatermark motif="ink-botanicals" className="ornament-settings" />
      </WorkspaceCard>
      <WorkspaceCard className="relative isolate flex min-h-0 flex-1 flex-col">
        <ScrollArea className="min-h-0 flex-1" viewportClassName="workspace-content space-y-8">
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
            <SystemInfoSection version={update.info?.currentVersion} />
    </section>
    <section id="settings-updates" className="scroll-mt-4">
            <UpdateSection model={update} />
    </section>
        </ScrollArea>
      <ActionBar note={settingsMessage}>
        <IconButton label={t('settings.actions.save')} variant="solid" onClick={() => void saveSettings()} disabled={savingSettings}><ActionIcon name="save" /></IconButton>
      </ActionBar>
      <BrandWatermark motif="settings" className="watermark-settings" />
      </WorkspaceCard>
    </WorkspaceFrame>
  );
}
