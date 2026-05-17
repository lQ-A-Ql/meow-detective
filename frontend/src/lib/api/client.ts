import { invoke } from '@tauri-apps/api/core';
import { ApiErrorDto, ApiMode } from '@/types/models';
import { ApiProvider, mockProvider } from './provider';

function getApiMode(): ApiMode {
  return import.meta.env.VITE_API_MODE === 'tauri' ? 'tauri' : 'mock';
}

function toApiError(error: unknown, fallbackCode: string): ApiErrorDto {
  if (isApiErrorDto(error)) {
    return error;
  }

  if (typeof error === 'string') {
    return {
      code: fallbackCode,
      message: error,
      recoverable: true,
    };
  }

  if (error instanceof Error) {
    return {
      code: fallbackCode,
      message: error.message,
      recoverable: true,
    };
  }

  return {
    code: fallbackCode,
    message: '未知接口错误',
    details: error,
    recoverable: true,
  };
}

function isApiErrorDto(value: unknown): value is ApiErrorDto {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const candidate = value as Partial<ApiErrorDto>;
  return typeof candidate.code === 'string' && typeof candidate.message === 'string' && typeof candidate.recoverable === 'boolean';
}

async function invokeTauriCommand<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, payload);
}

class ApiClient {
  readonly mode = getApiMode();

  private provider: ApiProvider = mockProvider;

  async request<T>(command: string, mockCall: () => Promise<T>, payload?: Record<string, unknown>) {
    if (this.mode === 'mock') {
      return mockCall();
    }

    try {
      return await invokeTauriCommand<T>(command, payload);
    } catch (error) {
      throw toApiError(error, `COMMAND_${command.toUpperCase()}_FAILED`);
    }
  }

  getMockProvider() {
    return this.provider;
  }
}

export const apiClient = new ApiClient();
export const apiMode = apiClient.mode;
