import { describe, expect, it } from 'vitest';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvidenceClassificationSummary,
  getRegistryExtractionSummary,
  getSystemInfo,
  getV2GovernanceSnapshot,
  runAnalysisExtraction,
} from '@/lib/api/analysis';
import { getArtifactById, getArtifactRows } from '@/lib/api/artifacts';
import { getFileJumpContext } from '@/lib/api/files';
import { getTimelineEventById, getTimelineEvents } from '@/lib/api/timeline';

describe('analysis API (mock mode)', () => {
  it('getSystemInfo returns parsed registry status without fake facts', async () => {
    const result = await getSystemInfo();

    expect(result.status).toBe('parsed');
    expect(result.computerName).toBe('BETA-LAB');
    expect(result.osVersion).toContain('Windows Evidence Edition');
    expect(result.warnings.length).toBeGreaterThan(0);
    expect(result.provenance.length).toBeGreaterThan(0);
    expect(result.provenance[0].parser).toBeDefined();
    expect(result.fieldProvenance.length).toBeGreaterThan(0);
    expect(result.fieldProvenance[0].field).toBe('computerName');
    expect(result.computerName).not.toBe('FORENSICS-PC');
    expect(result.osVersion).not.toBe('Windows 10');
  });

  it('classifyFiles returns camelCase classification fields', async () => {
    const result = await classifyFiles(1000);

    expect(result.length).toBeGreaterThan(0);
    expect(result[0].totalSize).toBeGreaterThan(0);
    expect(result[0].provenance.length).toBeGreaterThan(0);
    expect(result[0].files[0].fileId).toBeDefined();
    expect(result[0].files[0].magicDescription).toBeDefined();
    expect(result[0].files[0].provenance.parser).toBe('metadata.extension_path');
  });

  it('getEvidenceClassificationSummary includes v1 extraction categories', async () => {
    const result = await getEvidenceClassificationSummary();

    expect(result.categories.some((item) => item.category === 'Registry')).toBe(true);
    expect(result.categories.some((item) => item.category === 'BrowserHistory')).toBe(true);
    expect(result.categories.some((item) => item.category === 'Email')).toBe(true);
    expect(result.totals.candidateFileCount).toBeGreaterThan(0);
  });

  it('runAnalysisExtraction returns camelCase extraction run metrics', async () => {
    const result = await runAnalysisExtraction({
      categories: ['Registry', 'BrowserHistory', 'Email'],
    });

    expect(result.status).toBe('parsed');
    expect(result.scannedCount).toBeGreaterThan(0);
    expect(result.artifactCount).toBeGreaterThan(0);
    expect(result.timelineEventCount).toBeGreaterThan(0);
    expect(result.generatedAt).toBeTruthy();
    expect(Array.isArray(result.warnings)).toBe(true);
  });

  it('returns registry, browser and email extraction summaries in mock mode', async () => {
    const registry = await getRegistryExtractionSummary({ limit: 2 });
    const browser = await getBrowserHistorySummary({ limit: 3 });
    const email = await getEmailExtractionSummary({ limit: 2 });

    expect(registry.status).toBe('notFound');
    expect(registry.total).toBe(0);
    expect(registry.values).toHaveLength(0);

    expect(browser.visitTotal).toBeGreaterThan(0);
    expect(browser.visits.map((item) => item.browser)).toEqual(expect.arrayContaining(['Chrome', 'Edge', 'Firefox']));
    expect(browser.downloads[0].targetPath).toContain('Downloads');

    expect(email.total).toBeGreaterThan(0);
    expect(email.messages[0].from).toContain('@');
    expect(email.messages[0].attachments).toContain('triage.csv');
  });

  it('generateAnalysisSummary returns markdown without hardcoded fake system facts', async () => {
    const result = await generateAnalysisSummary();

    expect(result).toContain('# 数据源分析报告');
    expect(result).toContain('Windows Evidence Edition');
    expect(result).not.toContain('FORENSICS-PC');
    expect(result).not.toContain('Windows 10');
  });

  it('getV2GovernanceSnapshot returns governance runtime and release signals', async () => {
    const result = await getV2GovernanceSnapshot();

    expect(result.generatedAt).toBeTruthy();
    expect(result.verificationChains.length).toBeGreaterThan(0);
    expect(result.supportMatrix.gaCount).toBeGreaterThan(0);
    expect(result.supportMatrixEntries.length).toBeGreaterThan(0);
    expect(result.knownLimitations.length).toBeGreaterThan(0);
    expect(result.benchmark.scenarios.length).toBeGreaterThan(0);
    expect(result.benchmark.requiredChecks.length).toBeGreaterThan(0);
    expect(result.benchmark.coveredRequiredCount).toBeGreaterThan(0);
    expect(result.security.exportPathGuardEnabled).toBe(true);
    expect(result.security.auditEventCount).toBeGreaterThan(0);
    expect(result.security.recentAuditEntries.length).toBeGreaterThan(0);
    expect(result.errorTaxonomyEntries.length).toBeGreaterThan(0);
    expect(result.releaseGates.length).toBeGreaterThan(0);
    expect(result.releaseScorecard.totalScore).toBeGreaterThan(0);
    expect(result.runtimeResults.checkedAt).toBeTruthy();
    expect(result.runtimeResults.checks[0].subChecks.length).toBeGreaterThan(0);
    expect(result.runtimeSignals.dataSourceCount).toBeGreaterThan(0);
    expect(result.runtimeSignals.correlationSnapshotAvailable).toBe(true);
    expect(result.runtimeSignals.correlationLeadCount).toBeGreaterThan(0);
    expect(result.runtimeSignals.correlationHighConfidenceLeadCount).toBeGreaterThan(0);
    expect(result.runtimeSignals.correlationClusterCount).toBeGreaterThan(0);
    expect(result.runtimeSignals.correlationRuleFamilyCount).toBeGreaterThan(0);
    expect(result.runtimeSignals.correlationFamilyCoverage.length).toBeGreaterThan(0);
  });

  it('getCorrelationSnapshot returns lead and cluster summaries in mock mode', async () => {
    const result = await getCorrelationSnapshot();

    expect(result.generatedAt).toBeTruthy();
    expect(result.leadCount).toBeGreaterThan(0);
    expect(result.clusterCount).toBeGreaterThan(0);
    expect(result.familyCoverage.length).toBeGreaterThan(0);
    expect(result.leads[0].jumps.length).toBeGreaterThan(0);
    expect(result.clusters[0].provenance.length).toBeGreaterThan(0);
    expect(result.leads[0].primaryFileId).toBeDefined();
    expect(result.edges.some((edge) => edge.kind === 'pathMatch')).toBe(true);
    expect(result.edges.some((edge) => edge.kind === 'recoveredOriginalPath')).toBe(true);
    expect(
      result.leads.some(
        (lead) =>
          lead.summary.includes('JumpList') ||
          lead.provenance.some((item) => item.sourceLabel === 'JumpList'),
      ),
    ).toBe(true);
  });

  it('returns jump-location helpers for real navigation flows in mock mode', async () => {
    const fileJump = await getFileJumpContext('file-cmd-exe', {
      pageLimit: 100,
      showHidden: false,
    });
    const artifact = await getArtifactById('artifact-1');
    const timelinePage = await getTimelineEvents();
    const eventId = timelinePage.items[0]?.id;
    const timelineEvent = eventId ? await getTimelineEventById(eventId) : null;
    const artifactRows = await getArtifactRows();

    expect(fileJump.target.id).toBe('file-cmd-exe');
    expect(fileJump.directory.entryType).toBe('directory');
    expect(fileJump.ancestorDirectoryIds.length).toBeGreaterThan(0);
    expect(fileJump.rowOffset).toBeGreaterThanOrEqual(0);
    expect(artifact?.id).toBeDefined();
    expect(artifactRows.length).toBeGreaterThan(0);
    expect(timelineEvent?.id).toBe(eventId);
  });
});
