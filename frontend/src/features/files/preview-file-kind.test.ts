import { describe, expect, it } from 'vitest';
import { getDefaultFilePreviewTab, getFilePreviewKind } from './preview-file-kind';

describe('preview file kind', () => {
  it('routes supported rich formats to the shared preview tab', () => {
    const file = { entryType: 'file' as const, name: 'evidence.xlsx', ext: 'xlsx' };
    expect(getFilePreviewKind(file)).toBe('document');
    expect(getDefaultFilePreviewTab(file)).toBe('preview');
  });

  it('routes text and unknown binary formats without reading content', () => {
    expect(getDefaultFilePreviewTab({ entryType: 'file', name: 'audit.log', ext: 'log' })).toBe('text');
    expect(getDefaultFilePreviewTab({ entryType: 'file', name: 'archive.7z', ext: '7z' })).toBe('hex');
  });

  it('keeps directories on index-backed metadata', () => {
    expect(getDefaultFilePreviewTab({ entryType: 'directory', name: 'Downloads' })).toBe('metadata');
  });
});
