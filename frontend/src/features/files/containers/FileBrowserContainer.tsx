import { FileBrowserWorkspace } from '@/features/files/components/FileBrowserWorkspace';
import { useImageMountModel } from '@/features/files/hooks/use-image-mount-model';
import { useFileBrowserModel } from '@/features/files/use-file-browser-model';

export function FileBrowserContainer() {
  const model = useFileBrowserModel();
  const mountModel = useImageMountModel();
  return <FileBrowserWorkspace model={model} mountModel={mountModel} />;
}
