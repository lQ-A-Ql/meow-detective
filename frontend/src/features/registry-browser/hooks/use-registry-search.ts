import { useState } from 'react';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import type { RegistryHiveSession } from '@/types/registryBrowser';
export function useRegistrySearch(session?: RegistryHiveSession) {
  const [query, setQuery] = useState('');
  const task = useEvidenceTask(session?.source.caseId, `registry:${session?.sessionId}`);
  const page = task.task?.result?.kind === 'registrySearch' ? task.task.result.result : undefined;
  return { ...task, query, setQuery, page, run: () => { if (session && query.trim()) task.start({ kind: 'registrySearch', request: { fileId: session.source.fileId, sessionId: session.sessionId, query: query.trim() } }); } };
}
