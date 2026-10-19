import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CaseHome } from '@/app/pages/CaseHome';

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

    expect(screen.getByRole('heading', { name: 'Meow~Detective' })).toBeDefined();
    expect(screen.queryByText(/当前没有活动案件/)).toBeNull();
  });

  it('restores case deletion in the active workspace and confirms the selected recorded path', async () => {
    const remove = vi.fn();
    mocks.currentCase.mockReturnValue(mockQueryState({ data: { id: 'case-a', name: 'Same name' } }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [
      { name: 'Same name', caseRoot: 'D:\\CustomCases\\first', openedAt: '2026-10-19' },
      { name: 'Same name', caseRoot: 'E:\\OpenedElsewhere\\second', openedAt: '2026-10-18' },
    ] }));
    mocks.deleteCase.mockReturnValue(mockMutationState({ mutate: remove }));
    await renderPageAsync();
    fireEvent.click(screen.getByRole('button', { name: '删除案件' }));
    expect(screen.getByRole('heading', { name: '选择要删除的案件' })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole('button', { name: 'Same name' })[1]);
    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toHaveTextContent('E:\\OpenedElsewhere\\second');
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(remove).toHaveBeenCalledWith('E:\\OpenedElsewhere\\second', expect.any(Object));
  });

  it('shows a structured backend error for creating a case', async () => {
    mocks.createCase.mockReturnValue(mockMutationState({ error: { code: 'CONFLICT', message: '案件目录已存在', category: 'validation', recoverable: true } }));
    await renderPageAsync();
    expect(screen.getByRole('alert')).toHaveTextContent('案件目录已存在');
  });

  it('shows a structured backend error for opening a case', async () => {
    mocks.openCase.mockReturnValue(mockMutationState({ error: { code: 'NOT_FOUND', message: '案件数据库不存在', category: 'io', recoverable: true } }));
    await renderPageAsync();
    fireEvent.mouseDown(screen.getByRole('tab', { name: '打开已有案件' }), { button: 0, ctrlKey: false });
    expect(screen.getByRole('alert')).toHaveTextContent('案件数据库不存在');
  });

  it('shows create case form', async () => {
    await renderPageAsync();

    expect(screen.getByText('新建案件')).toBeDefined();
    expect(screen.getByPlaceholderText('案件父目录')).toBeDefined();
    expect(screen.getByPlaceholderText('案件名称')).toBeDefined();
    expect(screen.getByRole('button', { name: /创建案件|创建中/ })).toBeDefined();
  });

  it('shows open case form', async () => {
    await renderPageAsync();

    expect(screen.getByText('打开已有案件')).toBeDefined();
    fireEvent.mouseDown(screen.getByRole('tab', { name: '打开已有案件' }), { button: 0, ctrlKey: false });
    expect(screen.getByPlaceholderText('案件路径')).toBeDefined();
    expect(screen.getByRole('button', { name: /打开案件|打开中/ })).toBeDefined();
  });

  it('does not add a recent cases section to the landing page', async () => {
    await renderPageAsync();
    expect(screen.queryByText('最近打开案件')).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
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

  it('shows the centered case header and simplified workspace when case is open', async () => {
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

    expect(screen.getByRole('heading', { name: 'Test Case' })).toBeDefined();
    expect(screen.getByRole('region', { name: '已有数据源' })).toBeDefined();
    expect(screen.queryByText('已索引文件')).toBeNull();
    expect(screen.queryByText('时间线事件')).toBeNull();
    expect(screen.queryByText('提取痕迹')).toBeNull();
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

    expect(screen.getByRole('region', { name: '已有数据源' })).toBeDefined();
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
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'D:/evidence/disk.E01' } });
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    fireEvent.click(screen.getByRole('button', { name: '导入' }));

    expect(mutate).toHaveBeenCalledWith(
      {
        sourcePath: 'D:/evidence/disk.E01',
        platform: 'windows',
        profile: undefined,
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
    await waitFor(() => expect(screen.queryByText('步骤 4/4：确认导入信息')).toBeNull());
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
              uuid: '00112233-4455-6677-8899-aabbccddeeff',
              typeGuid: 'c12a7328-f81f-11d2-ba4b-00a0c93ec93b',
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
    expect(screen.getByText('分区 UUID：00112233-4455-6677-8899-aabbccddeeff')).toBeDefined();
    expect(screen.getByText('分区类型 GUID：c12a7328-f81f-11d2-ba4b-00a0c93ec93b')).toBeDefined();
    expect(screen.getAllByText('分区 UUID：未记录')).toHaveLength(2);
    expect(screen.getByText('分区2（NTFS）')).toBeDefined();
    expect(screen.getByText('分区3（RECOVERY）')).toBeDefined();
  });

  it('does not render the removed high value objects panel', async () => {
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

    expect(screen.queryByText('高价值对象')).toBeNull();
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

  it('shows evidence hash progress on the matching data source', async () => {
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
      data: [{
        id: 'ds-1',
        name: 'disk.E01',
        kind: 'e01',
        sourcePath: 'D:/evidence/disk.E01',
        importedAt: '2026-05-14T08:30:00Z',
        platform: 'windows',
        hashStatus: 'pending',
      }],
    }));
    mocks.recentCases.mockReturnValue(mockQueryState({ data: [] }));
    mocks.recentObjects.mockReturnValue(mockQueryState({ data: [] }));
    mocks.jobsSnapshot.mockReturnValue(mockQueryState({
      data: [{
        id: 'hash-job-1',
        name: 'Evidence SHA-256',
        scope: 'data source ds-1',
        progress: 42,
        status: 'running',
        detail: 'source=ds-1; hashing 42%',
        warningCount: 0,
        skippedCount: 0,
        failedCount: 0,
        partial: false,
      }],
    }));
    mocks.warnings.mockReturnValue(mockQueryState({ data: [] }));

    await renderPageAsync();

    expect(screen.getByText('后台计算中')).toBeDefined();
    expect(screen.getAllByText('42%').length).toBeGreaterThanOrEqual(1);
  });
});
