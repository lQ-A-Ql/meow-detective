import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { CaseWelcomeForms, ImportSection } from '@/features/case/components/CaseActions';
import type { CaseWelcomeFormsProps, ImportSectionProps } from '@/features/case/components/CaseActions';
import { openDialog } from '@/lib/platform/dialog';

vi.mock('@/lib/platform/dialog', () => ({
  openDialog: vi.fn(),
  singleDialogPath: (path: string | string[] | null) => (Array.isArray(path) ? path[0] ?? null : path),
}));

const mockedOpen = openDialog as unknown as ReturnType<typeof vi.fn>;

function baseWelcomeProps(overrides: Partial<CaseWelcomeFormsProps> = {}): CaseWelcomeFormsProps {
  return {
    caseRoot: '',
    setCaseRoot: vi.fn(),
    caseName: '',
    setCaseName: vi.fn(),
    onCreateCase: vi.fn(),
    createPending: false,
    createError: null,
    openCasePath: '',
    setOpenCasePath: vi.fn(),
    onOpenCase: vi.fn(),
    openPending: false,
    openError: null,
    recentCases: [],
    onDeleteCase: vi.fn(),
    ...overrides,
  };
}

function baseImportProps(overrides: Partial<ImportSectionProps> = {}): ImportSectionProps {
  return {
    importPath: '',
    setImportPath: vi.fn(),
    onImport: vi.fn(),
    importPending: false,
    importSuccess: null,
    importError: null,
    importJob: undefined,
    cancelImportPending: false,
    onCancelImport: vi.fn(),
    failedImportJob: undefined,
    onClose: vi.fn(),
    ...overrides,
  };
}

describe('CaseWelcomeForms', () => {
  it('disables create button when case root or name is missing', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ caseRoot: '', caseName: '' })} />);

    const createButton = screen.getByRole('button', { name: '创建案件' }) as HTMLButtonElement;
    expect(createButton.disabled).toBe(true);
  });

  it('enables create button and invokes onCreateCase when fields are filled', () => {
    const onCreateCase = vi.fn();
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({ caseRoot: 'C:\\cases', caseName: 'case-1', onCreateCase })}
      />,
    );

    const createButton = screen.getByRole('button', { name: '创建案件' }) as HTMLButtonElement;
    expect(createButton.disabled).toBe(false);
    fireEvent.click(createButton);
    expect(onCreateCase).toHaveBeenCalledTimes(1);
  });

  it('shows creating label and create error message', () => {
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          caseRoot: 'C:\\cases',
          caseName: 'case-1',
          createPending: true,
          createError: '目录已存在',
        })}
      />,
    );

    expect(screen.getByText('创建中...')).toBeDefined();
    expect(screen.getByText('目录已存在')).toBeDefined();
  });

  it('disables open button when path is empty and calls onOpenCase with the path otherwise', () => {
    const onOpenCase = vi.fn();
    const { rerender } = render(<CaseWelcomeForms {...baseWelcomeProps({ openCasePath: '' })} />);

    let openButton = screen.getByRole('button', { name: '打开案件' }) as HTMLButtonElement;
    expect(openButton.disabled).toBe(true);

    rerender(<CaseWelcomeForms {...baseWelcomeProps({ openCasePath: 'C:\\cases\\case-1', onOpenCase })} />);
    openButton = screen.getByRole('button', { name: '打开案件' }) as HTMLButtonElement;
    expect(openButton.disabled).toBe(false);
    fireEvent.click(openButton);
    expect(onOpenCase).toHaveBeenCalledWith('C:\\cases\\case-1');
  });

  it('shows open error message', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ openError: '路径无效' })} />);
    expect(screen.getByText('路径无效')).toBeDefined();
  });

  it('renders empty-state message when there are no recent cases', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ recentCases: [] })} />);
    expect(screen.getByText('这里会保留最近打开过的案件，便于重新进入分析现场。')).toBeDefined();
  });

  it('renders recent cases and opens one on click', () => {
    const onOpenCase = vi.fn();
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          recentCases: [
            { caseRoot: 'C:\\cases\\case-1', name: 'Case One', openedAt: '2026-06-01' },
          ],
          onOpenCase,
        })}
      />,
    );

    expect(screen.getByText('Case One')).toBeDefined();
    fireEvent.click(screen.getByText('Case One'));
    expect(onOpenCase).toHaveBeenCalledWith('C:\\cases\\case-1');
  });

  it('deletes a recent case only when the confirm dialog is accepted', () => {
    const onDeleteCase = vi.fn();
    const onOpenCase = vi.fn();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          recentCases: [
            { caseRoot: 'C:\\cases\\case-1', name: 'Case One', openedAt: '2026-06-01' },
          ],
          onDeleteCase,
          onOpenCase,
        })}
      />,
    );

    fireEvent.click(screen.getByTitle('删除案件'));
    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(onDeleteCase).not.toHaveBeenCalled();
    expect(onOpenCase).not.toHaveBeenCalled();

    confirmSpy.mockReturnValue(true);
    fireEvent.click(screen.getByTitle('删除案件'));
    expect(onDeleteCase).toHaveBeenCalledWith('C:\\cases\\case-1');

    confirmSpy.mockRestore();
  });
});

