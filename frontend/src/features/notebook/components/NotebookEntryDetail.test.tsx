import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

// Mock the CitationPicker to avoid QueryClient dependency
vi.mock('./NotebookEntryForm', () => ({
  CitationPicker: () => null,
}));

import { EntryDetailView, RepliesSection } from './NotebookEntryDetail';
import type { NotebookEntry, NotebookEntryListItem } from '@/types/models';

const detailActions = {
  updatePending: false,
  citationNodes: [],
  citationNodesLoading: false,
  onUpdate: vi.fn(),
  onAddCitations: vi.fn(),
};

describe('EntryDetailView', () => {
  it('renders loading state', () => {
    render(createElement(EntryDetailView, {
      ...detailActions,
      loading: true,
      error: false,
    }));
    expect(screen.getByText('加载笔记...')).toBeDefined();
  });

  it('renders error state when entry not found', () => {
    render(createElement(EntryDetailView, {
      ...detailActions,
      loading: false,
      error: true,
    }));
    expect(screen.getByText('笔记加载失败')).toBeDefined();
  });

  it('renders entry title and tags when loaded', () => {
    const entry: NotebookEntry = {
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
      };
    render(createElement(EntryDetailView, {
      ...detailActions,
      entry,
      loading: false,
      error: false,
    }));
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
