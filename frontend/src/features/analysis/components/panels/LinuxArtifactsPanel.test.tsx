import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { LinuxArtifactsPanel } from './LinuxArtifactsPanel';
import type { LinuxArtifactSummary } from '@/types/models';

function baseSummary(overrides: Partial<LinuxArtifactSummary> = {}): LinuxArtifactSummary {
  return {
    status: 'parsed',
    journalCount: 0,
    textLogCount: 0,
    loginCount: 0,
    bashCommandCount: 0,
    aptEventCount: 0,
    cronJobCount: 0,
    sudoEventCount: 0,
    systemConfigCount: 0,
    webSiteCount: 0,
    webAccessLogCount: 0,
    webErrorLogCount: 0,
    webFindingCount: 0,
    mysqlConfigCount: 0,
    mysqlLogCount: 0,
    mysqlFindingCount: 0,
    totalCount: 0,
    truncated: false,
    coverageRatio: 1,
    journalEntries: [],
    loginRecords: [],
    bashCommands: [],
    aptEvents: [],
    cronJobs: [],
    sudoEvents: [],
    systemConfigs: [],
    webSites: [],
    webAccessLogs: [],
    webErrorLogs: [],
    webFindings: [],
    mysqlConfigs: [],
    mysqlLogs: [],
    mysqlFindings: [],
    warnings: [],
    generatedAt: '2026-07-02T10:00:00Z',
    ...overrides,
  };
}

