import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AnalysisProgressDrawer } from './AnalysisProgressDrawer';
import type { DataSourceSummary } from '@/types/models';

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
  it('stays out of the drawer until the selected source has extraction activity', () => {
    render(<AnalysisProgressDrawer source={linuxSource} extractionRunning={false} progress={[]} />);

    expect(screen.queryByTestId('analysis-progress-drawer')).toBeNull();
  });

  it('shows the selected source progress and extraction counts', () => {
    render(
      <AnalysisProgressDrawer
        source={linuxSource}
        extractionRunning={false}
        progress={[{
          label: 'Linux 日志提取',
          status: 'success',
          scannedCount: 456,
          artifactCount: 123,
          timelineEventCount: 12,
          warnings: [],
          totalCandidateCount: 456,
          processedCandidateCount: 456,
          structuredCandidateCount: 123,
          unsupportedCandidateCount: 0,
          textFallbackCandidateCount: 0,
          warningCandidateCount: 0,
          checkpointHitCount: 0,
        }]}
      />,
    );

    const drawer = screen.getByTestId('analysis-progress-drawer');
    expect(drawer.textContent).toContain('数据源提取进度');
    expect(drawer.textContent).toContain('Linux Server');
    expect(drawer.textContent).toContain('Linux 日志提取');
    expect(drawer.textContent).toContain('artifacts=123');
    expect(drawer.textContent).not.toContain('注册表提取');
  });
});
