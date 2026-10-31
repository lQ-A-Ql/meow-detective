import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { listEvidenceDigests } from '@/lib/api/digest';
import type { DataSourceSummary, FileEntryRow } from '@/types/models';
import type { DigestAlgorithm, DigestScope } from '@/types/digest';
import { parseSourceScopedFileId } from '../source-scoped-file-id';

export const DIGEST_SCOPES: DigestScope[] = ['containerFile', 'containerSet', 'logicalDisk', 'partition', 'file', 'derivedEvidence'];
export function useEvidenceDigestModel(caseId: string | undefined, file: FileEntryRow | undefined, source: DataSourceSummary | undefined, partitionIndex?: number) {
  const scoped = parseSourceScopedFileId(file?.id);
  const sourceId = scoped?.sourceId ?? source?.id;
  const owner = JSON.stringify([caseId, sourceId, file?.id, partitionIndex]);
  const [selection, setSelection] = useState<{ owner: string; scope: DigestScope }>({ owner, scope: 'file' });
  const [forceSelection, setForceSelection] = useState({ owner, force: false });
  const scope = selection.owner === owner ? selection.scope : 'file';
  const forceRecompute = forceSelection.owner === owner ? forceSelection.force : false;
  const task = useEvidenceTask(caseId, owner);
  const cache = useQueryClient();
  const historyKey = ['evidence-digests', caseId, sourceId, file?.id, partitionIndex];
  const history = useQuery({ queryKey: historyKey, enabled: !!caseId && !!sourceId, retry: false,
    queryFn: () => listEvidenceDigests({ dataSourceId: sourceId!, fileId: file?.id, partitionIndex }),
  });
  useEffect(() => {
    if (task.task?.state !== 'completed') return;
    void cache.invalidateQueries({ queryKey: ['evidence-digests', caseId, sourceId] });
    if (task.request?.kind === 'digest' && task.request.request.algorithm === 'sha256') {
      void cache.invalidateQueries({ queryKey: ['files'] });
    }
  }, [task.task?.state, task.task?.taskId, task.request, cache, caseId, sourceId]);
  const available = !!caseId && !!sourceId && (scope !== 'file' || file?.entryType === 'file') && (scope !== 'partition' || partitionIndex !== undefined);
  return { ...task, scope, forceRecompute, available, scopes: DIGEST_SCOPES,
    results: history.data?.filter((result) => result.digest.scope === scope) ?? [], historyLoading: history.isFetching,
    historyError: history.error,
    setScope: (scope: DigestScope) => setSelection({ owner, scope }),
    setForceRecompute: (force: boolean) => setForceSelection({ owner, force }),
    calculate: (algorithm: DigestAlgorithm) => { if (available) task.start({ kind: 'digest', request: { scope, algorithm, dataSourceId: sourceId, ...(scope === 'file' ? { fileId: file?.id } : {}), ...(scope === 'partition' ? { partitionIndex } : {}), ...(forceRecompute ? { forceRecompute: true } : {}) } }); },
  };
}
