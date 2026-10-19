import { SettingsWorkspace } from '@/features/settings/components/SettingsWorkspace';
import { useSettingsPageModel } from '@/features/settings/use-settings-page-model';
import { useUpdateModel } from '@/features/app-update/use-update-model';

export function SettingsWorkspaceContainer() {
  const model = useSettingsPageModel();
  const update = useUpdateModel();
  return <SettingsWorkspace model={model} update={update} />;
}
