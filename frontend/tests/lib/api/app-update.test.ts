import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  request: vi.fn(),
  channels: [] as Array<{ onmessage?: unknown }>,
}));

vi.mock('@/lib/api/client', () => ({
  apiClient: { request: mocks.request },
  Channel: class MockChannel {
    onmessage?: unknown;
    constructor() {
      mocks.channels.push(this);
    }
  },
}));

import { COMMANDS } from '@/lib/api/commands';
import { checkAppUpdate, getAppUpdateInfo, installAppUpdate } from '@/lib/api/app-update';
import type { AppUpdateInfo, AppUpdateProgress } from '@/types/appUpdate';

const availableInfo: AppUpdateInfo = {
  currentVersion: '0.1.0',
  configured: true,
  portable: true,
  available: { version: '0.2.0', notes: 'Release notes', schemaCompatible: true },
};

const currentInfo: AppUpdateInfo = {
  currentVersion: '0.1.0',
  configured: true,
  portable: true,
};

describe('app-update API', () => {
  beforeEach(() => {
    mocks.request.mockReset();
    mocks.channels.length = 0;
  });

  it('getAppUpdateInfo requests the info command without a payload', async () => {
    mocks.request.mockResolvedValueOnce(currentInfo);

    await expect(getAppUpdateInfo()).resolves.toEqual(currentInfo);
    expect(mocks.request).toHaveBeenCalledWith(COMMANDS.appUpdate.INFO);
  });

  it('checkAppUpdate requests the check command without a payload', async () => {
    mocks.request.mockResolvedValueOnce(availableInfo);

    await expect(checkAppUpdate()).resolves.toEqual(availableInfo);
    expect(mocks.request).toHaveBeenCalledWith(COMMANDS.appUpdate.CHECK);
  });

  it('installAppUpdate forwards the version and a progress channel', async () => {
    mocks.request.mockResolvedValueOnce(undefined);
    const onProgress = vi.fn();

    await expect(installAppUpdate('0.2.0', onProgress)).resolves.toBeUndefined();

    expect(mocks.request).toHaveBeenCalledTimes(1);
    expect(mocks.request).toHaveBeenCalledWith(COMMANDS.appUpdate.INSTALL, {
      version: '0.2.0',
      progress: mocks.channels[0],
    });
    expect(mocks.channels[0].onmessage).toBe(onProgress);
  });

  it('delivers progress messages through the channel handler', () => {
    mocks.request.mockResolvedValueOnce(undefined);
    const onProgress = vi.fn();
    void installAppUpdate('0.2.0', onProgress);

    const progress: AppUpdateProgress = {
      phase: 'verifying',
      downloadedBytes: 512,
      totalBytes: 1024,
    };
    (mocks.channels[0].onmessage as (value: AppUpdateProgress) => void)(progress);

    expect(onProgress).toHaveBeenCalledWith(progress);
  });

  it('creates a fresh channel for every install call', async () => {
    mocks.request.mockResolvedValue(undefined);

    await installAppUpdate('0.2.0', vi.fn());
    await installAppUpdate('0.2.0', vi.fn());

    expect(mocks.channels).toHaveLength(2);
    expect(mocks.channels[0]).not.toBe(mocks.channels[1]);
  });

  it('propagates install request failures', async () => {
    mocks.request.mockRejectedValueOnce(new Error('transport offline'));

    await expect(installAppUpdate('0.2.0', vi.fn())).rejects.toThrow('transport offline');
  });
});
