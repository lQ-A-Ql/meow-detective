import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EventLogPanel } from './EventLogPanel';
import type { EvtxEventSummary } from '@/types/models';

describe('EventLogPanel', () => {
  it('renders header with section title', () => {
    render(createElement(EventLogPanel, {}));
    expect(screen.getByText('事件日志分析')).toBeDefined();
    // "开关机" appears in both the tab button and summary stats
    expect(screen.getAllByText('开关机').length).toBeGreaterThanOrEqual(1);
  });

  it('renders empty state for boot tab when no data', () => {
    render(createElement(EventLogPanel, {}));
    expect(screen.getByText('暂无开关机事件')).toBeDefined();
  });

  it('switches to application tab when clicked', () => {
    render(createElement(EventLogPanel, {}));
    fireEvent.click(screen.getByRole('tab', { name: '应用程序事件' }));
    expect(screen.getByText('暂无应用程序事件')).toBeDefined();
  });

  it('renders boot events when summary is provided', () => {
    const summary: EvtxEventSummary = {
      status: 'parsed',
      pageTotal: 1,
      bootShutdownCount: 1,
      logonLogoffCount: 0,
      privilegeEscalationCount: 0,
      processExecutionCount: 0,
      accountManagementCount: 0,
      scheduledTaskCount: 0,
      applicationCrashCount: 0,
      softwareInstallationCount: 0,
      otherCount: 0,
      totalCount: 1,
      bootEvents: [
        { eventId: 6005, kind: 'eventLogStarted', timestamp: '2026-06-01T08:00:00Z', provider: 'EventLog', recordId: 1, sourcePath: 'System.evtx', note: 'System started' },
      ],
      securityEvents: [],
      applicationEvents: [],
      warnings: [],
      generatedAt: '2026-06-01T10:00:00Z',
    };
    render(createElement(EventLogPanel, { summary }));
    expect(screen.getByText('6005')).toBeDefined();
  });

  it('labels operating-system lifecycle boundaries distinctly', () => {
    const summary: EvtxEventSummary = {
      status: 'parsed', bootShutdownCount: 2, logonLogoffCount: 0,
      pageTotal: 2,
      privilegeEscalationCount: 0, processExecutionCount: 0, accountManagementCount: 0,
      scheduledTaskCount: 0, applicationCrashCount: 0, softwareInstallationCount: 0,
      otherCount: 0, totalCount: 2, securityEvents: [], applicationEvents: [], warnings: [],
      generatedAt: '2026-06-01T10:00:00Z',
      bootEvents: [
        { eventId: 12, kind: 'operatingSystemStarted', timestamp: '2026-06-01T08:00:00Z', provider: 'Microsoft-Windows-Kernel-General', recordId: 1, sourcePath: 'System.evtx', note: 'startup' },
        { eventId: 13, kind: 'operatingSystemShutdown', timestamp: '2026-06-01T18:00:00Z', provider: 'Microsoft-Windows-Kernel-General', recordId: 2, sourcePath: 'System.evtx', note: 'shutdown' },
      ],
    };
    render(createElement(EventLogPanel, { summary }));
    expect(screen.getByText('操作系统启动完成')).toBeDefined();
    expect(screen.getByText('操作系统进入关闭阶段')).toBeDefined();
  });

  it('requests the selected server-side view when changing tabs', () => {
    const onActiveViewChange = vi.fn();
    render(createElement(EventLogPanel, { onActiveViewChange }));

    fireEvent.click(screen.getByRole('tab', { name: '进程创建' }));

    expect(onActiveViewChange).toHaveBeenCalledWith('process');
  });

  it('owns vertical scrolling inside the active event table viewport', () => {
    const { container } = render(createElement(EventLogPanel, {}));
    const activeContent = container.querySelector('[data-slot="tabs-content"][data-state="active"]');

    expect(activeContent?.className).toContain('flex-col');
    expect(activeContent?.className).toContain('overflow-hidden');
    expect(activeContent?.querySelector('.overflow-y-auto')).not.toBeNull();
  });

  it('routes a failed continuation through query recovery instead of the stale cursor', () => {
    const onLoadMore = vi.fn();
    const onRetryLoadMore = vi.fn();
    const { container, rerender } = render(createElement(EventLogPanel, {
      hasMore: true,
      onLoadMore,
    }));
    const activeContent = container.querySelector(
      '[data-slot="tabs-content"][data-state="active"]',
    );
    const scrollContainer = activeContent?.querySelector('.overflow-y-auto');
    expect(scrollContainer).toBeInstanceOf(HTMLDivElement);
    Object.defineProperties(scrollContainer as HTMLDivElement, {
      clientHeight: { configurable: true, value: 600 },
      scrollHeight: { configurable: true, value: 3_100 },
    });

    fireEvent.scroll(scrollContainer as HTMLDivElement, {
      target: { scrollTop: 2_500 },
    });
    expect(onLoadMore).toHaveBeenCalledTimes(1);

    rerender(createElement(EventLogPanel, {
      hasMore: true,
      loadMoreFailed: true,
      onLoadMore,
      onRetryLoadMore,
    }));
    fireEvent.scroll(scrollContainer as HTMLDivElement, {
      target: { scrollTop: 2_500 },
    });

    expect(onLoadMore).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onRetryLoadMore).toHaveBeenCalledTimes(1);
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });
});
