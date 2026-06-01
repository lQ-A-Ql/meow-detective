import { describe, it, expect, vi } from 'vitest';
import {
  extractFile,
  getFileRows,
  getFileTree,
  getMediaUrl,
  openFileHandle,
  readMediaRange,
  readFileRange,
} from '@/lib/api/files';

describe('files API (mock mode)', () => {
  it('getFileTree returns tree nodes', async () => {
    const result = await getFileTree();
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].id).toBeDefined();
    expect(result[0].name).toBeDefined();
    expect(result[0].depth).toBeDefined();
  });

  it('getFileRows returns file entries for known parent', async () => {
    const result = await getFileRows('tree-system32');
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].id).toBeDefined();
    expect(result[0].name).toBeDefined();
    expect(result[0].entryType).toBeDefined();
  });

  it('getFileRows returns empty for unknown parent', async () => {
    const result = await getFileRows('nonexistent');
    expect(result).toEqual([]);
  });

  it('openFileHandle returns handle', async () => {
    const result = await openFileHandle('file-cmd-exe');
    expect(result.handleId).toBe('file:file-cmd-exe');
    expect(result.size).toBeGreaterThan(0);
  });

  it('readFileRange returns hex lines', async () => {
    const result = await readFileRange({
      handleId: 'file:file-cmd-exe',
      offset: 0,
      length: 16,
    });
    expect(result.kind).toBe('hex');
    expect(result.lines.length).toBeGreaterThan(0);
  });

  it('media preview API returns a scoped mock handle shape', async () => {
    const media = await getMediaUrl('file-video');
    expect(media.url).toBe('');
    expect(media.handleId).toBe('file:file-video');
    expect(media.canReadRanges).toBe(false);
  });

  it('readMediaRange uses the API wrapper in mock mode', async () => {
    const range = await readMediaRange({
      handleId: 'file:file-video',
      offset: 0,
      length: 16,
    });

    expect(range.offset).toBe(0);
    expect(range.eof).toBe(true);
  });

  it('extractFile exports a mock file without Tauri IPC in mock mode', async () => {
    const click = vi.fn();
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName) => {
      const element = originalCreateElement(tagName);
      if (tagName === 'a') {
        element.click = click;
      }
      return element;
    });
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:file-export'),
      revokeObjectURL: vi.fn(),
    });

    const result = await extractFile({
      id: 'file-cmd-exe',
      path: 'C:/Windows/System32/cmd.exe',
      name: 'cmd.exe',
      entryType: 'file',
      deleted: false,
    });

    expect(result).toBe('Mock file exported');
    expect(URL.createObjectURL).toHaveBeenCalled();
    expect(click).toHaveBeenCalledTimes(1);
  });
});
