import { describe, expect, it } from 'vitest';
import { infrastructureInspectorModel } from '@/features/infrastructure/logic/inspector-model';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';

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

const edge = (sourceId: string, targetId: string): InfrastructureGraphEdge => ({
  sourceId,
  targetId,
  relationKind: 'boots',
  confidence: 'candidate',
  provenanceJson: '{}',
});

describe('infrastructureInspectorModel', () => {
  it('reads the parser name from object provenance', () => {
    const result = infrastructureInspectorModel(
      node({ provenanceJson: JSON.stringify({ parser: 'linux.network.config.v1', dataSourceId: 'source-1' }) }),
      [],
    );

    expect(result.parser).toBe('linux.network.config.v1');
  });

  it.each([
    ['malformed JSON', '{not-json'],
    ['array provenance', '["linux.network.config.v1"]'],
    ['scalar provenance', '"parser"'],
    ['null provenance', 'null'],
    ['non-string parser', '{"parser":42}'],
    ['missing parser key', '{"dataSourceId":"source-1"}'],
  ])('does not fabricate a parser for %s', (_label, provenanceJson) => {
    expect(infrastructureInspectorModel(node({ provenanceJson }), []).parser).toBeUndefined();
  });

  it('skips provenance above the 64 KiB inspection budget even when it is valid JSON', () => {
    const valid = JSON.stringify({ parser: 'linux.network.config.v1' });
    const oversized = valid + ' '.repeat(64 * 1024 + 1 - valid.length);

    expect(oversized.length).toBe(64 * 1024 + 1);
    expect(infrastructureInspectorModel(node({ provenanceJson: oversized }), []).parser).toBeUndefined();
  });

  it('still parses provenance at exactly the 64 KiB boundary', () => {
    const valid = JSON.stringify({ parser: 'linux.network.config.v1' });
    const exact = valid + ' '.repeat(64 * 1024 - valid.length);

    expect(exact.length).toBe(64 * 1024);
    expect(infrastructureInspectorModel(node({ provenanceJson: exact }), []).parser).toBe('linux.network.config.v1');
  });

  it('counts every edge touching the node and ignores unrelated edges', () => {
    const result = infrastructureInspectorModel(node({ id: 'host-a' }), [
      edge('host-a', 'vm-1'),
      edge('vm-2', 'host-a'),
      edge('host-a', 'host-a'),
      edge('vm-1', 'vm-2'),
    ]);

    // A self-loop touches the node once, not twice.
    expect(result.linkedCount).toBe(3);
    expect(infrastructureInspectorModel(node({ id: 'unlinked' }), [edge('vm-1', 'vm-2')]).linkedCount).toBe(0);
  });

  it.each(['ready', 'complete', 'verified'])('uses the default badge variant for %s evidence', (status) => {
    expect(infrastructureInspectorModel(node({ status }), []).statusVariant).toBe('default');
  });

  it.each(['candidate', 'partial', 'failed', 'parsed', 'Ready'])(
    'keeps %s evidence on the outline badge variant',
    (status) => {
      expect(infrastructureInspectorModel(node({ status }), []).statusVariant).toBe('outline');
    },
  );
});
