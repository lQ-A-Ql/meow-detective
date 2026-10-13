import type { RegistryHiveDescriptor, RegistryKeyMetadata, RegistrySubkeyPage, RegistryValueMetadata } from '@/types/registryBrowser';

export interface RegistryBrowserContext { caseId: string; dataSourceId: string; sourceRevision: string; }
export type RegistryDetailTab = 'decoded' | 'hex' | 'provenance';
export interface RegistryTreePage { keys: RegistryKeyMetadata[]; page: RegistrySubkeyPage; }
export interface RegistryTreeNode {
  id: string; domId: string; label: string; depth: number; expanded: boolean; hasChildren: boolean;
  position: number; setSize: number; hive: RegistryHiveDescriptor; key?: RegistryKeyMetadata;
  children: RegistryTreeNode[]; loading: boolean; error?: string; nextCursor?: string;
}
export interface RegistryValueRow extends RegistryValueMetadata { displayName: string; displayPreview: string; displayLength: string; }
