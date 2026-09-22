export const visualizationTokens = {
  graph: {
    node: {
      file: '--forensics-graph-node-file',
      artifact: '--forensics-graph-node-artifact',
      timelineEvent: '--forensics-graph-node-timeline-event',
      entity: '--forensics-graph-node-entity',
      lead: '--forensics-graph-node-lead',
      notebookEntry: '--forensics-graph-node-notebook-entry',
    },
    edge: {
      contains: '--forensics-graph-edge-contains',
      references: '--forensics-graph-edge-references',
      correlatesWith: '--forensics-graph-edge-correlates',
      derivesFrom: '--forensics-graph-edge-derives',
      precedes: '--forensics-graph-edge-precedes',
      cites: '--forensics-graph-edge-cites',
      annotates: '--forensics-graph-edge-annotates',
    },
  },
  infrastructure: {
    text: '--forensics-viz-text',
    muted: '--forensics-viz-muted',
    border: '--forensics-viz-border',
    canvas: '--forensics-viz-canvas',
    lane: '--forensics-viz-lane',
    selected: '--forensics-viz-selected',
    edge: '--forensics-viz-edge',
    environment: '--forensics-viz-environment',
    storage: '--forensics-viz-storage',
    analysis: '--forensics-viz-analysis',
    fontFamily: '--forensics-viz-font-family',
  },
} as const;

export type InfrastructureVisualizationPalette = Record<
  keyof typeof visualizationTokens.infrastructure,
  string
>;

export function resolveInfrastructureVisualizationPalette(element: Element): InfrastructureVisualizationPalette {
  const computed = window.getComputedStyle(element);
  return Object.fromEntries(
    Object.entries(visualizationTokens.infrastructure).map(([name, token]) => [name, computed.getPropertyValue(token).trim()]),
  ) as InfrastructureVisualizationPalette;
}
