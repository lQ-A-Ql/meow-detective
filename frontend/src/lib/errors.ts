import type { ApiErrorDto } from '@/types/models';
import i18n from '@/i18n';

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

export function errorMessage(error: unknown, fallback = i18n.t('common.errors.unknownApi')) {
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
