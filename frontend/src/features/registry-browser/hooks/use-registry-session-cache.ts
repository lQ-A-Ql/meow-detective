import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';

export function useRegistrySessionCache(sessionId?: string) {
  const client = useQueryClient();
  useEffect(() => () => {
    if (!sessionId) return;
    const filter = { predicate: (query: { queryKey: readonly unknown[] }) => query.queryKey[0] === 'registry-browser' && query.queryKey.includes(sessionId) };
    void client.cancelQueries(filter);
    client.removeQueries(filter);
  }, [client, sessionId]);
}
