import { describe, it, expect } from 'vitest';
import { getFileTree, getFileRows, openFileHandle, readFileRange } from '@/lib/api/files';

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
});
