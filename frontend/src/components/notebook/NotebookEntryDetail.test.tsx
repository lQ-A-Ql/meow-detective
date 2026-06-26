import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  useNotebookEntry: vi.fn(),
  useUpdateNotebookEntry: vi.fn(),
  useAddEvidenceCitation: vi.fn(),
}));

vi.mock('@/features/notebook/hooks', () => ({
  useNotebookEntry: hoisted.useNotebookEntry,
  useUpdateNotebookEntry: hoisted.useUpdateNotebookEntry,
  useAddEvidenceCitation: hoisted.useAddEvidenceCitation,
}));

// Mock the CitationPicker to avoid QueryClient dependency
vi.mock('./NotebookEntryForm', () => ({
  CitationPicker: () => null,
}));

import { EntryDetailView, RepliesSection } from './NotebookEntryDetail';
import type { NotebookEntryListItem } from '@/types/models';

describe('EntryDetailView', () => {
  it('renders loading state', () => {
    hoisted.useNotebookEntry.mockReturnValue({ data: undefined, isLoading: true, isError: false });
    hoisted.useUpdateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false });
    hoisted.useAddEvidenceCitation.mockReturnValue({ mutate: vi.fn() });
    render(createElement(EntryDetailView, { entryId: 'e1' }));
    expect(screen.getByText('加载笔记...')).toBeDefined();
  });

  it('renders error state when entry not found', () => {
    hoisted.useNotebookEntry.mockReturnValue({ data: undefined, isLoading: false, isError: true });
    hoisted.useUpdateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false });
    hoisted.useAddEvidenceCitation.mockReturnValue({ mutate: vi.fn() });
    render(createElement(EntryDetailView, { entryId: 'e1' }));
    expect(screen.getByText('笔记加载失败')).toBeDefined();
  });

  it('renders entry title and tags when loaded', () => {
    hoisted.useNotebookEntry.mockReturnValue({
      data: [{
        id: 'e1',
        caseId: 'c1',
        author: 'analyst',
        title: 'Test Note',
        bodyMarkdown: 'Hello **world**',
        entryType: 'observation',
        status: 'draft',
        tags: ['important'],
        createdAt: '2026-06-01T10:00:00Z',
        updatedAt: '2026-06-01T11:00:00Z',
      }],
      isLoading: false,
      isError: false,
    });
    hoisted.useUpdateNotebookEntry.mockReturnValue({ mutate: vi.fn(), isPending: false });
    hoisted.useAddEvidenceCitation.mockReturnValue({ mutate: vi.fn() });
    render(createElement(EntryDetailView, { entryId: 'e1' }));
    expect(screen.getByText('Test Note')).toBeDefined();
    expect(screen.getByText('important')).toBeDefined();
  });
});

describe('RepliesSection', () => {
  it('renders nothing when no replies match parentId', () => {
    const entry: NotebookEntryListItem = {
      id: 'e1',
      title: 'Root Entry',
      entryType: 'observation',
      status: 'draft',
      tags: [],
      replyCount: 0,
      createdAt: '2026-06-01T10:00:00Z',
      updatedAt: '2026-06-01T10:00:00Z',
    };
    const { container } = render(
      createElement(RepliesSection, {
        parentId: 'nonexistent',
        allEntries: [entry],
        selectedId: null,
        onSelect: vi.fn(),
      }),
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders reply entries when they exist', () => {
    const reply: NotebookEntryListItem = {
      id: 'r1',
      parentId: 'e1',
      title: 'My Reply',
      entryType: 'observation',
      status: 'draft',
      tags: [],
      replyCount: 0,
      createdAt: '2026-06-01T12:00:00Z',
      updatedAt: '2026-06-01T12:00:00Z',
    };
    render(
      createElement(RepliesSection, {
        parentId: 'e1',
        allEntries: [reply],
        selectedId: null,
        onSelect: vi.fn(),
      }),
    );
    expect(screen.getByText('My Reply')).toBeDefined();
  });
});
