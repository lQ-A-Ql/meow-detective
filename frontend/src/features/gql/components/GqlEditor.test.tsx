import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GqlEditor } from './GqlEditor';
import type { GraphQueryResult } from '@/types/models';

describe('GqlEditor', () => {
  it('renders query input area with default placeholder', () => {
    render(createElement(GqlEditor, {}));
    expect(screen.getByText('GQL Query')).toBeDefined();
    expect(screen.getByRole('button', { name: /Run/ })).toBeDefined();
  });

  it('renders error message when error is provided', () => {
    render(createElement(GqlEditor, { error: 'Syntax error at line 1' }));
    expect(screen.getByText('Syntax error at line 1')).toBeDefined();
  });

  it('renders result view when result is provided', () => {
    const result: GraphQueryResult = {
      nodes: [
        { id: 'n1', caseId: 'case-1', nodeType: 'file', label: 'readme.txt', summary: 'A file', tags: [], createdAt: '2026-06-01T10:00:00Z' },
      ],
      edges: [],
      nodeCount: 1,
      edgeCount: 0,
    };
    render(createElement(GqlEditor, { result }));
    // "1" appears in node count, total matched etc. — use getAllByText
    expect(screen.getAllByText('1').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('readme.txt')).toBeDefined();
  });

  it('calls onExecute when Run button is clicked', () => {
    const onExecute = vi.fn();
    render(createElement(GqlEditor, { onExecute, initialQuery: 'MATCH (n) RETURN n' }));
    fireEvent.click(screen.getByRole('button', { name: /Run/ }));
    expect(onExecute).toHaveBeenCalledWith('MATCH (n) RETURN n');
  });
});
