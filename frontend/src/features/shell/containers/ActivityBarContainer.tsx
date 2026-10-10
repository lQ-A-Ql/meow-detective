import { ActivityBar } from '@/components/navigation/ActivityBar';
import { useNavigationModel } from '@/features/shell/use-navigation-model';
import { useDensityStore } from '@/stores/density-store';

export function ActivityBarContainer() {
  const model = useNavigationModel();
  const density = useDensityStore((state) => state.density);
  const toggleDensity = useDensityStore((state) => state.toggleDensity);
  return <ActivityBar model={model} density={density} onToggleDensity={toggleDensity} />;
}
