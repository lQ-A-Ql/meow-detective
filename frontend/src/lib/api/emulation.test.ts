import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  getEmulationStatus,
  launchEmulation,
  listEmulationSessions,
  prepareEmulation,
  releaseEmulation,
} from './emulation';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('emulation API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('prepares an emulation session with the selected WinPE ISO', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    const request = {
      dataSourceId: 'source-1',
      recoveryIsoPath: 'C:\\Tools\\WinPE.iso',
      allowDirectBoot: false,
    };

    await prepareEmulation(request);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.emulation.PREPARE, { request });
  });

  it('routes lifecycle calls through the registered command names', async () => {
    requestMock
      .mockResolvedValueOnce({} as never)
      .mockResolvedValueOnce({} as never)
      .mockResolvedValueOnce([] as never)
      .mockResolvedValueOnce({} as never);

    await launchEmulation('emulation-1');
    await getEmulationStatus('emulation-1');
    await listEmulationSessions();
    await releaseEmulation('emulation-1');

    expect(requestMock).toHaveBeenNthCalledWith(1, COMMANDS.emulation.LAUNCH, {
      sessionId: 'emulation-1',
    });
    expect(requestMock).toHaveBeenNthCalledWith(2, COMMANDS.emulation.GET_STATUS, {
      sessionId: 'emulation-1',
    });
    expect(requestMock).toHaveBeenNthCalledWith(3, COMMANDS.emulation.LIST_SESSIONS);
    expect(requestMock).toHaveBeenNthCalledWith(4, COMMANDS.emulation.RELEASE, {
      sessionId: 'emulation-1',
    });
  });
});
