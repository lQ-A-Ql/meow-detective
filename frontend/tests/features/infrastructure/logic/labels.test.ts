import { describe, expect, it } from 'vitest';
import type { TFunction } from 'i18next';
import {
  domainLabel,
  groupNodesByDomain,
  kindLabel,
  matchesInfrastructureQuery,
  relationLabel,
  stateLabel,
} from '@/features/infrastructure/logic/labels';
import { INFRASTRUCTURE_DOMAIN_ORDER } from '@/features/infrastructure/types';
import type { InfrastructureGraphNode } from '@/types/models';

const node = (overrides: Partial<InfrastructureGraphNode> = {}): InfrastructureGraphNode => ({
  id: 'node-1',
  domain: 'environment',
  kind: 'physical_host',
  name: 'node-1',
  status: 'ready',
  confidence: 'candidate',
  provenanceJson: '{}',
  ...overrides,
});

/** Mirrors i18next's missing-key behavior: the key comes back unchanged. */
const echo = ((key: string) => key) as unknown as TFunction;
const translator = (translations: Record<string, string>) =>
  ((key: string) => translations[key] ?? key) as unknown as TFunction;

describe('groupNodesByDomain', () => {
  it('keeps every domain present and ordered even when no node matches', () => {
    const grouped = groupNodesByDomain([]);

    expect(Object.keys(grouped)).toEqual(INFRASTRUCTURE_DOMAIN_ORDER);
    expect(grouped.environment).toEqual([]);
    expect(grouped.storage).toEqual([]);
    expect(grouped.analysis).toEqual([]);
  });

  it('splits nodes by domain while preserving graph order', () => {
    const grouped = groupNodesByDomain([
      node({ id: 'vm-2', domain: 'environment' }),
      node({ id: 'ceph', domain: 'storage', kind: 'ceph_cluster' }),
      node({ id: 'host-1', domain: 'environment' }),
      node({ id: 'log', domain: 'analysis', kind: 'audit_log' }),
    ]);

    expect(grouped.environment.map((item) => item.id)).toEqual(['vm-2', 'host-1']);
    expect(grouped.storage.map((item) => item.id)).toEqual(['ceph']);
    expect(grouped.analysis.map((item) => item.id)).toEqual(['log']);
  });
});

describe('matchesInfrastructureQuery', () => {
  it('treats an empty or whitespace-only query as matching everything', () => {
    expect(matchesInfrastructureQuery(node(), '', echo)).toBe(true);
    expect(matchesInfrastructureQuery(node(), '   ', echo)).toBe(true);
  });

  it('matches ids and names case-insensitively after trimming', () => {
    const value = node({ id: 'host-9', name: 'PVE Node A' });

    expect(matchesInfrastructureQuery(value, '  HOST-9 ', echo)).toBe(true);
    expect(matchesInfrastructureQuery(value, 'pve node', echo)).toBe(true);
    expect(matchesInfrastructureQuery(value, 'node b', echo)).toBe(false);
  });

  it('matches a translated domain label', () => {
    const t = translator({ 'infrastructure.domains.environment': '运行环境' });

    expect(matchesInfrastructureQuery(node({ name: 'x', id: 'x', kind: 'unknown_kind' }), '运行环境', t)).toBe(true);
  });

  it('matches a humanized kind label when no translation exists', () => {
    expect(matchesInfrastructureQuery(node({ name: 'x', id: 'x' }), 'physical host', echo)).toBe(true);
    expect(matchesInfrastructureQuery(node({ name: 'x', id: 'x' }), 'physical host', translator({
      'infrastructure.kinds.physical_host': '物理主机',
    }))).toBe(false);
  });

  it('rejects a query absent from name, id, domain label and kind label', () => {
    expect(matchesInfrastructureQuery(node({ kind: 'audit_log' }), 'nonexistent-object', echo)).toBe(false);
  });
});

describe('label helpers', () => {
  it('returns the domain translation verbatim, without humanizing a missing key', () => {
    expect(domainLabel('storage', echo)).toBe('infrastructure.domains.storage');
    expect(domainLabel('storage', translator({ 'infrastructure.domains.storage': '存储基础' }))).toBe('存储基础');
  });

  it('humanizes kinds only when the translation is missing', () => {
    expect(kindLabel('physical_host', echo)).toBe('Physical Host');
    expect(kindLabel('pve', echo)).toBe('Pve');
    expect(kindLabel('virtual_machine', translator({ 'infrastructure.kinds.virtual_machine': '虚拟机' }))).toBe('虚拟机');
  });

  it('humanizes relations only when the translation is missing', () => {
    expect(relationLabel('network_link', echo)).toBe('Network Link');
    expect(relationLabel('derived_from', translator({ 'infrastructure.relations.derived_from': '派生于' }))).toBe('派生于');
  });

  it('humanizes states only when the translation is missing', () => {
    expect(stateLabel('ready', echo)).toBe('Ready');
    expect(stateLabel('candidate', translator({ 'infrastructure.states.candidate': '候选' }))).toBe('候选');
  });
});
