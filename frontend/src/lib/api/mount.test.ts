import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  getMountStatus,
  listMounts,
  mountImage,
  mountPhysicalImage,
  unmountImage,
} from './mount';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('mount API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('mountImage sends the real mount request through the command client', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    const request = { dataSourceId: 'ds-1', partitionIndex: 2, mountPoint: 'M:' };

    await mountImage(request);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.mount.MOUNT_IMAGE, { request });
  });

  it('unmountImage sends the mount id without local state changes', async () => {
    requestMock.mockResolvedValueOnce(undefined as never);

    await unmountImage('mount-1');

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.mount.UNMOUNT_IMAGE, { mountId: 'mount-1' });
  });

  it('mountPhysicalImage sends only the data source identity', async () => {
    requestMock.mockResolvedValueOnce({} as never);
    const request = { dataSourceId: 'ds-1' };

    await mountPhysicalImage(request);

    expect(requestMock).toHaveBeenCalledWith(COMMANDS.mount.MOUNT_PHYSICAL_IMAGE, { request });
  });

  it('queries one mount status and the complete active mount list', async () => {
    requestMock.mockResolvedValueOnce({} as never).mockResolvedValueOnce([] as never);

    await getMountStatus('mount-1');
    await listMounts();

    expect(requestMock).toHaveBeenNthCalledWith(1, COMMANDS.mount.GET_MOUNT_STATUS, { mountId: 'mount-1' });
    expect(requestMock).toHaveBeenNthCalledWith(2, COMMANDS.mount.LIST_MOUNTS);
  });
});
