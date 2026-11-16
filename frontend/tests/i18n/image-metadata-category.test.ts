import { describe, expect, it } from 'vitest';
import { i18nTranslations } from '@/i18n/resources';
import { PROGRESS_CATEGORIES_BY_PLATFORM } from '@/features/analysis/types';

describe('image metadata extraction category', () => {
  it('has a label in both locales', () => {
    expect(i18nTranslations.en.analysis.extraction.ImageMetadata).toBe('Image metadata');
    expect(i18nTranslations['zh-CN'].analysis.extraction.ImageMetadata).toBe('图片元数据与结构');
  });

  it('appears in the Windows and Linux extraction category selectors', () => {
    expect(PROGRESS_CATEGORIES_BY_PLATFORM.windows).toContain('ImageMetadata');
    expect(PROGRESS_CATEGORIES_BY_PLATFORM.linux).toContain('ImageMetadata');
    expect(PROGRESS_CATEGORIES_BY_PLATFORM.android).not.toContain('ImageMetadata');
  });
});
