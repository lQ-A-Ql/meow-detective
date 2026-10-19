import { invoke } from '@tauri-apps/api/core';
export { Channel } from '@tauri-apps/api/core';
import i18n from '@/i18n';
import { ApiErrorDto } from '@/types/models';
import { isApiErrorDto } from '@/lib/errors';

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
    message: i18n.t('common.errors.unknownApi'),
    category: 'internal',
    details: error,
    recoverable: true,
  };
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
