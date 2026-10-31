import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { startEvidenceTask, getEvidenceTask, cancelEvidenceTask } from '@/lib/api/evidence-tasks';
import { errorMessage } from '@/lib/errors';
import {
  readStoredDigestTaskOwner,
  writeStoredDigestTaskOwner,
  type StoredDigestTaskOwner,
} from '@/features/jobs/digest-task-owner-storage';
import type { EvidenceTask, EvidenceTaskRequest } from '@/types/evidenceTask';

interface StartOperation {
  caseId: string;
  contextKey: string;
  key: string;
  epoch: number;
  attempt: number;
  request: EvidenceTaskRequest;
}

const active = (state?: string) => state === 'queued' || state === 'running' || state === 'cancelling';
const ownerQueryKey = (caseId: string, contextKey: string) =>
  ['evidence-task-owner', caseId, contextKey] as const;
const taskQueryKey = (caseId: string, taskId: string) =>
  ['evidence-task', caseId, taskId] as const;

export function useEvidenceTask(caseId?: string, contextKey = '') {
  const key = JSON.stringify([caseId, contextKey]);
  const [owner, setOwner] = useState({ key, epoch: 0 });
  const attempt = useRef(0);
  if (owner.key !== key) setOwner({ key, epoch: owner.epoch + 1 });
  const cache = useQueryClient();
  const ownerLookup = useQuery({
    queryKey: caseId ? ownerQueryKey(caseId, contextKey) : ['evidence-task-owner', null, contextKey],
    queryFn: () => caseId
      ? cache.getQueryData<StoredDigestTaskOwner>(ownerQueryKey(caseId, contextKey))
        ?? readStoredDigestTaskOwner(caseId, contextKey)
        ?? null
      : null,
    enabled: !!caseId,
    staleTime: Infinity,
    gcTime: Infinity,
  });
  const start = useMutation({
    mutationFn: async (operation: StartOperation) => {
      const task = await startEvidenceTask(operation.request);
      const owner: StoredDigestTaskOwner = {
        caseId: operation.caseId,
        contextKey: operation.contextKey,
        request: operation.request,
        taskId: task.taskId,
        task,
        updatedAt: Date.now(),
      };
      cache.setQueryData(ownerQueryKey(operation.caseId, operation.contextKey), owner);
      cache.setQueryData(taskQueryKey(operation.caseId, task.taskId), task);
      writeStoredDigestTaskOwner(owner);
      return { operation, task };
    },
  });
  const current = start.variables?.key === key && start.variables.epoch === owner.epoch && start.variables.attempt === attempt.current;
  const started = current && start.data && start.data.operation === start.variables ? start.data.task : undefined;
  const cachedOwner = start.variables?.key === key ? undefined : ownerLookup.data ?? undefined;
  const initialTaskId = started?.taskId ?? cachedOwner?.taskId;
  const initialTask = initialTaskId && caseId
    ? cache.getQueryData<EvidenceTask>(taskQueryKey(caseId, initialTaskId)) ?? cachedOwner?.task
    : undefined;
  const status = useQuery({
    queryKey: ['evidence-task', caseId ?? null, initialTaskId ?? null],
    queryFn: () => getEvidenceTask(initialTaskId!),
    enabled: !!caseId && !!initialTaskId,
    retry: false,
    staleTime: 0,
    initialData: initialTask,
    refetchInterval: (query) => active(query.state.data?.state ?? started?.state ?? initialTask?.state) ? 250 : false,
  });
  const task = current ? status.data ?? started : status.data ?? initialTask;
  const request = current ? start.variables?.request : cachedOwner?.request;
  useEffect(() => {
    if (!caseId || !request || !task || active(task.state)) return;
    const stored: StoredDigestTaskOwner = {
      caseId,
      contextKey,
      request,
      taskId: task.taskId,
      task,
      updatedAt: Date.now(),
    };
    cache.setQueryData(ownerQueryKey(caseId, contextKey), stored);
    writeStoredDigestTaskOwner(stored);
  }, [cache, caseId, contextKey, request, task]);
  const cancel = useMutation({
    mutationFn: async (owner: { caseId: string; taskId: string }) => ({ owner, task: await cancelEvidenceTask(owner.taskId) }),
    onSuccess: ({ owner, task }) => cache.setQueryData(['evidence-task', owner.caseId, task.taskId], task),
  });
  return {
    request,
    task,
    loading: (current && start.isPending) || active(task?.state) || (!!initialTaskId && !task && status.isPending),
    errorText: current && start.error
      ? errorMessage(start.error)
      : status.error
        ? errorMessage(status.error)
        : cancel.variables?.taskId === task?.taskId && cancel.error
          ? errorMessage(cancel.error)
          : undefined,
    cancelPending: cancel.isPending || task?.state === 'cancelling',
    start: (request: EvidenceTaskRequest) => {
      if (caseId && owner.key === key) {
        start.mutate({
          caseId,
          contextKey,
          key,
          epoch: owner.epoch,
          attempt: ++attempt.current,
          request,
        });
      }
    },
    retry: () => {
      if (caseId && owner.key === key && request) {
        start.mutate({
          caseId,
          contextKey,
          key,
          epoch: owner.epoch,
          attempt: ++attempt.current,
          request,
        });
      }
    },
    cancel: () => { if (caseId && task && active(task.state)) cancel.mutate({ caseId, taskId: task.taskId }); },
  };
}
