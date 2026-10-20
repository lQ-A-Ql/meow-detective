import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FilePreviewTabs } from '@/components/preview/FilePreviewTabs';
import type { FileHexViewerState } from '@/types/viewer';

function emptyHexState(overrides: Partial<FileHexViewerState>): FileHexViewerState {
  return {
    handle: { handleId: 'file:empty', size: 0, mime: undefined },
    mode: 'full',
    chunkSize: 64 * 1024,
    fileSize: 0,
    lines: Object.assign([], { rawBytes: [], baseOffset: 0, fileSize: 0 }),
    rawBytes: [],
    baseOffset: 0,
    loadedRanges: [{ start: 0, end: 0 }],
    activeOffset: 0,
    jumpOffsetInput: '0x0',
    isFullyLoaded: true,
    isLoadingMore: false,
    hasMoreBefore: false,
    hasMoreAfter: false,
    ...overrides,
  };
}

describe('FilePreviewTabs', () => {
  it('sizes spreadsheet and database tables from their actual preview rows', () => {
    const { container } = render(
      <div className="h-[600px]">
        <FilePreviewTabs
          viewerTab="preview"
          setViewerTab={vi.fn()}
          viewer={undefined}
          previewKind="document"
          onHexJumpInputChange={vi.fn()}
          onHexJump={async () => false}
          onLoadNextHexRange={vi.fn()}
          onLoadPreviousHexRange={vi.fn()}
          textPreview={null}
          imagePreview={null}
          mediaUrl={null}
          documentPreview={{
            kind: 'spreadsheet',
            summary: 'Sheet1',
            sections: [
              {
                title: 'Sheet1',
                lines: [],
                table: {
                  columns: ['Name', 'Value'],
                  rows: [
                    ['alpha', '1'],
                    ['beta', '2'],
                  ],
                },
              },
            ],
            truncated: false,
          }}
          selectedFile={undefined}
        />
      </div>,
    );

    expect(screen.getByText('alpha')).toBeDefined();
    const tableFrame = container.querySelector<HTMLElement>('[style*="height: min"]');
    expect(tableFrame).toHaveStyle({
      height: 'min(90px, min(60vh, 35rem))',
    });
  });

  it('says a zero-byte file is empty instead of asking for a selection', () => {
    render(
      <FilePreviewTabs
        viewerTab="hex"
        setViewerTab={vi.fn()}
        viewer={emptyHexState({})}
        previewKind={undefined}
        onHexJumpInputChange={vi.fn()}
        onHexJump={async () => false}
        onLoadNextHexRange={vi.fn()}
        onLoadPreviousHexRange={vi.fn()}
        textPreview={null}
        imagePreview={null}
        mediaUrl={null}
        documentPreview={null}
        selectedFile={{ id: 'file:empty', path: '/evidence/empty.txt', name: 'empty.txt', entryType: 'file', size: 0, deleted: false, hidden: false, system: false, readOnly: false, archive: false }}
      />,
    );

    expect(screen.getByText(/空文件（0 字节）/)).toBeDefined();
    expect(screen.queryByText('选择文件后显示十六进制预览')).toBeNull();
  });

  it('keeps the selection prompt when no file is selected', () => {
    render(
      <FilePreviewTabs
        viewerTab="hex"
        setViewerTab={vi.fn()}
        viewer={undefined}
        previewKind={undefined}
        onHexJumpInputChange={vi.fn()}
        onHexJump={async () => false}
        onLoadNextHexRange={vi.fn()}
        onLoadPreviousHexRange={vi.fn()}
        textPreview={null}
        imagePreview={null}
        mediaUrl={null}
        documentPreview={null}
        selectedFile={undefined}
      />,
    );

    expect(screen.getByText('选择文件后显示十六进制预览')).toBeDefined();
  });
});
