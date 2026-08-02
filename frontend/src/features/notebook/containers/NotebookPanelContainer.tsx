import { NotebookPanel } from '@/features/notebook/components/NotebookPanel';
import { useNotebookPanelModel } from '@/features/notebook/use-notebook-panel-model';

export function NotebookPanelContainer() {
  const model = useNotebookPanelModel();
  return <NotebookPanel model={model} />;
}
