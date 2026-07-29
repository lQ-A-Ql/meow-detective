import { FileBrowserWorkspace } from '@/features/files/components/FileBrowserWorkspace';
import { useFileBrowserModel } from '@/features/files/use-file-browser-model';

export function FileBrowserContainer() {
  const model = useFileBrowserModel();
  return <FileBrowserWorkspace model={model} />;
}
