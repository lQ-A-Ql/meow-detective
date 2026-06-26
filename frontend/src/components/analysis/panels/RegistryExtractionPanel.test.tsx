import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RegistryExtractionPanel } from './RegistryExtractionPanel';
import type { RegistryStructuredSummary } from '@/types/models';

describe('RegistryExtractionPanel', () => {
  it('renders with sub-tabs', () => {
    render(createElement(RegistryExtractionPanel, {}));
    expect(screen.getByText('注册表提取')).toBeDefined();
    expect(screen.getByText('用户账户')).toBeDefined();
    expect(screen.getByText('用户活动')).toBeDefined();
    expect(screen.getByText('网络配置')).toBeDefined();
    expect(screen.getByText('软件列表')).toBeDefined();
    expect(screen.getByText('USB 设备')).toBeDefined();
    expect(screen.getByText('原始键值')).toBeDefined();
  });

  it('renders empty state for users tab when no data', () => {
    render(createElement(RegistryExtractionPanel, {}));
    expect(screen.getByText('暂无用户账户数据')).toBeDefined();
  });

  it('switches tabs when tab buttons are clicked', () => {
    render(createElement(RegistryExtractionPanel, {}));
    fireEvent.click(screen.getByText('原始键值'));
    expect(screen.getByText('暂无原始键值数据')).toBeDefined();
  });

  it('renders SAM user data when provided', () => {
    const structured: RegistryStructuredSummary = {
      samUsers: [
        {
          username: 'Administrator',
          rid: 500,
          ridHex: '0x1f4',
          sid: 'S-1-5-21-12345-500',
          accountStatus: 'enabled',
          groups: ['Administrators'],
          loginCount: 10,
          lastLogin: '2026-06-01T10:00:00Z',
          profilePath: 'C:\\Users\\Admin',
          passwordHash: undefined,
          passwordHint: undefined,
          dataSourceId: 'ds-1',
          hivePath: 'C:\\Windows\\System32\\config\\SAM',
          keyPath: 'SAM\\Domains\\Account\\Users\\000001F4',
          parser: 'sam',
        },
      ],
      userAssistEntries: [],
      networkProfiles: [],
      installedSoftware: [],
      usbDevices: [],
      hiveOverviews: [],
      mountedDevices: [],
      systemServices: [],
      shutdownTimes: [],
      shimcacheEntries: [],
      runKeys: [],
      openSaveMru: [],
      lastVisitedMru: [],
      runMru: [],
      shellbagEntries: [],
      muicacheEntries: [],
      amcacheApplications: [],
      amcacheApplicationFiles: [],
      lsaPackages: [],
      appCompatLayers: [],
      securityPolicies: [],
      lsaSecrets: [],
      cachedCredentials: [],
      status: 'parsed',
      generatedAt: '2026-06-01T10:00:00Z',
      warnings: [],
    };
    render(createElement(RegistryExtractionPanel, { structured }));
    expect(screen.getByText('Administrator')).toBeDefined();
  });
});
