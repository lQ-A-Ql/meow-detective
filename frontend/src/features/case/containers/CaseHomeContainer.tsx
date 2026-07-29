import { CaseHomeWorkspace } from '@/features/case/components/CaseHomeWorkspace';
import { useCaseHomeModel } from '@/features/case/use-case-home-model';

export function CaseHomeContainer() {
  const model = useCaseHomeModel();
  return <CaseHomeWorkspace model={model} />;
}
