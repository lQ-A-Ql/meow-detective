import { useCallback, useEffect, useRef, useState } from 'react';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryHiveSession } from '@/types/registryBrowser';
import type { RegistryBrowserContext } from '../types';

export function useRegistrySession(client: RegistryBrowserClient, context: RegistryBrowserContext) {
  const [session, setSession] = useState<RegistryHiveSession>();
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<unknown>();
  const [requestedFileId, setRequestedFileId] = useState<string>();
  const generation = useRef(0);
  const activeSession = useRef<string>();
  const release = useCallback((id?: string) => { if (id) void client.closeHive({ sessionId: id }).catch(() => undefined); }, [client]);
  useEffect(() => () => { generation.current += 1; release(activeSession.current); }, [release]);
  const openHive = useCallback(async (fileId: string) => {
    const token = ++generation.current;
    release(activeSession.current); activeSession.current = undefined;
    setSession(undefined); setError(undefined); setOpening(true); setRequestedFileId(fileId);
    try {
      const opened = await client.openHive({ fileId, view: 'base' });
      if (generation.current !== token) { release(opened.sessionId); return undefined; }
      if (opened.source.caseId !== context.caseId || opened.source.dataSourceId !== context.dataSourceId
        || opened.source.fileId !== fileId || opened.source.sourceRevision !== context.sourceRevision) {
        release(opened.sessionId); throw Object.assign(new Error('sourceMismatch'), { code: 'expiredSession' });
      }
      activeSession.current = opened.sessionId; setSession(opened); return opened;
    } catch (failure) { if (generation.current === token) setError(failure); return undefined; }
    finally { if (generation.current === token) setOpening(false); }
  }, [client, context.caseId, context.dataSourceId, context.sourceRevision, release]);
  return { session, opening, error, requestedFileId, openHive };
}
