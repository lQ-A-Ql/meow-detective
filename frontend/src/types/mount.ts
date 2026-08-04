export type MountState = 'preparing' | 'mounted' | 'unmounting' | 'released' | 'failed';

export interface MountImageRequest {
  dataSourceId: string;
  partitionIndex: number;
  mountPoint?: string | null;
}

export interface MountTarget {
  mountId: string;
  dataSourceId: string;
  partitionIndex: number;
  filesystem: string;
  mountPoint: string;
  readOnly: boolean;
}

export interface MountStatus {
  target: MountTarget;
  state: MountState;
  activeHandleCount: number;
  error?: string;
}
