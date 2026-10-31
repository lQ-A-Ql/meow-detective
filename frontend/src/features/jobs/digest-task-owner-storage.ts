import type { EvidenceTask, EvidenceTaskRequest } from '@/types/evidenceTask';

const STORAGE_KEY = 'meow-detective.digest-task-owners.v1';
const MAX_OWNERS = 64;

export interface StoredDigestTaskOwner {
  caseId: string;
  contextKey: string;
  request: EvidenceTaskRequest;
  taskId: string;
  task?: EvidenceTask;
  updatedAt: number;
}

type StoredDigestTaskOwners = Record<string, StoredDigestTaskOwner>;

const ownerId = (caseId: string, contextKey: string) => JSON.stringify([caseId, contextKey]);

function storage(): Storage | undefined {
  return typeof window === 'undefined' ? undefined : window.localStorage;
}

function readOwners(target: Storage): StoredDigestTaskOwners {
  const raw = target.getItem(STORAGE_KEY);
  return raw ? JSON.parse(raw) as StoredDigestTaskOwners : {};
}

export function readStoredDigestTaskOwner(caseId: string, contextKey: string): StoredDigestTaskOwner | undefined {
  const target = storage();
  return target ? readOwners(target)[ownerId(caseId, contextKey)] : undefined;
}

export function writeStoredDigestTaskOwner(owner: StoredDigestTaskOwner): void {
  if (owner.request.kind !== 'digest') return;
  const target = storage();
  if (!target) return;
  const owners = readOwners(target);
  owners[ownerId(owner.caseId, owner.contextKey)] = owner;
  const retained = Object.entries(owners)
    .sort((left, right) => right[1].updatedAt - left[1].updatedAt)
    .slice(0, MAX_OWNERS);
  target.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(retained)));
}
