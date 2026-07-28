import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FileExtractionModel } from '@/features/files/hooks/use-file-extraction';
import { FileExtractionDialogs } from './FileExtractionDialogs';

function createModel(overrides: Partial<FileExtractionModel> = {}): FileExtractionModel {
  return {
    file: {
      id: 'file-1',
      name: 'evidence.bin',
      path: '/evidence.bin',
      entryType: 'file',
      size: 4096,
      deleted: false,
      hidden: false,
      system: false,
    },
    formOpen: false,
    resultOpen: false,
    destinationPath: 'D:/exports/evidence.bin',
    validationError: undefined,
    error: undefined,
    progress: undefined,
    result: undefined,
    isExtracting: false,
    openForFile: vi.fn(),
    setFormOpen: vi.fn(),
    setResultOpen: vi.fn(),
    setDestinationPath: vi.fn(),
    browseDestination: vi.fn(),
    submit: vi.fn(),
    ...overrides,
  };
}

describe('FileExtractionDialogs', () => {
  it('renders an editable path selector and real backend progress values', () => {
    const model = createModel({
      formOpen: true,
      isExtracting: true,
      progress: {
        operationId: 'operation-1',
        fileId: 'file-1',
        phase: 'copying',
        bytesWritten: 2048,
        totalBytes: 4096,
        percent: 50,
      },
    });

    render(<FileExtractionDialogs model={model} />);

    expect(screen.getByLabelText('目标路径')).toHaveValue('D:/exports/evidence.bin');
    expect(screen.getByText('2.0 KB / 4.0 KB')).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
  });

  it('renders the verified extraction result and completion acknowledgement', () => {
    const model = createModel({
      resultOpen: true,
      result: {
        fileId: 'file-1',
        bytesWritten: 4096,
        sourceSize: 4096,
        sha256: 'b'.repeat(64),
        destinationFileName: 'evidence.bin',
        sizeVerified: true,
      },
    });

    render(<FileExtractionDialogs model={model} />);

    expect(screen.getByText('文件提取完成')).toBeInTheDocument();
    expect(screen.getByText('大小校验')).toBeInTheDocument();
    expect(screen.getByText('通过')).toBeInTheDocument();
    expect(screen.getByText('b'.repeat(64))).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '确定' }));
    expect(model.setResultOpen).toHaveBeenCalledWith(false);
  });
});
