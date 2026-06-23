import { invoke } from '@tauri-apps/api/core';
import { ApiErrorDto } from '@/types/models';

function toApiError(error: unknown, fallbackCode: string): ApiErrorDto {
  if (isApiErrorDto(error)) {
    return error;
  }

  if (typeof error === 'string') {
    return {
      code: fallbackCode,
      message: error,
      category: 'internal',
      recoverable: true,
    };
  }

  if (error instanceof Error) {
    return {
      code: fallbackCode,
      message: error.message,
      category: 'internal',
      recoverable: true,
    };
  }

  return {
    code: fallbackCode,
    message: '未知接口错误',
    category: 'internal',
    details: error,
    recoverable: true,
  };
}

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

async function invokeTauriCommand<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, payload);
}

class ApiClient {
  async request<T>(
    command: string,
    payload?: Record<string, unknown>,
  ) {
    try {
      return await invokeTauriCommand<T>(command, payload);
    } catch (error) {
      throw toApiError(error, `COMMAND_${command.toUpperCase()}_FAILED`);
    }
  }
}

export const apiClient = new ApiClient();
