import { createElement } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  usePluginFamilyEntries: vi.fn(),
}));

vi.mock('@/features/analysis/hooks', () => ({
  usePluginFamilyEntries: mocks.usePluginFamilyEntries,
}));

vi.mock('@/features/analysis/plugin-action-hooks', () => ({
  usePluginActions: () => ({ data: [], isLoading: false }),
}));

import { PluginModulePanel, deriveDynamicAttrKeys } from './PluginModulePanel';
import type { PluginArtifactEntry, PluginModule } from '@/types/models';

function entry(artifactId: string, attrs: Record<string, unknown> = {}): PluginArtifactEntry {
  return {
    artifactId,
    fileId: 'file-1',
    sourcePath: 'C:/Windows/Prefetch/APP.EXE-1234.pf',
    title: `title-${artifactId}`,
    summary: `summary-${artifactId}`,
    confidence: 0.9,
    attrs,
    createdAt: '2026-08-01T00:00:00Z',
  };
}

function moduleFixture(overrides: Partial<PluginModule> = {}): PluginModule {
  return {
    pluginId: 'plugin-prefetch',
    displayName: 'Prefetch 插件',
    pluginVersion: '1.2.3',
    evidencePlatform: 'windows',
    families: [{ family: 'ProgramExecution', count: 2 }],
    totalCount: 2,
    warnings: [],
    ...overrides,
  };
}

function queryState(
  entries: PluginArtifactEntry[],
  overrides: Record<string, unknown> = {},
) {
  return {
    data: {
      pluginId: 'plugin-prefetch',
      family: 'ProgramExecution',
      totalCount: entries.length,
      truncated: false,
      entries,
    },
    isLoading: false,
    isError: false,
    hasNextPage: false,
    isFetchingNextPage: false,
    isFetchNextPageError: false,
    dataUpdatedAt: 1,
    fetchNextPage: vi.fn(),
    refetch: vi.fn(),
    ...overrides,
  };
}

describe('deriveDynamicAttrKeys', () => {
  it('ranks keys by row coverage and caps at six columns', () => {
    const rows = [
      entry('a1', { k1: 1, k2: 1, k3: 1, k4: 1, k5: 1, k6: 1, k7: 1, k8: 1 }),
      entry('a2', { k1: 1, k2: 1, k3: 1 }),
      entry('a3', { k1: 1, k2: 1 }),
    ];

    expect(deriveDynamicAttrKeys(rows)).toEqual(['k1', 'k2', 'k3', 'k4', 'k5', 'k6']);
  });

  it('returns an empty list when no attrs exist', () => {
    expect(deriveDynamicAttrKeys([entry('a1'), entry('a2')])).toEqual([]);
  });
});

describe('PluginModulePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.usePluginFamilyEntries.mockReturnValue(queryState([]));
  });

  it('renders header with display name, version and warnings', () => {
    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture({ warnings: ['family 白名单拒绝: Foo'] }),
    }));

    expect(screen.getByText('Prefetch 插件')).toBeDefined();
    expect(screen.getByText('v1.2.3')).toBeDefined();
    expect(screen.getByText('family 白名单拒绝: Foo')).toBeDefined();
  });

  it('renders one table section per declared family', () => {
    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture({
        families: [
          { family: 'ProgramExecution', count: 2 },
          { family: 'UserActivity', count: 1 },
        ],
      }),
    }));

    expect(screen.getByText('ProgramExecution (2)')).toBeDefined();
    expect(screen.getByText('UserActivity (1)')).toBeDefined();
    expect(mocks.usePluginFamilyEntries).toHaveBeenCalledTimes(2);
  });

  it('renders fixed columns plus dynamic attrs columns from loaded rows', () => {
    mocks.usePluginFamilyEntries.mockReturnValue(queryState([
      entry('a1', { exeName: 'APP.EXE', runCount: 3 }),
    ]));

    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    // Filter-bar select labels repeat filterable column titles; pin to the header cell.
    expect(screen.getByRole('columnheader', { name: '标题' })).toBeDefined();
    expect(screen.getByRole('columnheader', { name: '摘要' })).toBeDefined();
    expect(screen.getByRole('columnheader', { name: '置信度' })).toBeDefined();
    expect(screen.getByRole('columnheader', { name: '来源路径' })).toBeDefined();
    expect(screen.getByRole('columnheader', { name: 'exeName' })).toBeDefined();
    expect(screen.getByRole('columnheader', { name: 'runCount' })).toBeDefined();
    // The filterable title column also lists the value as a filter option;
    // the cell role pins the assertion to the table row.
    expect(screen.getByRole('cell', { name: 'title-a1' })).toBeDefined();
    expect(screen.getByText('90%')).toBeDefined();
  });

  it('expands the full attrs detail when a row is clicked', () => {
    mocks.usePluginFamilyEntries.mockReturnValue(queryState([
      entry('a1', { exeName: 'APP.EXE', runCount: 3 }),
    ]));

    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    expect(screen.queryByText('属性明细')).toBeNull();
    fireEvent.click(screen.getByRole('cell', { name: 'title-a1' }));
    expect(screen.getByText('属性明细')).toBeDefined();
    // Cell + expanded detail both render the attr value.
    expect(screen.getAllByText('APP.EXE').length).toBeGreaterThan(1);
    expect(screen.getAllByText('3').length).toBeGreaterThan(1);
  });

  it('renders the empty state when the family has no entries', () => {
    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    expect(screen.getByText('暂无插件痕迹')).toBeDefined();
  });

  it('renders the error state and retries the initial load', () => {
    const refetch = vi.fn();
    mocks.usePluginFamilyEntries.mockReturnValue(queryState([], {
      data: undefined,
      isError: true,
      refetch,
    }));

    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    expect(screen.getByText('插件痕迹加载失败。')).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(refetch).toHaveBeenCalled();
  });

  it('shows load progress while more rows remain on the backend', () => {
    mocks.usePluginFamilyEntries.mockReturnValue(queryState(
      [entry('a1')],
      {
        data: {
          pluginId: 'plugin-prefetch',
          family: 'ProgramExecution',
          totalCount: 5,
          truncated: false,
          entries: [entry('a1')],
        },
        hasNextPage: true,
      },
    ));

    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    expect(screen.getByText('已加载 1 / 5')).toBeDefined();
  });

  it('shows the loading placeholder while the first page is in flight', () => {
    mocks.usePluginFamilyEntries.mockReturnValue(queryState([], {
      data: undefined,
      isLoading: true,
    }));

    render(createElement(PluginModulePanel, {
      dataSourceId: 'ds-1',
      module: moduleFixture(),
    }));

    expect(screen.getByText('正在加载插件痕迹...')).toBeDefined();
  });
});
