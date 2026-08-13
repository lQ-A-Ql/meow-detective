import { createElement } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AnalysisSourceSidebar } from './AnalysisSourceSidebar';
import type { DataSourceSummary, PluginModule } from '@/types/models';
import {
  ANALYSIS_EXTRACTION_CATEGORIES,
  type AnalysisExtractionProgressInfo,
  type ExtractionCategory,
} from '@/features/analysis/types';

const windowsSource: DataSourceSummary = {
  id: 'ds-win',
  name: 'Win10-C盘',
  kind: 'logical_directory',
  sourcePath: 'C:\\Cases\\win10',
  importedAt: '2026-06-01T10:00:00Z',
  platform: 'windows',
  partitions: [],
};

const linuxSource: DataSourceSummary = {
  id: 'ds-linux',
  name: 'Ubuntu-Root',
  kind: 'e01',
  sourcePath: 'E:\\cases\\ubuntu.E01',
  importedAt: '2026-06-02T10:00:00Z',
  platform: 'linux',
  partitions: [],
};

function pluginModule(overrides: Partial<PluginModule> = {}): PluginModule {
  return {
    pluginId: 'plugin-wechat',
    displayName: '微信',
    pluginVersion: '1.0.0',
    evidencePlatform: 'windows',
    families: [{ family: 'ChatMessages', count: 12 }],
    totalCount: 12,
    warnings: [],
    ...overrides,
  };
}

function idleProgress(): Record<ExtractionCategory, AnalysisExtractionProgressInfo> {
  const entry: AnalysisExtractionProgressInfo = {
    label: '',
    status: 'idle',
    scannedCount: 0,
    artifactCount: 0,
    timelineEventCount: 0,
    warnings: [],
  };
  return Object.fromEntries(
    ANALYSIS_EXTRACTION_CATEGORIES.map((category) => [category, entry]),
  ) as Record<ExtractionCategory, AnalysisExtractionProgressInfo>;
}

function renderSidebar(overrides: Partial<Parameters<typeof AnalysisSourceSidebar>[0]> = {}) {
  const props: Parameters<typeof AnalysisSourceSidebar>[0] = {
    dataSources: [windowsSource],
    selectedDataSourceId: windowsSource.id,
    progress: idleProgress(),
    activeWindowsTab: 'system',
    activeLinuxTab: 'overview',
    onSelectDataSource: vi.fn(),
    onWindowsTabChange: vi.fn(),
    onLinuxTabChange: vi.fn(),
    onSelectPluginModule: vi.fn(),
    ...overrides,
  };
  render(createElement(AnalysisSourceSidebar, props));
  return props;
}

describe('AnalysisSourceSidebar plugin group', () => {
  it('renders the plugin group with module nodes and total-count badges', () => {
    renderSidebar({
      pluginModules: [
        pluginModule(),
        pluginModule({ pluginId: 'plugin-qq', displayName: 'QQ', totalCount: 3 }),
      ],
    });

    expect(screen.getByText('应用插件')).toBeDefined();
    expect(screen.getByText('微信(12)')).toBeDefined();
    expect(screen.getByText('QQ(3)')).toBeDefined();
  });

  it('hides plugin modules whose evidence platform does not match the source', () => {
    renderSidebar({
      pluginModules: [
        pluginModule(),
        pluginModule({ pluginId: 'plugin-bash', displayName: 'Bash 历史', evidencePlatform: 'linux' }),
      ],
    });

    expect(screen.getByText('微信(12)')).toBeDefined();
    expect(screen.queryByText('Bash 历史(12)')).toBeNull();
  });

  it('renders linux-platform plugins under a linux data source', () => {
    renderSidebar({
      dataSources: [linuxSource],
      selectedDataSourceId: linuxSource.id,
      pluginModules: [
        pluginModule(),
        pluginModule({ pluginId: 'plugin-bash', displayName: 'Bash 历史', evidencePlatform: 'linux' }),
      ],
    });

    expect(screen.queryByText('微信(12)')).toBeNull();
    expect(screen.getByText('Bash 历史(12)')).toBeDefined();
  });

  it('does not render the plugin group when no module matches', () => {
    renderSidebar({ pluginModules: [] });

    expect(screen.queryByText('应用插件')).toBeNull();
  });

  it('notifies plugin selection with the plugin id', () => {
    const props = renderSidebar({ pluginModules: [pluginModule()] });

    fireEvent.click(screen.getByRole('button', { name: 'Win10-C盘 / 微信' }));

    expect(props.onSelectPluginModule).toHaveBeenCalledWith('plugin-wechat');
  });
});
