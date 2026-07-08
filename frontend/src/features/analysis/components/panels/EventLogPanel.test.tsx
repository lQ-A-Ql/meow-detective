import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
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
});
