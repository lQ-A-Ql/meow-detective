import { useState } from 'react';
import { EmulationWorkspace } from '../components/EmulationWorkspace';
import type { EmulationWorkspaceModel } from '../use-emulation-workspace-model';

export function EmulationWorkspaceController({ model }: { model: EmulationWorkspaceModel }) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  return <EmulationWorkspace model={model} settingsOpen={settingsOpen} onSettingsOpenChange={setSettingsOpen} />;
}
