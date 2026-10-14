import type { NtfsTechnicalFile } from '@/types/files';

export function ntfsTechnicalFile(runCount = 1): NtfsTechnicalFile {
  return {
    inode: 42, sequenceNumber: 1, flags: 1, recordOffset: 1024, recordSize: 1024, recordRaw: [], recordDecoded: [], sourceRanges: [],
    attributes: [{
      recordOffset: 64, attributeType: 128, instance: 0, nonResident: true, raw: [],
      dataRuns: Array.from({ length: runCount }, (_, index) => ({
        header: 0x11, lengthFieldSize: 1, offsetFieldSize: 1,
        clusterCount: 1, logicalOffset: index * 4096, absoluteLcn: index + 2,
        raw: [0x11, 1, 2],
      })),
    }],
  };
}
