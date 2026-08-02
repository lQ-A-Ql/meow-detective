import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { NotebookPanel } from './NotebookPanel';
import type { NotebookPanelModel } from '@/features/notebook/model/notebook-panel-model';
import type { NotebookEntryListItem } from '@/types/models';

function buildModel(overrides: Partial<NotebookPanelModel> = {}): NotebookPanelModel {
  return {
    caseLoading: false,
    hasActiveCase: true,
    entriesLoading: false,
    entriesError: false,
    entries: [],
    rootEntries: [],
    typeCounts: {},
    selectedId: null,
    showNewEntry: false,
    showNewReply: false,
    filterType: '',
    filterStatus: '',
    filterDate: 'all',
    createPending: false,
    detailLoading: false,
    detailError: false,
    updatePending: false,
    citationNodes: [],
    citationNodesLoading: false,
    selectEntry: vi.fn(),
    setShowNewEntry: vi.fn(),
    setShowNewReply: vi.fn(),
    setFilterType: vi.fn(),
    setFilterStatus: vi.fn(),
    setFilterDate: vi.fn(),
    retryEntries: vi.fn(),
    createEntry: vi.fn(),
    updateEntry: vi.fn(),
    addCitations: vi.fn(),
    ...overrides,
  };
}

describe('NotebookPanel', () => {
  it('shows loading state when case is loading', () => {
    render(createElement(NotebookPanel, { model: buildModel({ caseLoading: true }) }));
    expect(screen.getByText('正在加载案件...')).toBeDefined();
  });

  it('shows empty state when no case is active', () => {
    render(createElement(NotebookPanel, { model: buildModel({ hasActiveCase: false }) }));
    expect(screen.getByText('请先打开或创建一个案件')).toBeDefined();
  });

  it('shows empty notebook state when entries are empty', () => {
    render(createElement(NotebookPanel, { model: buildModel() }));
    expect(screen.getByText('笔记面板')).toBeDefined();
    expect(screen.getByText('暂无笔记')).toBeDefined();
  });

  it('renders entry list when entries exist', () => {
    const entries: NotebookEntryListItem[] = [
        {
          id: 'entry-1',
          title: 'First observation',
          entryType: 'observation',
          status: 'draft',
          tags: [],
          replyCount: 0,
          createdAt: '2026-06-01T10:00:00Z',
          updatedAt: '2026-06-01T10:00:00Z',
        },
        {
          id: 'entry-2',
          title: 'Key finding',
          entryType: 'finding',
          status: 'reviewed',
          tags: ['important'],
          replyCount: 0,
          createdAt: '2026-06-02T10:00:00Z',
          updatedAt: '2026-06-02T10:00:00Z',
        },
      ];
    render(createElement(NotebookPanel, {
      model: buildModel({
        entries,
        rootEntries: entries,
        typeCounts: { observation: 1, finding: 1 },
      }),
    }));
    expect(screen.getByText('First observation')).toBeDefined();
    expect(screen.getByText('Key finding')).toBeDefined();
    expect(screen.getByText('总计: 2')).toBeDefined();
  });
});
