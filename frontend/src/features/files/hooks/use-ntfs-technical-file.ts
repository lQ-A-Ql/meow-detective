import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { inspectNtfsFile } from '@/lib/api/files';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
import type { NtfsTechnicalFile } from '@/types/files';
import { isSourceScopedMftId } from '../source-scoped-file-id';
import { ntfsInspectorRuns } from '../ntfs-inspector-runs';

export function useNtfsTechnicalFile(caseId?: string, fileId?: string, isFile = false) {
  const key = JSON.stringify([caseId, fileId]);
  const [display, setDisplay] = useState<{ key: string; page: number; range?: { offset: number; length: number }; decoded: boolean }>({ key, page: 0, decoded: false });
  const eligible = Boolean(caseId && isFile && isSourceScopedMftId(fileId));
  const query = useQuery<NtfsTechnicalFile>({
    queryKey: ['files', 'ntfs-technical', caseId, fileId],
    enabled: eligible,
    retry: false,
    staleTime: Infinity,
    queryFn: () => inspectNtfsFile(fileId!),
  });
  const technical = eligible && !query.isError ? query.data : undefined;
  const currentDisplay = display.key === key ? display : { key, page: 0, decoded: false, range: undefined };
  const page = currentDisplay.page;
  const runs = ntfsInspectorRuns(technical, page);
  const range = currentDisplay.range;
  const decoded = currentDisplay.decoded;
  return {
    eligible,
    technical,
    runs,
    previousRuns: () => setDisplay({ ...currentDisplay, key, page: Math.max(0, page - 1) }),
    nextRuns: () => setDisplay({ ...currentDisplay, key, page: Math.min(Math.max(0, runs.totalPages - 1), page + 1) }),
    selectHex: (offset: number, length: number) => {
      if (!technical || !Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length <= 0 || offset + length > technical.recordRaw.length) return;
      setDisplay({ key, page, range: { offset, length }, decoded });
    },
    setDecoded: (decoded: boolean) => setDisplay({ ...currentDisplay, key, page, decoded }),
    hex: range && technical && range.offset + range.length <= technical.recordRaw.length ? { bytes: decoded ? technical.recordDecoded : technical.recordRaw, offset: range.offset, length: range.length, decoded } : undefined,
    loading: eligible && query.isFetching,
    errorText: eligible && query.error != null ? errorMessage(query.error) : undefined,
    unsupported: eligible && isApiErrorDto(query.error) && query.error.category === 'unsupported',
    retry: () => { if (eligible) void query.refetch(); },
  };
}
