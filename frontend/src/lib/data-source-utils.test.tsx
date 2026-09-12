import { describe, expect, expectTypeOf, it } from 'vitest';
import type { DataSourceSummary } from '@/types/models';
import {
  dataSourcePlatformLabel,
  inferDataSourcePlatform,
  sourceKindIcon,
  sourceKindIconLarge,
  sourceKindLabel,
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
    expectTypeOf<DataSourcePlatform>().toEqualTypeOf<'windows' | 'linux' | 'android'>();
    expectTypeOf<DataSourceSummary['platform']>().toEqualTypeOf<
      'windows' | 'linux' | 'android'
    >();
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

describe('Android platform presentation', () => {
  it('keeps Android as a distinct persisted platform', () => {
    const source = dataSource('android', {});
    expect(inferDataSourcePlatform(source)).toBe('android');
    expect(dataSourcePlatformLabel(source)).toBe('Android');
  });
});

describe('logical archive presentation', () => {
  it('uses a stable label and archive icons', () => {
    expect(sourceKindLabel('logical_archive')).toBe('归档');
    expect(sourceKindIcon('logical_archive')).toBe(sourceKindIconLarge('logical_archive'));
  });

  it('uses a stable local disk label and icon', () => {
    expect(sourceKindLabel('local_disk')).toBe('本地磁盘');
    expect(sourceKindIcon('local_disk')).toBe(sourceKindIconLarge('local_disk'));
  });
});
