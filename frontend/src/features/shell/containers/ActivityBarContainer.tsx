import { ActivityBar } from '@/components/navigation/ActivityBar';
import { useTopBarModel } from '@/features/shell/use-top-bar-model';

export function ActivityBarContainer() {
  const model = useTopBarModel();
  return <ActivityBar model={model} />;
}
