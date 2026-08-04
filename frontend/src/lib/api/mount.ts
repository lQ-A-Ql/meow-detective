import { COMMANDS } from '@/lib/api/commands';
import { apiClient } from '@/lib/api/client';
import type { MountImageRequest, MountPhysicalImageRequest, MountStatus } from '@/types/models';

export async function mountImage(request: MountImageRequest): Promise<MountStatus> {
  return apiClient.request<MountStatus>(COMMANDS.mount.MOUNT_IMAGE, { request });
}

export async function mountPhysicalImage(
  request: MountPhysicalImageRequest,
): Promise<MountStatus> {
  return apiClient.request<MountStatus>(COMMANDS.mount.MOUNT_PHYSICAL_IMAGE, { request });
}

export async function unmountImage(mountId: string): Promise<void> {
  return apiClient.request(COMMANDS.mount.UNMOUNT_IMAGE, { mountId });
}

export async function getMountStatus(mountId: string): Promise<MountStatus> {
  return apiClient.request<MountStatus>(COMMANDS.mount.GET_MOUNT_STATUS, { mountId });
}

export async function listMounts(): Promise<MountStatus[]> {
  return apiClient.request<MountStatus[]>(COMMANDS.mount.LIST_MOUNTS);
}
