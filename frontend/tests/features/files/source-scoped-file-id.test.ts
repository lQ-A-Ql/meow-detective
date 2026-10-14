import { describe, expect, it } from 'vitest';
import { isSourceScopedMftId, parseSourceScopedFileId } from '@/features/files/source-scoped-file-id';

describe('source scoped file identifiers', () => {
  it('retains every local identifier separator', () => {
    expect(parseSourceScopedFileId('ds:sample-source:mft:0:42')).toEqual({ sourceId: 'sample-source', localId: 'mft:0:42' });
  });
  it.each(['ds:sample-source:mft:42', 'ds:sample-source:mft:0:42'])('recognizes %s as an MFT candidate', (value) => {
    expect(isSourceScopedMftId(value)).toBe(true);
  });
  it.each([undefined, '', 'mft:42', 'ds::mft:42', 'ds:bad/source:mft:42', 'ds:s:', 'ds:s:mft:x', 'ds:s:mft:0:x', 'ds:s:mft:1:2:3', 'ds:s:inode:42', 'ds:s:path:note.txt'])('rejects %s as an MFT candidate', (value) => {
    expect(isSourceScopedMftId(value)).toBe(false);
  });
  it('bounds the transported identifier', () => {
    expect(parseSourceScopedFileId(`ds:source:${'x'.repeat(512)}`)).toBeUndefined();
  });
});
