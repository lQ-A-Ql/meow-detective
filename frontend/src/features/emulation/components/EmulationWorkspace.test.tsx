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
    bypassPartition: undefined,
    selectBypassPartition: vi.fn(),
    bypassAccounts: [],
    bypassAccountsLoading: false,
    bypassRid: undefined,
    selectBypassRid: vi.fn(),
    bypassAction: 'clearPassword',
    selectBypassAction: vi.fn(),
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

  it('warns when PE media is selected but the maintenance tool is unavailable', () => {
    render(<EmulationWorkspace model={createModel({
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

  it('delegates network mode selection and resource sizing', () => {
    const selectNetworkMode = vi.fn();
    const setResourceValue = vi.fn();
    render(<EmulationWorkspace model={createModel({
      options: { networkMode: 'nat', clipboard: false, timeSync: false, processorCount: 4, memoryMib: 8192 },
      selectNetworkMode,
      setResourceValue,
    })} />);

    expect(screen.getByDisplayValue('4')).toBeInTheDocument();
    expect(screen.getByDisplayValue('8192')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('CPU 核心数'), { target: { value: '8' } });
    expect(setResourceValue).toHaveBeenCalledWith('processorCount', 8);
    fireEvent.change(screen.getByLabelText('内存 (MiB)'), { target: { value: '16384' } });
    expect(setResourceValue).toHaveBeenCalledWith('memoryMib', 16384);
    expect(screen.getByRole('combobox', { name: '网络模式' })).toHaveTextContent('NAT');
  });
});
