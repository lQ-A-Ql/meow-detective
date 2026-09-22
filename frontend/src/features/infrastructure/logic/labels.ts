import type { TFunction } from 'i18next';
import type { InfrastructureGraphNode } from '@/types/models';
import { INFRASTRUCTURE_DOMAIN_ORDER, type InfrastructureDomain } from '../types';

export function groupNodesByDomain(nodes: InfrastructureGraphNode[]) {
  return Object.fromEntries(
    INFRASTRUCTURE_DOMAIN_ORDER.map((domain) => [
      domain,
      nodes.filter((node) => node.domain === domain),
    ]),
  ) as Record<InfrastructureDomain, InfrastructureGraphNode[]>;
}

export function matchesInfrastructureQuery(
  node: InfrastructureGraphNode,
  query: string,
  t: TFunction,
) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  return [node.name, node.id, domainLabel(node.domain, t), kindLabel(node.kind, t)]
    .some((value) => value.toLocaleLowerCase().includes(normalized));
}

export function domainLabel(domain: InfrastructureDomain, t: TFunction) {
  return t(`infrastructure.domains.${domain}`);
}

export function kindLabel(kind: string, t: TFunction) {
  return translatedOrHumanized(`infrastructure.kinds.${kind}`, kind, t);
}

export function relationLabel(relation: string, t: TFunction) {
  return translatedOrHumanized(`infrastructure.relations.${relation}`, relation, t);
}

export function stateLabel(value: string, t: TFunction) {
  return translatedOrHumanized(`infrastructure.states.${value}`, value, t);
}

function translatedOrHumanized(key: string, value: string, t: TFunction) {
  const translated = t(key);
  return translated === key ? humanize(value) : translated;
}

function humanize(value: string) {
  return value.replace(/_/g, ' ').replace(/\b\w/g, (letter: string) => letter.toUpperCase());
}
