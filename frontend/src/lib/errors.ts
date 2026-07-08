import type { ApiErrorDto } from '@/types/models';

export function isApiErrorDto(value: unknown): value is ApiErrorDto {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const candidate = value as Partial<ApiErrorDto>;
  return typeof candidate.code === 'string'
    && typeof candidate.message === 'string'
    && (candidate.category === undefined || typeof candidate.category === 'string')
    && (candidate.recoverable === undefined || typeof candidate.recoverable === 'boolean');
}

export function errorMessage(error: unknown, fallback = '未知接口错误') {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return fallback;
}
