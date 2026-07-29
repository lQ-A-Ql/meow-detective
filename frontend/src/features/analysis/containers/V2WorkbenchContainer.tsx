import { V2WorkbenchWorkspace } from '@/features/analysis/components/V2WorkbenchWorkspace';
import { useV2WorkbenchModel } from '@/features/analysis/use-v2-workbench-model';

export function V2WorkbenchContainer() {
  const model = useV2WorkbenchModel();
  return <V2WorkbenchWorkspace model={model} />;
}
