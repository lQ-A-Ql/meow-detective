import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  useCreateNotebookEntry: vi.fn(),
}));

vi.mock('@/features/notebook/hooks', () => ({
  useCreateNotebookEntry: hoisted.useCreateNotebookEntry,
}));

vi.mock('@/features/graph/hooks', () => ({
  useGraphSnapshot: vi.fn(() => ({ data: undefined })),
}));

vi.mock('@/lib/api/graph', () => ({
  getNodeNeighborhood: vi.fn(() => Promise.resolve({ nodes: [], edges: [] })),
}));

import { EntryEditor, EntryTreeItem } from './NotebookEntryForm';
import type { NotebookEntryListItem } from '@/types/models';

const mockItem: NotebookEntryListItem = {
  id: 'e1',
  title: 'Analysis Note',
  entryType: 'observation',
  status: 'draft',
  tags: ['tag1'],
  replyCount: 2,
  createdAt: '2026-06-01T10:00:00Z',
  updatedAt: '2026-06-01T11:00:00Z',
};

describe('EntryEditor', () => {
  it('renders form fields and save button', () => {
    hoisted.useCreateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false, isError: false, error: null });
    render(createElement(EntryEditor, { onSaved: vi.fn(), onCancel: vi.fn() }));
    expect(screen.getByText('新建笔记')).toBeDefined();
    expect(screen.getByPlaceholderText('笔记标题')).toBeDefined();
    expect(screen.getByPlaceholderText('使用 Markdown 格式记录分析笔记...')).toBeDefined();
  });

  it('calls onCancel when cancel button is clicked', () => {
    const onCancel = vi.fn();
    hoisted.useCreateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false, isError: false, error: null });
    render(createElement(EntryEditor, { onSaved: vi.fn(), onCancel }));
    fireEvent.click(screen.getByText('取消'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('disables save when title is empty', () => {
    hoisted.useCreateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false, isError: false, error: null });
    render(createElement(EntryEditor, { onSaved: vi.fn(), onCancel: vi.fn() }));
    // The save button has text "保存" and a Plus icon; use getByRole to find it
    const buttons = screen.getAllByRole('button');
    const saveBtn = buttons.find((b) => b.textContent?.includes('保存'));
    expect(saveBtn).toBeDefined();
    expect((saveBtn as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('EntryTreeItem', () => {
  it('renders item title and badges', () => {
    render(
      createElement(EntryTreeItem, {
        item: mockItem,
        allItems: [mockItem],
        selectedId: null,
        onSelect: vi.fn(),
      }),
    );
    expect(screen.getByText('Analysis Note')).toBeDefined();
  });

  it('calls onSelect when clicked', () => {
    const onSelect = vi.fn();
    render(
      createElement(EntryTreeItem, {
        item: mockItem,
        allItems: [mockItem],
        selectedId: null,
        onSelect,
      }),
    );
    (screen.getByText('Analysis Note').closest('div[class*="cursor-pointer"]') as HTMLElement | null)?.click();
    expect(onSelect).toHaveBeenCalledWith('e1');
  });
});
