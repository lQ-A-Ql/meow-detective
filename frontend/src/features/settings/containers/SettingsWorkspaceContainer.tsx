import { SettingsWorkspace } from '@/features/settings/components/SettingsWorkspace';
import { useSettingsPageModel } from '@/features/settings/use-settings-page-model';

export function SettingsWorkspaceContainer() {
  const model = useSettingsPageModel();
  return <SettingsWorkspace model={model} />;
}
