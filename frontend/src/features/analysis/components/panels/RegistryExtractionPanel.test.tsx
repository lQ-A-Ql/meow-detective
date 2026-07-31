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
    const { container } = render(createElement(RegistryExtractionPanel, {}));
    expect(screen.getByText('暂无用户账户数据')).toBeDefined();

    const activeContent = container.querySelector(
      '[data-slot="tabs-content"][data-state="active"]',
    );
    expect(activeContent?.className).toContain('flex-col');
    expect(activeContent?.className).toContain('overflow-hidden');
  });

  it('switches tabs when tab buttons are clicked', () => {
    render(createElement(RegistryExtractionPanel, {}));
    fireEvent.mouseDown(screen.getByRole('tab', { name: '原始键值' }), { button: 0 });
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
      networkAdapters: [],
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

  it('renders physical adapters separately with row-sized table viewports', () => {
    const { container } = render(createElement(RegistryExtractionPanel, {
      structured: {
        hiveOverviews: [], samUsers: [], userAssistEntries: [], installedSoftware: [],
        usbDevices: [], mountedDevices: [], systemServices: [], shutdownTimes: [],
        shimcacheEntries: [], runKeys: [], openSaveMru: [], lastVisitedMru: [], runMru: [],
        shellbagEntries: [], muicacheEntries: [], amcacheApplications: [],
        amcacheApplicationFiles: [], lsaPackages: [], appCompatLayers: [], securityPolicies: [],
        lsaSecrets: [], cachedCredentials: [], status: 'parsed', generatedAt: '', warnings: [],
        networkAdapters: [{
          guid: '{ADAPTER-GUID}', name: 'Ethernet', description: 'Intel Ethernet Controller',
          macAddress: '00:11:22:33:44:55', ipAddresses: ['192.0.2.10'],
          permanentMacAddress: '00:11:22:33:44:56',
          subnetMasks: ['255.255.255.0'], gateways: ['192.0.2.1'], dhcpEnabled: true,
          dhcpServer: '192.0.2.2', dnsServers: ['192.0.2.53'],
        }],
        networkProfiles: [{
          profileGuid: '{PROFILE-GUID}', profileName: 'Office Network', managed: false,
          sourceKeyPath: 'NetworkList\\Profiles\\{PROFILE-GUID}',
        }],
      },
    }));
    fireEvent.mouseDown(screen.getByRole('tab', { name: '网络配置' }), { button: 0 });
    expect(screen.getByText('网络适配器（物理与虚拟）')).toBeDefined();
    expect(screen.getByText('Intel Ethernet Controller')).toBeDefined();
    expect(screen.getByText('网络配置文件与连接历史')).toBeDefined();
    expect(screen.getByText('Office Network')).toBeDefined();
    expect(container.innerHTML).not.toContain('min-h-[220px]');
    const tableFrames = Array.from(container.querySelectorAll<HTMLElement>('[style*="height: min"]'));
    expect(tableFrames.filter((frame) => frame.style.height.includes('61px'))).toHaveLength(2);
  });
});
