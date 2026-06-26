import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { LeadDetailPanel } from './LeadDetail';
import type { CorrelationCluster, CorrelationLead, CorrelationSnapshot } from '@/types/models';

function makeLead(overrides: Partial<CorrelationLead> = {}): CorrelationLead {
  return {
    id: 'lead-1',
    title: 'Malware Beacon Detected',
    summary: 'A C2 beacon pattern identified from network artifacts',
    confidence: 'high',
    primaryFileId: 'file-1',
    supportingNodeIds: ['node-1'],
    families: ['browser'],
    matchSignals: ['Periodic DNS query to known C2 domain'],
    jumps: [{ route: '/timeline', targetId: 'evt-1', label: 'View Timeline' }],
    provenance: [
      {
        sourceKind: 'artifact',
        sourceRecordId: 'art-1',
        sourceLabel: 'Browser History',
        guaranteeLevel: 'strong',
        producer: 'artifacts-windows',
        warningSummary: [],
      },
    ],
    caveats: ['May be false positive'],
    ...overrides,
  } as CorrelationLead;
}

function makeNode(overrides: Partial<CorrelationSnapshot['nodes'][number]> = {}): CorrelationSnapshot['nodes'][number] {
  return {
    id: 'node-1',
    kind: 'file',
    title: 'malware.exe',
    subtitle: 'C:\\Users\\test\\malware.exe',
    badges: ['executed', 'suspicious'],
    jumps: [],
    ...overrides,
  } as CorrelationSnapshot['nodes'][number];
}

describe('LeadDetailPanel', () => {
  it('renders lead title and summary', () => {
    const lead = makeLead();
    render(
      createElement(LeadDetailPanel, {
        lead,
        primaryFileNode: undefined,
        supportingNodes: [],
        edges: [],
        relatedClusters: [],
        onJump: vi.fn(),
      }),
    );
    expect(screen.getByText('Lead 明细')).toBeDefined();
    expect(screen.getByText('Malware Beacon Detected')).toBeDefined();
    expect(screen.getByText('May be false positive')).toBeDefined();
  });

  it('renders primary file node when provided', () => {
    const lead = makeLead();
    const node = makeNode();
    render(
      createElement(LeadDetailPanel, {
        lead,
        primaryFileNode: node,
        supportingNodes: [],
        edges: [],
        relatedClusters: [],
        onJump: vi.fn(),
      }),
    );
    expect(screen.getByText('malware.exe')).toBeDefined();
    expect(screen.getByText('主文件节点')).toBeDefined();
  });

  it('renders related clusters', () => {
    const lead = makeLead();
    const cluster = {
      id: 'cl-1',
      title: 'Network C2 Cluster',
      summary: 'Grouped C2 communications',
      confidence: 'medium',
      leadIds: ['lead-1'],
      families: ['browser'],
      primaryFileId: 'file-1',
      supportingNodeIds: [],
      provenance: [],
      caveats: [],
    } as unknown as CorrelationCluster;
    render(
      createElement(LeadDetailPanel, {
        lead,
        primaryFileNode: undefined,
        supportingNodes: [],
        edges: [],
        relatedClusters: [cluster],
        onJump: vi.fn(),
      }),
    );
    expect(screen.getByText('Network C2 Cluster')).toBeDefined();
  });
});
