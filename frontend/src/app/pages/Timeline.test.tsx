import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Timeline } from './Timeline';

const mocks = vi.hoisted(() => ({
  timelineEvents: vi.fn(),
  selectionState: {
    selectedTimelineId: undefined as string | undefined,
    selectedFileId: undefined as string | undefined,
    selectedArtifactId: undefined as string | undefined,
    setSelectedTimelineId: vi.fn(),
    setSelectedFileId: vi.fn(),
    setSelectedArtifactId: vi.fn(),
  },
}));

vi.mock('@/features/timeline/hooks', () => ({
  useTimelineEvents: mocks.timelineEvents,
}));

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn((selector) => selector(mocks.selectionState)),
}));

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
}));

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
    ...overrides,
  };
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(Timeline),
    ),
  );
}

const emptyTimelineResult = { items: [], total: 0 };
const populatedTimelineResult = {
  items: [
    {
      id: 'evt-1',
      sourceObjectId: 'file-1',
      eventType: 'FileAccess',
      ts: '2026-06-01T10:15:00Z',
      title: 'Opened report.docx',
      description: 'User opened document',
      attrs: { source: 'MFT' },
    },
    {
      id: 'evt-2',
      sourceObjectId: 'artifact-2',
      eventType: 'RegistryModified',
      ts: '2026-06-01T11:30:00Z',
      title: 'Registry key modified',
      description: 'Autorun key changed',
      attrs: { source: 'Registry' },
    },
  ],
  total: 2,
};

describe('Timeline page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(mocks.selectionState, {
      selectedTimelineId: undefined,
      selectedFileId: undefined,
      selectedArtifactId: undefined,
    });
  });

  it('renders empty state when no events', () => {
    mocks.timelineEvents.mockReturnValue(queryState({ data: emptyTimelineResult }));

    renderPage();

    expect(screen.getByText('当前时间范围无事件')).toBeDefined();
    expect(screen.getByText('请扩大时间范围或调整事件过滤条件。')).toBeDefined();
    expect(screen.getByText(/事件 0 条/)).toBeDefined();
  });

  it('renders event list when data is available', () => {
    mocks.timelineEvents.mockReturnValue(queryState({ data: populatedTimelineResult }));

    renderPage();

    expect(screen.getAllByText('Opened report.docx').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Registry key modified').length).toBeGreaterThan(0);
    expect(screen.getAllByText('FileAccess').length).toBeGreaterThan(0);
    expect(screen.getAllByText('RegistryModified').length).toBeGreaterThan(0);
    expect(screen.getByText(/事件 2 条/)).toBeDefined();
    expect(screen.getByText(/数据源 2 个/)).toBeDefined();
  });

  it('shows inspector pane for selected event', () => {
    mocks.timelineEvents.mockReturnValue(queryState({ data: populatedTimelineResult }));
    Object.assign(mocks.selectionState, { selectedTimelineId: 'evt-1' });

    renderPage();

    expect(screen.getByText('事件检查器')).toBeDefined();
    expect(screen.getAllByText('2026-06-01T10:15:00Z').length).toBeGreaterThan(0);
    expect(screen.getAllByText('User opened document').length).toBeGreaterThan(0);
    expect(screen.getByText(/当前事件 evt-1/)).toBeDefined();
  });
});
