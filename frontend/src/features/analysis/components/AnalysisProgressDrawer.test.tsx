import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AnalysisProgressDrawer } from './AnalysisProgressDrawer';
import { useAnalysisStore } from '@/stores/analysis-store';
import type { DataSourceSummary } from '@/types/models';

const mocks = vi.hoisted(() => ({
  dataSources: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useDataSources: mocks.dataSources,
}));

const linuxSource: DataSourceSummary = {
  id: 'ds-linux',
  name: 'Linux Server',
  kind: 'e01',
  sourcePath: 'E:/cases/linux.E01',
  importedAt: '2026-06-01T00:00:00Z',
  importState: 'ready',
  platform: 'linux',
};

describe('AnalysisProgressDrawer', () => {
  beforeEach(() => {
    act(() => useAnalysisStore.getState().reset());
    mocks.dataSources.mockReturnValue({ data: [linuxSource] });
  });

  it('stays out of the drawer until the selected source has extraction activity', () => {
    act(() => useAnalysisStore.getState().setSelectedDataSourceId(linuxSource.id));

    render(<AnalysisProgressDrawer />);

    expect(screen.queryByTestId('analysis-progress-drawer')).toBeNull();
  });

  it('shows the selected source progress and extraction counts', () => {
    act(() => {
      const store = useAnalysisStore.getState();
      store.setSelectedDataSourceId(linuxSource.id);
      store.updateExtractionProgress('LinuxJournal', {
        status: 'success',
        scannedCount: 456,
        artifactCount: 123,
        timelineEventCount: 12,
      });
    });

    render(<AnalysisProgressDrawer />);

    const drawer = screen.getByTestId('analysis-progress-drawer');
    expect(drawer.textContent).toContain('数据源提取进度');
    expect(drawer.textContent).toContain('Linux Server');
    expect(drawer.textContent).toContain('Linux 日志提取');
    expect(drawer.textContent).toContain('artifacts=123');
    expect(drawer.textContent).not.toContain('注册表提取');
  });
});
