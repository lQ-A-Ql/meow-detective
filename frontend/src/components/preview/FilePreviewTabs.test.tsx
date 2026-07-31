import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FilePreviewTabs } from './FilePreviewTabs';

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
});
