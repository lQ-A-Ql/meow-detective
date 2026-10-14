import type { NtfsTechnicalFile } from '@/types/files';

const DISPLAYED_RUN_LIMIT = 32;

/** Produce a bounded display projection and retain the full run count. */
export function ntfsInspectorRuns(technical?: NtfsTechnicalFile, page = 0) {
  const rows: {
    logicalOffset: number; absoluteLcn?: number | null; relativeLcn?: number | null;
    physicalOffset?: number | null; clusterCount: number; header: number;
    lengthFieldSize: number; offsetFieldSize: number; rawHex: string; recordOffset: number;
    rawLength: number; attributeType: number;
  }[] = [];
  let total = 0;
  for (const attribute of technical?.attributes ?? []) {
    const start = attribute.raw[0x20] ?? 0;
    const end = attribute.raw[0x21] ?? 0;
    let recordOffset = attribute.recordOffset + start + end * 256;
    for (const run of attribute.dataRuns) {
      const index = total++;
      if (index >= page * DISPLAYED_RUN_LIMIT && rows.length < DISPLAYED_RUN_LIMIT) rows.push({
        ...run, recordOffset, rawLength: run.raw.length, attributeType: attribute.attributeType,
        logicalOffset: run.logicalOffset,
        absoluteLcn: run.absoluteLcn,
        clusterCount: run.clusterCount,
        rawHex: run.raw.map((value) => value.toString(16).padStart(2, '0')).join(' '),
      });
      recordOffset += run.raw.length;
    }
  }
  return { rows, total, page, totalPages: Math.ceil(total / DISPLAYED_RUN_LIMIT) };
}
