import { describe, it, expect } from 'vitest';
import {
  getReportTemplates,
  getReportHistory,
  exportHtmlReport,
  exportCsvReport,
  exportJsonReport,
  exportCsvCorrelationReport,
} from '@/lib/api/reports';

describe('reports API (mock mode)', () => {
  it('getReportTemplates returns templates', async () => {
    const result = await getReportTemplates();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getReportTemplates items have required fields', async () => {
    const result = await getReportTemplates();
    for (const template of result) {
      expect(template.id).toBeDefined();
      expect(template.name).toBeDefined();
      expect(template.description).toBeDefined();
    }
  });

  it('getReportHistory returns history items', async () => {
    const result = await getReportHistory();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getReportHistory items have required fields', async () => {
    const result = await getReportHistory();
    for (const item of result) {
      expect(item.id).toBeDefined();
      expect(item.fileName).toBeDefined();
      expect(item.createdBy).toBeDefined();
      expect(item.createdAt).toBeDefined();
      expect(['completed', 'running']).toContain(item.status);
    }
  });

  it('exportHtmlReport returns mock fallback string', async () => {
    const result = await exportHtmlReport();
    expect(typeof result).toBe('string');
    expect(result).toContain('not available');
  });

  it('exportCsvReport returns mock fallback string', async () => {
    const result = await exportCsvReport();
    expect(typeof result).toBe('string');
    expect(result).toContain('not available');
  });

  it('exportJsonReport returns mock fallback string', async () => {
    const result = await exportJsonReport();
    expect(typeof result).toBe('string');
    expect(result).toContain('not available');
  });

  it('exportHtmlReport accepts scope parameter', async () => {
    const result = await exportHtmlReport({
      fileSystemMetadata: true,
      registry: true,
      fullTimeline: false,
      rawFileExtraction: false,
    });
    expect(typeof result).toBe('string');
  });

  it('exportCsvCorrelationReport returns mock fallback string', async () => {
    const result = await exportCsvCorrelationReport();
    expect(typeof result).toBe('string');
    expect(result).toContain('not available');
  });

  it('exportCsvCorrelationReport accepts scope parameter', async () => {
    const result = await exportCsvCorrelationReport({
      fileSystemMetadata: true,
      registry: false,
      fullTimeline: true,
      rawFileExtraction: false,
    });
    expect(typeof result).toBe('string');
  });
});
