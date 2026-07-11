import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CaseHome } from './CaseHome';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  caseMetrics: vi.fn(),
  dataSources: vi.fn(),
  recentCases: vi.fn(),
  recentObjects: vi.fn(),
  jobsSnapshot: vi.fn(),
  warnings: vi.fn(),
  createCase: vi.fn(),
  openCase: vi.fn(),
  deleteCase: vi.fn(),
  deleteDataSource: vi.fn(),
  renameDataSource: vi.fn(),
  removeCaseFromList: vi.fn(),
  importDataSource: vi.fn(),
  appSettings: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useCaseMetrics: mocks.caseMetrics,
  useDataSources: mocks.dataSources,
  useRecentCases: mocks.recentCases,
  useRecentObjects: mocks.recentObjects,
  useCreateCase: mocks.createCase,
  useOpenCase: mocks.openCase,
  useDeleteCase: mocks.deleteCase,
  useDeleteDataSource: mocks.deleteDataSource,
  useRenameDataSource: mocks.renameDataSource,
  useRemoveCaseFromList: mocks.removeCaseFromList,
}));

vi.mock('@/features/files/hooks', () => ({
  useImportDataSource: mocks.importDataSource,
}));

vi.mock('@/features/jobs/hooks', () => ({
  useJobsSnapshot: mocks.jobsSnapshot,
  useWarnings: mocks.warnings,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('@/features/settings/hooks', () => ({
  useAppSettings: mocks.appSettings,
}));

vi.mock('@/lib/settings', () => ({
  readLocalSettings: vi.fn(() => ({
    caseRoot: 'C:\\Meow_Detective\\cases',
    imageSearchPaths: 'E:\\cases\\; D:\\images\\',
    devEventTrace: false,
    maxImportWorkers: '',
    maxAnalysisWorkers: '',
    importAnalysisMode: 'metadataOnly',
  })),
}));

function mockQueryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    isPending: false,
    isError: false,
    refetch: vi.fn(),
    ...overrides,
  };
}

function mockMutationState(overrides: Record<string, unknown> = {}) {
  return {
    mutate: vi.fn(),
    mutateAsync: vi.fn(),
    isPending: false,
    isSuccess: false,
    isError: false,
    error: null,
    reset: vi.fn(),
    ...overrides,
  };
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(CaseHome),
    ),
  );
}

async function renderPageAsync() {
  const result = renderPage();
  await act(async () => { await Promise.resolve(); });
  return result;
}

