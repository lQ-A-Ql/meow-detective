import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: () => number }) => ({
    getTotalSize: () => count * estimateSize(),
    getVirtualItems: () =>
      Array.from({ length: Math.min(count, 12) }, (_, index) => ({
        index,
        size: estimateSize(),
        start: index * estimateSize(),
      })),
  }),
}));

import { VirtualFileTree } from './VirtualFileTree';
import type { FileTreeNode } from '@/types/models';

describe('VirtualFileTree', () => {
  it('renders a bounded visible window for large node lists', () => {
    const nodes: FileTreeNode[] = Array.from({ length: 10_000 }, (_, index) => ({
      id: `node-${index}`,
      name: `Node ${index}`,
      depth: index % 4,
      hasChildren: index % 7 === 0,
      entryType: index % 7 === 0 ? 'directory' : 'file',
      expanded: index % 7 === 0,
    }));

    const { container } = render(
      <div style={{ height: '280px' }}>
        <VirtualFileTree nodes={nodes} onNodeClick={vi.fn()} itemSize={28} overscan={2} />
      </div>
    );

    const renderedButtons = container.querySelectorAll('button');
    expect(renderedButtons.length).toBe(12);
    expect(renderedButtons.length).toBeLessThan(nodes.length);
    expect(container.textContent).toContain('Node 0');
    expect(container.textContent).toContain('Node 11');
    expect(container.textContent).not.toContain('Node 12');
    expect(container.textContent).not.toContain('Node 9999');
  });
});
