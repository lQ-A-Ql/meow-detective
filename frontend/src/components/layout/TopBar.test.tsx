import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TopBar } from './TopBar';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  jobs: vi.fn(),
  warnings: vi.fn(),
  uiState: {
    currentPage: 'home',
    setCurrentPage: vi.fn(),
    globalSearchQuery: '',
    setGlobalSearchQuery: vi.fn(),
    toggleDrawer: vi.fn(),
  },
  apiMode: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobs,
  useWarnings: mocks.warnings,
}));

vi.mock('@/stores/ui-store', () => ({
  useUiStore: (selector: (state: typeof mocks.uiState) => unknown) => selector(mocks.uiState),
}));

vi.mock('@/lib/api/client', () => ({
  apiMode: () => mocks.apiMode(),
}));

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
  };
}

describe('TopBar mock mode label', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryState({
      number: '2026-FX-091',
      name: 'WannaCry 爆发溯源',
      examiner: 'Qin Ao',
      updatedAt: '2026-05-16T11:20:00Z',
    }));
    mocks.jobs.mockReturnValue(queryState([]));
    mocks.warnings.mockReturnValue(queryState([]));
    mocks.apiMode.mockReturnValue('mock');
    mocks.uiState.currentPage = 'home';
    mocks.uiState.globalSearchQuery = '';
  });

  it('shows an accessible mock data label in mock mode', () => {
    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.getByRole('status', { name: 'Mock mode data label' })).toBeDefined();
    expect(screen.getByText('Mock Mode')).toBeDefined();
    expect(screen.getByText('显示演示取证数据')).toBeDefined();
  });

  it('hides the mock data label in tauri mode', () => {
    mocks.apiMode.mockReturnValue('tauri');

    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.queryByRole('status', { name: 'Mock mode data label' })).toBeNull();
  });
});
