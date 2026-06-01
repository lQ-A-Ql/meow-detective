import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  applyTheme,
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
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.classList.remove('dark');
  });

  it('returns defaults when no local settings exist', () => {
    expect(readLocalSettings()).toEqual(defaultSettings);
  });

  it('persists settings and applies theme', () => {
    const saved = writeLocalSettings({
      ...defaultSettings,
      theme: 'dark',
      devEventTrace: true,
    });

    expect(readLocalSettings()).toEqual(saved);
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('validates semicolon separated path lists', () => {
    expect(validatePathList('C:\\cases; D:\\images')).toBe(true);
    expect(validatePathList('C:\\cases\0')).toBe(false);
  });

  it('parses and formats semicolon separated path lists', () => {
    expect(parsePathList('C:\\cases; D:\\images; ')).toEqual(['C:\\cases', 'D:\\images']);
    expect(formatPathList(['C:\\cases', 'D:\\images'])).toBe('C:\\cases; D:\\images');
  });

  it('can switch back to light theme', () => {
    applyTheme('dark');
    applyTheme('light');

    expect(document.documentElement.dataset.theme).toBe('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });
});
