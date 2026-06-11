import { describe, it, expect } from 'vitest';
import { getArtifactFamilies, getArtifactRows, getArtifactFamilyCounts } from '@/lib/api/artifacts';

describe('artifacts API (mock mode)', () => {
  it('getArtifactFamilies returns string array', async () => {
    const result = await getArtifactFamilies();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
    for (const family of result) {
      expect(typeof family).toBe('string');
    }
  });

  it('getArtifactFamilies includes expected families', async () => {
    const result = await getArtifactFamilies();
    expect(result).toContain('LNK');
    expect(result).toContain('Prefetch');
  });

  it('getArtifactRows returns artifact rows', async () => {
    const result = await getArtifactRows();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getArtifactRows items have required fields', async () => {
    const result = await getArtifactRows();
    for (const row of result) {
      expect(row.id).toBeDefined();
      expect(row.artifactType).toBeDefined();
      expect(row.title).toBeDefined();
      expect(row.summary).toBeDefined();
      expect(typeof row.attrs).toBe('object');
    }
  });

  it('getArtifactRows filters by family', async () => {
    const allRows = await getArtifactRows();
    const lnkRows = await getArtifactRows('LNK');
    expect(lnkRows.length).toBeLessThanOrEqual(allRows.length);
    for (const row of lnkRows) {
      expect(row.artifactType).toBe('LNK');
    }
  });

  it('getArtifactFamilyCounts returns array', async () => {
    const result = await getArtifactFamilyCounts();
    expect(Array.isArray(result)).toBe(true);
  });
});
