import type { InfrastructureGraphNode } from '@/types/models';

export type InfrastructureDomain = InfrastructureGraphNode['domain'];

export const INFRASTRUCTURE_DOMAIN_ORDER: InfrastructureDomain[] = [
  'environment',
  'storage',
  'analysis',
];
