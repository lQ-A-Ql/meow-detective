import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EmulationWorkspace } from './EmulationWorkspace';
import type { EmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';

function createModel(overrides: Partial<EmulationWorkspaceModel> = {}): EmulationWorkspaceModel {
  return {
    caseLoaded: true,
    hasCase: true,
    caseName: '启动验证案件',
    loading: false,
    sourceOptions: [{
      id: 'source-1',
      name: '早起王的PC镜像',
      kind: 'E01',
      platform: 'WINDOWS',
      partitionCount: 4,
      evidenceSize: 90_619_311_073,
    }],
    selectedSourceId: 'source-1',
    selectedSource: {
      id: 'source-1',
      name: '早起王的PC镜像',
      kind: 'E01',
      platform: 'WINDOWS',
      partitionCount: 4,
      evidenceSize: 90_619_311_073,
    },
    selectSource: vi.fn(),
    preflight: undefined,
    preflightLoading: false,
    recoveryIsoPath: '',
    bootRoute: 'directSystem',
    pickRecoveryIso: vi.fn().mockResolvedValue(undefined),
    clearRecoveryIso: vi.fn(),
    options: { networkMode: 'off', clipboard: false, timeSync: false, processorCount: 2, memoryMib: 4096 },
    toggleOption: vi.fn(),
    selectNetworkMode: vi.fn(),
    setResourceValue: vi.fn(),
    osdataCleanupPartitions: [],
    cleanupOsdata: true,
    toggleCleanupOsdata: vi.fn(),
    needsEfiFallback: false,
    installEfiFallback: true,
    toggleInstallEfiFallback: vi.fn(),
    needsFsRepair: false,
    repairFilesystems: true,
    toggleRepairFilesystems: vi.fn(),
    bypassPartition: undefined,
    selectBypassPartition: vi.fn(),
    bypassIsLinux: false,
    bypassAccounts: [],
    bypassAccountsLoading: false,
    bypassRid: undefined,
    selectBypassRid: vi.fn(),
    bypassAction: 'clearPassword',
    selectBypassAction: vi.fn(),
    linuxAccounts: [],
    linuxAccountsLoading: false,
    linuxAccountsError: undefined,
    linuxUsername: undefined,
    selectLinuxUsername: vi.fn(),
    sessions: [],
    metrics: { sourceCount: 1, activeCount: 0, runningCount: 0, failedCount: 0 },
    canStart: true,
    starting: false,
    releasingSessionId: undefined,
    refreshing: false,
    error: undefined,
    start: vi.fn().mockResolvedValue(undefined),
    release: vi.fn().mockResolvedValue(undefined),
    refresh: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe('EmulationWorkspace', () => {
  it('renders direct-system boot and delegates launch to the workspace model', () => {
    const start = vi.fn().mockResolvedValue(undefined);
    render(<EmulationWorkspace model={createModel({ start })} />);

    expect(screen.getByText('镜像仿真')).toBeInTheDocument();
    expect(screen.getByText('原系统（需确认）')).toBeInTheDocument();
    expect(screen.getByText('COW 证据隔离')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '启动仿真' }));
    expect(start).toHaveBeenCalledOnce();
  });

  it('shows selected PE media as the first boot route and delegates clearing it', () => {
    const clearRecoveryIso = vi.fn();
    render(<EmulationWorkspace model={createModel({
      recoveryIsoPath: 'C:\\Users\\QAQ\\Tools\\iso\\LaoMaoTao.iso',
      bootRoute: 'recoveryMedia',
      clearRecoveryIso,
    })} />);

    expect(screen.getByDisplayValue('C:\\Users\\QAQ\\Tools\\iso\\LaoMaoTao.iso')).toBeInTheDocument();
    expect(screen.getByText('PE ISO 优先')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '清除 WinPE ISO' }));
    expect(clearRecoveryIso).toHaveBeenCalledOnce();
  });

  it('renders session state and releases the selected session by id', () => {    const release = vi.fn().mockResolvedValue(undefined);
    render(<EmulationWorkspace model={createModel({
      sessions: [{
        sessionId: 'emulation-session-1',
        dataSourceId: 'source-1',
        sourceName: '早起王的PC镜像',
        state: 'running',
        logicalLength: 214_748_364_800,
        controlMode: 'interactiveOnly',
        active: true,
        releasable: true,
      }],
      metrics: { sourceCount: 1, activeCount: 1, runningCount: 1, failedCount: 0 },
      canStart: false,
      release,
    })} />);

    expect(screen.getAllByText('运行中')).toHaveLength(2);
    expect(screen.getByText('200.00 GB')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '释放 早起王的PC镜像 的仿真会话' }));
    expect(release).toHaveBeenCalledWith('emulation-session-1');
  });

  it('offers OSDATA cleanup when preflight detects the entry and delegates the toggle', () => {    const toggleCleanupOsdata = vi.fn();
    render(<EmulationWorkspace model={createModel({
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 2,
          osdataPresent: true,
          samPresent: true,
          utilmanBypassAvailable: true,
        }],
        recommendedBootRoute: 'directSystem',
      },
      osdataCleanupPartitions: [2],
      cleanupOsdata: true,
      toggleCleanupOsdata,
    })} />);

    expect(screen.getByText('存在 OSDATA')).toBeInTheDocument();
    expect(screen.getAllByText('[P2]')).toHaveLength(2);
    fireEvent.click(screen.getByRole('checkbox', { name: '启动前清除 OSDATA（仅写入覆盖层）' }));
    expect(toggleCleanupOsdata).toHaveBeenCalledOnce();
  });

  it('replaces the OSDATA cleanup checkbox with a hint when OSDATA is not empty', () => {
    render(<EmulationWorkspace model={createModel({
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 2,
          osdataPresent: true,
          osdataEmpty: false,
          samPresent: true,
          utilmanBypassAvailable: true,
        }],
        recommendedBootRoute: 'recoveryMedia',
      },
      osdataCleanupPartitions: [2],
    })} />);

    expect(screen.getByText('OSDATA 非空，无法自动清除，请使用 PE 路线')).toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: '启动前清除 OSDATA（仅写入覆盖层）' })).not.toBeInTheDocument();
  });

  it('warns when PE media is selected but the maintenance tool is unavailable', () => {    render(<EmulationWorkspace model={createModel({
      recoveryIsoPath: 'C:\\Tools\\WinPE.iso',
      bootRoute: 'recoveryMedia',
      preflight: {
        dataSourceId: 'source-1',
        installs: [],
        recommendedBootRoute: 'recoveryMedia',
        maintenanceToolAvailable: false,
      },
    })} />);

    expect(screen.getByText('PE 中将没有维护工具，只能手动操作')).toBeInTheDocument();
  });

  it('delegates network mode selection and resource sizing via sliders', () => {
    const selectNetworkMode = vi.fn();
    const setResourceValue = vi.fn();
    render(<EmulationWorkspace model={createModel({
      options: { networkMode: 'nat', clipboard: false, timeSync: false, processorCount: 4, memoryMib: 8192 },
      selectNetworkMode,
      setResourceValue,
    })} />);

    expect(screen.getByText('CPU 核心数: 4')).toBeInTheDocument();
    expect(screen.getByText('内存 (MiB): 8192')).toBeInTheDocument();
    const cores = screen.getByRole('slider', { name: 'CPU 核心数' });
    expect(cores).toHaveAttribute('aria-valuenow', '4');
    fireEvent.keyDown(cores, { key: 'ArrowRight' });
    expect(setResourceValue).toHaveBeenCalledWith('processorCount', 5);
    const memory = screen.getByRole('slider', { name: '内存 (MiB)' });
    expect(memory).toHaveAttribute('aria-valuenow', '8192');
    fireEvent.keyDown(memory, { key: 'ArrowRight' });
    expect(setResourceValue).toHaveBeenCalledWith('memoryMib', 8704);
    expect(screen.getByRole('combobox', { name: '网络模式' })).toHaveTextContent('NAT');
  });

  it('renders linux installs with distro, boot risks and the host-side bypass hint', () => {    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
          osReleasePrettyName: 'CentOS Linux 7 (Core)',
          kernelPresent: false,
          fstabPresent: true,
          bootRiskNotes: ['no-kernel'],
        }],
        recommendedBootRoute: 'directSystem',
      },
    })} />);

    expect(screen.getByText('[P5]')).toBeInTheDocument();
    expect(screen.getByText('Linux')).toBeInTheDocument();
    expect(screen.getByText('CentOS Linux 7 (Core)')).toBeInTheDocument();
    expect(screen.getByText('无内核')).toBeInTheDocument();
    expect(screen.getByText(/系统绕密.*选择账户/)).toBeInTheDocument();
    expect(screen.queryByText('可绕密')).not.toBeInTheDocument();
  });

  it('shows the live ISO hint for linux sources only', () => {
    const { rerender } = render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
    })} />);
    expect(screen.getByText(/SystemRescue/)).toBeInTheDocument();

    rerender(<EmulationWorkspace model={createModel()} />);
    expect(screen.queryByText(/SystemRescue/)).not.toBeInTheDocument();
  });

  it('renders the Linux panel for linux sources without the Windows bypass selects', () => {
    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
        }],
        recommendedBootRoute: 'directSystem',
      },
      bypassPartition: 5,
      bypassIsLinux: true,
      linuxAccounts: [{ username: 'root', hasPassword: true, locked: false }],
    })} />);

    expect(screen.getByRole('combobox', { name: '目标分区' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: '目标账户' })).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: '绕密方式' })).not.toBeInTheDocument();
    expect(screen.queryByText('维护盘：自动生成')).not.toBeInTheDocument();
    expect(screen.queryByText(/密码为 123456/)).not.toBeInTheDocument();
  });

  it('shows the configured password for a selected Linux account', () => {
    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
        }],
        recommendedBootRoute: 'directSystem',
      },
      bypassPartition: 5,
      bypassIsLinux: true,
      linuxAccounts: [{ username: 'root', hasPassword: true, locked: false }],
      linuxUsername: 'root',
    })} />);

    expect(screen.getByText(/root 的密码为 123456/)).toBeInTheDocument();
    expect(screen.getByText(/SSH 或 root 登录策略仍可能限制密码登录/)).toBeInTheDocument();
  });

  it('renders the Windows panel with SAM bypass selects for windows sources', () => {
    render(<EmulationWorkspace model={createModel({
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 2,
          osdataPresent: true,
          samPresent: true,
          utilmanBypassAvailable: true,
        }],
        recommendedBootRoute: 'directSystem',
      },
      bypassPartition: 2,
      bypassAccounts: [{ rid: 500, username: 'Administrator', disabled: false, lockedOut: false, hasPassword: true }],
    })} />);

    expect(screen.getByRole('combobox', { name: '目标账户' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: '绕密方式' })).toBeInTheDocument();
    expect(screen.queryByText(/GRUB 菜单按 e/)).not.toBeInTheDocument();
  });

  it('shows the EFI fallback row only when preflight reports no-efi-fallback', () => {
    const toggleInstallEfiFallback = vi.fn();
    const linuxPreflight = (bootRiskNotes?: string[]) => createModel({
      selectedSource: {
        id: 'source-1',
        name: 'Kali 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 2,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 2,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
          bootRiskNotes,
        }],
        recommendedBootRoute: 'directSystem',
      },
      needsEfiFallback: (bootRiskNotes ?? []).includes('no-efi-fallback'),
      toggleInstallEfiFallback,
    });

    const { rerender } = render(<EmulationWorkspace model={linuxPreflight(['no-efi-fallback'])} />);
    expect(screen.getByText('无 EFI fallback 引导')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: '启动前安装 EFI fallback 引导（仅写入覆盖层）' }));
    expect(toggleInstallEfiFallback).toHaveBeenCalledOnce();

    rerender(<EmulationWorkspace model={linuxPreflight(undefined)} />);
    expect(screen.queryByText('无 EFI fallback 引导')).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: '启动前安装 EFI fallback 引导（仅写入覆盖层）' })).not.toBeInTheDocument();
  });

  it('uses the Linux bypass title without SAM wording', () => {
    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
        }],
        recommendedBootRoute: 'directSystem',
      },
    })} />);

    expect(screen.getByText('系统绕密：设置账户密码（仅写入覆盖层）')).toBeInTheDocument();
    expect(screen.queryByText(/SAM/)).not.toBeInTheDocument();
  });

  it('surfaces the linux account query error under the username selector', () => {
    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
        }],
        recommendedBootRoute: 'directSystem',
      },
      bypassPartition: 5,
      bypassIsLinux: true,
      linuxAccountsError: '该文件系统不支持离线绕密',
    })} />);

    expect(screen.getByText('该文件系统不支持离线绕密')).toBeInTheDocument();
    expect(screen.queryByText('未在该分区找到可设置密码的 Linux 账户')).not.toBeInTheDocument();
  });

  it('shows an explicit hint when the linux account list loads empty', () => {
    render(<EmulationWorkspace model={createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
        }],
        recommendedBootRoute: 'directSystem',
      },
      bypassPartition: 5,
      bypassIsLinux: true,
      linuxAccounts: [],
      linuxAccountsLoading: false,
    })} />);

    expect(screen.getByText('未在该分区找到可设置密码的 Linux 账户')).toBeInTheDocument();
  });

  it('offers host-side XFS repair only when preflight reports xfs-log-dirty', () => {
    const toggleRepairFilesystems = vi.fn();
    const linuxModel = (bootRiskNotes?: string[]) => createModel({
      selectedSource: {
        id: 'source-1',
        name: 'CentOS 镜像',
        kind: 'E01',
        platform: 'LINUX',
        partitionCount: 3,
      },
      preflight: {
        dataSourceId: 'source-1',
        installs: [{
          partitionIndex: 5,
          platform: 'linux',
          osdataPresent: false,
          samPresent: false,
          utilmanBypassAvailable: false,
          bootRiskNotes,
        }],
        recommendedBootRoute: 'directSystem',
      },
      needsFsRepair: (bootRiskNotes ?? []).includes('xfs-log-dirty'),
      toggleRepairFilesystems,
    });

    const { rerender } = render(<EmulationWorkspace model={linuxModel(['xfs-log-dirty'])} />);
    expect(screen.getByText('XFS 日志脏')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: '启动前回放并修复 XFS 日志（仅写入覆盖层）' }));
    expect(toggleRepairFilesystems).toHaveBeenCalledOnce();

    rerender(<EmulationWorkspace model={linuxModel(undefined)} />);
    expect(screen.queryByText('XFS 日志脏')).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: '启动前回放并修复 XFS 日志（仅写入覆盖层）' })).not.toBeInTheDocument();
  });
});
