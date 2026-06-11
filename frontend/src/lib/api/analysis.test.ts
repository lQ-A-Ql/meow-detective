import { describe, expect, it } from 'vitest';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getEmailExtractionSummary,
  getEvidenceClassificationSummary,
  getRegistryExtractionSummary,
  getSystemInfo,
  runAnalysisExtraction,
} from '@/lib/api/analysis';

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

    expect(registry.total).toBeGreaterThan(0);
    expect(registry.values[0].keyPath).toContain('ComputerName');
    expect(registry.values[0].valueName).toBe('ComputerName');

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
});
