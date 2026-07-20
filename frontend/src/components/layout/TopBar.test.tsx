import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TopBar } from './TopBar';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  jobs: vi.fn(),
  uiState: {
    setCurrentPage: vi.fn(),
    globalSearchQuery: '',
    setGlobalSearchQuery: vi.fn(),
    toggleDrawer: vi.fn(),
  },
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobs,
}));

vi.mock('@/stores/ui-store', () => ({
  useUiStore: (selector: (state: typeof mocks.uiState) => unknown) => selector(mocks.uiState),
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

describe('TopBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryState({
      number: '2026-FX-091',
      name: 'WannaCry 爆发溯源',
      examiner: 'Qin Ao',
      updatedAt: '2026-05-16T11:20:00Z',
    }));
    mocks.jobs.mockReturnValue(queryState([]));
    mocks.uiState.globalSearchQuery = '';
  });

  it('keeps navigation, the case name, search, and compact tools', () => {
    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.getByText('案件概览')).toBeDefined();
    expect(screen.getByText('WannaCry 爆发溯源')).toBeDefined();
    expect(screen.getByPlaceholderText('输入全局检索语句或 IOC')).toBeDefined();
    expect(screen.getByRole('button', { name: '0 运行中' })).toBeDefined();
    expect(screen.getByRole('button', { name: '设置' })).toBeDefined();
  });

  it('uses a count badge instead of detailed runtime status chips', () => {
    mocks.jobs.mockReturnValue(queryState([
      { id: 'job-1', status: 'running' },
      { id: 'job-2', status: 'running' },
      { id: 'job-3', status: 'completed' },
    ]));

    render(
      <MemoryRouter>
        <TopBar />
      </MemoryRouter>,
    );

    expect(screen.getByRole('button', { name: '2 运行中' })).toBeDefined();
    expect(screen.getByText('2')).toBeDefined();
    expect(screen.queryByText('Import')).toBeNull();
    expect(screen.queryByText('Hash')).toBeNull();
  });
});
