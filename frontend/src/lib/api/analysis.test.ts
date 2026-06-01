import { describe, expect, it } from 'vitest';
import {
  classifyFiles,
  generateAnalysisSummary,
  getSystemInfo,
} from '@/lib/api/analysis';

describe('analysis API (mock mode)', () => {
  it('getSystemInfo returns explicit notParsed status without fake facts', async () => {
    const result = await getSystemInfo();

    expect(result.status).toBe('notParsed');
    expect(result.computerName).toBeUndefined();
    expect(result.osVersion).toBeUndefined();
    expect(result.warnings.length).toBeGreaterThan(0);
    expect(result.provenance.length).toBeGreaterThan(0);
    expect(result.provenance[0].parser).toBeDefined();
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
    expect(result).toContain('未解析');
    expect(result).not.toContain('FORENSICS-PC');
    expect(result).not.toContain('Windows 10');
  });
});
