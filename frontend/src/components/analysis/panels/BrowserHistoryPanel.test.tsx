import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { BrowserHistoryPanel } from './BrowserHistoryPanel';
import type { BrowserHistorySummary } from '@/types/models';

describe('BrowserHistoryPanel', () => {
  it('renders header with section title and status', () => {
    render(createElement(BrowserHistoryPanel, {}));
    expect(screen.getByText('浏览器记录')).toBeDefined();
    // warnings are rendered for the unavailable state
    expect(screen.getByText('浏览器记录暂不可用。')).toBeDefined();
  });

  it('renders tab buttons for all browser data categories', () => {
    render(createElement(BrowserHistoryPanel, {}));
    // Tab labels appear in both the tab bar and the TableBlock title area;
    // use getAllByText to disambiguate
    expect(screen.getAllByText('访问历史').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('下载记录').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Cookies').length).toBeGreaterThanOrEqual(1);
  });

  it('switches to downloads tab when clicked', () => {
    render(createElement(BrowserHistoryPanel, {}));
    // "下载记录" appears in both summary stats and tab buttons;
    // find the actual button element among the matches
    const allMatches = screen.getAllByText('下载记录');
    const tabBtn = allMatches.find((el) => el.tagName === 'BUTTON');
    expect(tabBtn).toBeDefined();
    fireEvent.click(tabBtn!);
    expect(screen.getByText('暂无下载记录')).toBeDefined();
  });

  it('renders visit data when summary is provided', () => {
    const summary: BrowserHistorySummary = {
      status: 'parsed',
      visitTotal: 1,
      downloadTotal: 0,
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
      downloads: [],
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
    };
    render(createElement(BrowserHistoryPanel, { summary }));
    expect(screen.getByText('Example Site')).toBeDefined();
    expect(screen.getByText('https://example.com')).toBeDefined();
  });
});
