import { AnalysisProgressDrawerContainer } from '@/features/analysis/containers/AnalysisProgressDrawerContainer';
import { BottomDrawer } from '@/features/jobs/components/BottomDrawer';
import { useBottomDrawerModel } from '@/features/jobs/use-bottom-drawer-model';

export function BottomDrawerContainer() {
  const model = useBottomDrawerModel();
  return <BottomDrawer model={model} analysisProgress={<AnalysisProgressDrawerContainer />} />;
}
