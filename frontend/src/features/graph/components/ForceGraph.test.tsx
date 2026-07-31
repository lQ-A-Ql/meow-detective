import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ForceGraph } from './ForceGraph';

describe('ForceGraph', () => {
  it('ends panning when the pointer is released outside the graph', () => {
    render(
      <ForceGraph
        nodes={[]}
        edges={[]}
        width={640}
        height={480}
        running={false}
      />,
    );
    const graph = screen.getByRole('img', { name: '关系图谱力导向图' });
    const viewport = graph.querySelector('g[transform]');

    fireEvent(
      graph,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 30,
        clientY: 20,
      }),
    );
    expect(viewport).toHaveAttribute('transform', 'translate(20 10) scale(1)');

    fireEvent(window, new MouseEvent('pointerup', { bubbles: true }));
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 60,
        clientY: 50,
      }),
    );
    expect(viewport).toHaveAttribute('transform', 'translate(20 10) scale(1)');
  });

  it('ends panning when the window loses focus', () => {
    render(
      <ForceGraph
        nodes={[]}
        edges={[]}
        width={640}
        height={480}
        running={false}
      />,
    );
    const graph = screen.getByRole('img', { name: '关系图谱力导向图' });
    const viewport = graph.querySelector('g[transform]');

    fireEvent(
      graph,
      new MouseEvent('pointerdown', {
        bubbles: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    );
    fireEvent.blur(window);
    fireEvent(
      graph,
      new MouseEvent('pointermove', {
        bubbles: true,
        clientX: 50,
        clientY: 30,
      }),
    );

    expect(viewport).toHaveAttribute('transform', 'translate(0 0) scale(1)');
  });
});
