import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { startEvidenceTask, getEvidenceTask, cancelEvidenceTask } from '@/lib/api/evidence-tasks';
import { errorMessage } from '@/lib/errors';
import type { EvidenceTaskRequest } from '@/types/evidenceTask';
const active = (state?: string) => state === 'queued' || state === 'running' || state === 'cancelling';
export function useEvidenceTask(caseId?: string, contextKey = '') {
  const key = JSON.stringify([caseId, contextKey]);
  const [owner, setOwner] = useState({ key, epoch: 0 });
  const attempt = useRef(0);
  if (owner.key !== key) setOwner({ key, epoch: owner.epoch + 1 });
  const cache = useQueryClient();
  const start = useMutation({
    mutationFn: async (operation: { key: string; epoch: number; attempt: number; request: EvidenceTaskRequest }) =>
      ({ operation, task: await startEvidenceTask(operation.request) }),
  });
  const current = start.variables?.key === key && start.variables.epoch === owner.epoch && start.variables.attempt === attempt.current;
  const initial = current && start.data && start.data.operation === start.variables ? start.data.task : undefined;
  const status = useQuery({
    queryKey: ['evidence-task', caseId, initial?.taskId],
    queryFn: () => getEvidenceTask(initial!.taskId),
    enabled: !!caseId && !!initial,
    retry: false,
    staleTime: 0,
    refetchInterval: (query) => active(query.state.data?.state ?? initial?.state) ? 250 : false,
  });
  const task = current ? status.data ?? initial : undefined;
  const cancel = useMutation({
    mutationFn: async (owner: { caseId: string; taskId: string }) => ({ owner, task: await cancelEvidenceTask(owner.taskId) }),
    onSuccess: ({ owner, task }) => cache.setQueryData(['evidence-task', owner.caseId, task.taskId], task),
  });
  return {
    request: current ? start.variables?.request : undefined,
    task,
    loading: current && (start.isPending || active(task?.state)),
    errorText: current ? start.error ? errorMessage(start.error) : status.error ? errorMessage(status.error) : cancel.variables?.taskId === task?.taskId && cancel.error ? errorMessage(cancel.error) : undefined : undefined,
    cancelPending: cancel.isPending || task?.state === 'cancelling',
    start: (request: EvidenceTaskRequest) => {
      if (caseId && owner.key === key) start.mutate({ key, epoch: owner.epoch, attempt: ++attempt.current, request });
    },
    retry: () => {
      if (caseId && current && start.variables) start.mutate({ ...start.variables, attempt: ++attempt.current });
    },
    cancel: () => { if (caseId && task && active(task.state)) cancel.mutate({ caseId, taskId: task.taskId }); },
  };
}
