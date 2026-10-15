import { ForceGraph as ForceGraphView } from '../visualization/ForceGraph';
import type { ComponentProps } from 'react';
export function ForceGraph(props: ComponentProps<typeof ForceGraphView>) { return <ForceGraphView {...props} />; }
