import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { BrowserHistoryPanel } from './BrowserHistoryPanel';
import type { BrowserHistorySummary } from '@/types/models';

const emptySummary: BrowserHistorySummary = {
  status: 'parsed',
  visitTotal: 0,
  downloadTotal: 0,
  cookieTotal: 0,
  sessionTotal: 0,
  passwordTotal: 0,
  visits: [],
  downloads: [],
  cookies: [],
  sessions: [],
  passwords: [],
  generatedAt: '2026-06-01T10:00:00Z',
  warnings: [],
};

describe('BrowserHistoryPanel', () => {
  it('renders header with section title and status', () => {
    render(createElement(BrowserHistoryPanel, {}));
    expect(screen.getByText('浏览器记录')).toBeDefined();
    // warnings are rendered for the unavailable state
    expect(screen.getByText('浏览器记录暂不可用。')).toBeDefined();
  });

  it('renders visit and download sections', () => {
    render(createElement(BrowserHistoryPanel, {}));
    expect(screen.getAllByText('访问历史').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('下载记录').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('暂无浏览历史')).toBeDefined();
    expect(screen.getByText('暂无下载记录')).toBeDefined();
  });

  it('renders visit data when summary is provided', () => {
    const summary: BrowserHistorySummary = {
      ...emptySummary,
      visitTotal: 1,
      visits: [
        {
          artifactId: 'v1',
          fileId: 'f1',
          sourcePath: '/path',
          browser: 'Chrome',
          profile: 'Default',
          url: 'https://example.com',
          title: 'Example Site',
          visitTime: '2026-06-01T10:00:00Z',
          visitCount: 3,
        },
      ],
    };
    render(createElement(BrowserHistoryPanel, { summary }));
    expect(screen.getByText('Example Site')).toBeDefined();
    expect(screen.getByText('https://example.com')).toBeDefined();
  });

  it('renders cookie, session and password sections', () => {
    render(createElement(BrowserHistoryPanel, {}));
    expect(screen.getAllByText('Cookies').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('会话 / 标签页').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('保存的密码').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('暂无 Cookie 记录')).toBeDefined();
    expect(screen.getByText('暂无会话记录')).toBeDefined();
    expect(screen.getByText('暂无密码记录')).toBeDefined();
  });
});
