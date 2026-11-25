import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RegistryHardwareContainer } from '@/features/registry-hardware/containers/RegistryHardwareContainer';
import { formatHardwareBytes } from '@/features/registry-hardware/hardware-model';
import type { RegistryHardwareDevice } from '@/types/models';

const keyPath = 'ControlSet001\\Services\\mssmbios\\Data';
const records: RegistryHardwareDevice[] = [
  {
    kind: 'cpu',
    deviceId: 'cpu-1',
    name: 'Fixture CPU',
    manufacturer: 'CPU Vendor',
    properties: { coreCount: '8', threadCount: '16', maxSpeedMhz: '4200' },
    keyPath,
    valueName: 'SMBiosData',
    sourceOffset: 8,
  },
  {
    kind: 'gpu',
    deviceId: 'gpu-1',
    name: 'Fixture GPU',
    properties: { capacityBytes: '12884901888' },
    keyPath: 'ControlSet001\\Control\\Class\\display\\0000',
  },
  {
    kind: 'ram',
    deviceId: 'ram-1',
    name: 'DIMM_A1',
    properties: { capacityBytes: '17179869184', memoryType: 'DDR5', configuredSpeedMtS: '6000' },
    keyPath,
  },
  { kind: 'ram', deviceId: 'ram-2', name: 'DIMM_A2', properties: {}, keyPath },
];

function preview(devices: RegistryHardwareDevice[], warnings: string[] = []) {
  return render(createElement(RegistryHardwareContainer, { devices, warnings }));
}

describe('registry system hardware summary', () => {
  it('renders published CPU, GPU and RAM records without a registry session', () => {
    preview(records);
    expect(screen.getByText('Fixture CPU')).toBeInTheDocument();
    expect(screen.getByText('Fixture GPU')).toBeInTheDocument();
    expect(screen.getByText('已识别内存模块合计：16 GiB')).toBeInTheDocument();
    expect(screen.getByText('另有 1 条内存记录的容量未知，未计入合计。')).toBeInTheDocument();
    expect(screen.getByText('6000 MT/s')).toBeInTheDocument();
    expect(screen.getByText('12 GiB')).toBeInTheDocument();
    expect(screen.getByText('核心数')).toBeInTheDocument();
    expect(screen.getByText(`${keyPath}\\SMBiosData`)).toBeInTheDocument();
  });

  it('renders the empty state from the extraction summary', () => {
    preview([]);
    expect(screen.getByText('配置单元可能未保存该信息，或对应缓存不可读取。')).toBeInTheDocument();
  });

  it('reports extraction warnings with available records', () => {
    preview([records[0]], ['[REG-SYSTEM] SYSTEM: SMBiosData unavailable']);
    expect(screen.getByText('部分注册表缓存无法解析或已达到读取上限；以下仅展示可确认的记录。')).toBeInTheDocument();
    expect(screen.getByText('Fixture CPU')).toBeInTheDocument();
  });

  it('formats unsigned 64-bit capacities without a lossy Number conversion', () => {
    expect(formatHardwareBytes('18446744073709551615')).toBe('16 EiB');
    expect(formatHardwareBytes('17179869184')).toBe('16 GiB');
    expect(formatHardwareBytes(undefined)).toBeUndefined();
    expect(formatHardwareBytes('-1')).toBeUndefined();
  });
});
