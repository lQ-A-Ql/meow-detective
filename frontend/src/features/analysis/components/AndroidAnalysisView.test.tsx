import { createElement } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AndroidAnalysisView } from './AndroidAnalysisView';
import type { AndroidDeviceInfo, AndroidPackageSummary } from '@/types/models';

const deviceInfo: AndroidDeviceInfo = {
  status: 'parsed',
  model: 'Pixel Test',
  androidVersion: '15',
  facts: [
    {
      field: 'model',
      value: 'Pixel Test',
      confidence: 'direct',
      sourceFileId: 'build-prop',
      sourcePath: 'system/build.prop',
      parser: 'android.system-packages.v1',
    },
  ],
  warnings: [],
};

const packageSummary: AndroidPackageSummary = {
  status: 'parsed',
  totalCount: 1,
  pageTotal: 1,
  packages: [
    {
      packageName: 'org.example.app',
      appName: 'Example',
      versionCode: '42',
      installTime: '2026-01-02T03:04:05Z',
      userId: 0,
      userState: 'disabled',
      sourceFileId: 'packages',
      sourcePath: 'data/system/packages.xml',
      parser: 'android.system-packages.v1',
    },
  ],
  warnings: [],
};

describe('AndroidAnalysisView', () => {
  it('renders the device panel through shared UI primitives', () => {
    render(createElement(AndroidAnalysisView, {
      deviceInfo,
      packageSummary,
      activePanel: 'device',
      loading: false,
      running: false,
      hasMore: false,
      loadingMore: false,
      onRetry: vi.fn(),
      onLoadMore: vi.fn(),
    }));

    expect(screen.getByText('Pixel Test')).toBeDefined();
    expect(screen.queryByText('org.example.app')).toBeNull();
  });

  it('renders installed packages in a separate panel', () => {
    render(createElement(AndroidAnalysisView, {
      deviceInfo,
      packageSummary,
      activePanel: 'packages',
      loading: false,
      running: false,
      hasMore: false,
      loadingMore: false,
      onRetry: vi.fn(),
      onLoadMore: vi.fn(),
    }));

    expect(screen.getByText('org.example.app')).toBeDefined();
    expect(screen.getByText('Example')).toBeDefined();
    expect(screen.getByText('已禁用')).toBeDefined();
  });

  it('keeps device fields visible when no standard device artifact was parsed', () => {
    render(createElement(AndroidAnalysisView, {
      activePanel: 'device',
      loading: false,
      running: false,
      hasMore: false,
      loadingMore: false,
      onRetry: vi.fn(),
      onLoadMore: vi.fn(),
    }));

    expect(screen.getByText('型号')).toBeDefined();
    expect(screen.queryByText('当前数据源尚未运行 Android 分析。')).toBeNull();
  });
});
