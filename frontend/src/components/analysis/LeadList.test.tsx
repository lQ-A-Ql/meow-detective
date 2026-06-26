import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { LeadCard, CorrelationFamilyCoveragePanel } from './LeadList';
import type { CorrelationFamilyCoverage, CorrelationLead } from '@/types/models';

function makeLead(overrides: Partial<CorrelationLead> = {}): CorrelationLead {
  return {
    id: 'lead-1',
    title: 'Suspicious Executable',
    summary: 'An executable found in temp folder',
    confidence: 'high',
    primaryFileId: 'file-1',
    supportingNodeIds: ['node-1', 'node-2'],
    families: ['registry', 'browser'],
    matchSignals: ['Signal A', 'Signal B'],
    jumps: [{ route: '/timeline', targetId: 'evt-1', label: 'View Timeline' }],
    provenance: [
      {
        sourceKind: 'artifact',
        sourceRecordId: 'art-1',
        sourceLabel: 'Prefetch',
        guaranteeLevel: 'strong',
        producer: 'artifacts-windows',
        warningSummary: [],
      },
    ],
    caveats: ['Needs manual review'],
    ...overrides,
  } as CorrelationLead;
}

function makeFamilyCoverage(overrides: Partial<CorrelationFamilyCoverage> = {}): CorrelationFamilyCoverage {
  return {
    family: 'registry',
    displayName: 'Registry Artifacts',
    status: 'covered',
    leadCount: 3,
    highConfidenceLeadCount: 1,
    reviewLeadCount: 1,
    clusterCount: 2,
    sampleSignals: ['SAM user found', 'Run key detected'],
    ...overrides,
  } as CorrelationFamilyCoverage;
}

describe('LeadCard', () => {
  it('renders lead title and summary', () => {
    const lead = makeLead();
    render(createElement(LeadCard, { lead, selected: false, onJump: vi.fn(), onSelect: vi.fn() }));
    expect(screen.getByText('Suspicious Executable')).toBeDefined();
    expect(screen.getByText('An executable found in temp folder')).toBeDefined();
  });

  it('renders caveats when present', () => {
    const lead = makeLead();
    render(createElement(LeadCard, { lead, selected: false, onJump: vi.fn(), onSelect: vi.fn() }));
    expect(screen.getByText('Needs manual review')).toBeDefined();
  });

  it('calls onSelect when clicked', () => {
    const onSelect = vi.fn();
    const lead = makeLead();
    render(createElement(LeadCard, { lead, selected: false, onJump: vi.fn(), onSelect }));
    fireEvent.click(screen.getByText('Suspicious Executable'));
    expect(onSelect).toHaveBeenCalledOnce();
  });
});

describe('CorrelationFamilyCoveragePanel', () => {
  it('renders family items with display names', () => {
    const items = [makeFamilyCoverage()];
    render(createElement(CorrelationFamilyCoveragePanel, { items }));
    expect(screen.getByText('规则家族覆盖')).toBeDefined();
    expect(screen.getByText('Registry Artifacts')).toBeDefined();
  });

  it('renders sample signals', () => {
    const items = [makeFamilyCoverage()];
    render(createElement(CorrelationFamilyCoveragePanel, { items }));
    expect(screen.getByText('SAM user found')).toBeDefined();
    expect(screen.getByText('Run key detected')).toBeDefined();
  });

  it('renders empty signal fallback when no sample signals', () => {
    const items = [makeFamilyCoverage({ sampleSignals: [] })];
    render(createElement(CorrelationFamilyCoveragePanel, { items }));
    expect(screen.getByText('当前没有可展示的该家族命中信号。')).toBeDefined();
  });
});
