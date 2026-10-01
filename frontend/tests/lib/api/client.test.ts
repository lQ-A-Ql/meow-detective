import { describe, it, expect } from 'vitest';
import { isApiErrorDto } from '@/lib/errors';

describe('isApiErrorDto', () => {
  it('returns true for valid ApiErrorDto', () => {
    expect(
      isApiErrorDto({
        code: 'NOT_FOUND',
        message: 'Not found',
        recoverable: true,
      }),
    ).toBe(true);
  });

  it('returns false for string', () => {
    expect(isApiErrorDto('error')).toBe(false);
  });

  it('returns false for null', () => {
    expect(isApiErrorDto(null)).toBe(false);
  });

  it('returns false for object missing required fields', () => {
    expect(isApiErrorDto({ code: 'X' })).toBe(false);
  });

  it('returns false for Error instance', () => {
    expect(isApiErrorDto(new Error('test'))).toBe(false);
  });
});