describe('LinuxArtifactsPanel', () => {
  it('renders header with section title', () => {
    render(createElement(LinuxArtifactsPanel, {}));
    expect(screen.getByText('Linux 痕迹分析')).toBeDefined();
  });

  it('renders empty overview state when no data', () => {
    render(createElement(LinuxArtifactsPanel, {}));
    expect(screen.getByText('尚未从当前数据源发现或提取 Linux 痕迹。')).toBeDefined();
  });

  it('does not present the persisted coverage summary as live extraction progress', () => {
    const note = 'Structured output coverage is 414 of 749 Linux artifact candidate source(s).';
    const summary = baseSummary({ coverageNotes: [note] });
    const { container, rerender } = render(createElement(LinuxArtifactsPanel, {
      summary,
      extractionRunning: true,
    }));

    expect(screen.queryByText(note)).toBeNull();
    rerender(createElement(LinuxArtifactsPanel, { summary, extractionRunning: false }));
    expect(screen.getByText(note)).toBeDefined();
    // 覆盖摘要是 muted 信息条，而不是 WarningList 黄块。
    expect(container.querySelector('.bg-forensics-warning-bg')).toBeNull();
  });

  it('keeps real warnings on the warning list while coverage notes stay muted', () => {
    const summary = baseSummary({
      warnings: ['auth.log truncated mid-line'],
      coverageNotes: ['Structured output coverage is 414 of 749 Linux artifact candidate source(s).'],
    });
    const { container } = render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.getByText('auth.log truncated mid-line')).toBeDefined();
    expect(container.querySelector('.bg-forensics-warning-bg')).not.toBeNull();
    expect(
      screen.getByText('Structured output coverage is 414 of 749 Linux artifact candidate source(s).'),
    ).toBeDefined();
  });

  it('renders journal content when activeTab is journal', () => {
    render(createElement(LinuxArtifactsPanel, { activeTab: 'journal' }));
    expect(screen.getByText('暂无 systemd 日志')).toBeDefined();
  });

  it('renders sudo content when activeTab is sudo', () => {
    render(createElement(LinuxArtifactsPanel, { activeTab: 'sudo' }));
    expect(screen.getByText('暂无 sudo/提权事件')).toBeDefined();
  });

  it('keeps overview counts out of the work area', () => {
    const summary = baseSummary({
      journalCount: 3,
      loginCount: 2,
      bashCommandCount: 5,
      aptEventCount: 1,
      cronJobCount: 4,
      sudoEventCount: 6,
      systemConfigCount: 7,
      totalCount: 28,
    });
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.queryByText('28')).toBeNull();
  });

  it('renders bash commands with monospace command text', () => {
    const summary = baseSummary({
      bashCommandCount: 1,
      totalCount: 1,
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
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'commands' }));
    expect(screen.getByText('ls -la /home')).toBeDefined();
  });

  it('renders sudo events with success/failure indicators', () => {
    const summary = baseSummary({
      sudoEventCount: 2,
      totalCount: 2,
      truncated: true,
      coverageRatio: 0.5,
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
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'sudo' }));
    expect(screen.getByText('apt update')).toBeDefined();
    expect(screen.getByText('rm -rf /')).toBeDefined();
  });

  it('renders Linux system config records', () => {
    const summary = baseSummary({
      systemConfigCount: 1,
      totalCount: 1,
      systemConfigs: [
        {
          artifactId: 'config-1',
          fileId: 'file-1',
          sourcePath: '/etc/passwd',
          configKind: 'passwdAccount',
          line: '',
          lineNumber: 0,
          username: 'root',
          uid: 0,
          gid: 0,
          home: '/root',
          shell: '/bin/bash',
        },
      ],
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'systemConfig' }));
    expect(screen.getByText('root')).toBeDefined();
    expect(screen.getByText('/bin/bash')).toBeDefined();
  });

  it('renders Linux MySQL service configs, logs, and findings', () => {
    const summary = baseSummary({
      mysqlConfigCount: 1,
      mysqlLogCount: 1,
      mysqlFindingCount: 1,
      totalCount: 3,
      mysqlConfigs: [
        {
          artifactId: 'mysql-config-1',
          fileId: 'file-mycnf',
          sourcePath: '/etc/mysql/my.cnf',
          section: 'mysqld',
          key: 'bind-address',
          value: '0.0.0.0',
          lineNumber: 2,
        },
      ],
      mysqlLogs: [
        {
          artifactId: 'mysql-log-1',
          fileId: 'file-mysql-log',
          sourcePath: '/var/log/mysql/error.log',
          timestamp: '2026-07-02T10:00:00Z',
          severity: 'warning',
          threadId: '8',
          message: "Access denied for user 'root'@'192.0.2.10'",
          lineNumber: 1,
        },
      ],
      mysqlFindings: [
        {
          artifactId: 'mysql-finding-1',
          fileId: 'file-mycnf',
          sourcePath: '/etc/mysql/my.cnf',
          findingKind: 'bindAddressAny',
          severity: 'medium',
          confidence: 0.86,
          evidence: 'bind-address=0.0.0.0',
          lineNumber: 2,
        },
      ],
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'mysqlServices' }));
    expect(screen.getByText('bindAddressAny')).toBeDefined();
    expect(screen.getByText('bind-address')).toBeDefined();
    expect(screen.getByText("Access denied for user 'root'@'192.0.2.10'")).toBeDefined();
  });

  it('renders Linux web service findings and site records', () => {
    const summary = baseSummary({
      webSiteCount: 1,
      webAccessLogCount: 1,
      webFindingCount: 1,
      totalCount: 3,
      webSites: [
        {
          artifactId: 'site-1',
          fileId: 'file-nginx',
          sourcePath: '/etc/nginx/nginx.conf',
          serverKind: 'nginx',
          siteName: 'nginx server line 2',
          hostnames: ['example.test'],
          listen: ['80'],
          documentRoots: ['/var/www/html'],
          accessLogs: ['/var/log/nginx/access.log'],
          errorLogs: ['/var/log/nginx/error.log'],
          lineNumber: 2,
        },
      ],
      webAccessLogs: [
        {
          artifactId: 'access-1',
          fileId: 'file-access',
          sourcePath: '/var/log/nginx/access.log',
          clientIp: '192.0.2.10',
          method: 'GET',
          uri: '/products?id=1 UNION SELECT password',
          protocol: 'HTTP/1.1',
          status: 200,
          responseBytes: 4532,
          userAgent: 'sqlmap/1.7',
          lineNumber: 1,
        },
      ],
      webFindings: [
        {
          artifactId: 'finding-1',
          fileId: 'file-access',
          sourcePath: '/var/log/nginx/access.log',
          findingKind: 'sqlInjection',
          severity: 'high',
          confidence: 0.9,
          evidence: '/products?id=1 UNION SELECT password',
          clientIp: '192.0.2.10',
          uri: '/products?id=1 UNION SELECT password',
          lineNumber: 1,
        },
      ],
    });

    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'webServices' }));

    expect(screen.getByText('sqlInjection')).toBeDefined();
    expect(screen.getByText('example.test')).toBeDefined();
    expect(screen.getByText('sqlmap/1.7')).toBeDefined();
  });
  it('shows loaded/total progress and truncation warning for partially loaded families', () => {
    const summary = baseSummary({
      sudoEventCount: 3,
      totalCount: 3,
      truncated: true,
      sudoEvents: [
        {
          artifactId: 'sudo-1',
          fileId: 'file-1',
          sourcePath: '/var/log/auth.log',
          user: 'alice',
          command: 'apt update',
          success: true,
        },
      ],
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'sudo' }));
    expect(screen.getByText('已加载 1 / 共 3')).toBeDefined();
    expect(screen.getByText('结果已被截断，仅显示部分内容。')).toBeDefined();
  });

  it('renders the journal log kind column distinguishing journald and text fallback rows', () => {
    const summary = baseSummary({
      journalCount: 1,
      textLogCount: 1,
      totalCount: 2,
      journalEntries: [
        {
          artifactId: 'journal-1',
          fileId: 'file-journal',
          sourcePath: '/var/log/journal/system.journal',
          message: 'Started ssh.service',
          logKind: 'journald',
        },
        {
          artifactId: 'journal-2',
          fileId: 'file-syslog',
          sourcePath: '/var/log/syslog',
          message: 'cron job ran',
          logKind: 'syslog',
        },
      ],
    });
    render(createElement(LinuxArtifactsPanel, { summary, activeTab: 'journal' }));
    // 列筛选下拉会重复渲染列标题与取值，断言至少出现一次即可。
    expect(screen.getAllByText('日志来源').length).toBeGreaterThan(0);
    expect(screen.getAllByText('journald').length).toBeGreaterThan(0);
    expect(screen.getAllByText('syslog').length).toBeGreaterThan(0);
  });

  it('renders the system info overview when systemInfo is present', () => {
    const summary = baseSummary({
      systemConfigCount: 5,
      totalCount: 5,
      systemInfo: {
        osPrettyName: 'CentOS Stream 9',
        osId: 'centos',
        osVersionId: '9',
        hostname: 'forensic-host',
        accountCount: 3,
        userAccountCount: 2,
        lockedAccountCount: 1,
      },
    });
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.getByText('操作系统')).toBeDefined();
    expect(screen.getByText('CentOS Stream 9')).toBeDefined();
    expect(screen.getByText('主机名')).toBeDefined();
    expect(screen.getByText('forensic-host')).toBeDefined();
    expect(screen.getByText('锁定账户')).toBeDefined();
  });

  it('falls back to a hint when artifacts exist without system info', () => {
    const summary = baseSummary({ journalCount: 2, totalCount: 2 });
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(
      screen.getByText('已发现 Linux 痕迹，但未解析到系统基本信息（os-release/主机名/账户）。'),
    ).toBeDefined();
  });

  it('renders the kernel versions card and the account table on overview', () => {
    const summary = baseSummary({
      systemConfigCount: 2,
      totalCount: 2,
      systemInfo: {
        osPrettyName: 'Ubuntu 22.04 LTS',
        hostname: 'forensic-host',
        accountCount: 2,
        userAccountCount: 1,
        lockedAccountCount: 1,
        kernelVersions: ['5.15.0-91-generic', '6.5.0-14-generic'],
        accounts: [
          {
            username: 'root',
            uid: 0,
            gid: 0,
            home: '/root',
            shell: '/bin/bash',
            locked: false,
            hasPassword: true,
          },
          {
            username: 'alice',
            uid: 1000,
            gid: 1000,
            home: '/home/alice',
            shell: '/bin/zsh',
            locked: true,
            hasPassword: false,
          },
        ],
      },
    });
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.getByText('内核版本')).toBeDefined();
    expect(screen.getByText('5.15.0-91-generic、6.5.0-14-generic')).toBeDefined();
    expect(screen.getByText('用户账户')).toBeDefined();
    expect(screen.getByText('alice')).toBeDefined();
    expect(screen.getByText('/home/alice')).toBeDefined();
    expect(screen.getAllByText('/bin/bash').length).toBeGreaterThan(0);
    expect(screen.getAllByText('有密码').length).toBeGreaterThan(0);
  });

  it('shows 未解析 for an empty kernel version list and hides the account table without accounts', () => {
    const summary = baseSummary({
      systemConfigCount: 1,
      totalCount: 1,
      systemInfo: {
        osPrettyName: 'Debian GNU/Linux 12',
        hostname: 'forensic-host',
        accountCount: 0,
        userAccountCount: 0,
        lockedAccountCount: 0,
        kernelVersions: [],
        accounts: [],
      },
    });
    render(createElement(LinuxArtifactsPanel, { summary }));
    expect(screen.getByText('内核版本')).toBeDefined();
    expect(screen.getAllByText('未解析').length).toBeGreaterThan(0);
    expect(screen.queryByText('用户账户')).toBeNull();
  });
});
