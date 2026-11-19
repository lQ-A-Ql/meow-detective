import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TimelineProjectionBanner } from '@/features/timeline/components/TimelineProjectionBanner';
import type { TimelineProjectionSourceStatus } from '@/types/models';

const deferredSource: TimelineProjectionSourceStatus = {
  dataSourceId: 'source-1',
  state: 'deferred',
  insertedEvents: 0,
  reason: 'workspace limit exceeded',
  requiredBytes: 134_217_728,
  availableBytes: 67_108_864,
};

const buildingSource: TimelineProjectionSourceStatus = {
  dataSourceId: 'source-1',
  state: 'building',
  insertedEvents: 42,
};

function renderBanner(overrides: Partial<Parameters<typeof TimelineProjectionBanner>[0]> = {}) {
  const onRebuild = vi.fn();
  render(
    <TimelineProjectionBanner
      state="complete"
      sources={[]}
      rebuilding={false}
      rebuildError={undefined}
      onRebuild={onRebuild}
      {...overrides}
    />,
  );
  return { onRebuild };
}

describe('TimelineProjectionBanner', () => {
  it('renders nothing when the projection is complete', () => {
    renderBanner({ state: 'complete', sources: [deferredSource] });
    expect(screen.queryByTestId('timeline-projection-banner')).not.toBeInTheDocument();
  });

  it('renders nothing before the status has loaded', () => {
    renderBanner({ state: undefined, sources: [] });
    expect(screen.queryByTestId('timeline-projection-banner')).not.toBeInTheDocument();
  });

  it('shows the building notice with a disabled action and never triggers rebuild', () => {
    const { onRebuild } = renderBanner({ state: 'building', sources: [buildingSource] });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toBeInTheDocument();
    const button = banner.querySelector('button');
    expect(button).not.toBeNull();
    expect(button).toBeDisabled();
    fireEvent.click(button!);
    expect(onRebuild).not.toHaveBeenCalled();
  });

  it('shows deferred capacity details and rebuilds case-wide when the button is clicked', () => {
    const { onRebuild } = renderBanner({ state: 'deferred', sources: [deferredSource] });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toHaveTextContent('workspace limit exceeded');
    expect(banner).toHaveTextContent('128.0 MB');
    expect(banner).toHaveTextContent('64.0 MB');
    const button = screen.getByRole('button', { name: /立即重建/ });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(onRebuild).toHaveBeenCalledTimes(1);
    expect(onRebuild).toHaveBeenCalledWith(undefined);
  });

  it('disables the rebuild button while a rebuild is in flight', () => {
    renderBanner({ state: 'deferred', sources: [deferredSource], rebuilding: true });
    const button = screen.getByRole('button', { name: /重建中/ });
    expect(button).toBeDisabled();
  });

  it('surfaces a rebuild failure instead of swallowing it', () => {
    renderBanner({ state: 'deferred', sources: [deferredSource], rebuildError: new Error('boom') });
    expect(screen.getByTestId('timeline-projection-banner')).toHaveTextContent('boom');
  });

  it('shows running progress from the capability row with a disabled action', () => {
    const { onRebuild } = renderBanner({
      state: undefined,
      capabilityState: 'running',
      capabilityProgress: { done: 42, total: 100, phase: 'indexing' },
    });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toHaveTextContent('42/100');
    expect(banner).toHaveTextContent('indexing');
    const button = banner.querySelector('button')!;
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(onRebuild).not.toHaveBeenCalled();
  });

  it('marks partial results as non-complete and keeps rebuild available', () => {
    const { onRebuild } = renderBanner({ state: undefined, capabilityState: 'partial' });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toHaveTextContent('部分');
    const button = screen.getByRole('button', { name: /立即重建/ });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(onRebuild).toHaveBeenCalledWith(undefined);
  });

  it('shows failed capability state with a retry action and the error code', () => {
    const { onRebuild } = renderBanner({
      state: undefined,
      capabilityState: 'failed',
      capabilityError: 'PROJECTION_WRITE_FAILED',
    });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toHaveTextContent('时间线生成失败');
    expect(banner).toHaveTextContent('PROJECTION_WRITE_FAILED');
    const button = screen.getByRole('button', { name: /重试重建/ });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(onRebuild).toHaveBeenCalledWith(undefined);
  });

  it('uses the deferred capability detail with a rebuild action', () => {
    renderBanner({ state: undefined, capabilityState: 'deferred', capabilityDetail: 'workspace limit' });
    const banner = screen.getByTestId('timeline-projection-banner');
    expect(banner).toHaveTextContent('workspace limit');
    expect(screen.getByRole('button', { name: /立即重建/ })).toBeEnabled();
  });

  it('keeps the legacy building branch when the capability row is ready', () => {
    renderBanner({ state: 'building', sources: [buildingSource], capabilityState: 'ready' });
    const button = screen.getByTestId('timeline-projection-banner').querySelector('button')!;
    expect(button).toBeDisabled();
  });
});
