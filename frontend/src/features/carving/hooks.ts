import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { exportCarvedArtifact, readCarvedArtifactRange, searchCarvedArtifacts } from '@/lib/api/carving';
import { saveDialog } from '@/lib/platform/dialog';
import { errorMessage } from '@/lib/errors';
import type { CarvedArtifact, CarvedExport } from '@/types/carving';

const validDecimal = (value: string) => /^(0|[1-9]\d{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;
export function useCarvingModel(format: 'png' | 'zip', caseId?: string, sourceId?: string, enabled = true) {
  const key = JSON.stringify([caseId, sourceId]);
  const live = useRef(key); live.current = key;
  const attempt = useRef(0);
  const [owner, setOwner] = useState(key);
  const [offset, setOffset] = useState('0');
  const [length, setLength] = useState('');
  const [query, setQueryValue] = useState('');
  const [cursor, setCursor] = useState<string>();
  const [preview, setPreview] = useState<{ id: string; bytes: number[] }>();
  const [exported, setExported] = useState<CarvedExport>();
  const [operationError, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [requested, setRequested] = useState(false);
  if (owner !== key) {
    setOwner(key); setOffset('0'); setLength(''); setQueryValue(''); setCursor(undefined);
    setPreview(undefined); setExported(undefined); setError(undefined); setBusy(false); setRequested(false); attempt.current++;
  }
  const task = useEvidenceTask(caseId, `${format}:${sourceId ?? ''}`);
  const cache = useQueryClient();
  const refreshed = useRef<string>();
  useEffect(() => {
    if (task.task?.state === 'completed' && refreshed.current !== task.task.taskId) {
      refreshed.current = task.task.taskId;
      void cache.invalidateQueries({ predicate: q => !!caseId && q.queryKey.includes(caseId) });
    }
  }, [cache, caseId, task.task]);
  const results = useQuery({
    queryKey: ['carving', caseId, sourceId, query, cursor],
    queryFn: () => searchCarvedArtifacts(sourceId!, query, cursor),
    enabled: enabled && requested && !!caseId && !!sourceId, retry: false,
  });
  const valid = validDecimal(offset) && (!length || (validDecimal(length) && BigInt(length) > 0n && BigInt(length) <= 1073741824n));
  const perform = async (action: () => Promise<void>) => {
    const selected = key; const generation = ++attempt.current;
    setBusy(true); setError(undefined);
    try { await action(); } catch (error) { if (live.current === selected && generation === attempt.current) setError(errorMessage(error)); }
    finally { if (live.current === selected && generation === attempt.current) setBusy(false); }
  };
  const read = (artifact: CarvedArtifact) => {
    const selected = key; const generation = attempt.current + 1;
    void perform(async () => {
      const response = await readCarvedArtifactRange(artifact.artifactId);
      if (live.current === selected && generation === attempt.current) setPreview({ id: artifact.artifactId, bytes: Array.from(atob(response.base64), c => c.charCodeAt(0)) });
    });
  };
  const extract = (artifact: CarvedArtifact) => {
    const selected = key; const generation = attempt.current + 1;
    void perform(async () => {
      const path = await saveDialog({ defaultPath: `carved-${artifact.offset}.png` });
      if (!path || live.current !== selected || generation !== attempt.current) return;
      const result = await exportCarvedArtifact(artifact.artifactId, path);
      if (live.current === selected && generation === attempt.current) setExported(result);
    });
  };
  const resultKind = format === 'png' ? 'carvePng' : 'carveZip';
  const summary = task.task?.result?.kind === resultKind ? task.task.result.result : undefined;
  return { supported: !!caseId && !!sourceId, offset, setOffset, length, setLength, valid, query,
    setQuery: (value: string) => { setRequested(true); setQueryValue(value); setCursor(undefined); },
    load: () => setRequested(true),
    items: results.data?.items ?? [], next: () => setCursor(results.data?.nextCursor), first: () => setCursor(undefined), hasNext: !!results.data?.nextCursor, hasPrevious: !!cursor,
    start: () => { if (enabled && caseId && sourceId && valid) { setRequested(true); task.start({ kind: format === 'png' ? 'carvePng' : 'carveZip', request: { dataSourceId: sourceId, offset, length: length || undefined } }); } },
    cancel: task.cancel, scanning: task.loading, summary, preview, exported, busy,
    error: task.errorText ?? operationError ?? (results.error ? errorMessage(results.error) : undefined), read, extract };
}
export function usePngCarvingModel(caseId?: string, sourceId?: string, enabled = true) { return useCarvingModel('png', caseId, sourceId, enabled); }
export function useZipCarvingModel(caseId?: string, sourceId?: string, enabled = true) { return useCarvingModel('zip', caseId, sourceId, enabled); }
export type PngCarvingModel = ReturnType<typeof usePngCarvingModel>;
export type ZipCarvingModel = ReturnType<typeof useZipCarvingModel>;
