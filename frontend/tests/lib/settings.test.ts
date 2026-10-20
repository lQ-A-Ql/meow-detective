import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  defaultSettings,
  formatPathList,
  getPreviewSettings,
  parsePathList,
  readLocalSettings,
  validatePathList,
  writeLocalSettings,
} from '@/lib/settings';

describe('local settings', () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => store.set(key, value),
      removeItem: (key: string) => store.delete(key),
      clear: () => store.clear(),
    });
  });

  it('returns defaults when no local settings exist', () => {
    expect(readLocalSettings()).toEqual(defaultSettings);
  });

  it('persists settings', () => {
    const saved = writeLocalSettings({
      ...defaultSettings,
      devEventTrace: true,
    });

    expect(readLocalSettings()).toEqual(saved);
  });

  it('validates semicolon separated path lists', () => {
    expect(validatePathList('C:\\cases; D:\\images')).toBe(true);
    expect(validatePathList('C:\\cases\0')).toBe(false);
  });

  it('clamps preview ranges to the transport contract', () => {
    writeLocalSettings({
      ...defaultSettings,
      hexChunkBytes: '4194304',
      maxViewerRangeLength: '8388608',
    });

    const preview = getPreviewSettings();

    expect(preview.hexChunkBytes).toBe(1024 * 1024);
    expect(preview.maxViewerRangeLength).toBe(1024 * 1024);
  });

  it('keeps preview ranges that already fit the transport contract', () => {
    writeLocalSettings({
      ...defaultSettings,
      hexChunkBytes: '32768',
      maxViewerRangeLength: '262144',
    });

    const preview = getPreviewSettings();

    expect(preview.hexChunkBytes).toBe(32768);
    expect(preview.maxViewerRangeLength).toBe(262144);
  });

  it('raises preview ranges below the transport minimum', () => {
    writeLocalSettings({
      ...defaultSettings,
      hexChunkBytes: '10',
      maxViewerRangeLength: '100',
    });

    const preview = getPreviewSettings();

    expect(preview.hexChunkBytes).toBe(1024);
    expect(preview.maxViewerRangeLength).toBe(4096);
  });

  it('parses and formats semicolon separated path lists', () => {
    expect(parsePathList('C:\\cases; D:\\images; ')).toEqual(['C:\\cases', 'D:\\images']);
    expect(formatPathList(['C:\\cases', 'D:\\images'])).toBe('C:\\cases; D:\\images');
  });
});
