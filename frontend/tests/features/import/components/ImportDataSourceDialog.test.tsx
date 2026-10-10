import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { ImportDataSourceDialog } from '@/features/import/components/ImportDataSourceDialog';

const mocks = vi.hoisted(() => ({
  pickSourcePath: vi.fn(),
  pickDirectoryPath: vi.fn(),
}));

describe('ImportDataSourceDialog', () => {
  const baseProps = {
    open: true,
    onOpenChange: vi.fn(),
    onImport: vi.fn(),
    importPending: false,
    pickSourcePath: mocks.pickSourcePath,
    pickDirectoryPath: mocks.pickDirectoryPath,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.pickSourcePath.mockResolvedValue(undefined);
    mocks.pickDirectoryPath.mockResolvedValue(undefined);
  });

  function clickNext() {
    fireEvent.click(screen.getByRole('button', { name: /下一步/ }));
  }

  function clickBack() {
    fireEvent.click(screen.getByRole('button', { name: /上一步/ }));
  }

  function clickImport() {
    fireEvent.click(screen.getByRole('button', { name: /^导入$|导入中/ }));
  }

  /** Platform -> source. */
  function advanceToSource() {
    clickNext();
  }

  /** Platform -> source -> path/config. */
  function advanceToConfig() {
    clickNext();
    clickNext();
  }

  /** path/config -> confirmation (the path must already be filled). */
  function advanceToConfirm() {
    clickNext();
  }

  it('renders the platform selection step on open', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    expect(screen.getByLabelText('Windows')).toBeDefined();
    expect(screen.getByLabelText('Linux')).toBeDefined();
    expect(screen.getByLabelText('Android')).toBeDefined();
  });

  it('preserves the shared dialog fixed viewport positioning', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    const content = document.querySelector('[data-slot="dialog-content"]');
    expect(content?.classList.contains('fixed')).toBe(true);
    expect(content?.classList.contains('relative')).toBe(false);
  });

  it('advances to the source step, then to the path and configuration step', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToSource();
    expect(screen.getByLabelText('单源镜像')).toBeDefined();
    expect(screen.queryAllByRole('textbox')).toHaveLength(0);

    advanceToConfig();
    expect(screen.getAllByRole('textbox')).toHaveLength(2);
  });

  it('goes back to the platform step from the form step', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    clickBack();
    clickBack();

    expect(screen.getByLabelText('Windows')).toBeDefined();
  });

  it('shows error when importing with empty path', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    clickNext();

    expect(screen.getByText('请选择数据源路径')).toBeDefined();
    expect(baseProps.onImport).not.toHaveBeenCalled();
  });

  it('calls onImport with trimmed path, platform, and optional profile when form is valid', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToConfig();
    const [nameInput, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(nameInput, { target: { value: '  ubuntu-server  ' } });
    fireEvent.change(pathInput, { target: { value: '  /path/to/source  ' } });
    advanceToConfirm();
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: '/path/to/source',
      platform: 'linux',
      profile: 'ubuntu-server',
    });
  });

  it('calls onImport with linuxEvidenceSet source kind when cluster mode is selected', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToSource();
    fireEvent.click(screen.getByLabelText('Linux 证据集合'));
    advanceToConfig();
    const [nameInput, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(nameInput, { target: { value: '  pve-cluster  ' } });
    fireEvent.change(pathInput, { target: { value: '  D:/pve/images  ' } });
    advanceToConfirm();
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: 'D:/pve/images',
      sourceKind: 'linuxEvidenceSet',
      platform: 'linux',
      profile: 'pve-cluster',
    });
  });

  it('omits profile when the optional profile field is blank', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'C:/evidence/win.E01' } });
    advanceToConfirm();
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: 'C:/evidence/win.E01',
      platform: 'windows',
      profile: undefined,
    });
  });

  it('shows the confirmation summary before the import starts', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'C:/evidence/win.E01' } });
    advanceToConfirm();

    expect(screen.getByText('证据路径')).toBeDefined();
    expect(screen.getAllByText('C:/evidence/win.E01').length).toBeGreaterThan(0);
    expect(screen.getByText('预计大小')).toBeDefined();
    expect(screen.getByText('是否只读')).toBeDefined();
    expect(screen.getByText('是否包含分区')).toBeDefined();
    expect(screen.getByText('预计导入阶段')).toBeDefined();
    expect(screen.getByText('单源镜像')).toBeDefined();
  });

  it('shows loading state when import is pending', () => {
    render(createElement(ImportDataSourceDialog, { ...baseProps, importPending: true }));

    advanceToConfig();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'C:/evidence/win.E01' } });
    advanceToConfirm();

    expect(screen.getByRole('button', { name: /^导入$|导入中/ })).toHaveProperty('disabled', true);
  });

  it('preserves form values when going back and forward between steps', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: '/test/path' } });
    clickBack();
    clickNext();

    expect((screen.getAllByRole('textbox')[1] as HTMLInputElement).value).toBe('/test/path');
  });

  it('does not render when open is false', () => {
    render(createElement(ImportDataSourceDialog, { ...baseProps, open: false }));

    expect(screen.queryByLabelText('Windows')).toBeNull();
  });

  it('calls native file picker when file button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    fireEvent.click(screen.getByRole('button', { name: '文件' }));

    expect(mocks.pickSourcePath).toHaveBeenCalledWith('数据源');
  });

  it('calls native directory picker when directory button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();
    fireEvent.click(screen.getByRole('button', { name: '目录' }));

    expect(mocks.pickDirectoryPath).toHaveBeenCalledOnce();
  });

  it('shows linux cluster folder picker for linux imports', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToConfig();

    expect(screen.getByRole('button', { name: '集群目录' })).toBeDefined();
  });

  it('loads and submits a selected local physical disk', async () => {
    const listLocalDisks = vi.fn().mockResolvedValue([
      { diskNumber: 0, path: '\\\\.\\PhysicalDrive0', size: 128 * 1024 ** 3 },
    ]);
    render(createElement(ImportDataSourceDialog, { ...baseProps, listLocalDisks }));

    advanceToSource();
    fireEvent.click(screen.getByLabelText('本地物理磁盘'));
    advanceToConfig();
    const selector = await screen.findByRole('combobox', { name: '本地物理磁盘' });

    expect(listLocalDisks).toHaveBeenCalledOnce();
    expect(selector).toHaveTextContent('PhysicalDrive0');

    advanceToConfirm();
    expect(screen.getByText(/GB/)).toBeDefined();
    clickImport();
    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: '\\\\.\\PhysicalDrive0',
      sourceKind: 'localDisk',
      platform: 'windows',
      profile: undefined,
    });
  });

  it('resets a Windows-only mode when switching to Linux', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToSource();
    fireEvent.click(screen.getByLabelText('本地物理磁盘'));
    clickBack();
    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToSource();

    expect(screen.getByLabelText('单源镜像')).toHaveAttribute('aria-checked', 'true');
    expect(screen.queryByLabelText('本地物理磁盘')).toBeNull();
  });

  it('renders the step navigation as its own vertical tablist, not as action buttons', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(4);
    expect(tabs.map((tab) => tab.getAttribute('data-state'))).toEqual(['current', 'todo', 'todo', 'todo']);
    expect(tabs.filter((tab) => tab.getAttribute('tabindex') === '0')).toHaveLength(1);
    // A step must span the rail: reusing the circular action shape squeezed the labels vertically.
    expect(tabs[0].className).toContain('w-full');
    expect(tabs[0].className).not.toContain('size-7');
  });

  it('gives the file and directory pickers different artwork', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToConfig();

    const fileButton = screen.getByRole('button', { name: '文件' });
    const directoryButton = screen.getByRole('button', { name: '目录' });
    expect(fileButton.innerHTML).not.toBe(directoryButton.innerHTML);
  });
});
