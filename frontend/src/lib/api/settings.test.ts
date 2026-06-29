import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import { getAppSettings, saveAppSettings } from './settings';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('settings API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getAppSettings calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce({
      language: 'en',
      recentCases: [],
    } as never);
    const result = await getAppSettings();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.settings.GET_APP_SETTINGS);
    expect(result).toEqual({
      language: 'en',
      recentCases: [],
    });
  });

  it('saveAppSettings sends settings in payload', async () => {
    const settings = {
      language: 'zh' as const,
      recentCases: ['/cases/test'],
    };
    requestMock.mockResolvedValueOnce(settings as never);
    const result = await saveAppSettings(settings as never);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.settings.SAVE_APP_SETTINGS, {
      settings,
    });
    expect(result).toEqual(settings);
  });
});
