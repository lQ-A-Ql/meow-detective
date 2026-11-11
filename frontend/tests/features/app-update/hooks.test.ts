import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  checkAppUpdate: vi.fn(),
  getAppUpdateInfo: vi.fn(),
  installAppUpdate: vi.fn(),
}));

vi.mock('@/lib/api/app-update', () => ({
  checkAppUpdate: mocks.checkAppUpdate,
  getAppUpdateInfo: mocks.getAppUpdateInfo,
  installAppUpdate: mocks.installAppUpdate,
}));

import { UPDATE_CHECK_INTERVAL_MS, UPDATE_STORAGE_KEY } from '@/design/app-update';
import { useUpdateStore } from '@/features/app-update/hooks';
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

const unconfiguredInfo: AppUpdateInfo = {
  currentVersion: '0.1.0',
  configured: false,
  portable: true,
};

function resetStore() {
  useUpdateStore.setState({
    info: undefined,
    phase: 'idle',
    progress: undefined,
    errorKey: undefined,
  });
}

async function loadFreshHooks() {
  vi.resetModules();
  return import('@/features/app-update/hooks');
}

describe('app-update hooks', () => {
  beforeEach(() => {
    mocks.checkAppUpdate.mockReset();
    mocks.getAppUpdateInfo.mockReset();
    mocks.installAppUpdate.mockReset();
    localStorage.clear();
    resetStore();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  describe('check', () => {
    it('moves to available when a compatible release exists', async () => {
      mocks.checkAppUpdate.mockResolvedValue(availableInfo);

      await useUpdateStore.getState().check();

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(useUpdateStore.getState()).toMatchObject({
        info: availableInfo,
        phase: 'available',
        errorKey: undefined,
      });
    });

    it('moves to current when configured without a release', async () => {
      mocks.checkAppUpdate.mockResolvedValue(currentInfo);

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState().info).toEqual(currentInfo);
      expect(useUpdateStore.getState().phase).toBe('current');
    });

    it('stays idle when updates are not configured', async () => {
      mocks.checkAppUpdate.mockResolvedValue(unconfiguredInfo);

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState()).toMatchObject({
        info: unconfiguredInfo,
        phase: 'idle',
      });
    });

    it('clears a previous error key when a new check succeeds', async () => {
      useUpdateStore.setState({ phase: 'failed', errorKey: 'appUpdate.errors.network' });
      mocks.checkAppUpdate.mockResolvedValue(currentInfo);

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState().phase).toBe('current');
      expect(useUpdateStore.getState().errorKey).toBeUndefined();
    });

    it('maps a known failure code to its copy key', async () => {
      mocks.checkAppUpdate.mockRejectedValue({ code: 'UPDATE_NETWORK' });

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'failed',
        errorKey: 'appUpdate.errors.network',
      });
    });

    it('falls back to the generic error key for unknown failures', async () => {
      mocks.checkAppUpdate.mockRejectedValue({ code: 'UPDATE_UNLISTED' });

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'failed',
        errorKey: 'appUpdate.errors.failed',
      });
    });

    it('falls back to the generic error key for non-object failures', async () => {
      mocks.checkAppUpdate.mockRejectedValue('offline');

      await useUpdateStore.getState().check();

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'failed',
        errorKey: 'appUpdate.errors.failed',
      });
    });

    it('does not surface errors for automatic checks', async () => {
      mocks.checkAppUpdate.mockRejectedValue({ code: 'UPDATE_NETWORK' });

      await useUpdateStore.getState().check(true);

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'idle',
        errorKey: undefined,
      });
    });

    it('ignores checks while another update phase is busy', async () => {
      useUpdateStore.setState({ phase: 'downloading' });

      await useUpdateStore.getState().check();

      expect(mocks.checkAppUpdate).not.toHaveBeenCalled();
      expect(useUpdateStore.getState().phase).toBe('downloading');
    });

    it('skips an automatic check inside the configured interval', async () => {
      const checkedAt = Date.now();
      localStorage.setItem(UPDATE_STORAGE_KEY, String(checkedAt));
      useUpdateStore.setState({ info: currentInfo, phase: 'current' });

      await useUpdateStore.getState().check(true);

      expect(mocks.checkAppUpdate).not.toHaveBeenCalled();
      expect(useUpdateStore.getState()).toMatchObject({ info: currentInfo, phase: 'current' });
      expect(localStorage.getItem(UPDATE_STORAGE_KEY)).toBe(String(checkedAt));
    });

    it('runs an automatic check and records the timestamp when none exists', async () => {
      vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000);
      mocks.checkAppUpdate.mockResolvedValue(availableInfo);

      await useUpdateStore.getState().check(true);

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(localStorage.getItem(UPDATE_STORAGE_KEY)).toBe('1700000000000');
      expect(useUpdateStore.getState().phase).toBe('available');
    });

    it('runs an automatic check after the interval elapsed', async () => {
      const checkedAt = Date.now() - UPDATE_CHECK_INTERVAL_MS - 1_000;
      localStorage.setItem(UPDATE_STORAGE_KEY, String(checkedAt));
      mocks.checkAppUpdate.mockResolvedValue(currentInfo);

      await useUpdateStore.getState().check(true);

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(useUpdateStore.getState().phase).toBe('current');
    });

    it('runs an automatic check when the stored timestamp is in the future', async () => {
      localStorage.setItem(UPDATE_STORAGE_KEY, String(Date.now() + 60_000));
      mocks.checkAppUpdate.mockResolvedValue(currentInfo);

      await useUpdateStore.getState().check(true);

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
    });

    it('still checks automatically when storage access throws', async () => {
      vi.stubGlobal('localStorage', {
        getItem: () => {
          throw new Error('storage blocked');
        },
        setItem: vi.fn(),
      });
      mocks.checkAppUpdate.mockResolvedValue(currentInfo);

      await useUpdateStore.getState().check(true);

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(useUpdateStore.getState().phase).toBe('current');
    });
  });

  describe('install', () => {
    it('does nothing without update info', async () => {
      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
      expect(useUpdateStore.getState().phase).toBe('idle');
    });

    it('does nothing when no release is available', async () => {
      useUpdateStore.setState({ info: currentInfo });

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
    });

    it('refuses a release that is not schema compatible', async () => {
      useUpdateStore.setState({
        info: {
          ...availableInfo,
          available: { ...availableInfo.available!, schemaCompatible: false },
        },
      });

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
    });

    it('refuses installation for a non-portable build', async () => {
      useUpdateStore.setState({ info: { ...availableInfo, portable: false } });

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
    });

    it('refuses installation when updates are not configured', async () => {
      useUpdateStore.setState({ info: { ...availableInfo, configured: false } });

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
    });

    it('ignores installs while another update phase is busy', async () => {
      useUpdateStore.setState({ info: availableInfo, phase: 'verifying' });

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).not.toHaveBeenCalled();
      expect(useUpdateStore.getState().phase).toBe('verifying');
    });

    it('drives the phase from progress events and finishes restarting', async () => {
      const phases: string[] = [];
      let progressBeforeFirstEvent: AppUpdateProgress | undefined | 'unset' = 'unset';
      useUpdateStore.setState({
        info: availableInfo,
        phase: 'available',
        errorKey: 'stale',
        progress: { phase: 'stale', downloadedBytes: 1 },
      });
      mocks.installAppUpdate.mockImplementation(
        async (_version: string, onProgress: (progress: AppUpdateProgress) => void) => {
          progressBeforeFirstEvent = useUpdateStore.getState().progress;
          onProgress({ phase: 'downloading', downloadedBytes: 10, totalBytes: 100 });
          phases.push(useUpdateStore.getState().phase);
          onProgress({ phase: 'verifying', downloadedBytes: 50, totalBytes: 100 });
          phases.push(useUpdateStore.getState().phase);
          onProgress({ phase: 'restarting', downloadedBytes: 100, totalBytes: 100 });
          phases.push(useUpdateStore.getState().phase);
          onProgress({ phase: 'extracting', downloadedBytes: 100, totalBytes: 100 });
          phases.push(useUpdateStore.getState().phase);
        },
      );

      await useUpdateStore.getState().install();

      expect(mocks.installAppUpdate).toHaveBeenCalledWith('0.2.0', expect.any(Function));
      expect(progressBeforeFirstEvent).toBeUndefined();
      expect(phases).toEqual(['downloading', 'verifying', 'restarting', 'downloading']);
      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'restarting',
        errorKey: undefined,
        progress: { phase: 'extracting', downloadedBytes: 100, totalBytes: 100 },
      });
    });

    it('surfaces a known install failure code', async () => {
      useUpdateStore.setState({ info: availableInfo, phase: 'available' });
      mocks.installAppUpdate.mockRejectedValue({ code: 'UPDATE_SIGNATURE' });

      await useUpdateStore.getState().install();

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'failed',
        errorKey: 'appUpdate.errors.signature',
      });
    });

    it('surfaces a generic install failure for unknown errors', async () => {
      useUpdateStore.setState({ info: availableInfo, phase: 'available' });
      mocks.installAppUpdate.mockRejectedValue(new Error('network down'));

      await useUpdateStore.getState().install();

      expect(useUpdateStore.getState()).toMatchObject({
        phase: 'failed',
        errorKey: 'appUpdate.errors.failed',
      });
    });
  });

  describe('initializeAppUpdates', () => {
    it('fetches update info and runs one automatic check', async () => {
      mocks.getAppUpdateInfo.mockResolvedValue(availableInfo);
      mocks.checkAppUpdate.mockResolvedValue(availableInfo);
      const hooks = await loadFreshHooks();

      await hooks.initializeAppUpdates();
      await hooks.initializeAppUpdates();

      expect(mocks.getAppUpdateInfo).toHaveBeenCalledTimes(1);
      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(hooks.useUpdateStore.getState()).toMatchObject({
        info: availableInfo,
        phase: 'available',
      });
    });

    it('stores unconfigured info without running a check', async () => {
      mocks.getAppUpdateInfo.mockResolvedValue(unconfiguredInfo);
      const hooks = await loadFreshHooks();

      await hooks.initializeAppUpdates();

      expect(mocks.checkAppUpdate).not.toHaveBeenCalled();
      expect(hooks.useUpdateStore.getState()).toMatchObject({
        info: unconfiguredInfo,
        phase: 'idle',
      });
    });

    it('keeps startup working when update info cannot be read', async () => {
      mocks.getAppUpdateInfo.mockRejectedValue(new Error('no transport'));
      const hooks = await loadFreshHooks();

      await expect(hooks.initializeAppUpdates()).resolves.toBeUndefined();

      expect(mocks.checkAppUpdate).not.toHaveBeenCalled();
      const state = hooks.useUpdateStore.getState();
      expect(state.info).toBeUndefined();
      expect(state.phase).toBe('idle');
    });

    it('keeps startup working when the automatic check fails', async () => {
      mocks.getAppUpdateInfo.mockResolvedValue(currentInfo);
      mocks.checkAppUpdate.mockRejectedValue({ code: 'UPDATE_NETWORK' });
      const hooks = await loadFreshHooks();

      await expect(hooks.initializeAppUpdates()).resolves.toBeUndefined();

      expect(mocks.checkAppUpdate).toHaveBeenCalledTimes(1);
      expect(hooks.useUpdateStore.getState()).toMatchObject({
        phase: 'idle',
        errorKey: undefined,
      });
    });
  });
});
