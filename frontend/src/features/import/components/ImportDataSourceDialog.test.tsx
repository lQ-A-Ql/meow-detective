import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { ImportDataSourceDialog } from './ImportDataSourceDialog';

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

  function advanceToForm() {
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
  }

  function clickBack() {
    fireEvent.click(screen.getByRole('button', { name: '上一步' }));
  }

  function clickImport() {
    fireEvent.click(screen.getByRole('button', { name: '导入' }));
  }

  it('renders the platform selection step on open', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    expect(screen.getByLabelText('Windows')).toBeDefined();
    expect(screen.getByLabelText('Linux')).toBeDefined();
  });

  it('preserves the shared dialog fixed viewport positioning', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    const content = document.querySelector('[data-slot="dialog-content"]');
    expect(content?.classList.contains('fixed')).toBe(true);
    expect(content?.classList.contains('relative')).toBe(false);
  });

  it('advances to the form step when clicking next', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();

    expect(screen.getAllByRole('textbox')).toHaveLength(2);
  });

  it('goes back to platform step from form step', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    clickBack();

    expect(screen.getByLabelText('Windows')).toBeDefined();
  });

  it('shows error when importing with empty path', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    clickImport();

    expect(screen.getByText('请选择数据源路径')).toBeDefined();
    expect(baseProps.onImport).not.toHaveBeenCalled();
  });

  it('calls onImport with trimmed path, platform, and optional profile when form is valid', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToForm();
    const [nameInput, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(nameInput, { target: { value: '  ubuntu-server  ' } });
    fireEvent.change(pathInput, { target: { value: '  /path/to/source  ' } });
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: '/path/to/source',
      platform: 'linux',
      profile: 'ubuntu-server',
    });
  });

  it('calls onImport with linuxCluster source kind when cluster mode is selected', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToForm();
    fireEvent.click(screen.getByLabelText('Linux 集群'));
    const [nameInput, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(nameInput, { target: { value: '  pve-cluster  ' } });
    fireEvent.change(pathInput, { target: { value: '  D:/pve/images  ' } });
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: 'D:/pve/images',
      sourceKind: 'linuxCluster',
      platform: 'linux',
      profile: 'pve-cluster',
    });
  });

  it('omits profile when the optional profile field is blank', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: 'C:/evidence/win.E01' } });
    clickImport();

    expect(baseProps.onImport).toHaveBeenCalledWith({
      sourcePath: 'C:/evidence/win.E01',
      platform: 'windows',
      profile: undefined,
    });
  });

  it('shows loading state when import is pending', () => {
    render(createElement(ImportDataSourceDialog, { ...baseProps, importPending: true }));

    advanceToForm();

    expect(screen.getByRole('button', { name: '导入中...' })).toHaveProperty('disabled', true);
  });

  it('preserves form values when going back and forward between steps', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    const [, pathInput] = screen.getAllByRole('textbox') as HTMLInputElement[];
    fireEvent.change(pathInput, { target: { value: '/test/path' } });
    clickBack();
    advanceToForm();

    expect((screen.getAllByRole('textbox')[1] as HTMLInputElement).value).toBe('/test/path');
  });

  it('does not render when open is false', () => {
    render(createElement(ImportDataSourceDialog, { ...baseProps, open: false }));

    expect(screen.queryByLabelText('Windows')).toBeNull();
  });

  it('calls native file picker when file button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    fireEvent.click(screen.getByRole('button', { name: '文件' }));

    expect(mocks.pickSourcePath).toHaveBeenCalledWith('数据源');
  });

  it('calls native directory picker when directory button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    advanceToForm();
    fireEvent.click(screen.getByRole('button', { name: '目录' }));

    expect(mocks.pickDirectoryPath).toHaveBeenCalledOnce();
  });

  it('shows linux cluster folder picker for linux imports', () => {
    render(createElement(ImportDataSourceDialog, baseProps));

    fireEvent.click(screen.getByLabelText('Linux'));
    advanceToForm();

    expect(screen.getByRole('button', { name: '集群目录' })).toBeDefined();
  });
});
