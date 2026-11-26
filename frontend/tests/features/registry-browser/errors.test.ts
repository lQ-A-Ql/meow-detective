import { describe, expect, it } from 'vitest';
import { registryBrowserErrorCode } from '@/features/registry-browser/errors';
import en from '@/i18n/registry-browser/en.json';
import zh from '@/i18n/registry-browser/zh-CN.json';

describe('registry browser deleted hive error', () => {
  it('preserves the backend code and provides both translations', () => {
    expect(registryBrowserErrorCode({ code: 'deletedHive', category: 'unsupported' })).toBe('deletedHive');
    expect(zh.errors.deletedHive).toBe('该配置单元已删除');
    expect(en.errors.deletedHive).toBe('The selected hive is deleted');
  });
});
