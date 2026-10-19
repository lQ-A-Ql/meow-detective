import { Cpu, MemoryStick, Monitor, CircuitBoard, Microchip } from 'lucide-react';

export const REGISTRY_HARDWARE_GROUPS = [
  { kind: 'cpu', icon: Cpu }, { kind: 'gpu', icon: Monitor }, { kind: 'ram', icon: MemoryStick },
  { kind: 'system', icon: Monitor }, { kind: 'baseboard', icon: CircuitBoard }, { kind: 'firmware', icon: Microchip },
] as const;

export const REGISTRY_HARDWARE_PROPERTIES = [
  'coreCount', 'enabledCoreCount', 'threadCount', 'socket', 'maxSpeedMhz', 'currentSpeedMhz',
  'capacityBytes', 'memoryType', 'formFactor', 'deviceLocator', 'bankLocator', 'speedMtS',
  'configuredSpeedMtS', 'partNumber', 'serialNumber', 'driverVersion', 'driverDate', 'chipType',
  'biosVersion', 'matchingDeviceId', 'version', 'releaseDate', 'sku', 'family', 'registryInstanceCount',
] as const;

export const HARDWARE_BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB'] as const;
