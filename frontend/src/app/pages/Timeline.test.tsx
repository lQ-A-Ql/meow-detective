import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PagedResponse } from '@/lib/api/timeline';
import type { TimelineEvent } from '@/types/models';
import { Timeline } from './Timeline';

const mocks = vi.hoisted(() => ({
  infiniteTimelineEvents: vi.fn(),
  timelineEventById: vi.fn(),
  navigate: vi.fn(),
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
  useTimelineEventById: mocks.timelineEventById,
  useInfiniteTimelineEvents: mocks.infiniteTimelineEvents,
}));

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn((selector) => selector(mocks.selectionState)),
}));

vi.mock('react-router', () => ({
  useNavigate: () => mocks.navigate,
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

function infiniteQueryState(page: typeof emptyTimelineResult) {
  return queryState({
    data: { pages: [page], pageParams: [0] },
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    isFetchingNextPage: false,
    isFetchNextPageError: false,
  });
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

const emptyTimelineResult: PagedResponse<TimelineEvent> = { items: [], total: 0 };
const populatedTimelineResult: PagedResponse<TimelineEvent> = {
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
    mocks.timelineEventById.mockReturnValue(queryState({ data: null }));
  });

  it('renders empty state when no events', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(emptyTimelineResult));

    renderPage();

    expect(screen.getByText('当前时间范围无事件')).toBeDefined();
    expect(screen.getByText('请扩大时间范围或调整事件过滤条件。')).toBeDefined();
    expect(screen.getByText(/事件 0\/0 条/)).toBeDefined();
  });

  it('renders event list when data is available', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(populatedTimelineResult));

    renderPage();

    expect(screen.getAllByText('Opened report.docx').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Registry key modified').length).toBeGreaterThan(0);
    expect(screen.getAllByText('FileAccess').length).toBeGreaterThan(0);
    expect(screen.getAllByText('RegistryModified').length).toBeGreaterThan(0);
    expect(screen.getByText(/事件 2\/2 条/)).toBeDefined();
    expect(screen.getByText(/数据源 2 个/)).toBeDefined();
  });

  it('shows inspector pane for selected event', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(populatedTimelineResult));
    Object.assign(mocks.selectionState, { selectedTimelineId: 'evt-1' });

    renderPage();

    expect(screen.getByText('事件检查器')).toBeDefined();
    expect(screen.getAllByText('2026-06-01T10:15:00Z').length).toBeGreaterThan(0);
    expect(screen.getAllByText('User opened document').length).toBeGreaterThan(0);
    expect(screen.getByText(/当前事件 evt-1/)).toBeDefined();
  });

  it('hydrates a selected event from by-id query when it is outside the current page', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(emptyTimelineResult));
    Object.assign(mocks.selectionState, { selectedTimelineId: 'evt-offpage' });
    mocks.timelineEventById.mockReturnValue(
      queryState({
        data: {
          id: 'evt-offpage',
          sourceObjectId: 'file-9',
          eventType: 'BrowserDownload',
          ts: '2026-06-13T08:00:00Z',
          title: 'Downloaded payload.exe',
          description: 'Browser saved payload.exe',
          attrs: { source: 'BrowserHistory' },
        },
      }),
    );

    renderPage();

    expect(screen.getAllByText('Downloaded payload.exe').length).toBeGreaterThan(0);
    expect(screen.getByText(/当前事件 evt-offpage/)).toBeDefined();
  });

  it('zoom buttons change the number of rendered timeline bars', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(populatedTimelineResult));

    renderPage();

    const bars = () => screen.getAllByTitle(/条事件/);
    const initialCount = bars().length;

    fireEvent.click(screen.getByLabelText('放大'));
    expect(bars().length).toBeGreaterThan(initialCount);

    fireEvent.click(screen.getByLabelText('缩小'));
    fireEvent.click(screen.getByLabelText('缩小'));
    expect(bars().length).toBeLessThan(initialCount);
  });

  it('disables Apply and shows an error when the date input does not parse', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(populatedTimelineResult));

    renderPage();

    const startInput = screen.getByLabelText('起始') as HTMLInputElement;
    // jsdom sanitizes datetime-local values to the HTML5 format at the DOM level, so an
    // out-of-range year is the way to get a value that is format-valid but Date.parse-invalid.
    fireEvent.change(startInput, { target: { value: '275760-09-14T00:00' } });

    expect(screen.getByText('日期无效')).toBeDefined();
    expect((screen.getByRole('button', { name: '应用' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('applies a valid date range without throwing', () => {
    mocks.infiniteTimelineEvents.mockReturnValue(infiniteQueryState(populatedTimelineResult));

    renderPage();

    const startInput = screen.getByLabelText('起始') as HTMLInputElement;
    fireEvent.change(startInput, { target: { value: '2026-06-01T00:00' } });

    expect(screen.queryByText('日期无效')).toBeNull();
    const applyButton = screen.getByRole('button', { name: '应用' }) as HTMLButtonElement;
    expect(applyButton.disabled).toBe(false);
    fireEvent.click(applyButton);
  });
});
