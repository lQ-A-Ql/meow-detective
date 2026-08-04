export type MountState = 'preparing' | 'mounted' | 'unmounting' | 'released' | 'failed';

export interface MountImageRequest {
  dataSourceId: string;
  partitionIndex: number;
  mountPoint?: string | null;
}

export interface MountPhysicalImageRequest {
  dataSourceId: string;
}

export type MountMode = 'logicalPartition' | 'physicalDisk';

export interface MountTarget {
  mountId: string;
  dataSourceId: string;
  partitionIndex: number;
  filesystem: string;
  mountPoint: string;
  readOnly: boolean;
  mode: MountMode;
  physicalDevicePath?: string;
  targetAddress?: string;
}

export interface MountStatus {
  target: MountTarget;
  state: MountState;
  activeHandleCount: number;
  error?: string;
}
