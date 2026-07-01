import { createElement } from 'react';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { GqlResultView } from './GqlResultView';
import type { GraphQueryResult } from '@/types/models';

const populatedResult: GraphQueryResult = {
  nodes: [
    {
      id: 'n1',
      caseId: 'case-1',
      nodeType: 'file',
      label: 'readme.txt',
      summary: 'A readme file',
      tags: ['doc'],
      createdAt: '2026-06-01T10:00:00Z',
    },
  ],
  edges: [
    {
      id: 'e1',
      caseId: 'case-1',
      edgeType: 'references',
      sourceId: 'n1',
      targetId: 'n2',
      confidence: 0.85,
      createdAt: '2026-06-01T10:00:00Z',
    },
  ],
  nodeCount: 1,
  edgeCount: 1,
};

const emptyResult: GraphQueryResult = {
  nodes: [],
  edges: [],
  nodeCount: 0,
  edgeCount: 0,
};

describe('GqlResultView', () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the empty state when there are no nodes or edges', () => {
    render(createElement(GqlResultView, { result: emptyResult }));
    expect(screen.getByText('Query returned no results.')).toBeDefined();
  });

  it('renders result stats and node/edge summary rows', () => {
    render(createElement(GqlResultView, { result: populatedResult }));
    expect(screen.getByText('readme.txt')).toBeDefined();
    expect(screen.getByText('n1 → n2')).toBeDefined();
    expect(screen.getByText('85%')).toBeDefined();
  });

  it('expands and collapses a node row to reveal its fields', () => {
    render(createElement(GqlResultView, { result: populatedResult }));
    const nodeRow = screen.getByText('readme.txt').closest('[role="button"]') as HTMLElement;

    expect(screen.queryByText(/summary: A readme file/)).toBeNull();
    fireEvent.click(nodeRow);
    expect(screen.getByText(/summary: A readme file/)).toBeDefined();

    fireEvent.click(nodeRow);
    expect(screen.queryByText(/summary: A readme file/)).toBeNull();
  });

  it('expands a node row via keyboard (Enter)', () => {
    render(createElement(GqlResultView, { result: populatedResult }));
    const nodeRow = screen.getByText('readme.txt').closest('[role="button"]') as HTMLElement;

    fireEvent.keyDown(nodeRow, { key: 'Enter' });
    expect(screen.getByText(/summary: A readme file/)).toBeDefined();
  });

  it('expands an edge row to reveal its fields', () => {
    render(createElement(GqlResultView, { result: populatedResult }));
    const edgeRow = screen.getByText('n1 → n2').closest('[role="button"]') as HTMLElement;

    fireEvent.click(edgeRow);
    expect(screen.getByText(/confidence: 0.85/)).toBeDefined();
  });

  it('copies the node id to the clipboard and shows a confirmation icon', async () => {
    render(createElement(GqlResultView, { result: populatedResult }));
    const nodeRow = screen.getByText('readme.txt').closest('div')?.parentElement as HTMLElement;
    const copyButton = within(nodeRow).getByTitle('Copy ID');

    fireEvent.click(copyButton);
    await vi.waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('n1'),
    );
  });
});
