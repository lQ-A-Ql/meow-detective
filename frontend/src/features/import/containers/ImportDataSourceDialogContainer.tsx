import { ImportDataSourceDialogView } from '../components/ImportDataSourceDialogView';
import { useImportWizardModel } from '../use-import-wizard-model';
import type { ImportDataSourceDialogProps } from '../types';

export function ImportDataSourceDialogContainer(props: ImportDataSourceDialogProps) {
  const model = useImportWizardModel(props);
  return <ImportDataSourceDialogView model={model} />;
}
