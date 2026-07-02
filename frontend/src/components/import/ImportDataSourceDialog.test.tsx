import { createElement } from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { ImportDataSourceDialog } from './ImportDataSourceDialog';

const mocks = vi.hoisted(() => ({
  tauriOpen: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: mocks.tauriOpen,
}));

describe('ImportDataSourceDialog', () => {
  const baseProps = {
    open: true,
    onOpenChange: vi.fn(),
    onImport: vi.fn(),
    importPending: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the platform selection step on open', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    expect(screen.getByText('导入数据源')).toBeDefined();
    expect(screen.getByText('步骤 1/2：选择目标平台')).toBeDefined();
    expect(screen.getByLabelText('Windows')).toBeDefined();
    expect(screen.getByLabelText('Linux')).toBeDefined();
  });

  it('advances to the form step when clicking next', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    expect(screen.getByText('步骤 2/2：填写数据源信息')).toBeDefined();
    expect(screen.getByLabelText('数据源名称')).toBeDefined();
  });

  it('goes back to platform step from form step', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    fireEvent.click(screen.getByText('上一步'));
    expect(screen.getByText('步骤 1/2：选择目标平台')).toBeDefined();
  });

  it('shows error when importing with empty path', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    fireEvent.click(screen.getByText('导入'));
    expect(screen.getByText('请选择数据源路径')).toBeDefined();
    expect(baseProps.onImport).not.toHaveBeenCalled();
  });

  it('calls onImport with trimmed path when form is valid', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    const pathInput = screen.getByLabelText('数据源路径') as HTMLInputElement;
    fireEvent.change(pathInput, { target: { value: '  /path/to/source  ' } });
    fireEvent.click(screen.getByText('导入'));
    expect(baseProps.onImport).toHaveBeenCalledWith('/path/to/source');
  });

  it('shows loading state when import is pending', () => {
    render(
      createElement(ImportDataSourceDialog, { ...baseProps, importPending: true }),
    );
    fireEvent.click(screen.getByText('下一步'));
    expect(screen.getByText('导入中...')).toBeDefined();
  });

  it('preserves form values when going back and forward between steps', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    const pathInput = screen.getByLabelText('数据源路径') as HTMLInputElement;
    fireEvent.change(pathInput, { target: { value: '/test/path' } });
    fireEvent.click(screen.getByText('上一步'));
    fireEvent.click(screen.getByText('下一步'));
    // Form values survive step transition
    expect((screen.getByLabelText('数据源路径') as HTMLInputElement).value).toBe('/test/path');
  });

  it('does not render when open is false', () => {
    render(
      createElement(ImportDataSourceDialog, { ...baseProps, open: false }),
    );
    expect(screen.queryByText('导入数据源')).toBeNull();
  });

  it('calls native file picker when file button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    fireEvent.click(screen.getByText('文件'));
    expect(mocks.tauriOpen).toHaveBeenCalledWith(
      expect.objectContaining({ directory: false }),
    );
  });

  it('calls native directory picker when directory button is clicked', () => {
    render(createElement(ImportDataSourceDialog, baseProps));
    fireEvent.click(screen.getByText('下一步'));
    fireEvent.click(screen.getByText('目录'));
    expect(mocks.tauriOpen).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true }),
    );
  });
});