describe('ImportSection', () => {
  beforeEach(() => {
    mockedOpen.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('updates the import path from the text input', () => {
    const setImportPath = vi.fn();
    render(<ImportSection {...baseImportProps({ setImportPath })} />);

    fireEvent.change(screen.getByPlaceholderText('镜像路径或逻辑目录路径'), {
      target: { value: 'D:\\evidence\\case.e01' },
    });
    expect(setImportPath).toHaveBeenCalledWith('D:\\evidence\\case.e01');
  });

  it('sets the import path from the file dialog result', async () => {
    mockedOpen.mockResolvedValue('D:\\evidence\\case.e01');
    const setImportPath = vi.fn();
    render(<ImportSection {...baseImportProps({ setImportPath })} />);

    fireEvent.click(screen.getByRole('button', { name: /文件/ }));
    await vi.waitFor(() => expect(setImportPath).toHaveBeenCalledWith('D:\\evidence\\case.e01'));
    expect(mockedOpen).toHaveBeenCalledWith(
      expect.objectContaining({ directory: false, multiple: false }),
    );
  });

  it('sets the import path from the directory dialog result', async () => {
    mockedOpen.mockResolvedValue('D:\\evidence\\logical');
    const setImportPath = vi.fn();
    render(<ImportSection {...baseImportProps({ setImportPath })} />);

    fireEvent.click(screen.getByRole('button', { name: /目录/ }));
    await vi.waitFor(() => expect(setImportPath).toHaveBeenCalledWith('D:\\evidence\\logical'));
    expect(mockedOpen).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: false }));
  });

  it('does not throw when the dialog plugin is unavailable', async () => {
    mockedOpen.mockRejectedValue(new Error('dialog unavailable'));
    const setImportPath = vi.fn();
    render(<ImportSection {...baseImportProps({ setImportPath })} />);

    fireEvent.click(screen.getByRole('button', { name: /文件/ }));
    await vi.waitFor(() => expect(mockedOpen).toHaveBeenCalledTimes(1));
    expect(setImportPath).not.toHaveBeenCalled();
  });

  it('disables the import button while pending or while a background job is active', () => {
    const { rerender } = render(<ImportSection {...baseImportProps({ importPending: true })} />);
    let importButton = screen.getByRole('button', { name: /提交中|导入/ }) as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);

    rerender(
      <ImportSection
        {...baseImportProps({
          importJob: { id: 'job-1', name: '导入任务', progress: 42, detail: '解析 MFT' } as never,
        })}
      />,
    );
    importButton = screen.getByRole('button', { name: /后台导入中/ }) as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);
    expect(screen.getByText(/导入任务 · 42% · 解析 MFT/)).toBeDefined();
  });

  it('invokes onImport when enabled', () => {
    const onImport = vi.fn();
    render(<ImportSection {...baseImportProps({ importPath: 'D:\\evidence\\case.e01', onImport })} />);

    fireEvent.click(screen.getByRole('button', { name: '导入' }));
    expect(onImport).toHaveBeenCalledTimes(1);
  });

  it('disables cancel while cancelling and invokes onCancelImport otherwise', () => {
    const onCancelImport = vi.fn();
    const importJob = { id: 'job-1', name: '导入任务', progress: 10, detail: '' } as never;

    const { rerender } = render(
      <ImportSection {...baseImportProps({ importJob, cancelImportPending: true, onCancelImport })} />,
    );
    let cancelButton = screen.getByRole('button', { name: /取消中/ }) as HTMLButtonElement;
    expect(cancelButton.disabled).toBe(true);

    rerender(<ImportSection {...baseImportProps({ importJob, cancelImportPending: false, onCancelImport })} />);
    cancelButton = screen.getByRole('button', { name: '取消导入' }) as HTMLButtonElement;
    expect(cancelButton.disabled).toBe(false);
    fireEvent.click(cancelButton);
    expect(onCancelImport).toHaveBeenCalledTimes(1);
  });

  it('invokes onClose when the close button is clicked', () => {
    const onClose = vi.fn();
    render(<ImportSection {...baseImportProps({ onClose })} />);

    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('renders import success, error, and failed job messages', () => {
    render(
      <ImportSection
        {...baseImportProps({
          importSuccess: '导入完成',
          importError: '磁盘空间不足',
          failedImportJob: { id: 'job-2', name: '导入任务', progress: 0, detail: '解析失败' } as never,
        })}
      />,
    );

    expect(screen.getByText('导入完成')).toBeDefined();
    expect(screen.getByText(/导入失败: 磁盘空间不足/)).toBeDefined();
    expect(screen.getByText(/后台导入失败: 解析失败/)).toBeDefined();
  });
});
