import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { LinuxArtifactsPanel } from './LinuxArtifactsPanel';
import type { LinuxArtifactSummary } from '@/types/models';

describe('LinuxArtifactsPanel', () => {
  it('renders header with section title', () => {
    render(createElement(LinuxArtifactsPanel, {}));
    expect(screen.getByText('Linux 痕迹分析')).toBeDefined();
  });

  it('renders empty overview state when no data', () => {
    render(createElement(LinuxArtifactsPanel, {}));
    expect(screen.getByText('尚未从当前数据源发现或提取 Linux 痕迹。')).toBeDefined();
  });

  it('renders journal content when activeTab is journal', () => {
    render(createElement(LinuxArtifactsPanel, { activeTab: 'journal' }));
    expect(screen.getByText('暂无 systemd 日志')).toBeDefined();
  });

  it('renders sudo content when activeTab is sudo', () => {
    render(createElement(LinuxArtifactsPanel, { activeTab: 'sudo' }));
    expect(screen.getByText('暂无 sudo/提权事件')).toBeDefined();
  });

  it('renders overview stat cards when summary is provided', () => {
    const summary: LinuxArtifactSummary = {
      status: 'parsed',
      journalCount: 3,
      loginCount: 2,
      bashCommandCount: 5,
      aptEventCount: 1,
      cronJobCount: 4,
      sudoEventCount: 6,
      totalCount: 21,
      truncated: false,
      coverageRatio: 1,
      journalEntries: [],
      loginRecords: [],
      bashCommands: [],
      aptEvents: [],
      cronJobs: [],
      sudoEvents: [],
      warnings: [],
      generatedAt: '2026-07-02T10:00:00Z',
    };
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.getByText('21')).toBeDefined();
  });

  it('renders bash commands with monospace command text', () => {
    const summary: LinuxArtifactSummary = {
      status: 'parsed',
      journalCount: 0,
      loginCount: 0,
      bashCommandCount: 1,
      aptEventCount: 0,
      cronJobCount: 0,
      sudoEventCount: 0,
      totalCount: 1,
      truncated: false,
      coverageRatio: 1,
      journalEntries: [],
      loginRecords: [],
      bashCommands: [
        {
          artifactId: 'bash-1',
          fileId: 'file-1',
          sourcePath: 'home/alice/.bash_history',
          command: 'ls -la /home',
          lineNumber: 1,
          timestamp: '2026-07-02T10:00:00Z',
        },
      ],
      aptEvents: [],
      cronJobs: [],
      sudoEvents: [],
      warnings: [],
      generatedAt: '2026-07-02T10:00:00Z',
    };
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'commands' }));
    expect(screen.getByText('ls -la /home')).toBeDefined();
  });

  it('renders sudo events with success/failure indicators', () => {
    const summary: LinuxArtifactSummary = {
      status: 'parsed',
      journalCount: 0,
      loginCount: 0,
      bashCommandCount: 0,
      aptEventCount: 0,
      cronJobCount: 0,
      sudoEventCount: 2,
      totalCount: 2,
      truncated: true,
      coverageRatio: 0.5,
      journalEntries: [],
      loginRecords: [],
      bashCommands: [],
      aptEvents: [],
      cronJobs: [],
      sudoEvents: [
        {
          artifactId: 'sudo-1',
          fileId: 'file-1',
          sourcePath: '/var/log/auth.log',
          user: 'alice',
          targetUser: 'root',
          command: 'apt update',
          success: true,
          timestamp: '2026-07-02T10:00:00Z',
        },
        {
          artifactId: 'sudo-2',
          fileId: 'file-1',
          sourcePath: '/var/log/auth.log',
          user: 'bob',
          command: 'rm -rf /',
          success: false,
          timestamp: '2026-07-02T10:01:00Z',
        },
      ],
      warnings: [],
      generatedAt: '2026-07-02T10:00:00Z',
    };
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'sudo' }));
    expect(screen.getByText('apt update')).toBeDefined();
    expect(screen.getByText('rm -rf /')).toBeDefined();
  });
});
