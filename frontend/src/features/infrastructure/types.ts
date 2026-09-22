import type { InfrastructureGraphNode } from '@/types/models';

export type InfrastructureDomain = InfrastructureGraphNode['domain'];

export const INFRASTRUCTURE_DOMAIN_ORDER: InfrastructureDomain[] = [
  'environment',
  'storage',
  'analysis',
];

export interface InfrastructureHostFact {
  hostname?: string;
  operatingSystem?: string;
  operatingSystemVersion?: string;
  kernelVersion?: string;
}
