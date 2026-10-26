import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createMcpHostClient, listMcpHostClients, revokeMcpHostClient } from '@/lib/api/mcp-host';
import type { McpHostCreatedClient } from '@/types/mcpHost';

const key = ['mcp-host', 'clients'] as const;

export function useMcpAccessModel() {
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [hours, setHours] = useState('24');
  const [readEvidence, setReadEvidence] = useState(false);
  const [created, setCreated] = useState<McpHostCreatedClient>();
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<unknown>();
  const clients = useQuery({ queryKey: key, queryFn: listMcpHostClients, retry: false });
  const revoke = useMutation({
    mutationFn: revokeMcpHostClient,
    onSuccess: () => { void queryClient.invalidateQueries({ queryKey: key }); },
  });
  const expiresAfterHours = Number(hours);
  return {
    clients: clients.data ?? [], name, setName, hours, setHours, readEvidence, setReadEvidence,
    created, closeCreated: () => setCreated(undefined),
    busy: clients.isLoading || creating || revoke.isPending,
    error: createError ?? revoke.error ?? clients.error,
    canCreate: name.trim().length > 0 && name.length <= 100 && Number.isInteger(expiresAfterHours)
      && expiresAfterHours >= 1 && expiresAfterHours <= 720,
    create: async () => {
      if (creating) return;
      setCreating(true); setCreateError(undefined);
      try {
        const result = await createMcpHostClient({ name, expiresAfterHours, readEvidence });
        setCreated(result); setName('');
        void queryClient.invalidateQueries({ queryKey: key });
      } catch (error) { setCreateError(error); }
      finally { setCreating(false); }
    },
    revoke: (id: string) => revoke.mutate(id),
  };
}