describe('CaseHome page', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Default: no case open
    mocks.currentCase.mockReturnValue(mockQueryState({ data: undefined }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({ data: undefined }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: undefined }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    mocks.createCase.mockReturnValue(mockMutationState());
    mocks.openCase.mockReturnValue(mockMutationState());
    mocks.deleteCase.mockReturnValue(mockMutationState());
    mocks.deleteDataSource.mockReturnValue(mockMutationState());
    mocks.renameDataSource.mockReturnValue(mockMutationState());
    mocks.removeCaseFromList.mockReturnValue(mockMutationState());
    mocks.importDataSource.mockReturnValue(mockMutationState());
    mocks.appSettings.mockReturnValue({
      data: {
        caseRoot: 'D:\\ForensicsCases',
        imageSearchPaths: [],
        devEventTrace: false,
        maxImportWorkers: undefined,
        maxAnalysisWorkers: undefined,
        importAnalysisMode: 'metadataOnly',
      },
      error: null,
      isLoading: false,
    });
  });

  it('renders welcome screen when no case is open', async () => {
    await renderPageAsync();

    expect(screen.getByText('Meow~Detective')).toBeDefined();
    expect(screen.getByText(/当前没有活动案件/)).toBeDefined();
  });

  it('shows create case form', async () => {
    await renderPageAsync();

    expect(screen.getByText('新建案件')).toBeDefined();
    expect(screen.getByPlaceholderText('案件父目录')).toBeDefined();
    expect(screen.getByPlaceholderText('案件名称')).toBeDefined();
    expect(screen.getByText('创建案件')).toBeDefined();
  });

  it('shows open case form', async () => {
    await renderPageAsync();

    expect(screen.getByText('打开已有案件')).toBeDefined();
    expect(screen.getByPlaceholderText('案件路径')).toBeDefined();
    expect(screen.getByText('打开案件')).toBeDefined();
  });

  it('shows recent cases section', async () => {
    await renderPageAsync();

    expect(screen.getByText('最近打开案件')).toBeDefined();
  });

  it('renders case dashboard when case is open', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: {
        dataSourceCount: 2,
        indexedFileCount: 1000,
        timelineEventCount: 500,
        artifactCount: 100,
      },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getByText('Test Case')).toBeDefined();
    expect(screen.getByText(/2026-001/)).toBeDefined();
  });

  it('shows metric blocks when case is open', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: {
        dataSourceCount: 2,
        indexedFileCount: 1000,
        timelineEventCount: 500,
        artifactCount: 100,
      },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getAllByText('数据源').length).toBeGreaterThan(0);
    expect(screen.getAllByText('已索引文件').length).toBeGreaterThan(0);
    expect(screen.getAllByText('时间线事件').length).toBeGreaterThan(0);
    expect(screen.getAllByText('提取痕迹').length).toBeGreaterThan(0);
  });

  it('shows data sources panel when case is open', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: { dataSourceCount: 0, indexedFileCount: 0, timelineEventCount: 0, artifactCount: 0 },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getByText('已有数据源')).toBeDefined();
  });

  it('closes the import dialog after the import command is accepted', async () => {
    const mutate = vi.fn((_request, options) => {
      options?.onSuccess?.('job-import-1');
    });
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: { dataSourceCount: 0, indexedFileCount: 0, timelineEventCount: 0, artifactCount: 0 },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.importDataSource.mockReturnValue(mockMutationState({ mutate }));

    await renderPageAsync();

    fireEvent.click(screen.getByRole('button', { name: '导入数据源' }));
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'D:/evidence/disk.E01' } });
    fireEvent.click(screen.getByRole('button', { name: '导入' }));

    expect(mutate).toHaveBeenCalledWith(
      {
        sourcePath: 'D:/evidence/disk.E01',
        platform: 'windows',
        profile: undefined,
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
    await waitFor(() => expect(screen.queryByText('步骤 2/2：填写数据源信息')).toBeNull());
  });

  it('formats partition names with the shared partition display formatter', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: { dataSourceCount: 1, indexedFileCount: 0, timelineEventCount: 0, artifactCount: 0 },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({
      data: [
        {
          id: 'ds-1',
          name: 'disk.E01',
          kind: 'e01',
          sourcePath: 'D:/evidence/disk.E01',
          importedAt: '2026-05-14T08:30:00Z',
          platform: 'windows',
          fileCount: 0,
          partitions: [
            {
              index: 1,
              name: 'EFI system partition',
              kindLabel: 'FAT',
              status: 'supported',
              offset: 0,
              length: 1024,
              filesystem: 'FAT',
            },
            {
              index: 2,
              name: 'Basic data partition',
              kindLabel: 'Basic data',
              status: 'supported',
              offset: 1024,
              length: 2048,
              filesystem: 'NTFS',
            },
            {
              index: 3,
              name: 'Windows Recovery Environment',
              kindLabel: 'Basic data',
              status: 'supported',
              offset: 3072,
              length: 1024,
              filesystem: 'NTFS',
            },
          ],
        },
      ],
    }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getByText('分区1（FAT）')).toBeDefined();
    expect(screen.getByText('分区2（NTFS）')).toBeDefined();
    expect(screen.getByText('分区3（RECOVERY）')).toBeDefined();
  });

  it('shows high value objects panel when case is open', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: { dataSourceCount: 0, indexedFileCount: 0, timelineEventCount: 0, artifactCount: 0 },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({ data: [] }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getByText('高价值对象')).toBeDefined();
  });

  it('shows running job progress when jobs are running', async () => {
    mocks.currentCase.mockReturnValue(mockQueryState({
      data: {
        id: 'case-001',
        name: 'Test Case',
        number: '2026-001',
        examiner: 'Test Examiner',
        createdAt: '2026-05-14T08:30:00Z',
        updatedAt: '2026-05-16T11:20:00Z',
      },
    }));
    mocks.caseMetrics.mockReturnValue(mockQueryState({
      data: { dataSourceCount: 0, indexedFileCount: 0, timelineEventCount: 0, artifactCount: 0 },
    }));
    mocks.dataSources.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({
      data: [
        {
          id: 'job-1',
          name: '导入数据源',
          scope: '分区 1/3',
          progress: 45,
          status: 'running',
          detail: 'Enumerating...',
          warningCount: 0,
          skippedCount: 0,
          failedCount: 0,
          partial: false,
        },
      ],
    }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    // "导入数据源" may appear in both sidebar nav and job list — use getAllByText
    const matches = screen.getAllByText('导入数据源');
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });
});
