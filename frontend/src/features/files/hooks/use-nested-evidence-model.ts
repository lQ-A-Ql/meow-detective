import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import type { FileEntryRow } from '@/types/models';
import { parseSourceScopedFileId } from '../source-scoped-file-id';

export function canOpenNested(file: FileEntryRow) { return file.entryType === 'file' && /\.vhd$/i.test(file.name) && !!parseSourceScopedFileId(file.id); }
export function useNestedEvidenceModel(caseId?: string) {
  const task = useEvidenceTask(caseId, 'nested-import');
  const cache = useQueryClient();
  const result = task.task?.result?.kind === 'nestedImport' ? task.task.result.result : undefined;
  useEffect(() => {
    if (result) { void cache.invalidateQueries({ queryKey: ['case', 'data-sources', caseId] }); void cache.invalidateQueries({ queryKey: ['files'] }); }
  }, [result, caseId, cache]);
  return { ...task, result, canOpen: canOpenNested, open: (file: FileEntryRow) => {
    const id = parseSourceScopedFileId(file.id);
    if (id && canOpenNested(file) && !task.loading) task.start({ kind: 'nestedImport', request: { parentDataSourceId: id.sourceId, fileEntryId: id.localId } });
  } };
}
