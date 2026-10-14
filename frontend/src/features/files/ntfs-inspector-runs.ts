import type { NtfsTechnicalFile } from '@/types/files';

const DISPLAYED_RUN_LIMIT = 32;

/** Produce a bounded display projection and retain the full run count. */
export function ntfsInspectorRuns(technical?: NtfsTechnicalFile) {
  const rows: { logicalOffset: number; absoluteLcn?: number | null; clusterCount: number; rawHex: string }[] = [];
  let total = 0;
  for (const attribute of technical?.attributes ?? []) {
    total += attribute.dataRuns.length;
    for (const run of attribute.dataRuns.slice(0, DISPLAYED_RUN_LIMIT - rows.length)) {
      rows.push({
        logicalOffset: run.logicalOffset,
        absoluteLcn: run.absoluteLcn,
        clusterCount: run.clusterCount,
        rawHex: run.raw.map((value) => value.toString(16).padStart(2, '0')).join(' '),
      });
    }
  }
  return { rows, total };
}
