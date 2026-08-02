import { TopBar } from '@/components/layout/TopBar';
import { useTopBarModel } from '@/features/shell/use-top-bar-model';

export function TopBarContainer() {
  const model = useTopBarModel();
  return <TopBar model={model} />;
}
