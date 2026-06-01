import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomDrawer } from './BottomDrawer';

const mocks = vi.hoisted(() => ({
  jobs: vi.fn(),
  warnings: vi.fn(),
  trace: vi.fn(),
  uiStore: vi.fn(),
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobs,
  useWarnings: mocks.warnings,
  useTraceItems: mocks.trace,
}));

vi.mock('@/lib/api/client', () => ({
  apiMode: () => 'mock',
}));

vi.mock('@/stores/ui-store', () => ({
  useUiStore: (selector: (state: { drawerOpen: boolean; toggleDrawer: () => void }) => unknown) =>
    selector(mocks.uiStore()),
}));

describe('BottomDrawer jobs panel', () => {
  beforeEach(() => {
    mocks.jobs.mockReturnValue({
      data: [
        {
          id: 'job-running',
          name: 'Import data source',
          scope: 'Case ingest',
          progress: 41,
          status: 'running',
          detail: 'Enumerating',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
        {
          id: 'job-partial',
          name: 'Artifact extraction',
          scope: 'Post-import',
          progress: 100,
          status: 'completed',
          detail: 'Completed with warnings',
          warningCount: 2,
          skippedCount: 3,
          failedCount: 0,
          partial: true,
        },
      ],
    });
    mocks.warnings.mockReturnValue({ data: [] });
    mocks.trace.mockReturnValue({ data: [] });
    mocks.uiStore.mockReturnValue({ drawerOpen: true, toggleDrawer: vi.fn() });
  });

  it('renders partial badge and outcome counts', () => {
    render(<BottomDrawer />);

    expect(screen.getAllByText('PARTIAL').length).toBeGreaterThan(0);
    expect(screen.getByText('warnings 2')).toBeDefined();
    expect(screen.getByText('skipped 3')).toBeDefined();
    expect(screen.getByText('failed 0')).toBeDefined();
    expect(screen.getByText(/1 运行 \/ 1 完成 \/ 1 部分 \/ 0 失败/)).toBeDefined();
  });

  it('keeps running jobs visible for cancel surfaces outside the drawer', () => {
    render(<BottomDrawer />);

    expect(screen.getByText('Import data source')).toBeDefined();
    expect(screen.getByText('41%')).toBeDefined();
  });
});
