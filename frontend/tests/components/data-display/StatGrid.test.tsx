import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MetricCard, StatGrid } from '@/components/data-display';

describe('StatGrid zero collapse', () => {
  it('hides zero tiles when the grid also carries real values and reports how many', () => {
    render(
      <StatGrid collapseZeros hiddenNote={(hidden) => 'hidden:' + hidden}>
        <MetricCard label="Nodes" value={42} />
        <MetricCard label="Edges" value={0} />
        <MetricCard label="Density" value="0" />
      </StatGrid>,
    );

    expect(screen.getByText('Nodes')).toBeDefined();
    expect(screen.queryByText('Edges')).toBeNull();
    expect(screen.getByText('hidden:2')).toBeDefined();
  });

  it('keeps every tile when the whole grid is zero, so a section never becomes an empty shell', () => {
    render(
      <StatGrid collapseZeros hiddenNote={(hidden) => 'hidden:' + hidden}>
        <MetricCard label="Nodes" value={0} />
        <MetricCard label="Edges" value={0} />
      </StatGrid>,
    );

    expect(screen.getByText('Nodes')).toBeDefined();
    expect(screen.getByText('Edges')).toBeDefined();
    expect(screen.queryByText(/hidden:/)).toBeNull();
  });

  it('leaves the grid untouched when collapse is off', () => {
    render(
      <StatGrid>
        <MetricCard label="Nodes" value={7} />
        <MetricCard label="Edges" value={0} />
      </StatGrid>,
    );

    expect(screen.getByText('Edges')).toBeDefined();
  });
});
