import { describe, expect, it } from 'vitest';
import {
  classifyFiles,
  generateAnalysisSummary,
  getSystemInfo,
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
    expect(result[0].files[0].provenance.parser).toBe('analysis.magic');
  });

  it('generateAnalysisSummary returns markdown without hardcoded fake system facts', async () => {
    const result = await generateAnalysisSummary();

    expect(result).toContain('# 数据源分析报告');
    expect(result).toContain('Windows Evidence Edition');
    expect(result).not.toContain('FORENSICS-PC');
    expect(result).not.toContain('Windows 10');
  });
});
