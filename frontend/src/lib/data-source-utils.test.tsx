import { describe, expect, expectTypeOf, it } from 'vitest';
import type { DataSourceSummary } from '@/types/models';
import {
  dataSourcePlatformLabel,
  inferDataSourcePlatform,
  type DataSourcePlatform,
} from './data-source-utils';

function dataSource(
  platform: DataSourceSummary['platform'],
  overrides: Partial<DataSourceSummary>,
): DataSourceSummary {
  return {
    id: `source-${platform}`,
    name: `${platform} source`,
    kind: 'e01',
    sourcePath: `D:/evidence/${platform}.E01`,
    importedAt: '2026-07-11T00:00:00Z',
    platform,
    ...overrides,
  };
}

describe('data source platform selection', () => {
  it('exposes only backend-supported persisted platform values', () => {
    expectTypeOf<DataSourcePlatform>().toEqualTypeOf<'windows' | 'linux'>();
    expectTypeOf<DataSourceSummary['platform']>().toEqualTypeOf<'windows' | 'linux'>();
  });

  it('keeps persisted Windows platform despite Linux-looking metadata', () => {
    const source = dataSource('windows', {
      name: 'ubuntu-pve',
      sourcePath: '/home/ubuntu/server.raw',
      partitions: [{
        index: 1,
        name: 'Linux LVM',
        kindLabel: 'LVM',
        status: 'supported',
        offset: 0,
        length: 1024,
        filesystem: 'xfs',
      }],
    });

    expect(inferDataSourcePlatform(source)).toBe('windows');
    expect(dataSourcePlatformLabel(source)).toBe('Windows');
  });

  it('keeps persisted Linux platform despite Windows-looking metadata', () => {
    const source = dataSource('linux', {
      name: 'Windows 11 disk',
      sourcePath: 'C:/evidence/windows.E01',
      partitions: [{
        index: 1,
        name: 'Windows data',
        kindLabel: 'Basic data',
        status: 'supported',
        offset: 0,
        length: 1024,
        filesystem: 'NTFS',
      }],
    });

    expect(inferDataSourcePlatform(source)).toBe('linux');
    expect(dataSourcePlatformLabel(source)).toBe('Linux');
  });
});
