import { GraphVisualizationSection } from '@/features/graph/components/GraphVisualizationSection';
import { useGraphVisualizationModel } from '@/features/graph/use-graph-visualization-model';

export function GraphVisualizationContainer() {
  const model = useGraphVisualizationModel();
  return <GraphVisualizationSection model={model} />;
}
