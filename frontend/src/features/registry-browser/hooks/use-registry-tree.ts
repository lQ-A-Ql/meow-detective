import { useCallback, useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryHiveSession, RegistryKeyMetadata } from '@/types/registryBrowser';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';
import { registryBrowserErrorCode } from '../errors';
import type { RegistryTreePage } from '../types';

export function useRegistryTree(client: RegistryBrowserClient, session?: RegistryHiveSession) {
  const queryClient = useQueryClient();
  const [pages, setPages] = useState<Record<string, RegistryTreePage>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const activeSession = useRef<string>();
  activeSession.current = session?.sessionId;
  const load = useCallback(async (key: RegistryKeyMetadata, cursor?: string) => {
    if (!session || key.subkeyCount === 0) return;
    const sessionId = session.sessionId;
    setLoading((state) => new Set(state).add(key.keyId));
    setErrors((state) => { const next = { ...state }; delete next[key.keyId]; return next; });
    try {
      const page = await queryClient.fetchQuery({
        queryKey: ['registry-browser', 'subkeys', session.source, sessionId, key.keyId, cursor ?? null],
        queryFn: () => client.listSubkeys({ sessionId, keyId: key.keyId, cursor, limit: REGISTRY_BROWSER_LAYOUT.pageSize }),
        staleTime: Infinity,
      });
      if (activeSession.current !== sessionId) return;
      setPages((state) => ({ ...state, [key.keyId]: { page, keys: cursor
        ? [...(state[key.keyId]?.keys ?? []), ...page.keys] : page.keys } }));
      return page;
    } catch (error) {
      if (activeSession.current === sessionId) setErrors((state) => ({ ...state, [key.keyId]: registryBrowserErrorCode(error) }));
    } finally {
      if (activeSession.current === sessionId) setLoading((state) => { const next = new Set(state); next.delete(key.keyId); return next; });
    }
  }, [client, queryClient, session]);
  useEffect(() => {
    setPages({}); setErrors({}); setLoading(new Set());
    setExpanded(session ? new Set([session.rootKey.keyId]) : new Set());
    if (session) void load(session.rootKey);
  }, [session?.sessionId]);
  const toggle = useCallback((key: RegistryKeyMetadata) => {
    const isExpanded = expanded.has(key.keyId);
    setExpanded((state) => { const next = new Set(state); if (isExpanded) next.delete(key.keyId); else next.add(key.keyId); return next; });
    if (!isExpanded && !pages[key.keyId]) void load(key);
  }, [expanded, load, pages]);
  useEffect(() => () => { activeSession.current = undefined; }, []);
  return { pages, expanded, loading, errors, toggle, load,
    expand: (key: RegistryKeyMetadata) => setExpanded((state) => new Set(state).add(key.keyId)) };
}
