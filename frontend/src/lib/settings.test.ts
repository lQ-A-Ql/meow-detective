import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  defaultSettings,
  formatPathList,
  parsePathList,
  readLocalSettings,
  validatePathList,
  writeLocalSettings,
} from './settings';

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

  it('parses and formats semicolon separated path lists', () => {
    expect(parsePathList('C:\\cases; D:\\images; ')).toEqual(['C:\\cases', 'D:\\images']);
    expect(formatPathList(['C:\\cases', 'D:\\images'])).toBe('C:\\cases; D:\\images');
  });
});
