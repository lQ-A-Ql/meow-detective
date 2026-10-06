import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { McpResourceList } from '@/features/mcp/components/McpResourceList';

describe('McpResourceList', () => {
  it('copies a resource URI from the row action', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    render(<McpResourceList resources={[{ uri: 'forensics://cases', name: 'Cases' }]} loading={false} onRefresh={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: '复制 Cases URI' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('forensics://cases'));
    expect(screen.getByTitle('复制资源 URI')).toBeInTheDocument();
  });
});
