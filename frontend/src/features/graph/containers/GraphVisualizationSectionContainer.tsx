import { useElementSize } from '@/hooks/use-element-size';
import type { GraphVisualizationModel } from '../use-graph-visualization-model';
import { GraphVisualizationSectionView } from '../components/GraphVisualizationSectionView';
export function GraphVisualizationSection({ model }: { model: GraphVisualizationModel }) {
  const canvas = useElementSize<HTMLDivElement>();
  return <GraphVisualizationSectionView model={model} canvasRef={canvas.ref} canvasSize={canvas.size} />;
}
