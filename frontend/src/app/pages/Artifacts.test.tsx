import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Artifacts } from './Artifacts';

const mocks = vi.hoisted(() => ({
  artifactFamilies: vi.fn(),
  artifactFamilyCounts: vi.fn(),
  infiniteArtifactRows: vi.fn(),
  artifactById: vi.fn(),
  navigate: vi.fn(),
  selectionState: {
    selectedArtifactFamily: 'LNK',
    selectedArtifactId: undefined as string | undefined,
    setSelectedArtifactFamily: vi.fn(),
    setSelectedArtifactId: vi.fn(),
    setSelectedFileId: vi.fn(),
    setSelectedTimelineId: vi.fn(),
  },
}));

vi.mock('@/features/artifacts/hooks', () => ({
  useArtifactById: mocks.artifactById,
  useArtifactFamilies: mocks.artifactFamilies,
  useArtifactFamilyCounts: mocks.artifactFamilyCounts,
  useInfiniteArtifactRows: mocks.infiniteArtifactRows,
}));

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn((selector) => selector(mocks.selectionState)),
}));

vi.mock('react-router', () => ({
  useNavigate: () => mocks.navigate,
}));

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
    ...overrides,
  };
}

function infiniteQueryState(items: unknown[]) {
  return queryState({
    data: { pages: [{ total: items.length, items }], pageParams: [0] },
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    isFetchingNextPage: false,
    isFetchNextPageError: false,
  });
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(Artifacts),
    ),
  );
}

const emptyRows: unknown[] = [];
const populatedRows = [
  {
    id: 'lnk-1',
    artifactType: 'LNK',
    title: 'C:\\Users\\admin\\Desktop\\secret.lnk',
    summary: '目标路径: D:\\Documents\\classified.pdf',
    sourceObjectId: 'file-10',
    createdAt: '2026-05-15T09:00:00Z',
    attrs: {
      targetPath: 'D:\\Documents\\classified.pdf',
      driveType: 'Fixed',
      volumeSerial: 'A1B2-C3D4',
      machineId: 'PC-001',
    },
  },
  {
    id: 'lnk-2',
    artifactType: 'LNK',
    title: 'C:\\Users\\admin\\Recent\\report.lnk',
    summary: '目标路径: E:\\Work\\report.xlsx',
    sourceObjectId: 'file-20',
    createdAt: '2026-05-20T14:30:00Z',
    attrs: {
      targetPath: 'E:\\Work\\report.xlsx',
      driveType: 'Fixed',
      volumeSerial: 'E5F6-G7H8',
      machineId: 'PC-001',
    },
  },
];

describe('Artifacts page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(mocks.selectionState, {
      selectedArtifactFamily: 'LNK',
      selectedArtifactId: undefined,
    });
    mocks.artifactFamilies.mockReturnValue(
      queryState({ data: ['LNK', 'Prefetch', 'JumpList'] }),
    );
    mocks.artifactFamilyCounts.mockReturnValue(
      queryState({
        data: [
          { family: 'LNK', count: 2 },
          { family: 'Prefetch', count: 5 },
          { family: 'JumpList', count: 3 },
        ],
      }),
    );
    mocks.artifactById.mockReturnValue(queryState({ data: null }));
  });

  it('renders empty state when no artifacts', () => {
    mocks.infiniteArtifactRows.mockReturnValue(infiniteQueryState(emptyRows));

    renderPage();

    expect(screen.getByText('当前痕迹家族无记录')).toBeDefined();
    expect(screen.getByText('请切换 family 或等待解析任务完成。')).toBeDefined();
  });

  it('renders family tabs when data is available', () => {
    mocks.infiniteArtifactRows.mockReturnValue(infiniteQueryState(populatedRows));

    renderPage();

    expect(screen.getAllByText(/LNK/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Prefetch/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/JumpList/).length).toBeGreaterThan(0);
    expect(screen.getByText(/痕迹家族控制/)).toBeDefined();
  });

  it('shows artifact table rows', () => {
    mocks.infiniteArtifactRows.mockReturnValue(infiniteQueryState(populatedRows));

    renderPage();

    expect(
      screen.getAllByText('C:\\Users\\admin\\Desktop\\secret.lnk').length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText('C:\\Users\\admin\\Recent\\report.lnk').length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText('D:\\Documents\\classified.pdf').length).toBeGreaterThan(0);
    expect(screen.getAllByText('E:\\Work\\report.xlsx').length).toBeGreaterThan(0);
    expect(screen.getByText(/记录 2\/2 条/)).toBeDefined();
  });

  it('hydrates a selected artifact from by-id query when family list does not contain it', () => {
    Object.assign(mocks.selectionState, {
      selectedArtifactFamily: 'LNK',
      selectedArtifactId: 'prefetch-1',
    });
    mocks.infiniteArtifactRows.mockReturnValue(infiniteQueryState(populatedRows));
    mocks.artifactById.mockReturnValue(
      queryState({
        data: {
          id: 'prefetch-1',
          artifactType: 'Prefetch',
          title: 'CMD.EXE-12345678.pf',
          summary: '目标路径: C:\\Windows\\System32\\cmd.exe',
          sourceObjectId: 'file-cmd-exe',
          createdAt: '2026-06-13T10:00:00Z',
          attrs: {
            targetPath: 'C:\\Windows\\System32\\cmd.exe',
            driveType: 'Fixed',
            volumeSerial: 'AA-BB',
            machineId: 'PC-001',
          },
        },
      }),
    );

    renderPage();

    expect(mocks.selectionState.setSelectedArtifactFamily).toHaveBeenCalledWith('Prefetch');
    expect(screen.getAllByText('CMD.EXE-12345678.pf').length).toBeGreaterThan(0);
    expect(screen.getAllByText('C:\\Windows\\System32\\cmd.exe').length).toBeGreaterThan(0);
  });
});
